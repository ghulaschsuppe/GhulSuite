// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import java.io.File
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** A second start hands its suite file to the running GhulSuite; without the token nothing. */
class SingleInstanceTest {
    @Test
    fun aSecondStartHandsItsFileToTheRunningOne() {
        val got = LinkedBlockingQueue<String>()
        SingleInstance.listen { got.add(it ?: "<none>") }
        assertTrue(SingleInstance.handOver("C:\\Daten\\GhulSuite.ghlzip"))
        assertEquals("C:\\Daten\\GhulSuite.ghlzip", got.poll(5, TimeUnit.SECONDS))
        assertTrue(SingleInstance.handOver(null))
        assertEquals("<none>", got.poll(5, TimeUnit.SECONDS))

        // A wrong token (another program guessing the port) is not taken.
        val portFile = File(DesktopPaths.dataDir, "instance.port")
        val real = portFile.readText()
        try {
            portFile.writeText(real.substringBefore(' ') + " 00")
            assertFalse(SingleInstance.handOver("C:\\boese.ghlzip"))
            assertEquals(null, got.poll(1, TimeUnit.SECONDS))
        } finally {
            portFile.writeText(real)
        }
    }

    /** The data folder belongs to the first start; a second one (also while the first starts) does not get it. */
    @Test
    fun theDataFolderBelongsToTheFirstStart() {
        val dir = java.nio.file.Files.createTempDirectory("ghul-instance").toFile()
        try {
            assertTrue(SingleInstance.claim(dir))
            assertTrue(SingleInstance.claim(dir), "the same program again")
            SingleInstance.release(dir)
            assertTrue(SingleInstance.claim(dir), "free again")
            // Another program (another JVM) does not get it while this one has it.
            val java = File(System.getProperty("java.home"), "bin/java").path
            val probe = File(dir, "Probe.java").apply {
                writeText(
                    """
                    public class Probe { public static void main(String[] a) throws Exception {
                      try (var c = java.nio.channels.FileChannel.open(java.nio.file.Path.of(a[0]),
                          java.nio.file.StandardOpenOption.CREATE, java.nio.file.StandardOpenOption.WRITE)) {
                        System.out.print(c.tryLock() == null ? "taken" : "free");
                      }
                    } }
                    """.trimIndent(),
                )
            }
            val lockPath = File(dir, "instance.lock").path
            fun probe(): String {
                val p = ProcessBuilder(java, probe.path, lockPath).redirectError(ProcessBuilder.Redirect.DISCARD).start()
                assertTrue(p.waitFor(60, TimeUnit.SECONDS))
                return p.inputStream.readBytes().toString(Charsets.UTF_8).trim()
            }
            assertEquals("taken", probe())
            SingleInstance.release(dir)
            assertEquals("free", probe())
        } finally {
            SingleInstance.release(dir)
            dir.deleteRecursively()
        }
    }

    /** A connection sends at most 8 KiB, and only for a few seconds (no endless line). */
    @Test
    fun aHandOverIsReadOnlyUpToItsLimit() {
        val far = System.currentTimeMillis() + 60_000
        fun line(text: String, used: IntArray = IntArray(1)) =
            SingleInstance.readLimitedLine(text.byteInputStream(), SingleInstance.MAX_HANDOVER, used, far)
        assertEquals("token", line("token\r\nrest"))
        assertEquals("C:\\Daten\\a.ghlzip", line("C:\\Daten\\a.ghlzip\n"))
        assertEquals(null, line("x".repeat(SingleInstance.MAX_HANDOVER + 1) + "\n"), "too long")
        assertEquals(null, line("no end"), "the connection ends first")
        // Both lines together count.
        val used = IntArray(1)
        assertEquals("a".repeat(5000), line("a".repeat(5000) + "\n", used))
        assertEquals(null, line("b".repeat(5000) + "\n", used))
        // An endless line ends at the limit, not when memory runs out.
        val endless = object : java.io.InputStream() {
            override fun read(): Int = 'x'.code
        }
        assertEquals(null, SingleInstance.readLimitedLine(endless, SingleInstance.MAX_HANDOVER, IntArray(1), far))
        assertEquals(null, SingleInstance.readLimitedLine(endless, Int.MAX_VALUE, IntArray(1), System.currentTimeMillis() - 1), "too late")
    }

    @Test
    fun onlySuiteFilesAreTakenFromTheArguments() {
        assertEquals("a.GHLZIP", SingleInstance.suiteFileArg(arrayOf("-x", "a.GHLZIP")))
        assertEquals("C:\\Daten\\Steuer.ghldocs", SingleInstance.suiteFileArg(arrayOf("C:\\Daten\\Steuer.ghldocs")))
        assertEquals(null, SingleInstance.suiteFileArg(arrayOf("notiz.txt")))
    }
}
