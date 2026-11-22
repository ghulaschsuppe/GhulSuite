// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.FactorProof
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException

/**
 * The keys of the app PIN in a keystore (Android Keystore; a fake in the desktop tests). Each key
 * is an HMAC-SHA256 key that works exactly once (M19).
 */
interface PinKeyStore {
    /** The aliases of all PIN keys there are (those starting with [PinSlots.PREFIX]). */
    fun aliases(): List<String>

    /**
     * Imports [key] under [alias] as a single use HMAC-SHA256 key that works only while the device
     * is unlocked. Throws when the keystore cannot hold it that way (nothing is left then).
     */
    fun import(alias: String, key: ByteArray)

    fun delete(alias: String)

    /**
     * Prepares the key [alias] for its one use. Throws [DeviceLocked] when the device is locked:
     * then nothing is used up.
     */
    fun mac(alias: String): PinMac

    /** The device is locked: the keys cannot be used now (none of them is used up). */
    class DeviceLocked(cause: Throwable? = null) : Exception("the device is locked", cause)
}

/** One prepared use of a PIN key: [doFinal] uses it up. */
fun interface PinMac {
    fun doFinal(message: ByteArray): ByteArray
}

/** The core's part of the app PIN (see GhulSuite: quickPinGeneration and the rest). */
interface PinCore {
    fun generation(): UInt?
    fun challenge(pin: String): Pair<UInt, ByteArray>
    fun unlock(slot: UInt, tag: ByteArray, message: ByteArray, nextKeys: List<ByteArray>): UInt
}

/** The core's functions as [PinCore] (errors mapped as for the other quick unlock calls). */
fun GhulSuite.pinCore(): PinCore = object : PinCore {
    // A file that cannot be read is an error (not "no PIN"), so the PIN is not switched off for it.
    override fun generation(): UInt? = mappedQuickErrors { quickPinGeneration() }
    override fun challenge(pin: String): Pair<UInt, ByteArray> = mappedQuickErrors { quickPinChallenge(pin).let { it.generation to it.message } }
    override fun unlock(slot: UInt, tag: ByteArray, message: ByteArray, nextKeys: List<ByteArray>): UInt =
        mappedQuickErrors { unlockQuickPin(slot, tag, message, nextKeys) }
}

/** Sets up the app PIN in the core with two new slot keys, see [PinSlots.enable]. */
fun GhulSuite.enablePinSlots(proof: FactorProof, pin: String, maxAgeSeconds: ULong): (List<ByteArray>) -> UInt =
    { keys -> mappedQuickErrors { quickUnlockEnablePin(proof, null, null, pin, maxAgeSeconds, keys) } }

/** Maps the core's quick unlock errors (the same kinds as on Android). */
internal inline fun <T> mappedQuickErrors(block: () -> T): T = try {
    block()
} catch (e: de.danerd.ghulsuite.core.GhulException.QuickUnlockInvalid) {
    throw VaultException(VaultErrorKind.QUICK_INVALID)
} catch (e: de.danerd.ghulsuite.core.GhulException.QuickUnlockExpired) {
    throw VaultException(VaultErrorKind.QUICK_EXPIRED)
} catch (e: de.danerd.ghulsuite.core.GhulException.QuickUnlockOutdated) {
    throw VaultException(VaultErrorKind.QUICK_OUTDATED)
} catch (e: de.danerd.ghulsuite.core.GhulException.QuickUnlockUnavailable) {
    throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
} catch (e: de.danerd.ghulsuite.core.GhulException) {
    throw e.toVault()
}

/**
 * M19: the app PIN with two single use keys ("slots") per generation. Every attempt uses up one
 * slot in the keystore, right or wrong, and the PIN goes into what the slot signs: two wrong PINs
 * and there is no slot left (then only password and second factor open the suite). A right PIN
 * seals the file anew for two new slots (the next generation) in the same core call.
 *
 * The core's file says the generation; the aliases follow from it. Whatever is not of that
 * generation is left over from an attempt cut off (a crash, the app ended) and is deleted before
 * the next attempt. So a cut off attempt costs at most the one slot it used, never more, and
 * never gives one back. Attempts left = the slots of the generation there are.
 */
class PinSlots(
    private val store: PinKeyStore,
    private val core: () -> PinCore,
    private val random: (Int) -> ByteArray = { n -> ByteArray(n).also { java.security.SecureRandom().nextBytes(it) } },
) {
    /** Thrown by [unlock] when no slot is left (or none was ever set up): full unlock. */
    class NoSlots : Exception("no slot of the app PIN is left")

    /**
     * The keystore failed before a slot was used (busy, the file changed meanwhile, a key used up
     * already): nothing against the PIN itself, it stays (unlike the core's QUICK_UNAVAILABLE,
     * which means its file is gone).
     */
    class KeystoreFailed(message: String, cause: Throwable? = null) : Exception(message, cause)

    fun alias(generation: UInt, slot: Int) = "$PREFIX$generation.$slot"

    private fun generationOf(alias: String): UInt? = alias.removePrefix(PREFIX).substringBefore('.').toUIntOrNull()

    /** r8 N9: one attempt or setup at a time (two at once would replace each other's next keys). */
    private val lock = Any()

    /** Deletes every PIN key not of [generation] (all of them for null). Best effort. */
    fun tidy(generation: UInt?) = synchronized(lock) {
        for (a in runCatching { store.aliases() }.getOrDefault(emptyList())) {
            if (generation == null || generationOf(a) != generation) runCatching { store.delete(a) }
        }
    }

    /** [tidy] for the generation the file has now (nothing when the core cannot say). */
    private fun tidyAsTheFileSays(c: PinCore) {
        val now = runCatching { c.generation() }
        if (now.isSuccess) tidy(now.getOrNull())
    }

    /**
     * The attempts left: the slots of the file's generation there are; 0 only when that is
     * certain (no file, or no slot of it). r8 N2: an error of the core or the keystore is thrown,
     * not taken for 0 (which would switch the PIN off).
     */
    fun left(): Int = synchronized(lock) {
        val gen = core().generation() ?: return 0
        val there = store.aliases().toSet()
        (0 until SLOTS).count { alias(gen, it) in there }
    }

    /**
     * Thrown by [enable] when the core sealed the file already but its keys could not go into the
     * keystore: the PIN there is unusable (the caller switches it off). Anything before (a wrong
     * password, a cancelled touch of the key) changed nothing: the PIN there before stays (r8 M2).
     */
    class SealedButNotStored(cause: Throwable) : Exception(cause.message, cause)

    /**
     * Sets up the app PIN: two new keys, [seal] has the core write the file for them (returns
     * its generation), then the keys go into the keystore as single use keys.
     */
    fun enable(seal: (List<ByteArray>) -> UInt): Unit = synchronized(lock) {
        val keys = List(SLOTS) { random(KEY_BYTES) }
        try {
            // A failure here changed nothing (the file and the keys of the PIN before stay).
            val gen = seal(keys)
            try {
                for (i in 0 until SLOTS) store.import(alias(gen, i), keys[i])
            } catch (e: Throwable) {
                for (i in 0 until SLOTS) runCatching { store.delete(alias(gen, i)) }
                throw SealedButNotStored(e)
            }
            tidy(gen)
        } finally {
            keys.forEach { it.fill(0) }
        }
    }

    /**
     * One attempt with [pin]. Wrong: [VaultException] QUICK_INVALID with the attempts left (0:
     * the PIN is used up; null: the keystore could not say, the PIN stays). A locked device
     * ([PinKeyStore.DeviceLocked]), an expired or outdated PIN and a failure before the slot is
     * used cost nothing.
     */
    fun unlock(pin: String): Unit = synchronized(lock) {
        val c = core()
        val gen = c.generation() ?: throw NoSlots()
        tidy(gen)
        val there = store.aliases().toSet()
        val slot = (0 until SLOTS).firstOrNull { alias(gen, it) in there } ?: throw NoSlots()
        // The core wraps from the largest generation to 1 (never 0).
        val next = if (gen == UInt.MAX_VALUE) 1u else gen + 1u
        val nextKeys = List(SLOTS) { random(KEY_BYTES) }
        // r8 N4: what stands for the PIN (Argon2id of it) and the tag are cleared after use. The
        // core and the keystore get copies of their own (UniFFI, SecretKeySpec), which this code
        // cannot clear; they go with the garbage collector.
        var message: ByteArray? = null
        var tag: ByteArray? = null
        fun dropNext() {
            for (i in 0 until SLOTS) runCatching { store.delete(alias(next, i)) }
        }
        try {
            try {
                for (i in 0 until SLOTS) store.import(alias(next, i), nextKeys[i])
            } catch (e: Throwable) {
                // No slot used: the PIN stays as it is.
                dropNext()
                throw KeystoreFailed(e.message.orEmpty(), e)
            }
            val (challengeGen, m) = try {
                c.challenge(pin)
            } catch (e: Throwable) {
                dropNext()
                throw e
            }
            message = m
            if (challengeGen != gen) {
                dropNext()
                throw KeystoreFailed("the app PIN changed meanwhile")
            }
            // Prepared first: a locked device throws here, before the slot is used up.
            val mac = try {
                store.mac(alias(gen, slot))
            } catch (e: Throwable) {
                dropNext()
                throw e
            }
            val t = mac.doFinal(m)
            tag = t
            val now = try {
                c.unlock(slot.toUInt(), t, m, nextKeys)
            } catch (e: VaultException) {
                if (e.kind == VaultErrorKind.QUICK_INVALID) {
                    // Nothing changed in the core; the used slot is gone (the keystore deletes a
                    // used up key; here as well).
                    dropNext()
                    runCatching { store.delete(alias(gen, slot)) }
                    throw VaultException(VaultErrorKind.QUICK_INVALID, e.detail, retries = runCatching { left() }.getOrNull())
                }
                // Perhaps sealed anew and the unlock after it failed (a full storage): the file
                // says which generation counts (the keys of the other one go).
                tidyAsTheFileSays(c)
                throw e
            } catch (e: Throwable) {
                tidyAsTheFileSays(c)
                throw e
            }
            if (now == next) {
                tidy(next)
            } else {
                // Not sealed anew (the suite is open all the same): the slot left stays.
                dropNext()
                runCatching { store.delete(alias(gen, slot)) }
            }
        } finally {
            nextKeys.forEach { it.fill(0) }
            message?.fill(0)
            tag?.fill(0)
        }
    }

    companion object {
        const val PREFIX = "ghulsuite.quick.pin."
        const val SLOTS = 2
        const val KEY_BYTES = 32
    }
}
