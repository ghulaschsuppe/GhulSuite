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

    override suspend fun install(update: UpdateUi, mayFinish: () -> Boolean): UpdateStep {
        if (DesktopPaths.portable) {
            runCatching { java.awt.Desktop.getDesktop().browse(URI(update.page)) }
            return UpdateStep.STARTED
        }
        // M24: a new folder with a random name for every run (another program cannot prepare
        // a file there in advance); folders of earlier runs go.
        val temp = File(System.getProperty("java.io.tmpdir"))
        temp.listFiles { f -> f.isDirectory && (f.name == "GhulSuite-Update" || f.name.startsWith("GhulSuite-Update-")) }
            ?.forEach { it.deleteRecursively() }
        val dir = java.nio.file.Files.createTempDirectory(temp.toPath(), "GhulSuite-Update-").toFile()
        val msi = download(update, dir)
        // M7: checked again right before the app ends (a save or an edit started meanwhile).
        if (!mayFinish()) {
            dir.deleteRecursively()
            return UpdateStep.WAIT
        }
        resultFile.delete()
        // M24: no script file that could be changed while it waits: the commands go to
        // PowerShell as one argument, and the installer is held open (others may read, not
        // write, rename or delete it) from the hash check until msiexec has ended. Without
        // PowerShell this throws here and the app stays open with the message.
        val script = updateScript(ProcessHandle.current().pid(), msi.absoluteFile, expectedSha256(update), exe!!.absoluteFile, resultFile.absoluteFile)
        // r9 H-1: by its full path only. A bare "powershell" is looked for in the current folder of
        // GhulSuite first (the folder of a suite file opened by double click, e.g. on a stick), so
        // a planted powershell.exe there would run. Children do not search it either.
        val ps = powershellExe(System.getenv("SystemRoot"))
        try {
            if (ps == null) throw java.io.IOException("no PowerShell")
            ProcessBuilder(updateCommand(ps, script)).directory(dir)
                .apply { environment()["NoDefaultCurrentDirectoryInExePath"] = "1" }
                .start()
        } catch (e: java.io.IOException) {
            dir.deleteRecursively()
            throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.INVALID_INPUT, NO_POWERSHELL)
        }
        // The script overwrites this with its result. Still there at the next start: it never ran
        // (a virus scanner or AppLocker stopped it), and the app says so.
        runCatching { resultFile.writeText(PENDING + " " + System.currentTimeMillis()) }
        // The installer replaces the files of the running app: it ends now (the keys go with the
        // process; the app made sure right before that nothing is being saved or edited).
        exitProcess(0)
    }

    /** Where the script leaves the exit code of msiexec for the next start (M24). */
    private val resultFile: File get() = File(DesktopPaths.dataDir, "update-result.txt")

    override fun takeFailedInstall(): String? {
        val f = resultFile
        if (!f.exists()) return null
        val text = runCatching { f.readText() }.getOrDefault("")
        // N6: started by hand while the script still waits for msiexec: no verdict yet, the
        // file stays for the script (and for the next start).
        val code = installResult(text, System.currentTimeMillis()) ?: return null
        f.delete()
        return code.takeIf { failedInstall(it) }
    }
}

/**
 * The result the script left ([text]), as [failedInstall] reads it; null while it may still run.
 * "pending <time>" (written before the app ended) counts as "it never ran" only once
 * [PENDING_GRACE_MS] have passed since (an installation takes a few minutes at most); a plain
 * "pending" (an older version) right away.
 */
internal fun installResult(text: String, now: Long): String? {
    val parts = text.trim().split(' ')
    if (parts.first() != PENDING) return text.trim()
    val at = parts.getOrNull(1)?.toLongOrNull() ?: return PENDING
    // A time in the future (the clock was set back): no reason to wait for ever.
    return if (now - at in 0 until PENDING_GRACE_MS) null else PENDING
}

/** How long after the app ended for an update "pending" still means: the script may be running. */
internal const val PENDING_GRACE_MS = 10 * 60 * 1000L

/** Exit codes of msiexec that mean success: 0, and "done, restart needed" (1641, 3010). */
internal fun failedInstall(code: String): Boolean = code !in setOf("0", "1641", "3010")

/**
 * The PowerShell of Windows by its full path (r9 H-1), null when it is not there. [systemRoot]
 * is %SystemRoot%; without it the usual folder.
 */
internal fun powershellExe(systemRoot: String?, isFile: (File) -> Boolean = { it.isFile }): File? {
    val root = systemRoot?.takeIf { it.isNotBlank() && File(it).isAbsolute } ?: "C:\\Windows"
    return File(root, "System32\\WindowsPowerShell\\v1.0\\powershell.exe").takeIf(isFile)
}

/** The command line of the update: [ps] (absolute) with the script as one argument. */
internal fun updateCommand(ps: File, script: String): List<String> {
    require(ps.isAbsolute) { "PowerShell only by its full path" }
    return listOf(ps.path, "-NoProfile", "-NonInteractive", "-EncodedCommand", encodedCommand(script))
}

/** PowerShell's -EncodedCommand: the script in UTF-16LE, Base64. */
internal fun encodedCommand(script: String): String =
    java.util.Base64.getEncoder().encodeToString(script.toByteArray(Charsets.UTF_16LE))

/**
 * The PowerShell commands that install the update once this process ([pid]) has ended (M24). The
 * installer [msi] is opened with read sharing only and stays open until msiexec has ended, so
 * nothing can change, rename or delete it between the check of its SHA-256 against [sha256] and
 * the installation. A mismatch installs nothing. [result] gets the exit code of msiexec, "hash"
 * (wrong checksum) or "error" (the check itself failed); then GhulSuite ([exe]) starts again.
 */
internal fun updateScript(pid: Long, msi: File, sha256: String, exe: File, result: File): String {
    // Inside single quotes only ' itself needs doubling.
    fun q(text: String) = "'" + text.replace("'", "''") + "'"
    val hash = sha256.filter { it.isLetterOrDigit() }.uppercase()
    return listOf(
        "Wait-Process -Id $pid -ErrorAction SilentlyContinue",
        "\$code = 'error'",
        "try {",
        "  \$f = [IO.File]::Open(${q(msi.path)}, 'Open', 'Read', 'Read')",
        "  try {",
        "    \$h = -join ([Security.Cryptography.SHA256]::Create().ComputeHash(\$f) | ForEach-Object { \$_.ToString('X2') })",
        "    if (\$h -ne '$hash') { \$code = 'hash' } else {",
        "      \$p = Start-Process -FilePath (Join-Path \$env:SystemRoot 'System32\\msiexec.exe') -ArgumentList @('/i', ${q("\"" + msi.path + "\"")}, '/passive', '/norestart') -Wait -PassThru",
        "      \$code = [string]\$p.ExitCode",
        "    }",
        "  } finally { \$f.Close() }",
        "} catch { \$code = 'error' }",
        "Set-Content -LiteralPath ${q(result.path)} -Value \$code -Encoding ASCII",
        "Remove-Item -LiteralPath ${q(msi.path)} -ErrorAction SilentlyContinue",
        "Start-Process -FilePath ${q(exe.path)}",
    ).joinToString("\r\n")
}

/** Written before the app ends for an update; the script replaces it with its result. */
internal const val PENDING = "pending"

/** PowerShell could not be started (the update needs it; translated by the app). */
internal const val NO_POWERSHELL = "the update needs PowerShell"
