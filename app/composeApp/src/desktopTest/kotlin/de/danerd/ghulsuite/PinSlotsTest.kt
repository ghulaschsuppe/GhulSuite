// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.FactorProof
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.NewVault
import de.danerd.ghulsuite.core.SetupOptions
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.generateKeyfile
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import java.io.File
import java.nio.file.Files
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * M19: the app PIN with two single use keys, through the real core and a keystore that enforces
 * the single use (as the Android Keystore does with setMaxUsageCount(1)). Runs with the app's own
 * bindings (see desktopAppBindingTest).
 */
class PinSlotsTest {
    /** Each key works once; [locked] refuses every use without using it up; [crash] throws where asked. */
    private class FakeKeyStore : PinKeyStore {
        val keys = linkedMapOf<String, ByteArray>()
        var locked = false
        var failImportOf: String? = null
        var crashAfterUse = false
        var failAllImports = false
        var failAliasesAfterUse = false
        var failAliases = false

        override fun aliases(): List<String> {
            if (failAliases) throw IllegalStateException("keystore not reachable")
            return keys.keys.toList()
        }

        override fun import(alias: String, key: ByteArray) {
            if (alias == failImportOf || failAllImports) throw IllegalStateException("keystore busy")
            keys[alias] = key.copyOf()
        }

        override fun delete(alias: String) {
            keys.remove(alias)
        }

        override fun mac(alias: String): PinMac {
            if (locked) throw PinKeyStore.DeviceLocked()
            val key = keys[alias] ?: throw VaultException(VaultErrorKind.QUICK_UNAVAILABLE)
            val mac = javax.crypto.Mac.getInstance("HmacSHA256").apply { init(javax.crypto.spec.SecretKeySpec(key, "HmacSHA256")) }
            return PinMac { message ->
                // Used up by its one use, whatever comes after.
                keys.remove(alias)
                if (failAliasesAfterUse) failAliases = true
                mac.doFinal(message).also { if (crashAfterUse) throw IllegalStateException("the app ended") }
            }
        }
    }

    /** The core, with a crash right after its unlock (sealed anew, the app ends before tidying). */
    private class CrashingCore(private val real: PinCore) : PinCore by real {
        var crashAfterUnlock = false
        override fun unlock(slot: UInt, tag: ByteArray, message: ByteArray, nextKeys: List<ByteArray>): UInt =
            real.unlock(slot, tag, message, nextKeys).also { if (crashAfterUnlock) throw IllegalStateException("the app ended") }
    }

    private val password = "Gruener Kaktus singt 1987!"
    private val pin = "24681357"
    private val dir: File = Files.createTempDirectory("ghul-pin").toFile()
    private val kf = generateKeyfile()
    private val suite = GhulSuite(localStorage(File(dir, "suite").path), File(dir, "local").path).apply {
        create(SetupOptions(password, kf, "Key", "Datei", "Code", 65536u, 3u, listOf(NewVault(VaultKind.PASS, "Privat"))), null, null, null)
    }
    private val store = FakeKeyStore()
    private val core = CrashingCore(suite.pinCore())
    private fun slots() = PinSlots(store, { core })

    @AfterTest
    fun cleanUp() {
        dir.deleteRecursively()
    }

    private fun setUp() {
        if (!suite.isUnlocked()) suite.unlock(password, null, null, null, kf)
        slots().enable(suite.enablePinSlots(FactorProof(password, null, kf), pin, 3600uL))
        suite.lock()
    }

    private fun wrong(): VaultException = assertFailsWith<VaultException> { slots().unlock("13579246") }.also {
        assertEquals(VaultErrorKind.QUICK_INVALID, it.kind)
    }

    @Test
    fun exactlyTwoWrongAttemptsThenOnlyTheFullUnlock() {
        setUp()
        assertEquals(2, slots().left())
        assertEquals(1, wrong().retries)
        assertEquals(0, wrong().retries)
        assertEquals(0, slots().left())
        // Not even the right PIN opens now.
        assertFailsWith<PinSlots.NoSlots> { slots().unlock(pin) }
        assertFalse(suite.isUnlocked())
        assertTrue(store.keys.isEmpty(), "no key left over: ${store.keys.keys}")
    }

    @Test
    fun aRightPinSealsAnewForTwoAttempts() {
        setUp()
        val first = suite.quickPinGeneration()
        assertEquals(1, wrong().retries)
        slots().unlock(pin)
        assertTrue(suite.isUnlocked())
        assertEquals(2, slots().left(), "two new attempts")
        assertTrue(suite.quickPinGeneration() != first, "the next generation")
        assertEquals(2, store.keys.size, "only the new keys: ${store.keys.keys}")
        suite.lock()
        slots().unlock(pin)
        assertTrue(suite.isUnlocked())
    }

    @Test
    fun aLockedDeviceOrABusyKeystoreUsesNothingUp() {
        setUp()
        store.locked = true
        assertFailsWith<PinKeyStore.DeviceLocked> { slots().unlock(pin) }
        store.locked = false
        store.failImportOf = slots().alias(suite.quickPinGeneration()!! + 1u, 1)
        assertFailsWith<PinSlots.KeystoreFailed> { slots().unlock(pin) }
        store.failImportOf = null
        assertEquals(2, slots().left())
        assertEquals(2, store.keys.size, "nothing left over")
        slots().unlock(pin)
        assertTrue(suite.isUnlocked())
    }

    /** The app ends at each step of an attempt: it costs at most the one attempt, never more, never one back. */
    @Test
    fun aCrashAtEachStepCostsAtMostOneAttempt() {
        setUp()
        // After the new keys were imported, before the key was used: nothing used up.
        val gen = suite.quickPinGeneration()!!
        store.import(slots().alias(gen + 1u, 0), ByteArray(32))
        store.import(slots().alias(gen + 1u, 1), ByteArray(32))
        assertEquals(2, slots().left())
        // After the key was used, before the core saw the tag: that attempt is gone.
        store.crashAfterUse = true
        assertFailsWith<IllegalStateException> { slots().unlock(pin) }
        store.crashAfterUse = false
        assertEquals(1, slots().left())
        // After the core sealed anew and opened, before the keys were tidied: the next generation counts.
        core.crashAfterUnlock = true
        assertFailsWith<IllegalStateException> { slots().unlock(pin) }
        core.crashAfterUnlock = false
        assertTrue(suite.isUnlocked())
        suite.lock()
        assertEquals(2, slots().left(), "the new generation's two")
        slots().unlock(pin)
        assertEquals(2, store.keys.size, "the old keys went: ${store.keys.keys}")
    }

    /** r8 M2: setting the PIN up again with a wrong password changes nothing; the PIN there stays. */
    @Test
    fun aFailedSetupAgainKeepsThePinThere() {
        setUp()
        suite.unlock(password, null, null, null, kf)
        assertFailsWith<Exception> { slots().enable(suite.enablePinSlots(FactorProof("Falsches Passwort 2024!", null, kf), "11223344", 3600uL)) }
            .also { assertFalse(it is PinSlots.SealedButNotStored, "nothing was sealed") }
        suite.lock()
        assertEquals(2, slots().left())
        slots().unlock(pin)
        assertTrue(suite.isUnlocked())
        // Sealed, but the keys did not get into the keystore: said as such (the app switches it off).
        store.failAllImports = true
        assertFailsWith<PinSlots.SealedButNotStored> { slots().enable(suite.enablePinSlots(FactorProof(password, null, kf), "11223344", 3600uL)) }
        store.failAllImports = false
    }

    /** r8 N2: when the keystore cannot say what is left after a wrong PIN, that is no "0 left". */
    @Test
    fun aKeystoreErrorAfterAWrongPinIsNoZeroLeft() {
        setUp()
        store.failAliasesAfterUse = true
        assertEquals(null, wrong().retries)
        store.failAliases = false
        store.failAliasesAfterUse = false
        assertEquals(1, slots().left())
        // left() itself throws instead of saying 0.
        store.failAliases = true
        assertFailsWith<IllegalStateException> { slots().left() }
        store.failAliases = false
    }

    @Test
    fun aRightPinAfterOneWrongStillOpensAndAWrongOneChangesNothingInTheFile() {
        setUp()
        val gen = suite.quickPinGeneration()
        wrong()
        assertEquals(gen, suite.quickPinGeneration())
        slots().unlock(pin)
        assertTrue(suite.isUnlocked())
    }
}
