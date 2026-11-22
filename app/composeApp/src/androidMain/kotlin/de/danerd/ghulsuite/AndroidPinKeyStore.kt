// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.KeyProtection
import java.security.KeyStore
import javax.crypto.Mac
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.SecretKeySpec

/**
 * The keys of the app PIN in the Android Keystore (M19): HMAC-SHA256, imported (so setting up
 * does not use up the one use), each usable once (`setMaxUsageCount(1)`) and only while the
 * device is unlocked (where it has a screen lock). StrongBox first, else the TEE; both slots of
 * a generation on the same level. After the import it is checked that the key is in secure
 * hardware and has exactly one use left; otherwise it is deleted and the PIN is not set up.
 * There is no way without the usage limit.
 */
class AndroidPinKeyStore(private val context: Context, private val keyStore: KeyStore) : PinKeyStore {
    override fun aliases(): List<String> =
        keyStore.aliases().toList().filter { it.startsWith(PinSlots.PREFIX) }

    override fun import(alias: String, key: ByteArray) {
        if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
        val secure = context.getSystemService(android.app.KeyguardManager::class.java)?.isDeviceSecure == true
        // The other slot of this generation decides the level, if it is there already.
        val sibling = aliases().firstOrNull { it != alias && it.substringBeforeLast('.') == alias.substringBeforeLast('.') }
        // StrongBox missing ("No such security level") fails here and the TEE is tried next; a
        // failed attempt leaves no alias behind (deleted in the catch).
        val levels = when (sibling?.let { levelOf(it) }) {
            KeyProperties.SECURITY_LEVEL_STRONGBOX -> listOf(true)
            null -> listOf(true, false)
            else -> listOf(false)
        }
        var last: Exception? = null
        for (strongBox in levels) {
            try {
                val protection = KeyProtection.Builder(KeyProperties.PURPOSE_SIGN)
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .setMaxUsageCount(1)
                    .apply {
                        if (secure) setUnlockedDeviceRequired(true)
                        if (strongBox) setIsStrongBoxBacked(true)
                    }
                    .build()
                // r8 N4: SecretKeySpec keeps a copy of the key of its own, which cannot be cleared
                // from here (it goes with the garbage collector); the caller clears its array.
                keyStore.setEntry(alias, KeyStore.SecretKeyEntry(SecretKeySpec(key, "HmacSHA256")), protection)
                val info = info(alias) ?: throw IllegalStateException("no key info")
                val level = levelName(info.securityLevel)
                android.util.Log.i("GhulSuite", "app PIN key: level $level (StrongBox tried: $strongBox)")
                if (info.remainingUsageCount != 1) throw IllegalStateException("not a single use key")
                if (!pinKeyLevelAccepted(level, devBuild, emulator)) throw IllegalStateException("not in secure hardware (level $level)")
                return
            } catch (e: Exception) {
                last = e
                runCatching { keyStore.deleteEntry(alias) }
            }
        }
        throw last ?: IllegalStateException("no key")
    }

    /** The development build (the CI's emulator runs use it), see [pinKeyLevelAccepted]. */
    private val devBuild = context.packageName.endsWith(".dev")

    private val emulator: Boolean by lazy {
        looksLikeEmulator(android.os.Build.FINGERPRINT.orEmpty(), android.os.Build.PRODUCT.orEmpty(), property("ro.boot.qemu"), property("ro.kernel.qemu"))
    }

    private fun property(key: String): String = runCatching {
        Class.forName("android.os.SystemProperties").getMethod("get", String::class.java).invoke(null, key) as String
    }.getOrDefault("")

    private fun levelName(level: Int): PinKeyLevel = when (level) {
        KeyProperties.SECURITY_LEVEL_STRONGBOX -> PinKeyLevel.STRONGBOX
        KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> PinKeyLevel.TEE
        KeyProperties.SECURITY_LEVEL_SOFTWARE -> PinKeyLevel.SOFTWARE
        else -> PinKeyLevel.OTHER
    }

    override fun delete(alias: String) {
        if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
    }

    override fun mac(alias: String): PinMac {
        val key = keyStore.getKey(alias, null) as? SecretKey ?: throw PinSlots.KeystoreFailed("the key of the app PIN is gone")
        val mac = Mac.getInstance("HmacSHA256")
        try {
            // Checks the lock state (a locked device throws here) without using the key up.
            mac.init(key)
        } catch (e: KeyPermanentlyInvalidatedException) {
            // Used up already (e.g. the app ended right after it was used).
            runCatching { keyStore.deleteEntry(alias) }
            throw PinSlots.KeystoreFailed("the key of the app PIN is used up", e)
        } catch (e: Exception) {
            val locked = context.getSystemService(android.app.KeyguardManager::class.java)?.isDeviceLocked == true
            if (locked) throw PinKeyStore.DeviceLocked(e)
            throw e
        }
        return PinMac { message -> mac.doFinal(message) }
    }

    private fun info(alias: String): KeyInfo? = runCatching {
        val key = keyStore.getKey(alias, null) as SecretKey
        SecretKeyFactory.getInstance(key.algorithm, "AndroidKeyStore").getKeySpec(key, KeyInfo::class.java) as KeyInfo
    }.getOrNull()

    private fun levelOf(alias: String): Int? = info(alias)?.securityLevel
}
