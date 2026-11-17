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

    @Test
    fun onlySuiteFilesAreTakenFromTheArguments() {
        assertEquals("a.GHLZIP", SingleInstance.suiteFileArg(arrayOf("-x", "a.GHLZIP")))
        assertEquals(null, SingleInstance.suiteFileArg(arrayOf("notiz.txt")))
    }
}
