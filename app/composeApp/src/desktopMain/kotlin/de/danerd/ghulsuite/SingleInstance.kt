// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.PickedSuiteFile
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.SecureRandom

/**
 * One GhulSuite at a time per data folder: two would work on the same files. A second start
 * (e.g. a double click on a suite file) hands its file to the one already running and ends.
 *
 * The running one listens on the loopback interface only; its port and a random token are in a
 * file in the data folder (readable by this user only, like the suite itself). Without the token
 * nothing is accepted, so another program on the machine cannot hand files over.
 */
object SingleInstance {
    private const val MAX_SUITE_BYTES = 512L * 1024 * 1024
    private val portFile: File get() = File(DesktopPaths.dataDir, "instance.port")

    /** Hands [path] (or nothing: just come to the front) to a running GhulSuite; true if one took it. */
    fun handOver(path: String?): Boolean {
        val parts = runCatching { portFile.readText().trim().split(' ') }.getOrNull() ?: return false
        val port = parts.getOrNull(0)?.toIntOrNull() ?: return false
        val token = parts.getOrNull(1) ?: return false
        return runCatching {
            Socket(InetAddress.getLoopbackAddress(), port).use { s ->
                s.soTimeout = 3000
                val out = s.getOutputStream().bufferedWriter(Charsets.UTF_8)
                out.write(token + "\n" + (path ?: "") + "\n")
                out.flush()
                s.getInputStream().read() == 'k'.code
            }
        }.getOrDefault(false)
    }

    /** Listens for later starts; [onStart] gets the file each one hands over (null: none). */
    fun listen(onStart: (String?) -> Unit) {
        val server = runCatching { ServerSocket(0, 8, InetAddress.getLoopbackAddress()) }.getOrNull() ?: return
        val token = ByteArray(24).also { SecureRandom().nextBytes(it) }.joinToString("") { "%02x".format(it) }
        runCatching {
            DesktopPaths.dataDir.mkdirs()
            portFile.writeText("${server.localPort} $token")
            portFile.deleteOnExit()
        }
        Thread {
            while (true) {
                val s = runCatching { server.accept() }.getOrNull() ?: break
                runCatching {
                    s.use {
                        it.soTimeout = 3000
                        val input = it.getInputStream().bufferedReader(Charsets.UTF_8)
                        if (input.readLine() == token) {
                            val path = input.readLine().orEmpty()
                            it.getOutputStream().apply { write('k'.code); flush() }
                            onStart(path.ifEmpty { null })
                        }
                    }
                }
            }
        }.apply { isDaemon = true; name = "GhulSuite single instance" }.start()
    }

    /** A suite file from the command line (double click) for the app to open. */
    fun open(path: String) {
        val f = File(path)
        // As large as the core reads at most (it checks the size itself as well).
        if (!f.isFile || f.length() > MAX_SUITE_BYTES) return
        val data = runCatching { f.readBytes() }.getOrNull() ?: return
        AppLaunch.suiteFile.value = PickedSuiteFile(SuiteFileRef(f.absolutePath, f.name), data)
    }

    /** The suite file among the program's arguments. */
    fun suiteFileArg(args: Array<String>): String? = args.firstOrNull { it.endsWith(".ghlzip", ignoreCase = true) }
}
