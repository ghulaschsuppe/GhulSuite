// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.NewVault
import de.danerd.ghulsuite.core.SetupOptions
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.VaultStorage
import de.danerd.ghulsuite.core.generateKeyfile
import de.danerd.ghulsuite.core.localStorage
import de.danerd.ghulsuite.model.ArchiveContentUi
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.EmergencySheetText
import de.danerd.ghulsuite.model.KeyValueStore
import de.danerd.ghulsuite.model.MemorySink
import de.danerd.ghulsuite.model.Settings
import de.danerd.ghulsuite.model.SingleSaveUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * GhulDocs through the app's own backend code and the real core: documents in and out as a
 * stream (progress, cancel), a damaged document, a GhulDocs vault saved on its own, opened on its
 * own and written back into its file.
 */
class DocsBackendTest {
    private class Backend(dir: File) : CoreVaultBackend(File(dir, "local").absolutePath) {
        private val root = dir
        private val prefs = mutableMapOf<String, String>()
        override fun storage(): VaultStorage = localStorage(File(root, "suite").absolutePath)
        override suspend fun <T> withKey(block: (HidTransport) -> T): T = throw UnsupportedOperationException()
        override fun devicePref(key: String): String? = prefs[key]
        override fun setDevicePref(key: String, value: String) { prefs[key] = value }
        override fun kdfMaxMib(): Int = 64
        override fun openSuiteInput(ref: SuiteFileRef): SuiteInput? = SuiteFiles.input(File(ref.id))
        override fun writeSuiteStreamed(
            ref: SuiteFileRef,
            created: Boolean,
            write: (java.io.OutputStream) -> Unit,
            check: (SuiteInput) -> Boolean,
        ): Boolean = SuiteFiles.writeReplacing(File(ref.id), write, check)
        override val settings = Settings(object : KeyValueStore {
            private val map = mutableMapOf<String, Any>()
            override fun getInt(key: String, default: Int) = map[key] as? Int ?: default
            override fun putInt(key: String, value: Int) { map[key] = value }
            override fun getBoolean(key: String, default: Boolean) = map[key] as? Boolean ?: default
            override fun putBoolean(key: String, value: Boolean) { map[key] = value }
        })
        override val cal = FakeCal()
        override val appVersion = "test"
        override fun copySensitive(label: String, text: String) {}
        override fun printEmergencySheet(code: String, text: EmergencySheetText) {}
        override suspend fun keyInfo() = de.danerd.ghulsuite.model.KeyStatus("none", false, false, 4)
        override suspend fun setKeyPin(newPin: String) {}
        override fun panicLock() {
            panicking = true
            try { lock() } finally { panicking = false }
        }
        override fun biometricAvailable() = false
        override fun quickUnlockReady() = de.danerd.ghulsuite.model.QuickUnlockReady(false, false)
        override suspend fun enableBiometricUnlock() {}
        override suspend fun enablePinUnlock(pin: String) {}
        override fun disableQuickUnlock() {}
        override fun disableQuickUnlockMethod(bio: Boolean) {}
        override suspend fun unlockWithBiometric() {}
        override suspend fun unlockWithPin(pin: String) {}
    }

    private val password = "Master Passwort!"

    private fun device(dir: File, kf: ByteArray): Backend {
        GhulSuite(localStorage(File(dir, "suite").path), File(dir, "local").path).create(
            SetupOptions(password, kf, "Key", "Datei", "Code", 65536u, 3u, listOf(NewVault(VaultKind.DOCS, "Dokumente"))),
            null, null, null,
        )
        return Backend(dir).also { runBlocking { it.unlock(password, null, kf) {} } }
    }

    /** [data] as a file picked on the device, read [step] bytes at a time. */
    private class Bytes(override val name: String, private val data: ByteArray, private val step: Int = 256 * 1024) : ByteSource {
        override val size = data.size.toLong()
        private var pos = 0
        var reads = 0
        override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
            if (pos >= data.size) return -1
            reads++
            Thread.sleep(2)
            val n = minOf(length, step, data.size - pos)
            data.copyInto(buffer, offset, pos, pos + n)
            pos += n
            return n
        }
        override fun close() {}
    }

    private class Counting : ByteSink {
        val out = java.io.ByteArrayOutputStream()
        override fun write(data: ByteArray) = out.write(data)
        override fun close() {}
    }

    private fun random(n: Int) = ByteArray(n).also { java.util.Random(n.toLong()).nextBytes(it) }

    @Test
    fun documentsInAndOutAsAStream() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-docs").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val vault = a.listVaults().single { it.kind == VaultKindUi.DOCS }
            val folder = a.docs.createFolder(vault.id, null, "Steuer 2025")
            val data = random(5_000_000)
            val steps = mutableListOf<Long>()
            val item = a.docs.add(vault.id, folder.id, "Bescheid.pdf", "application/pdf", Bytes("Bescheid.pdf", data)) { steps += it }
            assertEquals(data.size.toLong(), item.size)
            assertEquals("application/pdf", item.mime)
            assertEquals("Steuer 2025", item.path)
            assertTrue(steps.isNotEmpty() && steps.last() == data.size.toLong(), steps.toString())
            assertEquals(listOf("Steuer 2025"), a.docs.list(vault.id, null).map { it.name })
            assertEquals(listOf("Bescheid.pdf"), a.docs.list(vault.id, folder.id).map { it.name })
            // Read back: the same bytes, nothing of it written anywhere else.
            val sink = MemorySink()
            a.docs.read(vault.id, item.id, sink)
            assertContentEquals(data, sink.bytes())
            assertEquals(data.size.toLong(), a.docs.usage().used)
            assertTrue(a.docs.verify(vault.id).isEmpty())
            // No plaintext on the disk: only content files with the vault (the first bytes of the
            // document appear nowhere under the app's folders).
            val probe = data.copyOfRange(0, 64)
            val leaked = dir.walkTopDown().filter { it.isFile }.any { f -> f.readBytes().let { b -> indexOf(b, probe) >= 0 } }
            assertFalse(leaked, "plaintext of the document on the disk")
        } finally {
            dir.deleteRecursively()
        }
    }

    private fun indexOf(hay: ByteArray, needle: ByteArray): Int {
        outer@ for (i in 0..hay.size - needle.size) {
            for (j in needle.indices) if (hay[i + j] != needle[j]) continue@outer
            return i
        }
        return -1
    }

    /** Names from other apps are cleaned the way the core takes them, never refused (review G9 8). */
    @Test
    fun namesFromOtherAppsAreCleanedTheWayTheCoreTakesThem() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-docs-names").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val vault = a.listVaults().single { it.kind == VaultKindUi.DOCS }
            val long = "Ä".repeat(300) + "\uD83D\uDCC4".repeat(10) + ".pdf"
            val names = mapOf(
                "rechnung\u202Efdp.exe" to "rechnungfdp.exe",
                "\u2066Brief\u2069.pdf" to "Brief.pdf",
                "\u00A0\u3000Scan.pdf\u2003" to "Scan.pdf",
                "\u00A0\u3000" to "file",
                "a/b\\c\u0007.txt" to "a_b_c_.txt",
                ".." to "file",
                long to null,
            )
            for ((raw, expected) in names) {
                val clean = de.danerd.ghulsuite.ui.docName(raw)
                if (expected != null) assertEquals(expected, clean, raw)
                val item = a.docs.add(vault.id, null, clean, "", Bytes(clean, byteArrayOf(1, 2, 3)))
                assertTrue(item.name == clean || item.name.startsWith(clean.substringBeforeLast('.')), "${item.name} / $clean")
            }
            val cut = de.danerd.ghulsuite.ui.docName("x".repeat(254) + "\uD83D\uDCC4" + "y".repeat(10))
            assertTrue(cut.codePointCount(0, cut.length) <= 255)
            assertFalse(cut.last().isHighSurrogate(), "not between the halves of a character")
            a.docs.add(vault.id, null, cut, "", Bytes(cut, byteArrayOf(1)))
            val l = de.danerd.ghulsuite.ui.docName(long)
            assertTrue(l.endsWith(".pdf") && l.codePointCount(0, l.length) <= 255, l)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun cancelledAddKeepsNothing() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-docs-cancel").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val vault = a.listVaults().single { it.kind == VaultKindUi.DOCS }
            val src = Bytes("Gross.bin", random(20_000_000), step = 64 * 1024)
            val job = async { a.docs.add(vault.id, null, "Gross.bin", "", src) }
            while (src.reads < 5) delay(5)
            job.cancel()
            assertFailsWith<kotlinx.coroutines.CancellationException> { job.await() }
            assertTrue(a.docs.list(vault.id, null).isEmpty())
            assertEquals(0L, a.docs.usage().used)
            val blobs = dir.walkTopDown().filter { it.isFile && (it.name.endsWith(".ghlblob") || it.name.endsWith(".tmp")) }.toList()
            assertTrue(blobs.isEmpty(), "nothing stays: $blobs")
        } finally {
            dir.deleteRecursively()
        }
    }

    /** A damaged document: the parts before the damage reached the sink, so the app must throw them away. */
    @Test
    fun damagedDocumentFailsAfterEarlierParts() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-docs-damaged").toFile()
        try {
            val a = device(File(dir, "a"), generateKeyfile())
            val vault = a.listVaults().single { it.kind == VaultKindUi.DOCS }
            val data = random(9_000_000)
            val item = a.docs.add(vault.id, null, "Scan.pdf", "application/pdf", Bytes("Scan.pdf", data))
            val blob = dir.walkTopDown().single { it.isFile && it.name.endsWith(".ghlblob") }
            // A byte in the second section (after 4 MiB) flipped.
            val bytes = blob.readBytes()
            val at = 26 + 24 + 4_194_304 + 16 + 24 + 1000
            bytes[at] = (bytes[at].toInt() xor 1).toByte()
            blob.writeBytes(bytes)
            val sink = Counting()
            val e = assertFailsWith<VaultException> { a.docs.read(vault.id, item.id, sink) }
            assertEquals(VaultErrorKind.CORRUPTED, e.kind)
            assertTrue(sink.out.size() > 0, "the first section came out before the damage was found")
            assertEquals(listOf(item.id), a.docs.verify(vault.id).map { it.id })
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun docsLimitIsMapped() {
        val e = GhulException.DocsLimit(412_000_000u, 1_000_000_000u).toVault()
        assertEquals(VaultErrorKind.DOCS_LIMIT, e.kind)
        assertEquals("412000000|1000000000", e.detail)
    }

    /**
     * A GhulDocs vault saved on its own: a .ghlzip that the app tells from a suite file, opened on
     * its own on another device (no suite there), changed, written back into its file as a stream,
     * and opened again with the change.
     */
    @Test
    fun singleDocsVaultIsWrittenBack() = runBlocking<Unit> {
        val dir = Files.createTempDirectory("ghul-docs-single").toFile()
        try {
            val kf = generateKeyfile()
            val a = device(File(dir, "a"), kf)
            val vault = a.listVaults().single { it.kind == VaultKindUi.DOCS }
            val first = random(300_000)
            a.docs.add(vault.id, null, "Erstes.pdf", "application/pdf", Bytes("Erstes.pdf", first))
            val file = File(dir, "Dokumente.ghlzip")
            java.io.FileOutputStream(file).use { out ->
                a.docs.exportVault(vault.id, object : ByteSink {
                    override fun write(data: ByteArray) = out.write(data)
                    override fun close() {}
                })
            }
            val ref = SuiteFileRef(file.absolutePath, file.name)
            assertEquals(ArchiveContentUi.DOCS_VAULT, a.archiveInspect(ref)?.content)
            val suiteFile = File(dir, "GhulSuite.ghlzip")
            assertTrue(a.archiveSave(SuiteFileRef(suiteFile.absolutePath, suiteFile.name), "Laptop", created = true))
            assertEquals(ArchiveContentUi.SUITE, a.archiveInspect(SuiteFileRef(suiteFile.absolutePath, suiteFile.name))?.content)

            val b = Backend(File(dir, "b"))
            assertEquals(VaultKindUi.DOCS, b.openSingleDocs(ref, password, null, kf, {}))
            assertTrue(b.singleIsDocs)
            val bv = b.listVaults().single()
            assertEquals(listOf("Erstes.pdf"), b.docs.list(bv.id, null).map { it.name })
            val second = random(200_000)
            b.docs.add(bv.id, null, "Zweites.txt", "text/plain", Bytes("Zweites.txt", second))
            assertTrue(b.archiveUnsaved())
            assertEquals(SingleSaveUi.SAVED, b.saveSingle())
            assertFalse(b.archiveUnsaved())
            b.lock()
            // The close writes nothing more (all was in the file) and leaves nothing behind.
            for (i in 0 until 100) {
                if (b.pendingSingleWrites().isEmpty()) break
                Thread.sleep(50)
            }
            assertTrue(b.pendingSingleWrites().isEmpty())

            val c = Backend(File(dir, "c"))
            c.openSingleDocs(ref, password, null, kf, {})
            val cv = c.listVaults().single()
            assertEquals(listOf("Erstes.pdf", "Zweites.txt"), c.docs.list(cv.id, null).map { it.name })
            val sink = MemorySink()
            c.docs.read(cv.id, c.docs.list(cv.id, null)[1].id, sink)
            assertContentEquals(second, sink.bytes())
            // The file was changed elsewhere since c opened it: c does not write over it.
            b.openSingleDocs(ref, password, null, kf, {})
            b.docs.createFolder(b.listVaults().single().id, null, "Neu")
            assertEquals(SingleSaveUi.SAVED, b.saveSingle())
            c.docs.createFolder(cv.id, null, "Anders")
            assertEquals(SingleSaveUi.CHANGED_ELSEWHERE, c.saveSingle())
            b.lock()
            c.panicLock()
        } finally {
            Thread.sleep(300)
            dir.deleteRecursively()
        }
    }
}
