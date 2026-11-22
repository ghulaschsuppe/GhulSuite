// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.isRunnableFile
import de.danerd.ghulsuite.ui.mimeOf
import de.danerd.ghulsuite.ui.openEnding
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * H4: "Open with app" never hands a program, script or shortcut to the system, also not when the
 * name hides the ending (alternate data stream, trailing dots, a cut long name), and a copy on
 * Windows carries the Mark of the Web.
 */
class OpenWithSafetyTest {
    @Test
    fun runnableEndingsAreRefused() {
        for (n in listOf(
            "Rechnung.pdf.exe", "a.HTA", "b.js", "c.lnk", "d.url", "e.scf", "f.ps1", "g.msi", "h.jar", "i.library-ms",
            "j.settingcontent-ms", "k.appref-ms", "l.vbs", "m.bat", "n.cmd", "o.apk", "p.iso", "q.chm",
            "r.one", "s.onepkg", "t.xpi", "u.cab", "v.iqy", "w.slk", "x.wsb", "Login.html", "y.htm", "z.svg", "a.mhtml",
            // Windows drops trailing dots and spaces, and ":" starts a data stream.
            "Rechnung.exe.", "Rechnung.exe . ", "Rechnung.pdf:x.exe",
        )) assertTrue(isRunnableFile(n), n)
        for (n in listOf("Mietvertrag.pdf", "Foto.JPG", "Notizen.txt", "Brief.docx", "ohne Endung", "exe", ".bashrc.txt")) {
            assertFalse(isRunnableFile(n), n)
        }
        // PATHEXT of the PC adds its own endings.
        assertTrue(isRunnableFile("tool.xyz", pathExtEndings(".COM;.EXE;.XYZ")))
        assertEquals(listOf("com", "exe", "xyz"), pathExtEndings(".COM; .EXE;;.XYZ"))
        assertEquals(emptyList(), pathExtEndings(null))
    }

    @Test
    fun fileNamesKeepTheirEnding() {
        // No data stream and no dropped dot: the file written is the one checked.
        assertEquals("a.pdf_x.exe", safeFileName("a.pdf:x.exe", mutableSetOf()))
        assertEquals("Rechnung.exe", safeFileName("Rechnung.exe. . ", mutableSetOf()))
        assertEquals("a_b_c_d_e_f.txt", safeFileName("a<b>c\"d|e?f.txt", mutableSetOf()))
        // A long name is cut before its ending, so "aaa….exe.txt" stays a ".txt".
        val long = "a".repeat(116) + ".exe.txt"
        val cut = safeFileName(long, mutableSetOf())
        assertTrue(cut.length <= 120, cut)
        assertEquals("txt", openEnding(cut))
        assertFalse(isRunnableFile(cut))
        val used = mutableSetOf<String>()
        assertEquals("x.pdf", safeFileName("x.pdf", used))
        assertEquals("x (2).pdf", safeFileName("x.pdf", used))
    }

    @Test
    fun typeComesFromTheName() {
        assertEquals("application/pdf", mimeOf("Rechnung.PDF"))
        assertEquals("application/octet-stream", mimeOf("Rechnung.pdf.apk"))
        assertEquals("text/plain", mimeOf("x.txt."))
    }

    /** The Zone.Identifier text Windows reads (written next to the file on a file system without streams). */
    @Test
    fun markOfTheWeb() {
        val dir = kotlin.io.path.createTempDirectory("motw").toFile()
        try {
            val f = File(dir, "Rechnung.pdf").apply { writeText("%PDF") }
            assertTrue(markFromInternet(f))
            assertEquals("[ZoneTransfer]\r\nZoneId=3\r\n", File(f.path + ":Zone.Identifier").readText())
            assertEquals("%PDF", f.readText(), "the copy itself is unchanged")
        } finally {
            dir.deleteRecursively()
        }
    }
}

/** The crash report (seen before unlocking) holds classes and frames, never a message. */
class CrashTraceTest {
    @Test
    fun noMessages() {
        val e = IllegalStateException("Kontostand von Erika Mustermann", java.io.IOException("Rechnung.pdf fehlt"))
        e.addSuppressed(RuntimeException("geheim"))
        val t = crashTrace(e)
        assertTrue(t.startsWith("java.lang.IllegalStateException\n"), t)
        assertTrue(t.contains("Caused by: java.io.IOException"))
        assertTrue(t.contains("\tat de.danerd.ghulsuite.CrashTraceTest.noMessages"))
        for (secret in listOf("Erika", "Rechnung", "geheim")) assertFalse(t.contains(secret), t)
    }
}

/** H12: a full storage is recognised by codes and errno names, never by the words alone. */
class NoSpaceTest {
    @Test
    fun byCodeNotByWords() {
        assertTrue(isNoSpace(java.io.IOException("write failed: ENOSPC (No space left on device)")))
        // The code of this system (Windows 112, else 28); the other system's number means something else.
        val windows = System.getProperty("os.name").lowercase().contains("win")
        val (full, other) = if (windows) 112 to 28 else 28 to 112
        assertTrue(isNoSpace(java.io.IOException("outer", java.io.IOException("Auf dem Datenträger ist nicht genügend Speicherplatz vorhanden. (os error $full)"))))
        assertFalse(isNoSpace(java.io.IOException("x (os error $other)")))
        assertFalse(isNoSpace(java.io.IOException("No space left on device")), "words alone say nothing")
        assertFalse(isNoSpace(java.io.IOException("permission denied (os error 13)")))
        // Windows names no code: the room left at the place decides.
        assertTrue(isNoSpace(java.io.IOException("Es ist nicht genügend Speicherplatz vorhanden")) { 4096L })
        assertFalse(isNoSpace(java.io.IOException("Zugriff verweigert")) { 10L shl 30 })
        assertFalse(isNoSpace(java.io.IOException("?")) { throw IllegalStateException() })
    }
}

/**
 * G2-M2: after a change of the protection the file replaced at the place is overwritten with
 * zeros once the new one is in place; a "<name>.ghlold" left by a power cut takes its name back.
 */
class WipeOldSuiteFileTest {
    /** N1: a storage error before a quick unlock keeps it set up. */
    @Test
    fun aStorageErrorKeepsTheQuickUnlock() {
        assertTrue(de.danerd.ghulsuite.model.VaultErrorKind.STORAGE in de.danerd.ghulsuite.model.QUICK_UNLOCK_KEPT_ON)
        assertTrue(de.danerd.ghulsuite.model.VaultErrorKind.NO_SPACE in de.danerd.ghulsuite.model.QUICK_UNLOCK_KEPT_ON)
        assertFalse(de.danerd.ghulsuite.model.VaultErrorKind.CORRUPTED in de.danerd.ghulsuite.model.QUICK_UNLOCK_KEPT_ON)
    }

    @Test
    fun theOldFileIsWipedAfterTheNewOneIsInPlace() {
        val dir = kotlin.io.path.createTempDirectory("wipe-old").toFile()
        try {
            val target = File(dir, "GhulSuite.ghlzip").apply { writeText("alter Stand") }
            val ok = SuiteFiles.writeReplacing(target, { it.write("neuer Stand".toByteArray()) }, { true }, wipeOld = true)
            assertTrue(ok)
            assertEquals("neuer Stand", target.readText())
            assertEquals(listOf("GhulSuite.ghlzip"), dir.list()!!.toList(), "no .ghlold or .ghltmp left")
            // A check that fails leaves the old file as it was, also with wipeOld.
            assertFalse(SuiteFiles.writeReplacing(target, { it.write("kaputt".toByteArray()) }, { false }, wipeOld = true))
            assertEquals("neuer Stand", target.readText())
            // The power went between the two moves: the old file takes its name back first.
            target.renameTo(File(dir, "GhulSuite.ghlzip.ghlold"))
            assertFalse(SuiteFiles.writeReplacing(target, { it.write("x".toByteArray()) }, { false }))
            assertEquals("neuer Stand", target.readText())
            // N3: a half wiped ".ghlold" next to the file goes with the next save (said first).
            File(dir, "GhulSuite.ghlzip.ghlold").writeText("halb genullt")
            var said = false
            assertTrue(SuiteFiles.writeReplacing(target, { it.write("dritter Stand".toByteArray()) }, { true }, onWipe = { said = true }))
            assertTrue(said)
            assertEquals(listOf("GhulSuite.ghlzip"), dir.list()!!.toList())
            // Only "<name>.ghlold" there (cut off between the moves): opening finds it under its name.
            target.renameTo(File(dir, "GhulSuite.ghlzip.ghlold"))
            assertEquals("dritter Stand", SuiteFiles.input(target)!!.stream.use { it.readBytes().decodeToString() })
            assertEquals(listOf("GhulSuite.ghlzip"), dir.list()!!.toList())
            // Picked as it is: the same.
            target.renameTo(File(dir, "GhulSuite.ghlzip.ghlold"))
            assertEquals(target.absoluteFile, SuiteFiles.resolve(File(dir, "GhulSuite.ghlzip.ghlold")).absoluteFile)
        } finally {
            dir.deleteRecursively()
        }
    }
}
