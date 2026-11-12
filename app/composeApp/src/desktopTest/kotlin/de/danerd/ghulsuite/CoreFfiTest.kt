// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.coretest.FidoException
import de.danerd.ghulsuite.coretest.FidoListener
import de.danerd.ghulsuite.coretest.FidoStatus
import de.danerd.ghulsuite.coretest.HidTransport
import de.danerd.ghulsuite.coretest.TransportException
import de.danerd.ghulsuite.coretest.argon2Benchmark
import de.danerd.ghulsuite.coretest.coreVersion
import de.danerd.ghulsuite.coretest.fidoGetInfo
import de.danerd.ghulsuite.coretest.fidoRegister
import de.danerd.ghulsuite.coretest.fidoSetPin
import de.danerd.ghulsuite.coretest.fidoUnlockTest
import de.danerd.ghulsuite.coretest.mockAuthenticator
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

/**
 * Exercises the complete Kotlin ↔ Rust bridge: the HID transport is implemented in Kotlin
 * (like the Android USB transport) and forwards to the software authenticator in Rust.
 */
class CoreFfiTest {
    private class KotlinTransport(private val inner: HidTransport) : HidTransport {
        var writes = 0
        override fun reportSize() = inner.reportSize()
        override fun writeReport(report: ByteArray) {
            writes++
            inner.writeReport(report)
        }
        override fun readReport(timeoutMs: UInt) = inner.readReport(timeoutMs)
    }

    private class Listener : FidoListener {
        val events = mutableListOf<FidoStatus>()
        override fun onStatus(status: FidoStatus) {
            events += status
        }
    }

    @Test
    fun versionAndBenchmark() {
        assertContains(coreVersion(), "ghulsuite.invalid")
        assertTrue(argon2Benchmark(1024u, 1u, 1u) >= 0u)
        assertFailsWith<FidoException.InvalidInput> { argon2Benchmark(1u, 1u, 1u) }
    }

    @Test
    fun fullFlowThroughKotlinTransport() {
        val transport = KotlinTransport(mockAuthenticator("1234"))
        val listener = Listener()

        val info = fidoGetInfo(transport)
        assertTrue(info.supportsHmacSecret)
        assertTrue(info.pinSet)
        assertEquals(listOf(2u, 1u), info.pinProtocols)

        val reg = fidoRegister(transport, listener, "1234")
        assertTrue(reg.hmacSecretEnabled)
        assertEquals("ghulsuite.invalid", reg.rpId)

        val a = fidoUnlockTest(transport, listener, "1234", reg.credentialId, "test")
        val b = fidoUnlockTest(transport, listener, "1234", reg.credentialId, "test")
        val c = fidoUnlockTest(transport, listener, "1234", reg.credentialId, "other")
        assertEquals(a.keyFingerprint, b.keyFingerprint)
        assertEquals(a.kekFingerprint, b.kekFingerprint)
        assertEquals(a.keyFingerprint, c.keyFingerprint)
        assertNotEquals(a.kekFingerprint, c.kekFingerprint)
        assertTrue(a.userVerified)
        assertTrue(FidoStatus.TOUCH_NEEDED in listener.events)
        assertTrue(transport.writes > 10)

        val wrong = assertFailsWith<FidoException.PinInvalid> {
            fidoUnlockTest(transport, listener, "0000", reg.credentialId, "test")
        }
        assertEquals(7u, wrong.retries)
    }

    @Test
    fun setPinOnFreshKey() {
        val transport = KotlinTransport(mockAuthenticator(null))
        assertFailsWith<FidoException.PinNotSet> { fidoRegister(transport, Listener(), "1234") }
        fidoSetPin(transport, "246810")
        assertTrue(fidoGetInfo(transport).pinSet)
    }

    @Test
    fun timeoutAndTransportErrorsFromKotlin() {
        val silent = object : HidTransport {
            override fun reportSize() = 64u
            override fun writeReport(report: ByteArray) {}
            override fun readReport(timeoutMs: UInt): ByteArray? = null
        }
        assertFailsWith<FidoException.Timeout> { fidoGetInfo(silent) }

        val broken = object : HidTransport {
            override fun reportSize() = 64u
            override fun writeReport(report: ByteArray) = throw TransportException.Io("unplugged")
            override fun readReport(timeoutMs: UInt): ByteArray? = null
        }
        val e = assertFailsWith<FidoException.Transport> { fidoGetInfo(broken) }
        assertEquals("unplugged", e.msg)
    }
}
