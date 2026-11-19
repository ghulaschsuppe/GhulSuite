// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.PickedSuiteFile
import java.io.File
import java.io.InputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.nio.channels.FileChannel
import java.nio.channels.FileLock
import java.nio.channels.OverlappingFileLockException
import java.nio.file.StandardOpenOption
import java.security.SecureRandom

/**
 * One GhulSuite at a time per data folder: two would work on the same files. A second start
 * (e.g. a double click on a suite file) hands its file to the one already running and ends.
 *
 * The first thing a start does is take an exclusive lock on a file in the data folder ([claim]):
 * whoever has it is the GhulSuite of that folder, a later start hands over and ends, also while
 * the first one is still starting (it then waits until that one listens). The running one listens
 * on the loopback interface only; its port and a random token are in a file in the data folder
 * (readable by this user only, like the suite itself). Without the token nothing is accepted, so
 * another program on the machine cannot hand files over; what a connection sends is read up to
 * [MAX_HANDOVER] bytes and for at most a few seconds.
 */
object SingleInstance {
    private val portFile: File get() = File(DesktopPaths.dataDir, "instance.port")

    /** Most bytes read from one connection (token and path). */
    const val MAX_HANDOVER = 8 * 1024

    /** The locks taken (per data folder), held as long as the program runs. */
    private val locks = mutableMapOf<String, Pair<FileChannel, FileLock>>()

    /**
     * Takes the data folder [dir] for this program: true if no other GhulSuite has it (then this
     * one keeps it until it ends), false if one runs (or is starting) there. If the lock cannot be
     * made at all (e.g. a folder that cannot be written), true: then nothing could be saved there
     * either, and the app says so itself.
     */
    @Synchronized
    fun claim(dir: File = DesktopPaths.dataDir): Boolean {
        val key = dir.absolutePath
        if (locks.containsKey(key)) return true
        val channel = runCatching {
            dir.mkdirs()
            FileChannel.open(File(dir, "instance.lock").toPath(), StandardOpenOption.CREATE, StandardOpenOption.WRITE)
        }.getOrNull() ?: return true
        val lock = try {
            channel.tryLock()
        } catch (_: OverlappingFileLockException) {
            null
        } catch (_: java.io.IOException) {
            channel.close()
            return true
        }
        if (lock == null) {
            channel.close()
            return false
        }
        locks[key] = channel to lock
        return true
    }

    /** Gives the data folder free again (tests only; a program keeps it until it ends). */
    @Synchronized
    internal fun release(dir: File) {
        locks.remove(dir.absolutePath)?.let { (c, l) -> runCatching { l.release(); c.close() } }
    }

    /**
     * A later start: hands [path] to the GhulSuite that has the data folder, waiting up to
     * [waitMs] for it to listen (it may be starting itself). True if it took it.
     */
    fun handOverWaiting(path: String?, waitMs: Long = 15_000): Boolean {
        val until = System.currentTimeMillis() + waitMs
        while (true) {
            if (handOver(path)) return true
            if (System.currentTimeMillis() > until) return false
            Thread.sleep(250)
        }
    }

    /**
     * One line of at most [max] bytes in all ([used] already read on this connection), until
     * [deadline] (ms); null if it is longer, slower or the connection ends first.
     */
    internal fun readLimitedLine(input: InputStream, max: Int, used: IntArray, deadline: Long): String? {
        val buf = java.io.ByteArrayOutputStream()
        while (true) {
            if (System.currentTimeMillis() > deadline) return null
            val b = input.read()
            if (b < 0) return null
            if (++used[0] > max) return null
            if (b == '\n'.code) return buf.toString(Charsets.UTF_8).removeSuffix("\r")
            buf.write(b)
        }
    }

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
                        it.soTimeout = 1000
                        val input = java.io.BufferedInputStream(it.getInputStream())
                        val used = IntArray(1)
                        val deadline = System.currentTimeMillis() + 3000
                        if (readLimitedLine(input, MAX_HANDOVER, used, deadline) == token) {
                            val path = readLimitedLine(input, MAX_HANDOVER, used, deadline) ?: return@use
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
        // Read as a stream when it is opened; the core counts the size itself.
        if (!f.isFile) return
        AppLaunch.suiteFile.value = PickedSuiteFile(SuiteFileRef(f.absolutePath, f.name))
    }

    /** The suite file among the program's arguments. */
    fun suiteFileArg(args: Array<String>): String? = args.firstOrNull { it.endsWith(".ghlzip", ignoreCase = true) }
}
