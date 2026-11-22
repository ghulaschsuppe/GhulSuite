// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * M24: the Windows update goes to PowerShell as one argument (no script file to change while it
 * waits), holds the installer open from the hash check until msiexec has ended, installs nothing
 * on a mismatch, and leaves the exit code of msiexec for the next start.
 */
class UpdateScriptTest {
    private val msi = File("C:\\Users\\Jürgen 100%\\AppData\\Local\\Temp\\GhulSuite-Update-123\\GhulSuite-0.6.0.msi")
    private val exe = File("C:\\Program Files\\GhulSuite\\GhulSuite.exe")
    private val result = File("C:\\Users\\Jürgen 100%\\AppData\\Roaming\\GhulSuite\\update-result.txt")

    @Test
    fun hashIsCheckedWhileTheInstallerIsHeld() {
        val s = updateScript(4711, msi, "ab12cd" + "0".repeat(58), exe, result)
        val lines = s.split("\r\n")
        assertEquals("Wait-Process -Id 4711 -ErrorAction SilentlyContinue", lines[0])
        val open = lines.indexOfFirst { it.contains("[IO.File]::Open(") }
        val check = lines.indexOfFirst { it.contains("-ne 'AB12CD" + "0".repeat(58) + "'") }
        val install = lines.indexOfFirst { it.contains("msiexec.exe") }
        val close = lines.indexOfFirst { it.contains("\$f.Close()") }
        // Opened (read sharing only), checked, installed, and only then let go.
        assertTrue(open in 0 until check && check < install && install < close, s)
        assertTrue(lines[open].contains("'Open', 'Read', 'Read'"), lines[open])
        // Paths as they are ("%" means nothing to PowerShell in single quotes).
        assertTrue(lines[open].contains("'C:\\Users\\Jürgen 100%\\"), lines[open])
        assertTrue(s.contains("Set-Content -LiteralPath 'C:\\Users\\Jürgen 100%\\AppData\\Roaming\\GhulSuite\\update-result.txt'"))
        assertTrue(s.lines().last().startsWith("Start-Process -FilePath 'C:\\Program Files\\GhulSuite\\GhulSuite.exe'"))
        assertFalse(s.contains("update.cmd"))
    }

    /** r9 H-1: PowerShell only by its full path (never looked up in the current folder), else none. */
    @Test
    fun powershellIsStartedByItsFullPath() {
        val root = kotlin.io.path.createTempDirectory("sysroot").toFile()
        try {
            val ps = powershellExe(root.absolutePath) { true }!!
            val cmd = updateCommand(ps, "Write-Output 1")
            assertTrue(File(cmd.first()).isAbsolute, cmd.first())
            assertTrue(cmd.first().endsWith("powershell.exe"), cmd.first())
            assertEquals(listOf("-NoProfile", "-NonInteractive", "-EncodedCommand"), cmd.subList(1, 4))
            // Not there: no PowerShell (the app says so), never a bare name.
            assertEquals(null, powershellExe(root.absolutePath) { false })
            // A relative %SystemRoot% is not taken.
            assertFalse(powershellExe("Windows") { true }!!.path.startsWith("Windows"))
            kotlin.test.assertFailsWith<IllegalArgumentException> { updateCommand(File("powershell.exe"), "x") }
            // msiexec by its full path as well.
            assertTrue(updateScript(1, msi, "0".repeat(64), exe, result).contains("Join-Path \$env:SystemRoot 'System32\\msiexec.exe'"))
        } finally {
            root.deleteRecursively()
        }
    }

    @Test
    fun exitCodesOfMsiexec() {
        for (ok in listOf("0", "1641", "3010")) assertFalse(failedInstall(ok), ok)
        for (bad in listOf("1602", "1603", "1618", "hash", "error", "")) assertTrue(failedInstall(bad), bad)
    }

    @Test
    fun pendingCountsAsNotRunOnlyAfterSomeMinutes() {
        val t = 1_000_000_000L
        // The script may still wait for msiexec: no verdict, the file stays.
        assertEquals(null, installResult("pending $t", t + 60_000))
        assertEquals(PENDING, installResult("pending $t\r\n", t + PENDING_GRACE_MS))
        // The clock was set back: not waited for ever.
        assertEquals(PENDING, installResult("pending $t", t - 1))
        // An older version wrote it without a time.
        assertEquals(PENDING, installResult("pending", t))
        assertEquals("1603", installResult("1603\r\n", t))
        assertTrue(failedInstall(installResult("pending $t", t + PENDING_GRACE_MS)!!))
    }

    @Test
    fun aQuoteInAPathIsSafeForPowerShell() {
        val s = updateScript(1, File("C:\\x\\it's.msi"), "00", exe, result)
        assertTrue(s.contains("'C:\\x\\it''s.msi'"), s)
    }

    /** -EncodedCommand is UTF-16LE in Base64 (umlauts in user names survive). */
    @Test
    fun encodedAsPowerShellExpects() {
        val script = "Write-Output 'Jürgen'"
        assertEquals(script, String(java.util.Base64.getDecoder().decode(encodedCommand(script)), Charsets.UTF_16LE))
    }
}
