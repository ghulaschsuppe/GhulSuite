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
        script.writeText(
            "@echo off\r\n" +
                "msiexec /i \"${msi.absolutePath}\" /passive /norestart\r\n" +
                "start \"\" \"${exe!!.absolutePath}\"\r\n" +
                "del \"${msi.absolutePath}\"\r\n",
        )
        ProcessBuilder("cmd", "/c", script.absolutePath).directory(dir).start()
        // The installer replaces the files of the running app: close it (this locks the suite).
        exitProcess(0)
    }
}
