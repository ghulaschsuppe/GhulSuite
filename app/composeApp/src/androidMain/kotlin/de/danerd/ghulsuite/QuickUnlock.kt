// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.CancellationSignal
import android.provider.Settings as AndroidSettings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import de.danerd.ghulsuite.core.FactorProof
import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.QUICK_UNLOCK_KEPT_ON
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
 * - app PIN (M19): two single use HMAC keys per generation, usable only while the device is
 *   unlocked; the PIN goes into what they sign, so every attempt needs one of them (see
 *   [PinSlots]); after 2 wrong PINs none is left and password plus second factor are needed.
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
    private val pinSlots = PinSlots(AndroidPinKeyStore(context, keyStore), { suite().pinCore() })

    fun biometricAvailable(): Boolean {
        val bm = context.getSystemService(BiometricManager::class.java) ?: return false
        return bm.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS
    }

    private fun bootCount(): Int =
        AndroidSettings.Global.getInt(context.contentResolver, AndroidSettings.Global.BOOT_COUNT, -1)

    private fun usable(name: String): Boolean {
        val blob = prefs.getString("$name.blob", null) ?: return false
        if (blob.isEmpty() || prefs.getInt("$name.boot", -2) != bootCount()) return false
        // M30: run out; it waits for the next full unlock (which starts its time anew).
        if (prefs.getBoolean("$name.expired", false) || runOutByUptime(name)) return false
        return keyStore.containsAlias(alias(name)) && suite().quickUnlockExists(name)
    }

    /**
     * r9 dieb N1: the validity also by the time since boot, which a clock set back does not change
     * (the core counts by the wall clock). Within one boot only; unknown (older data): no verdict.
     */
    private fun runOutByUptime(name: String): Boolean {
        val since = prefs.getLong("$name.since", -1L).takeIf { it >= 0 && prefs.getInt("$name.boot", -2) == bootCount() } ?: return false
        val hours = (if (name == BIO) settings.quickHoursBio else settings.quickHoursPin).takeIf { it >= 0 } ?: settings.quickUnlockHours
        if (hours <= 0) return false
        val over = android.os.SystemClock.elapsedRealtime() - since > hours * 3_600_000L
        if (over) expired(name)
        return over
    }

    /**
     * The app PIN: its file, its time and at least one slot left in the keystore. A file the core
     * already removed (an old version: set it up again; damaged or of another suite: gone) is
     * handled here; one that cannot be read right now only hides the PIN for now (nothing goes).
     */
    private fun usablePin(): Boolean {
        if (!settings.quickUnlockPin) return false
        if (prefs.getInt("$PIN.boot", -2) != bootCount() || prefs.getBoolean("$PIN.expired", false) || runOutByUptime(PIN)) return false
        return try {
            pinSlots.left() > 0
        } catch (e: VaultException) {
            pinFileGone(e.kind)
            false
        } catch (e: Exception) {
            false
        }
    }

    /** The core removed the PIN's file ([kind]): what this device keeps for it goes the same way. */
    private fun pinFileGone(kind: VaultErrorKind) {
        when (kind) {
            VaultErrorKind.QUICK_OUTDATED -> outdated(PIN)
            VaultErrorKind.QUICK_UNAVAILABLE -> disable(PIN)
            else -> {}
        }
    }

    fun ready(): QuickUnlockReady {
        // M19: a PIN of the version before (its key unwrapped the quick key without the PIN):
        // its file goes, the setting asks to set it up again.
        if (prefs.contains("$PIN.blob")) {
            runCatching { suite().quickUnlockDisable(PIN) }
            outdated(PIN)
        }
        return readyNow()
    }

    private fun readyNow(): QuickUnlockReady = QuickUnlockReady(
        biometric = settings.quickUnlockBiometric && usable(BIO),
        pin = usablePin(),
        needsSetup = (settings.quickUnlockBiometric && prefs.getBoolean("$BIO.outdated", false)) ||
            (settings.quickUnlockPin && prefs.getBoolean("$PIN.outdated", false)),
    )

    /**
     * The quick unlock file was made by an older version (its validity was not in it): the core
     * removed it. The keys of this device for it go too; the setting stays, marked "set it up
     * again" (Options, Unlocking), until it is set up anew or switched off.
     */
    private fun outdated(name: String) {
        runCatching { if (keyStore.containsAlias(alias(name))) keyStore.deleteEntry(alias(name)) }
        if (name == PIN) pinSlots.tidy(null)
        prefs.edit().remove("$name.blob").remove("$name.boot").remove("$name.fails").remove("$name.expired").remove("$name.since")
            .putBoolean("$name.outdated", true).apply()
    }

    /**
     * 0 hours = "always": no expiry, but a reboot, panic lock or password change still ends it.
     * H6: fixed in the quick unlock file when it is set up; changing it means setting it up again.
     */
    private fun maxAgeSeconds(hours: Int): ULong =
        if (hours <= 0) ALWAYS_SECONDS else (hours.coerceIn(1, 24 * 7) * 3600L).toULong()

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
            if (strongBox) setIsStrongBoxBacked(true)
            if (unlockedOnly) setUnlockedDeviceRequired(true)
            if (biometric) {
                setUserAuthenticationRequired(true)
                setInvalidatedByBiometricEnrollment(true)
                setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
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
        // r10 M4: its time starts now (as the core's created_ms), not at the full unlock before.
        prefs.edit().putString("$name.blob", blob).putInt("$name.boot", bootCount()).putInt("$name.fails", 0).remove("$name.outdated")
            .putLong("$name.since", android.os.SystemClock.elapsedRealtime()).apply()
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

    suspend fun enableBiometric(proof: FactorProof, hours: Int, title: String, subtitle: String, cancel: String) {
        val qk = withContext(Dispatchers.IO) { mappedQuick { suite().quickUnlockEnable(proof, null, null, BIO, maxAgeSeconds(hours)) } }
        try {
            val key = createKey(BIO, biometric = true)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key) }
            val authed = authenticate(cipher, title, subtitle, cancel)
            store(BIO, authed, qk)
            settings.quickUnlockHours = hours
            settings.quickHoursBio = hours
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
            withContext(Dispatchers.IO) { mappedQuick { suite().unlockQuick(BIO, qk) } }
        } catch (e: VaultException) {
            when (e.kind) {
                // N1: nothing wrong with the quick unlock (e.g. an earlier opening of a suite
                // file that could not be finished, a full storage): it stays for the next try.
                VaultErrorKind.QUICK_INVALID, in QUICK_UNLOCK_KEPT_ON -> {}
                VaultErrorKind.QUICK_EXPIRED -> expired(BIO)
                VaultErrorKind.QUICK_OUTDATED -> outdated(BIO)
                else -> disable(BIO)
            }
            throw e
        } finally {
            qk.fill(0)
        }
    }

    // --- app PIN -------------------------------------------------------------------------

    suspend fun enablePin(proof: FactorProof, pin: String, hours: Int) {
        try {
            withContext(Dispatchers.IO) {
                // The key of the version before (a PIN file of version 3, deleted by the core).
                runCatching { if (keyStore.containsAlias(alias(PIN))) keyStore.deleteEntry(alias(PIN)) }
                pinSlots.enable(suite().enablePinSlots(proof, pin, maxAgeSeconds(hours)))
            }
            prefs.edit().putInt("$PIN.boot", bootCount()).remove("$PIN.blob").remove("$PIN.fails").remove("$PIN.expired")
                .putLong("$PIN.since", android.os.SystemClock.elapsedRealtime())
                .remove("$PIN.outdated").commit()
            settings.quickUnlockHours = hours
            settings.quickHoursPin = hours
            settings.quickUnlockPin = true
        } catch (e: PinSlots.SealedButNotStored) {
            // r8 M2: the core sealed already, the keys did not get into the keystore: that PIN
            // cannot work. Anything before (a wrong password, a cancelled touch) leaves the PIN
            // there before as it was.
            disable(PIN)
            throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE, e.message.orEmpty())
        }
    }

    /**
     * One attempt with the app PIN (M19, see [PinSlots]): a wrong one uses up a slot, after the
     * second there is none and the PIN is switched off. What says nothing against the PIN (a
     * locked device, a full storage, a keystore that is busy) leaves it as it is.
     */
    suspend fun unlockPin(pin: String) = withContext(Dispatchers.IO) {
        try {
            pinSlots.unlock(pin)
        } catch (e: PinSlots.NoSlots) {
            // Certain: no PIN file (null generation) or no slot of it left.
            disable(PIN)
            throw VaultException(VaultErrorKind.QUICK_INVALID, retries = 0)
        } catch (e: PinKeyStore.DeviceLocked) {
            throw VaultException(VaultErrorKind.DEVICE_LOCKED)
        } catch (e: PinSlots.KeystoreFailed) {
            // Nothing against the PIN: it stays (the next attempt tries again).
            throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE, e.message.orEmpty())
        } catch (e: VaultException) {
            when (e.kind) {
                // Only when it is certain that no attempt is left (null: the keystore could not say).
                VaultErrorKind.QUICK_INVALID -> if (e.retries == 0) disable(PIN)
                VaultErrorKind.QUICK_EXPIRED -> expired(PIN)
                // The core removed the file (old version, or damaged / of another suite).
                VaultErrorKind.QUICK_OUTDATED, VaultErrorKind.QUICK_UNAVAILABLE -> pinFileGone(e.kind)
                // A storage or read error (also N1) and anything else: the PIN stays, no key goes.
                else -> {}
            }
            throw e
        }
    }

    /** After a full unlock: quick unlock is valid again after a reboot (the core restarts the period). */
    fun onFullUnlock() {
        val edit = prefs.edit()
        for (name in listOf(BIO, PIN)) {
            if (prefs.contains("$name.blob") || (name == PIN && prefs.contains("$PIN.boot"))) edit.putInt("$name.boot", bootCount())
            edit.remove("$name.expired")
            edit.putLong("$name.since", android.os.SystemClock.elapsedRealtime())
        }
        edit.apply()
    }

    /**
     * M30: the validity ran out. Not switched off for good: the setting and the keys stay, and the
     * next full unlock makes it valid again (until then the full unlock is shown).
     */
    private fun expired(name: String) {
        prefs.edit().putBoolean("$name.expired", true).apply()
    }

    // --- removal -------------------------------------------------------------------------

    fun disable(name: String) {
        runCatching { suite().quickUnlockDisable(name) }
        runCatching { if (keyStore.containsAlias(alias(name))) keyStore.deleteEntry(alias(name)) }
        if (name == PIN) pinSlots.tidy(null)
        prefs.edit().remove("$name.blob").remove("$name.boot").remove("$name.fails").remove("$name.expired").remove("$name.outdated")
            .remove("$name.since").apply()
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
                    .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
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
                                // Locked out after too many attempts: said as such (not "cancelled").
                                val kind = if (errorCode == BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT ||
                                    errorCode == BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT_PERMANENT
                                ) VaultErrorKind.BIOMETRIC_LOCKOUT else VaultErrorKind.BIOMETRIC_CANCELLED
                                cont.resumeWithException(VaultException(kind, errString?.toString().orEmpty()))
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

    }
}

/**
 * Maps core errors of the quick unlock functions. r13 M1: every other error keeps its own kind
 * (storage, full storage, locked meanwhile), so it does not count as "something is wrong with
 * the quick unlock" (which would switch it off), see QUICK_UNLOCK_KEPT_ON.
 */
private inline fun <T> mappedQuick(block: () -> T): T = mappedQuickErrors(block)
