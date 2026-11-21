// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.model.UpdateStep
import de.danerd.ghulsuite.model.UpdateUi
import java.io.File
import java.net.URI
import kotlin.system.exitProcess

/**
 * Windows. Installed (MSI): the new installer is checked by the core (signed checksums), run
 * without questions for this user only (as the installed one), and GhulSuite starts again
 * afterwards. Portable: nothing is replaced on the stick; the release page opens with the new ZIP.
 */
class DesktopUpdater : CoreUpdater(if (DesktopPaths.portable) "-portable.zip" else ".msi") {
    private val exe: File? = System.getProperty("jpackage.app-path")?.let { File(it) }

    override val available: Boolean
        get() = hasKey && exe != null && System.getProperty("os.name").lowercase().contains("win")
    override val installs: Boolean get() = !DesktopPaths.portable

    override suspend fun install(update: UpdateUi): UpdateStep {
        if (DesktopPaths.portable) {
            runCatching { java.awt.Desktop.getDesktop().browse(URI(update.page)) }
            return UpdateStep.STARTED
        }
        val dir = File(System.getProperty("java.io.tmpdir"), "GhulSuite-Update")
        val msi = download(update, dir)
        val script = File(dir, "update.cmd")
        // cmd reads a batch file in the console code page: UTF-8 is switched on first (paths with
        // umlauts, e.g. C:\Users\Jürgen), and "%" in a path is doubled (else it is expanded). The
        // installer starts only once this process has ended (no file of the app in use).
        fun q(path: String) = "\"" + path.replace("%", "%%") + "\""
        val pid = ProcessHandle.current().pid()
        script.writeText(
            "@echo off\r\n" +
                "chcp 65001 >nul\r\n" +
                ":wait\r\n" +
                "tasklist /FI \"PID eq $pid\" 2>nul | find \"$pid\" >nul && (timeout /t 1 /nobreak >nul & goto wait)\r\n" +
                "msiexec /i ${q(msi.absolutePath)} /passive /norestart\r\n" +
                "start \"\" ${q(exe!!.absolutePath)}\r\n" +
                "del ${q(msi.absolutePath)}\r\n",
            Charsets.UTF_8,
        )
        ProcessBuilder("cmd", "/c", script.absolutePath).directory(dir).start()
        // The installer replaces the files of the running app: it ends now (the keys go with the
        // process; the app made sure before that nothing is being saved or edited).
        exitProcess(0)
    }
}
