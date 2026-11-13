// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.provider.Settings as AndroidSettings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.QuickUnlockReady
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/**
 * Quick unlock after a full unlock. The core stores PK and S encrypted under a random quick key
 * (QK); this class keeps QK wrapped by a hardware-backed Android Keystore key:
 * - fingerprint: key usable only after a strong biometric authentication, invalidated when a new
 *   fingerprint is enrolled;
 * - app PIN: key usable only while the device is unlocked; the PIN is also mixed into the core
 *   encryption; after 5 wrong PINs the PIN quick unlock is destroyed.
 * Every quick unlock also ends after a reboot, after the configured validity and on a panic lock.
 */
class AndroidQuickUnlock(
    private val context: Context,
    private val suite: () -> GhulSuite,
    private val settings: Settings,
    private val activity: () -> Activity?,
) {
    private val prefs = context.getSharedPreferences("quick_unlock", Context.MODE_PRIVATE)
    private val keyStore: KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    fun biometricAvailable(): Boolean {
        val bm = context.getSystemService(BiometricManager::class.java) ?: return false
        val result = if (Build.VERSION.SDK_INT >= 30) {
            bm.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)
        } else {
            @Suppress("DEPRECATION")
            bm.canAuthenticate()
        }
        return result == BiometricManager.BIOMETRIC_SUCCESS
    }

    private fun bootCount(): Int =
        AndroidSettings.Global.getInt(context.contentResolver, AndroidSettings.Global.BOOT_COUNT, -1)

    private fun usable(name: String): Boolean {
        val blob = prefs.getString("$name.blob", null) ?: return false
        if (blob.isEmpty() || prefs.getInt("$name.boot", -2) != bootCount()) return false
        return keyStore.containsAlias(alias(name)) && suite().quickUnlockExists(name)
    }

    fun ready(): QuickUnlockReady = QuickUnlockReady(
        biometric = settings.quickUnlockBiometric && usable(BIO),
        pin = settings.quickUnlockPin && usable(PIN),
    )

    /** 0 hours = "always": no expiry, but a reboot, panic lock or password change still ends it. */
    private fun maxAgeSeconds(): ULong =
        if (settings.quickUnlockHours <= 0) ALWAYS_SECONDS else (settings.quickUnlockHours.coerceIn(1, 24 * 7) * 3600L).toULong()

    private fun alias(name: String) = "ghulsuite.quick.$name"

    private fun createKey(name: String, biometric: Boolean): SecretKey {
        val alias = alias(name)
        if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
        fun spec(strongBox: Boolean, unlockedOnly: Boolean) = KeyGenParameterSpec.Builder(
            alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        ).apply {
            setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            setKeySize(256)
            if (strongBox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
            if (unlockedOnly && Build.VERSION.SDK_INT >= 28) setUnlockedDeviceRequired(true)
            if (biometric) {
                setUserAuthenticationRequired(true)
                setInvalidatedByBiometricEnrollment(true)
                if (Build.VERSION.SDK_INT >= 30) {
                    setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                } else {
                    @Suppress("DEPRECATION")
                    setUserAuthenticationValidityDurationSeconds(-1)
                }
            }
        }.build()
        // "Only while the device is unlocked" needs a screen lock on the device; without one Android
        // refuses such keys (and the protection would be empty anyway). StrongBox is used where there
        // is one; any failure there falls back to the normal hardware keystore.
        val secureDevice = context.getSystemService(android.app.KeyguardManager::class.java)?.isDeviceSecure == true
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        var last: Exception? = null
        for (strongBox in listOf(true, false)) {
            try {
                gen.init(spec(strongBox, unlockedOnly = secureDevice))
                return gen.generateKey()
            } catch (e: Exception) {
                last = e
                runCatching { keyStore.deleteEntry(alias) }
            }
        }
        throw last ?: IllegalStateException("no key")
    }

    private fun store(name: String, cipher: Cipher, qk: ByteArray) {
        val ct = cipher.doFinal(qk)
        val blob = Base64.encodeToString(cipher.iv + ct, Base64.NO_WRAP)
        prefs.edit().putString("$name.blob", blob).putInt("$name.boot", bootCount()).putInt("$name.fails", 0).apply()
        qk.fill(0)
    }

    private fun decryptCipher(name: String): Pair<Cipher, ByteArray> {
        val raw = Base64.decode(prefs.getString("$name.blob", null) ?: throw unavailable(), Base64.NO_WRAP)
        val key = keyStore.getKey(alias(name), null) as? SecretKey ?: throw unavailable()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw, 0, 12))
        return cipher to raw.copyOfRange(12, raw.size)
    }

    private fun unavailable() = VaultException(VaultErrorKind.QUICK_UNAVAILABLE)

    // --- fingerprint ---------------------------------------------------------------------

    suspend fun enableBiometric(title: String, subtitle: String, cancel: String) {
        val qk = withContext(Dispatchers.IO) { mappedQuick { suite().quickUnlockEnable(BIO, null) } }
        try {
            val key = createKey(BIO, biometric = true)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key) }
            val authed = authenticate(cipher, title, subtitle, cancel)
            store(BIO, authed, qk)
            settings.quickUnlockBiometric = true
        } catch (e: Exception) {
            disable(BIO)
            throw e
        }
    }

    suspend fun unlockBiometric(title: String, subtitle: String, cancel: String) {
        val (cipher, ct) = try {
            decryptCipher(BIO)
        } catch (e: KeyPermanentlyInvalidatedException) {
            disable(BIO)
            throw VaultException(VaultErrorKind.BIOMETRIC_CHANGED)
        }
        val authed = authenticate(cipher, title, subtitle, cancel)
        val qk = authed.doFinal(ct)
        try {
            withContext(Dispatchers.IO) { mappedQuick { suite().unlockQuick(BIO, qk, null, maxAgeSeconds()) } }
        } catch (e: VaultException) {
            if (e.kind != VaultErrorKind.QUICK_INVALID) disable(BIO)
            throw e
        } finally {
            qk.fill(0)
        }
    }

    // --- app PIN -------------------------------------------------------------------------

    suspend fun enablePin(pin: String) {
        val qk = withContext(Dispatchers.IO) { mappedQuick { suite().quickUnlockEnable(PIN, pin) } }
        try {
            val key = createKey(PIN, biometric = false)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key) }
            store(PIN, cipher, qk)
            settings.quickUnlockPin = true
        } catch (e: Exception) {
            disable(PIN)
            throw e
        }
    }

    suspend fun unlockPin(pin: String) {
        // Counted before the try and written to disk at once: killing the app right after a wrong
        // PIN (before a delayed write) must not give a free attempt.
        val fails = prefs.getInt("$PIN.fails", 0) + 1
        if (fails > MAX_PIN_ATTEMPTS) {
            disable(PIN)
            throw VaultException(VaultErrorKind.QUICK_INVALID, retries = 0)
        }
        prefs.edit().putInt("$PIN.fails", fails).commit()
        val (cipher, ct) = decryptCipher(PIN)
        val qk = cipher.doFinal(ct)
        try {
            withContext(Dispatchers.IO) { mappedQuick { suite().unlockQuick(PIN, qk, pin, maxAgeSeconds()) } }
            prefs.edit().putInt("$PIN.fails", 0).commit()
        } catch (e: VaultException) {
            if (e.kind == VaultErrorKind.QUICK_INVALID) {
                val left = MAX_PIN_ATTEMPTS - fails
                if (left <= 0) disable(PIN)
                throw VaultException(VaultErrorKind.QUICK_INVALID, retries = left.coerceAtLeast(0))
            }
            disable(PIN)
            throw e
        } finally {
            qk.fill(0)
        }
    }

    /** After a full unlock: quick unlock is valid again after a reboot (the core restarts the period). */
    fun onFullUnlock() {
        val edit = prefs.edit()
        for (name in listOf(BIO, PIN)) {
            if (prefs.contains("$name.blob")) edit.putInt("$name.boot", bootCount())
        }
        edit.apply()
    }

    // --- removal -------------------------------------------------------------------------

    fun disable(name: String) {
        runCatching { suite().quickUnlockDisable(name) }
        runCatching { if (keyStore.containsAlias(alias(name))) keyStore.deleteEntry(alias(name)) }
        prefs.edit().remove("$name.blob").remove("$name.boot").remove("$name.fails").apply()
        when (name) {
            BIO -> settings.quickUnlockBiometric = false
            PIN -> settings.quickUnlockPin = false
        }
    }

    /** Destroys every quick unlock (panic lock). */
    fun disableAll() {
        disable(BIO)
        disable(PIN)
        runCatching { suite().quickUnlockDisableAll() }
    }

    private suspend fun authenticate(cipher: Cipher, title: String, subtitle: String, cancel: String): Cipher =
        withContext(Dispatchers.Main) {
            val act = activity() ?: throw VaultException(VaultErrorKind.BIOMETRIC_CANCELLED, "no activity")
            suspendCancellableCoroutine { cont ->
                val signal = CancellationSignal()
                val prompt = BiometricPrompt.Builder(act)
                    .setTitle(title)
                    .setSubtitle(subtitle)
                    .setConfirmationRequired(false)
                    .setNegativeButton(cancel, act.mainExecutor) { _, _ ->
                        if (cont.isActive) cont.resumeWithException(VaultException(VaultErrorKind.BIOMETRIC_CANCELLED))
                    }
                    .apply {
                        if (Build.VERSION.SDK_INT >= 30) setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
                    }
                    .build()
                prompt.authenticate(
                    BiometricPrompt.CryptoObject(cipher), signal, act.mainExecutor,
                    object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                            val c = result.cryptoObject?.cipher
                            if (!cont.isActive) return
                            if (c == null) {
                                cont.resumeWithException(VaultException(VaultErrorKind.BIOMETRIC_CANCELLED, "no cipher"))
                            } else {
                                cont.resume(c)
                            }
                        }

                        override fun onAuthenticationError(errorCode: Int, errString: CharSequence?) {
                            if (cont.isActive) {
                                cont.resumeWithException(VaultException(VaultErrorKind.BIOMETRIC_CANCELLED, errString?.toString().orEmpty()))
                            }
                        }
                    },
                )
                cont.invokeOnCancellation { signal.cancel() }
            }
        }

    companion object {
        /** "Always" valid quick unlock: about 100 years. */
        const val ALWAYS_SECONDS: ULong = 3_153_600_000uL
        const val BIO = "bio"
        const val PIN = "pin"
        const val MAX_PIN_ATTEMPTS = 5
    }
}

/** Maps core errors of the quick unlock functions. */
private inline fun <T> mappedQuick(block: () -> T): T = try {
    block()
} catch (e: GhulException.QuickUnlockInvalid) {
    throw VaultException(VaultErrorKind.QUICK_INVALID)
} catch (e: GhulException.QuickUnlockExpired) {
    throw VaultException(VaultErrorKind.QUICK_EXPIRED)
} catch (e: GhulException.QuickUnlockUnavailable) {
    throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
} catch (e: GhulException) {
    throw VaultException(VaultErrorKind.OTHER, e.message.orEmpty())
}
