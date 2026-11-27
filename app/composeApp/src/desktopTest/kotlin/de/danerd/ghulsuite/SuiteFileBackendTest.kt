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
import de.danerd.ghulsuite.model.VaultKindUi
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
    private class Backend(dir: File, realCal: Boolean = false) : CoreVaultBackend(File(dir, "local").absolutePath) {
        private val root = dir
        val prefs = mutableMapOf<String, String>()
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
        override fun deleteSuiteDocument(ref: SuiteFileRef, wipe: Boolean): SuiteDocDelete {
            deleteCalls += ref.id
            if (wipe) wiped += ref.id
            return SuiteFiles.delete(File(ref.id), wipe)
        }
        val wiped = mutableListOf<String>()
        val deleteCalls = mutableListOf<String>()
        val released = mutableListOf<String>()
        override fun releaseOldPlace(old: SuiteFileRef) { released += old.id }
        override fun samePlace(a: SuiteFileRef, b: SuiteFileRef): Boolean = SuiteFiles.sameFile(File(a.id), File(b.id))
        override fun placeReachable(ref: SuiteFileRef): Boolean = File(ref.id).absoluteFile.parentFile?.isDirectory == true
        override fun presence(ref: SuiteFileRef): Presence = Presence.valueOf(SuiteFiles.presence(File(ref.id)))
        /** Runs between the check of the old file and its delete (a test changes the file there). */
        var beforeDelete: () -> Unit = {}
        override fun beforeOldFileDelete() = beforeDelete()
        override fun leftoversOf(ref: SuiteFileRef): List<SuiteFileRef> = SuiteFiles.leftovers(File(ref.id))
        override fun whereIs(ref: SuiteFileRef): String = File(ref.id).absolutePath
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
        override val cal: de.danerd.ghulsuite.model.CalBackend = if (realCal) CoreCalBackend { suite } else FakeCal()
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

    /**
     * Device test: after the suite moved, "Delete old file" deletes the old one and what a cut off
     * save of this suite left next to it; a file of another suite there stays; the current place
     * is never deleted.
     */
    @Test
    fun theOldFileIsDeletedButNeverTheCurrentOne() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-oldfile").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val stick = File(dir, "stick").apply { mkdirs() }
            val old = File(stick, "GhulSuite.ghlzip")
            val oldRef = SuiteFileRef(old.absolutePath, old.name)
            assertTrue(a.archiveSave(oldRef, "Laptop", created = true))
            // What a cut off save of this suite left beside it, and a file of another suite.
            old.copyTo(File(stick, "GhulSuite.ghlzip.ghlold"))
            val b = device(File(dir, "b"), generateKeyfile())
            val foreign = File(stick, "GhulSuite.ghlzip.ghltmp")
            assertTrue(b.archiveSave(SuiteFileRef(foreign.absolutePath, foreign.name), "PC", created = true))
            val now = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val nowRef = SuiteFileRef(now.absolutePath, now.name)
            assertTrue(a.archiveSave(nowRef, "Laptop", created = true))
            assertEquals(nowRef, a.suiteFile())
            // The current place: never.
            val refused = a.deleteOldSuiteFile(nowRef)
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.SAME_FILE, refused.outcome)
            assertTrue(now.isFile)
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.SAME, a.oldFileCheck(oldRef))
            val r = a.deleteOldSuiteFile(oldRef)
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DELETED, r.outcome, r.toString())
            assertEquals(old.absolutePath, r.where)
            assertFalse(old.exists())
            assertFalse(File(stick, "GhulSuite.ghlzip.ghlold").exists(), "the leftover of this suite went too")
            assertTrue(foreign.exists(), "another suite's file stays")
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Review H1: the current suite file is never deleted, whatever its address looks like: a path
     * with another case, a path with "..", a link to it, or a copy with the same content.
     */
    @Test
    fun theCurrentFileIsKnownByItsIdentityNotByItsText() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-samefile").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val stick = File(dir, "stick").apply { mkdirs() }
            val now = File(stick, "GhulSuite.ghlzip")
            assertTrue(a.archiveSave(SuiteFileRef(now.absolutePath, now.name), "Laptop", created = true))
            val link = File(dir, "link.ghlzip").also { Files.createSymbolicLink(it.toPath(), now.toPath()) }
            val copy = File(dir, "copy/GhulSuite.ghlzip").also { it.parentFile.mkdirs(); now.copyTo(it) }
            for (other in listOf(
                File(stick, "ghulsuite.GHLZIP").absolutePath,
                File(dir, "stick/../stick/GhulSuite.ghlzip").path,
                link.absolutePath,
                copy.absolutePath,
            )) {
                val r = a.deleteOldSuiteFile(SuiteFileRef(other, File(other).name))
                assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.SAME_FILE, r.outcome, other)
                assertTrue(now.isFile, "still there after $other")
            }
            assertTrue(copy.isFile)
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Review H2, M-a, M-b: a file with a state this device lacks is kept; a file of the protection
     * before is written over with zeros before it goes; a place not there is "not reachable".
     */
    @Test
    fun onlyAFileOfTheOwnLineIsDeletedAndWipedWhereMarked() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-oldline").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val stick = File(dir, "stick").apply { mkdirs() }
            val shared = File(stick, "GhulSuite.ghlzip")
            val sharedRef = SuiteFileRef(shared.absolutePath, shared.name)
            assertTrue(a.archiveSave(sharedRef, "Laptop", created = true))
            // Device B takes the file, changes it and saves it there: A lacks that state.
            val b = Backend(File(dir, "b"))
            b.archiveOpen(sharedRef, password, null, kf, {})
            b.archiveAccept()
            b.renameVault(b.listVaults().single().id, "Von B")
            assertTrue(b.archiveSave(sharedRef, "PC", created = false))
            val mine = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(mine.absolutePath, mine.name), "Laptop", created = true))
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.OTHER, a.oldFileCheck(sharedRef), "B's state: no delete offered")
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DIFFERENT_STATE, a.deleteOldSuiteFile(sharedRef).outcome)
            assertTrue(shared.isFile, "B's changes stay")
            // A stick that is not there: not reachable (no "deleted", no "other state").
            val gone = SuiteFileRef(File(dir, "weg/GhulSuite.ghlzip").absolutePath, "GhulSuite.ghlzip")
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.UNREACHABLE, a.oldFileCheck(gone))
            // Its folder there, the file not: "gone".
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.GONE, a.oldFileCheck(SuiteFileRef(File(stick, "nicht.ghlzip").absolutePath, "nicht.ghlzip")))
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.UNREACHABLE, a.deleteOldSuiteFile(gone).outcome)
            // Marked to be written over (a change of the protection, here the mark of an older
            // version for any place): zeros first, then deleted, and the mark is gone.
            val third = File(dir, "drei/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(third.absolutePath, third.name), "Laptop", created = true))
            val before = SuiteFileRef(mine.absolutePath, mine.name)
            a.prefs["wipe_old_file"] = "1"
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DELETED, a.deleteOldSuiteFile(before).outcome)
            assertEquals(listOf(mine.absolutePath), a.wiped)
            assertFalse(mine.exists())
            assertEquals("", a.prefs["wipe_old_file"], "the mark is cleared")
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Review M-a (third review): after a real change of the password, "Save as" elsewhere: the
     * file before is this device's own last file with the earlier protection: it may be written
     * over and deleted. A file another device saved there in between never may.
     */
    @Test
    fun theFileOfTheEarlierProtectionIsWipedAndDeleted() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-earlier").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val stick = File(dir, "stick").apply { mkdirs() }
            val old = File(stick, "GhulSuite.ghlzip")
            val oldRef = SuiteFileRef(old.absolutePath, old.name)
            assertTrue(a.archiveSave(oldRef, "Laptop", created = true))
            a.changePassword(de.danerd.ghulsuite.model.FactorProofUi(password, null, kf), "$password neu") {}
            val now = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(now.absolutePath, now.name), "Laptop", created = true))
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.EARLIER_PROTECTION, a.oldFileCheck(oldRef))
            val r = a.deleteOldSuiteFile(oldRef)
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DELETED, r.outcome)
            assertEquals(listOf(old.absolutePath), a.wiped, "written over with zeros first")
            assertFalse(old.exists())
            // The same with a file that another device wrote there meanwhile: kept.
            val other = File(stick, "Zwei.ghlzip")
            val otherRef = SuiteFileRef(other.absolutePath, other.name)
            assertTrue(a.archiveSave(otherRef, "Laptop", created = true))
            val b = Backend(File(dir, "b"))
            b.archiveOpen(otherRef, "$password neu", null, kf, {})
            b.archiveAccept()
            b.renameVault(b.listVaults().single().id, "Von B")
            assertTrue(b.archiveSave(otherRef, "PC", created = false))
            a.changePassword(de.danerd.ghulsuite.model.FactorProofUi("$password neu", null, kf), "$password neuer") {}
            val third = File(dir, "drei/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(third.absolutePath, third.name), "Laptop", created = true))
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.OTHER, a.oldFileCheck(otherRef), "B's save: never")
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DIFFERENT_STATE, a.deleteOldSuiteFile(otherRef).outcome)
            assertTrue(other.isFile)
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Review H-1 (fourth review): a readable file of this suite whose state the working copy no
     * longer has (an older state was taken over meanwhile) stays, even with the very bytes this
     * device wrote there: only an unreadable file (an earlier protection) counts by its bytes.
     */
    @Test
    fun aReadableOwnStateTheWorkingCopyLeftIsNeverDeleted() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-branch").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val stick = File(dir, "stick").apply { mkdirs() }
            val older = File(stick, "Alt.ghlzip")
            val olderRef = SuiteFileRef(older.absolutePath, older.name)
            assertTrue(a.archiveSave(olderRef, "Laptop", created = true))
            // A newer state S1 at P (its bytes are recorded as this device's last file there).
            a.renameVault(a.listVaults().single().id, "Neu")
            val p = File(stick, "GhulSuite.ghlzip")
            val pRef = SuiteFileRef(p.absolutePath, p.name)
            assertTrue(a.archiveSave(pRef, "Laptop", created = true))
            // The older file is taken over: the working copy no longer has S1; P stays the place.
            a.archiveOpen(olderRef, password, null, kf, {})
            a.archiveAccept()
            a.setSuiteFile(pRef)
            val info = a.archiveTargetInfo(pRef)!!
            assertTrue(info.readable, "S1 is readable with the keys of now")
            assertEquals(de.danerd.ghulsuite.model.ArchiveTargetUi.CHANGED_ELSEWHERE, info.target)
            val r = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(r.absolutePath, r.name), "Laptop", created = true))
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.OTHER, a.oldFileCheck(pRef), "S1 is not offered")
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DIFFERENT_STATE, a.deleteOldSuiteFile(pRef).outcome)
            assertTrue(p.isFile, "S1 stays")
            assertTrue(a.wiped.isEmpty())
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Options, Keys and key files, "Replace" on the key file: confirmed with the key file of now,
     * a new one made, the old one removed in the same change. The old key file opens nothing any
     * more, the new one does.
     */
    @Test
    fun aReplacedKeyFileNoLongerOpensTheSuite() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-replace").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val old = a.slots().single { it.type == de.danerd.ghulsuite.model.SlotType.KEYFILE }
            val proof = de.danerd.ghulsuite.model.FactorProofUi(password, null, kf)
            val fresh = generateKeyfile()
            a.confirmProtection(proof) {}
            a.replaceFactors(proof, listOf(de.danerd.ghulsuite.model.NewFactorUi.Keyfile(fresh, "Neu")), listOf(old.id)) {}
            val now = a.slots().filter { it.type == de.danerd.ghulsuite.model.SlotType.KEYFILE }
            assertEquals(listOf("Neu"), now.map { it.label })
            a.lock()
            assertFailsWith<VaultException> { a.unlock(password, null, kf) {} }
            assertFalse(a.unlocked.value)
            a.unlock(password, null, fresh) {}
            assertTrue(a.unlocked.value)
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Review N-1: confirmed with a key file, nothing is left behind (no confirmation the next
     * change could use with the password alone); a wrong password is refused at once.
     */
    @Test
    fun aKeyFileConfirmationLeavesNothingBehind() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-confirm").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            assertFailsWith<VaultException> { a.confirmProtection(de.danerd.ghulsuite.model.FactorProofUi("falsch-falsch-1", null, kf)) {} }
            a.confirmProtection(de.danerd.ghulsuite.model.FactorProofUi(password, null, kf)) {}
            val passwordAlone = de.danerd.ghulsuite.model.FactorProofUi(password, null, null)
            assertFailsWith<VaultException> {
                a.replaceFactors(passwordAlone, listOf(de.danerd.ghulsuite.model.NewFactorUi.Keyfile(generateKeyfile(), "Neu")), emptyList()) {}
            }
            assertEquals(1, a.slots().count { it.type == de.danerd.ghulsuite.model.SlotType.KEYFILE })
        } finally {
            dir.deleteRecursively()
        }
    }

    /**
     * Device test after PR #55: a new suite, set up as the wizard does (no area at first, then all
     * four areas, then the suite file), was "unsaved: GhulCal" without a touch: the first look at
     * GhulCal made the usual calendars and the area settings were filled in from this device, both
     * after the suite file. Now both go into the area files when they are made.
     */
    @Test
    fun aNewSuiteIsSavedAfterTheFirstLookAtEachArea() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-new").toFile()
        val scope = kotlinx.coroutines.CoroutineScope(
            kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Default +
                kotlinx.coroutines.CoroutineExceptionHandler { _, e -> if (e !is java.awt.HeadlessException) throw e },
        )
        try {
            val kf = generateKeyfile()
            val a = Backend(dir, realCal = true)
            a.create(
                de.danerd.ghulsuite.model.SetupUi(
                    password = password, keyPin = null, keyLabel = "Key", keyfile = kf, keyfileLabel = "Datei",
                    recoveryLabel = "Code", kdf = de.danerd.ghulsuite.model.KdfChoice(65536, 3, 0), vaults = emptyList(),
                ),
            ) {}
            if (!a.unlocked.value) a.unlock(password, null, kf) {}
            val defaults = listOf("Privat" to 0xD6336C, "Arbeit" to 0x2F74E8)
            // Step 4 of the wizard: the areas (applyAreas).
            for (kind in listOf(VaultKindUi.PASS, VaultKindUi.TOTP, VaultKindUi.DOCS, VaultKindUi.CAL)) {
                a.addArea(kind, "")
                de.danerd.ghulsuite.model.prepareNewArea(a, kind, defaults)
            }
            // Step 8: the suite file.
            val p = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val pRef = SuiteFileRef(p.absolutePath, p.name)
            assertTrue(a.archiveSave(pRef, "Laptop", created = true))
            a.setSuiteFile(pRef)
            assertTrue(a.archiveUnsavedAreas().isEmpty(), "saved: " + a.archiveUnsavedAreas())
            // The main screen: the vaults and their settings, then a look at each area.
            val state = de.danerd.ghulsuite.ui.AppState(a, scope)
            val vaults = a.listVaults()
            state.loadAreaOptions(vaults.map { it.kind }.distinct())
            for (v in vaults) a.areaOptions(v.kind)
            assertEquals(defaults.map { it.first }, a.cal.calendars().map { it.name }, "the usual calendars from the start")
            assertFalse(state.calFirstLook(a.cal.calendars(), defaults, vaults.first { it.kind == VaultKindUi.CAL }), "nothing made at the first look")
            for (v in vaults.filter { it.kind != VaultKindUi.CAL && it.kind != VaultKindUi.DOCS }) a.listEntries(v.id)
            assertFalse(a.archiveUnsaved(), "unsaved: " + a.archiveUnsavedAreas())
            assertTrue(a.archiveUnsavedAreas().isEmpty())
        } finally {
            scope.coroutineContext[kotlinx.coroutines.Job]?.cancel()
            dir.deleteRecursively()
        }
    }

    /**
     * Review N-b (fifth review): an old file already gone is not deleted again (an access to its
     * place could put a leftover back first): only its record and its grant go.
     */
    @Test
    fun anOldFileAlreadyGoneIsOnlyForgotten() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-gone").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val old = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val oldRef = SuiteFileRef(old.absolutePath, old.name)
            assertTrue(a.archiveSave(oldRef, "Laptop", created = true))
            val now = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(now.absolutePath, now.name), "Laptop", created = true))
            assertTrue(old.delete())
            assertEquals(de.danerd.ghulsuite.model.OldFileCheck.GONE, a.oldFileCheck(oldRef))
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DELETED, a.deleteOldSuiteFile(oldRef).outcome)
            assertTrue(a.deleteCalls.isEmpty(), "nothing deleted: " + a.deleteCalls)
            assertEquals(listOf(old.absolutePath), a.released)
            assertFalse(a.prefs["suite_written"].orEmpty().contains(old.absolutePath), "its record went")
            assertTrue(now.isFile)
        } finally {
            dir.deleteRecursively()
        }
    }

    /** Review N-1, N-2: a file changed between the check and the delete is kept; a deleted place forgets its record. */
    @Test
    fun aFileChangedSinceTheCheckIsKept() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-changed").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val old = File(dir, "stick/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            val oldRef = SuiteFileRef(old.absolutePath, old.name)
            assertTrue(a.archiveSave(oldRef, "Laptop", created = true))
            val now = File(dir, "neu/GhulSuite.ghlzip").apply { parentFile.mkdirs() }
            assertTrue(a.archiveSave(SuiteFileRef(now.absolutePath, now.name), "Laptop", created = true))
            // Changed right between the check and the delete (the backend reads it twice).
            a.beforeDelete = { old.appendBytes(byteArrayOf(1)) }
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.CHANGED, a.deleteOldSuiteFile(oldRef).outcome)
            assertTrue(old.isFile)
            a.beforeDelete = {}
            old.writeBytes(old.readBytes().copyOf(old.length().toInt() - 1))
            assertTrue(a.prefs["suite_written"].orEmpty().contains(old.absolutePath))
            assertEquals(de.danerd.ghulsuite.model.OldFileOutcome.DELETED, a.deleteOldSuiteFile(oldRef).outcome)
            assertFalse(a.prefs["suite_written"].orEmpty().contains(old.absolutePath), "its record went with it")
        } finally {
            dir.deleteRecursively()
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
