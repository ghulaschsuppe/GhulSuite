// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.NewVault
import de.danerd.ghulsuite.core.SetupOptions
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.generateKeyfile
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.SingleSaveUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import kotlinx.coroutines.runBlocking
import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/**
 * A vault opened on its own through the app's own backend code (the core as the app uses it):
 * every change goes back into the file, also when locking; when that fails, nothing is lost and
 * the changes are offered again. Only the app's bindings here (see desktopAppBindingTest).
 */
class SingleBackendTest {
    private class Backend(dir: File) : CoreVaultBackend(File(dir, "local").absolutePath) {
        private val root = dir
        @Volatile var failWrites = false
        private val prefs = mutableMapOf<String, String>()
        override fun storage(): VaultStorage = localStorage(File(root, "suite").absolutePath)
        override suspend fun <T> withKey(block: (HidTransport) -> T): T = throw UnsupportedOperationException()
        override fun devicePref(key: String): String? = prefs[key]
        override fun setDevicePref(key: String, value: String) { prefs[key] = value }
        override fun kdfMaxMib(): Int = 64
        override suspend fun readSuiteFile(ref: SuiteFileRef): ByteArray? = File(ref.id).takeIf { it.exists() }?.readBytes()
        override suspend fun writeSuiteFile(ref: SuiteFileRef, data: ByteArray): Boolean {
            if (failWrites) return false
            File(ref.id).writeBytes(data)
            return true
        }
        override val settings = Settings(object : KeyValueStore {
            private val map = mutableMapOf<String, Any>()
            override fun getInt(key: String, default: Int) = map[key] as? Int ?: default
            override fun putInt(key: String, value: Int) { map[key] = value }
            override fun getBoolean(key: String, default: Boolean) = map[key] as? Boolean ?: default
            override fun putBoolean(key: String, value: Boolean) { map[key] = value }
        })
        override val net = FakeNet()
        override val cal = FakeCal()
        override val appVersion = "test"
        override fun copySensitive(label: String, text: String) {}
        override fun printEmergencySheet(code: String, text: EmergencySheetText) {}
        override suspend fun keyInfo() = de.danerd.ghulsuite.model.KeyStatus("none", false, false, 4)
        override suspend fun setKeyPin(newPin: String) {}
        override fun panicLock() = lock()
        override fun biometricAvailable() = false
        override fun quickUnlockReady() = de.danerd.ghulsuite.model.QuickUnlockReady(false, false)
        override suspend fun enableBiometricUnlock() {}
        override suspend fun enablePinUnlock(pin: String) {}
        override fun disableQuickUnlock() {}
        override fun disableQuickUnlockMethod(bio: Boolean) {}
        override suspend fun unlockWithBiometric() {}
        override suspend fun unlockWithPin(pin: String) {}
    }

    /** A vault file (made with the core directly) with the given password and key file. */
    private fun vaultFile(dir: File, kf: ByteArray): ByteArray {
        val s = GhulSuite(localStorage(File(dir, "maker").path), File(dir, "maker-local").path)
        s.create(
            SetupOptions("Master Passwort!", kf, "Key", "Datei", "Code", 65536u, 3u, listOf(NewVault(VaultKind.PASS, "Privat"))),
            null, null, null,
        )
        return s.exportVault(s.listVaults().single().id)
    }

    /** The name of the vault in a file, opened with the core directly. */
    private fun nameIn(dir: File, file: File, kf: ByteArray): String {
        val t = GhulSuite(
            localStorage(Files.createTempDirectory(dir.toPath(), "check").toFile().path),
            Files.createTempDirectory(dir.toPath(), "check-local").toFile().path,
        )
        t.openSingleVault(file.readBytes(), "Master Passwort!", null, null, null, kf)
        return t.listVaults().single().name
    }

    private fun waitUntil(what: String, check: () -> Boolean) {
        val end = System.currentTimeMillis() + 10_000
        while (!check()) {
            if (System.currentTimeMillis() > end) error("not reached: $what")
            Thread.sleep(50)
        }
    }

    @Test
    fun changesGoBackIntoTheFileAlsoWhenLockingAndAreOfferedAgainWhenThatFails() = runBlocking {
        val dir = Files.createTempDirectory("ghul-single-app").toFile()
        try {
            val kf = generateKeyfile()
            val target = File(dir, "Privat.ghlpss").apply { writeBytes(vaultFile(dir, kf)) }
            val ref = SuiteFileRef(target.absolutePath, target.name)
            val b = Backend(dir)

            assertEquals(VaultKindUi.PASS, b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {})
            assertEquals(ref, b.singleFile())
            val id = b.listVaults().single().id
            // Nothing of a suite while a vault is open on its own.
            assertFailsWith<VaultException> { b.createVault(VaultKindUi.PASS, "Zweiter") }
            b.renameVault(id, "Neu")
            assertTrue(b.archiveUnsaved())
            assertEquals(SingleSaveUi.SAVED, b.saveSingle())
            assertTrue(!b.archiveUnsaved())
            assertEquals("Neu", nameIn(dir, target, kf))

            // A change right before locking: written back while closing.
            b.renameVault(id, "Beim Sperren")
            b.lock()
            assertEquals(null, b.singleFile())
            waitUntil("written on locking") { runCatching { nameIn(dir, target, kf) }.getOrNull() == "Beim Sperren" }
            waitUntil("folder removed") { b.pendingSingleWrites().isEmpty() }

            // The storage is gone while locking: the changes stay and are offered again.
            b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            b.failWrites = true
            b.renameVault(b.listVaults().single().id, "Nicht geschrieben")
            b.lock()
            waitUntil("offered again") { b.pendingSingleWrites().size == 1 }
            assertEquals("Beim Sperren", nameIn(dir, target, kf))
            val p = b.pendingSingleWrites().single()
            assertEquals(ref, p.file)
            assertEquals(SingleSaveUi.FAILED, b.retrySingleWrite(p.key), "still not reachable")
            assertEquals(1, b.pendingSingleWrites().size)
            b.failWrites = false
            assertEquals(SingleSaveUi.SAVED, b.retrySingleWrite(p.key))
            assertEquals("Nicht geschrieben", nameIn(dir, target, kf))
            assertTrue(b.pendingSingleWrites().isEmpty())

            // Given up: gone, the file stays as it was.
            b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            b.failWrites = true
            b.renameVault(b.listVaults().single().id, "Verworfen")
            b.lock()
            waitUntil("offered again") { b.pendingSingleWrites().size == 1 }
            b.discardSingleWrite(b.pendingSingleWrites().single().key)
            assertTrue(b.pendingSingleWrites().isEmpty())
            assertEquals("Nicht geschrieben", nameIn(dir, target, kf))
        } finally {
            dir.deleteRecursively()
        }
    }

    /** A file changed elsewhere meanwhile is not overwritten without asking; save-as keeps both. */
    @Test
    fun aFileChangedElsewhereIsNotOverwrittenUnasked() = runBlocking {
        val dir = Files.createTempDirectory("ghul-single-conflict").toFile()
        try {
            val kf = generateKeyfile()
            val original = vaultFile(dir, kf)
            val target = File(dir, "Privat.ghlpss").apply { writeBytes(original) }
            val ref = SuiteFileRef(target.absolutePath, target.name)
            val b = Backend(dir)
            b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            val id = b.listVaults().single().id

            // Someone else writes the file (another device, a synced folder).
            val other = Backend(File(dir, "other").apply { mkdirs() })
            other.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            other.renameVault(other.listVaults().single().id, "Von woanders")
            assertEquals(SingleSaveUi.SAVED, other.saveSingle())
            other.lock()

            b.renameVault(id, "Von hier")
            assertEquals(SingleSaveUi.CHANGED_ELSEWHERE, b.saveSingle())
            assertEquals("Von woanders", nameIn(dir, target, kf), "not overwritten")
            // Saved as a new file: that is where it goes from now on.
            val copy = File(dir, "Kopie.ghlpss")
            val copyRef = SuiteFileRef(copy.absolutePath, copy.name)
            assertTrue(b.saveSingleAs(copyRef))
            assertEquals(copyRef, b.singleFile())
            assertEquals("Von hier", nameIn(dir, copy, kf))
            b.renameVault(id, "Weiter")
            assertEquals(SingleSaveUi.SAVED, b.saveSingle())
            assertEquals("Weiter", nameIn(dir, copy, kf))
            assertEquals("Von woanders", nameIn(dir, target, kf))
            b.lock()
            waitUntil("closed") { b.pendingSingleWrites().isEmpty() }

            // A leftover over a file changed elsewhere: offered, not written unasked.
            b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            b.failWrites = true
            b.renameVault(b.listVaults().single().id, "Liegen geblieben")
            b.lock()
            waitUntil("offered") { b.pendingSingleWrites().size == 1 }
            b.failWrites = false
            target.writeBytes(original)
            val p = b.pendingSingleWrites().single()
            assertTrue(p.changedElsewhere && p.fileIntact)
            assertEquals(SingleSaveUi.CHANGED_ELSEWHERE, b.retrySingleWrite(p.key))
            assertEquals("Privat", nameIn(dir, target, kf))
            // Saved elsewhere instead: done, the file stays.
            val third = File(dir, "Rest.ghlpss")
            assertTrue(b.saveSingleWriteAs(p.key, SuiteFileRef(third.absolutePath, third.name)))
            assertEquals("Liegen geblieben", nameIn(dir, third, kf))
            assertTrue(b.pendingSingleWrites().isEmpty())

            // A file cut short: said, so giving up is not taken lightly.
            b.openSingle(ref, target.readBytes(), "Master Passwort!", null, kf) {}
            b.failWrites = true
            b.renameVault(b.listVaults().single().id, "Abgebrochen")
            b.lock()
            waitUntil("offered") { b.pendingSingleWrites().size == 1 }
            target.writeBytes(original.copyOf(10))
            assertTrue(!b.pendingSingleWrites().single().fileIntact)
            b.failWrites = false
            assertEquals(SingleSaveUi.SAVED, b.retrySingleWrite(b.pendingSingleWrites().single().key, overwrite = true))
            assertEquals("Abgebrochen", nameIn(dir, target, kf))
        } finally {
            dir.deleteRecursively()
        }
    }
}
