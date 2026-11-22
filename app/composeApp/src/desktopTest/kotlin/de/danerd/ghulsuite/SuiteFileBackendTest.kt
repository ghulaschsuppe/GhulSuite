// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.NewVault
import de.danerd.ghulsuite.core.SetupOptions
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.generateKeyfile
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.ArchiveRelationUi
import de.danerd.ghulsuite.model.ArchiveTargetUi
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultException
import kotlinx.coroutines.runBlocking
import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The suite file through the app's own backend code, as on Windows: written as a stream next to
 * the target and moved over it only after reading it back, opened as a stream, with progress.
 */
class SuiteFileBackendTest {
    private class Backend(dir: File) : CoreVaultBackend(File(dir, "local").absolutePath) {
        private val root = dir
        private val prefs = mutableMapOf<String, String>()
        override fun storage(): VaultStorage = localStorage(File(root, "suite").absolutePath)
        override suspend fun <T> withKey(block: (HidTransport) -> T): T = throw UnsupportedOperationException()
        override fun devicePref(key: String): String? = prefs[key]
        override fun setDevicePref(key: String, value: String) { prefs[key] = value }
        override fun kdfMaxMib(): Int = 64
        override fun openSuiteInput(ref: SuiteFileRef): SuiteInput? = SuiteFiles.input(File(ref.id))
        override fun writeSuiteStreamed(
            ref: SuiteFileRef,
            created: Boolean,
            write: (java.io.OutputStream) -> Unit,
            check: (SuiteInput) -> Boolean,
            wipeOld: Boolean,
            onWipe: () -> Unit,
        ): Boolean = SuiteFiles.writeReplacing(File(ref.id), write, check, wipeOld, onWipe)
        /** The room the place has (H12), as the platforms say it; null: not known. */
        var free: Long? = null
        override fun freeSpaceAt(ref: SuiteFileRef): Long? = free
        override val settings = Settings(object : KeyValueStore {
            private val map = mutableMapOf<String, Any>()
            override fun getInt(key: String, default: Int) = map[key] as? Int ?: default
            override fun putInt(key: String, value: Int) { map[key] = value }
            override fun getBoolean(key: String, default: Boolean) = map[key] as? Boolean ?: default
            override fun putBoolean(key: String, value: Boolean) { map[key] = value }
        })
        override val cal = FakeCal()
        override val appVersion = "test"
        override fun copySensitive(label: String, text: String) {}
        override fun printEmergencySheet(code: String, text: EmergencySheetText) {}
        override suspend fun keyInfo() = de.danerd.ghulsuite.model.KeyStatus("none", false, false, 4)
        override suspend fun setKeyPin(newPin: String) {}
        override fun panicLock() = lock()
        override fun biometricAvailable() = false
        override fun quickUnlockReady() = de.danerd.ghulsuite.model.QuickUnlockReady(false, false)
        override suspend fun enableBiometricUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, hours: Int, onTouchNeeded: () -> Unit) {}
        override suspend fun enablePinUnlock(proof: de.danerd.ghulsuite.model.FactorProofUi, pin: String, hours: Int, onTouchNeeded: () -> Unit) {}
        override fun disableQuickUnlock() {}
        override fun disableQuickUnlockMethod(bio: Boolean) {}
        override suspend fun unlockWithBiometric() {}
        override suspend fun unlockWithPin(pin: String) {}
    }

    private val password = "Gruener Kaktus singt 1987!"

    /** A device with a suite made by the core directly, unlocked through the backend. */
    private fun device(dir: File, kf: ByteArray): Backend {
        GhulSuite(localStorage(File(dir, "suite").path), File(dir, "local").path).create(
            SetupOptions(password, kf, "Key", "Datei", "Code", 65536u, 3u, listOf(NewVault(VaultKind.PASS, "Privat"))),
            null, null, null,
        )
        return Backend(dir).also { runBlocking { it.unlock(password, null, kf) {} } }
    }

    private class Steps {
        val list = java.util.Collections.synchronizedList(mutableListOf<Pair<Long, Long>>())
        val report: (Long, Long) -> Unit = { d, t -> list += d to t }
    }

    @Test
    fun savedAsAStreamAndOpenedOnAnotherDevice() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val stick = File(dir, "stick").apply { mkdirs() }
            val target = File(stick, "GhulSuite.ghlzip")
            val ref = SuiteFileRef(target.absolutePath, target.name)
            // No file there yet: nothing to read (a new place).
            assertNull(a.archiveTarget(ref))
            File(stick, "leer.ghlzip").writeBytes(ByteArray(0))
            assertEquals(ArchiveTargetUi.NEW, a.archiveTarget(SuiteFileRef(File(stick, "leer.ghlzip").absolutePath, "leer.ghlzip")))
            assertTrue(a.archiveUnsaved())
            val saving = Steps()
            assertTrue(a.archiveSave(ref, "Laptop", created = true, onProgress = saving.report))
            assertTrue(target.isFile)
            assertFalse(File(stick, "GhulSuite.ghlzip.ghltmp").exists(), "no temporary file left")
            assertFalse(a.archiveUnsaved())
            assertEquals(ArchiveTargetUi.SAME, a.archiveTarget(ref))
            // Writing and reading back: one bar up to the end.
            val (done, total) = saving.list.last()
            assertTrue(total > 0 && done == total, saving.list.toString())
            assertTrue(saving.list.zipWithNext().all { (x, y) -> x.first <= y.first }, saving.list.toString())

            // Saved again over it: the old file is replaced in one step.
            a.renameVault(a.listVaults().single().id, "Neu")
            assertTrue(a.archiveUnsaved())
            assertTrue(a.archiveSave(ref, "Laptop", created = false))
            assertFalse(a.archiveUnsaved())

            val b = Backend(File(dir, "b"))
            assertEquals(1, b.archiveProtection(ref).keyfiles)
            val opening = Steps()
            val check = b.archiveOpen(ref, password, null, kf, {}, opening.report)
            assertEquals(ArchiveRelationUi.NO_SUITE, check.relation)
            assertEquals(target.length() to target.length(), opening.list.last())
            b.archiveAccept()
            assertEquals(listOf("Neu"), b.listVaults().map { it.name })
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * H5: a file of another suite replaces the one on this device only with that one's proof
     * (both factors or its recovery code); a wrong proof leaves it as it is.
     */
    @Test
    fun anotherSuiteReplacesThisOneOnlyWithItsProof() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile-other").toFile()
        try {
            val kfA = generateKeyfile()
            val a = device(File(dir, "a"), kfA)
            val target = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val ref = SuiteFileRef(target.absolutePath, target.name)
            assertTrue(a.archiveSave(ref, "Laptop", created = true))

            val kfB = generateKeyfile()
            val b = device(File(dir, "b"), kfB)
            b.renameVault(b.listVaults().single().id, "Bs Tresor")
            assertEquals(ArchiveRelationUi.OTHER_SUITE, b.archiveOpen(ref, password, null, kfA, {}).relation)
            assertFailsWith<de.danerd.ghulsuite.model.VaultException> { b.archiveAccept() }
            // The key file of the file's suite is no proof of the one here.
            assertFailsWith<de.danerd.ghulsuite.model.VaultException> {
                b.archiveAcceptReplacing(de.danerd.ghulsuite.model.FactorProofUi(password, null, kfA), null)
            }
            assertEquals(listOf("Bs Tresor"), b.listVaults().map { it.name }, "nothing replaced")
            b.archiveAcceptReplacing(de.danerd.ghulsuite.model.FactorProofUi(password, null, kfB), null)
            assertEquals(listOf("Privat"), b.listVaults().map { it.name })
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * H12: a place with too little room for the suite file is refused before anything is
     * written, with what it needs and has; with room enough it is saved.
     */
    @Test
    fun tooLittleRoomIsSaidBeforeWriting() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile-room").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val target = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val ref = SuiteFileRef(target.absolutePath, target.name)
            a.free = 1000
            val e = assertFailsWith<de.danerd.ghulsuite.model.VaultException> { a.archiveSave(ref, "Laptop", created = false) }
            assertEquals(de.danerd.ghulsuite.model.VaultErrorKind.NO_SPACE, e.kind)
            val (free, need) = e.detail.split('|').map { it.toLong() }
            assertEquals(1000L, free)
            assertTrue(need > 1000, e.detail)
            assertFalse(target.exists(), "nothing written")
            a.free = need
            assertTrue(a.archiveSave(ref, "Laptop", created = false))
            assertTrue(target.length() <= need, "the estimate is not too small")
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * N2/N3: after a change of the protection the file it replaced is overwritten at the place of
     * the suite file only: a save to another place leaves the mark, the save there wipes (said in
     * the progress) and clears it. Opening a file with the recovery code marks that file.
     */
    @Test
    fun theOldFileIsWipedAtItsOwnPlaceOnly() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile-wipe").toFile()
        try {
            val kf = generateKeyfile()
            val code = GhulSuite(localStorage(File(dir, "a/suite").path), File(dir, "a/local").path).create(
                SetupOptions(password, kf, "Key", "Datei", "Code", 65536u, 3u, listOf(NewVault(VaultKind.PASS, "Privat"))),
                null, null, null,
            ).recoveryCode
            val a = Backend(File(dir, "a")).also { it.unlock(password, null, kf) {} }
            val stick = File(dir, "stick").apply { mkdirs() }
            fun ref(name: String) = File(stick, name).let { SuiteFileRef(it.absolutePath, it.name) }
            val main = ref("GhulSuite.ghlzip")
            val other = ref("Andere.ghlzip")
            fun wiped(steps: Steps) = steps.list.any { it.second == de.danerd.ghulsuite.model.WIPING_OLD_FILE }
            assertTrue(a.archiveSave(main, "Laptop", created = true))

            a.changePassword(de.danerd.ghulsuite.model.FactorProofUi(password, null, kf), "$password neu") {}
            val elsewhere = Steps()
            assertTrue(a.archiveSave(other, "Laptop", created = true, onProgress = elsewhere.report))
            assertFalse(wiped(elsewhere), "nothing was replaced there")
            // The app took the other place as the suite file's; the old one stays marked.
            a.setSuiteFile(main)
            val there = Steps()
            assertTrue(a.archiveSave(main, "Laptop", created = false, onProgress = there.report))
            assertTrue(wiped(there), "the old file at its place is overwritten")
            assertEquals(listOf("Andere.ghlzip", "GhulSuite.ghlzip"), stick.list()!!.sorted(), "no .ghlold left")
            val again = Steps()
            assertTrue(a.archiveSave(main, "Laptop", created = false, onProgress = again.report))
            assertFalse(wiped(again), "only once")

            // Another device opens the file of the old protection with the code.
            val old = ref("Alt.ghlzip")
            File(stick, "GhulSuite.ghlzip").copyTo(File(stick, "Alt.ghlzip"))
            val b = Backend(File(dir, "b"))
            b.archiveOpenWithCode(old, code!!, "Neues Passwort fuer B 2024!") { _, _ -> }
            b.archiveAccept()
            b.setSuiteFile(old)
            val opened = Steps()
            assertTrue(b.archiveSave(old, "PC", created = false, onProgress = opened.report))
            assertTrue(wiped(opened), "the file opened with the code is overwritten at the next save")
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * r13 M1: the core's other errors keep their kind through the quick unlock's mapping (they
     * used to become OTHER, which switched the fingerprint off after a full storage).
     */
    @Test
    fun quickUnlockErrorsKeepTheirKind() {
        for ((core, kind) in listOf(
            de.danerd.ghulsuite.core.GhulException.Storage("disk") to de.danerd.ghulsuite.model.VaultErrorKind.STORAGE,
            de.danerd.ghulsuite.core.GhulException.Locked() to de.danerd.ghulsuite.model.VaultErrorKind.LOCKED,
            de.danerd.ghulsuite.core.GhulException.QuickUnlockInvalid() to de.danerd.ghulsuite.model.VaultErrorKind.QUICK_INVALID,
        )) {
            val e = assertFailsWith<de.danerd.ghulsuite.model.VaultException> { mappedQuickErrors<Unit> { throw core } }
            assertEquals(kind, e.kind, core.toString())
            if (kind != de.danerd.ghulsuite.model.VaultErrorKind.QUICK_INVALID) assertTrue(kind in de.danerd.ghulsuite.model.QUICK_UNLOCK_KEPT_ON)
        }
    }

    /** The core's error for an older quick unlock file becomes its own kind (its own message). */
    @Test
    fun anOutdatedQuickUnlockHasItsOwnKind() {
        assertEquals(de.danerd.ghulsuite.model.VaultErrorKind.QUICK_OUTDATED, de.danerd.ghulsuite.core.GhulException.QuickUnlockOutdated().toVault().kind)
        assertEquals(de.danerd.ghulsuite.model.VaultErrorKind.NO_SPACE, de.danerd.ghulsuite.core.GhulException.NoSpace(10uL, 3uL).toVault().kind)
        assertEquals("3|10", de.danerd.ghulsuite.core.GhulException.NoSpace(10uL, 3uL).toVault().detail)
    }

    @Test
    fun aFileThatCannotBeWrittenOrReadIsNoSavedFile() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile-fail").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val gone = SuiteFileRef(File(dir, "kein/Ordner/GhulSuite.ghlzip").absolutePath, "GhulSuite.ghlzip")
            assertNull(a.archiveTarget(gone), "not readable")
            assertTrue(runCatching { a.archiveSave(gone, "Laptop", created = false) }.getOrDefault(false).not())
            assertTrue(a.archiveUnsaved(), "still unsaved")
            assertFailsWith<VaultException> { a.archiveProtection(gone) }

            // A damaged file: refused, nothing replaced.
            val target = File(dir, "GhulSuite.ghlzip")
            val ref = SuiteFileRef(target.absolutePath, target.name)
            assertTrue(a.archiveSave(ref, "Laptop", created = false))
            val bytes = target.readBytes()
            bytes[bytes.size - 300] = (bytes[bytes.size - 300].toInt() xor 1).toByte()
            target.writeBytes(bytes)
            val b = Backend(File(dir, "b"))
            assertFailsWith<VaultException> { b.archiveOpen(ref, password, null, kf, {}) }
            assertFalse(b.exists())
            // Cut off (a pulled stick).
            target.writeBytes(bytes.copyOf(bytes.size / 2))
            assertFailsWith<VaultException> { b.archiveOpen(ref, password, null, kf, {}) }
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * "Save the existing suite and replace it" through the app's backend: saved and read back,
     * then deleted on this device (nothing of it left: vault files, documents, quick unlock, the
     * remembered suite file, settings naming its entries), then a new suite set up in its place.
     * The saved file opens elsewhere with the old data. A save that fails deletes nothing.
     */
    @Test
    fun theExistingSuiteIsSavedThenDeletedAndReplaced() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-suitefile-replace").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            a.settings.markRecent("p1", "e1")
            a.settings.calHidden = "c1"
            val gone = SuiteFileRef(File(dir, "kein/Ordner/GhulSuite.ghlzip").absolutePath, "GhulSuite.ghlzip")
            assertFalse(runCatching { a.archiveSave(gone, "Laptop", created = true) }.getOrDefault(false))
            assertTrue(a.exists(), "a failed save deletes nothing")

            val target = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val ref = SuiteFileRef(target.absolutePath, target.name)
            assertTrue(a.archiveSave(ref, "Laptop", created = true))
            a.setSuiteFile(ref)
            // H5: a wrong proof deletes nothing; the right one (password and key file) does.
            assertFailsWith<de.danerd.ghulsuite.model.VaultException> {
                a.deleteSuite(de.danerd.ghulsuite.model.FactorProofUi("falsch falsch falsch", null, kf), null)
            }
            assertTrue(a.exists(), "a wrong proof deletes nothing")
            assertNotNull(a.suiteFile(), "nor the device's data")
            a.deleteSuite(de.danerd.ghulsuite.model.FactorProofUi(password, null, kf), null)
            assertFalse(a.exists())
            assertFalse(a.unlocked.value)
            assertEquals(emptyList(), File(dir, "a/suite").listFiles().orEmpty().map { it.name })
            assertTrue(File(dir, "a/local").listFiles().orEmpty().none { it.name.startsWith("quick-") || it.name == "archive-unsaved" })
            assertNull(a.suiteFile())
            assertEquals("", a.settings.recentEntries)
            assertEquals("", a.settings.calHidden)
            assertTrue(target.isFile, "the saved file is not touched")

            val code = a.create(
                de.danerd.ghulsuite.model.SetupUi(
                    password = "Ein neuer Morgen 4711 kommt!", keyPin = null, keyLabel = "Key", keyfile = generateKeyfile(),
                    keyfileLabel = "Datei", recoveryLabel = "Code", kdf = de.danerd.ghulsuite.model.KdfChoice(65536, 3, 0), vaults = emptyList(),
                ),
            ) {}
            assertTrue(code.isNotBlank())
            assertTrue(a.exists())
            assertEquals(emptyList(), a.listVaults().map { it.name })

            val b = Backend(File(dir, "b"))
            b.archiveOpen(ref, password, null, kf, {})
            b.archiveAccept()
            assertEquals(listOf("Privat"), b.listVaults().map { it.name })
        } finally {
            dir.deleteRecursively()
        }
    }
}
