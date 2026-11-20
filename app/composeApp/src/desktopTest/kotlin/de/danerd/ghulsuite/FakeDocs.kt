// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DocsBackend
import de.danerd.ghulsuite.model.DocsItemUi
import de.danerd.ghulsuite.model.DocsUsageUi
import de.danerd.ghulsuite.model.ShareVaultUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import kotlinx.coroutines.delay
import kotlinx.coroutines.yield

/**
 * GhulDocs as the core behaves (docs/FORMAT.md section 11): folders first, names unique in a
 * folder (a taken name of a new file gets a number), the trash with what is in a folder in it,
 * the 1 GB limit before anything is read when the size is known, else as soon as it is too much
 * (nothing stays), reading in sections where a damaged document fails after earlier sections
 * reached the sink. Every call waits a little, like the core on a phone; a cancelled transfer
 * stops at its next section.
 */
class FakeDocs : DocsBackend {
    /** What the UI asked to change (the UI tests check these). */
    val calls = java.util.Collections.synchronizedList(mutableListOf<String>())
    var limit = 1_000_000_000L
    /** Wait of every call (ms) and of every section of a transfer. */
    var delayMs = 20L
    var sectionDelayMs = 30L
    /** Bytes of a section of a transfer (the core's 4 MiB, smaller here so tests see the steps). */
    var section = 64 * 1024

    class Node(
        val vaultId: String,
        val id: String,
        var parent: String?,
        var name: String,
        val isFolder: Boolean,
        var data: ByteArray = ByteArray(0),
        var mime: String = "",
        val created: Long = 1_762_800_000_000,
        var modified: Long = 1_762_800_000_000,
        var favorite: Boolean = false,
        var trashed: Long? = null,
        /** The content file is damaged: reading fails after the first section. */
        var damaged: Boolean = false,
    )

    val nodes = java.util.Collections.synchronizedList(mutableListOf<Node>())
    private var next = 100

    private fun newId(): String = (next++).toString().padStart(32, '0')

    init {
        sample()
    }

    /** The sample documents of the screenshots (vault d1 "Dokumente"). */
    fun sample() {
        nodes.clear()
        val v = "d1"
        fun folder(id: String, name: String, parent: String? = null) = Node(v, id, parent, name, true).also { nodes += it }
        fun file(id: String, name: String, mime: String, data: ByteArray, parent: String? = null, fav: Boolean = false) =
            Node(v, id, parent, name, false, data, mime, favorite = fav).also { nodes += it }
        folder("f1", "Steuer 2025")
        folder("f2", "Versicherungen")
        file("a1", "Mietvertrag.pdf", "application/pdf", samplePdf(2), fav = true)
        file("a2", "Personalausweis.png", "image/png", samplePng())
        file("a3", "Notizen.txt", "text/plain", "Zählerstand Strom: 4711\nWasser: 815\n".encodeToByteArray())
        file("a4", "Lohnsteuerbescheinigung.pdf", "application/pdf", samplePdf(1), parent = "f1")
        file("a5", "Hausrat.docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", ByteArray(48_000) { 7 }, parent = "f2")
        file("a6", "Alter Scan.pdf", "application/pdf", samplePdf(1)).trashed = 1_762_900_000_000
    }

    private fun byId(vaultId: String, id: String) = synchronized(nodes) { nodes.firstOrNull { it.vaultId == vaultId && it.id == id } }
        ?: throw VaultException(VaultErrorKind.NOT_FOUND)

    private fun inTrash(n: Node): Boolean {
        var x: Node? = n
        while (x != null) {
            if (x.trashed != null) return true
            x = x.parent?.let { p -> nodes.firstOrNull { it.vaultId == n.vaultId && it.id == p } }
        }
        return false
    }

    private fun path(n: Node): String {
        val names = mutableListOf<String>()
        var p = n.parent
        while (p != null) {
            val f = nodes.firstOrNull { it.vaultId == n.vaultId && it.id == p } ?: break
            names.add(0, f.name)
            p = f.parent
        }
        return names.joinToString("/")
    }

    private fun ui(n: Node) = DocsItemUi(
        n.vaultId, n.id, n.parent, n.name, n.isFolder, n.data.size.toLong(), n.mime, n.created, n.modified, n.favorite, n.trashed,
        path(n), if (n.isFolder) nodes.count { it.vaultId == n.vaultId && it.parent == n.id && it.trashed == null } else 0,
    )

    private fun sorted(list: List<Node>) = list.sortedWith(compareBy<Node>({ !it.isFolder }, { it.name.lowercase() })).map { ui(it) }

    private fun taken(vaultId: String, folder: String?, name: String, except: String? = null) = nodes.any {
        it.vaultId == vaultId && it.parent == folder && it.id != except && !inTrash(it) && it.name.equals(name, ignoreCase = true)
    }

    private fun free(vaultId: String, folder: String?, name: String): String {
        if (!taken(vaultId, folder, name)) return name
        val dot = name.lastIndexOf('.').takeIf { it > 0 } ?: name.length
        var i = 2
        while (true) {
            val n = name.substring(0, dot) + " ($i)" + name.substring(dot)
            if (!taken(vaultId, folder, n)) return n
            i++
        }
    }

    private fun checkName(name: String): String {
        val n = name.trim()
        if (n.isEmpty() || n.contains('/') || n.contains('\\')) throw VaultException(VaultErrorKind.INVALID_INPUT, "invalid name")
        return n
    }

    private fun used(): Long = synchronized(nodes) { nodes.filter { !it.isFolder }.sumOf { it.data.size.toLong() } }

    override suspend fun list(vaultId: String, folder: String?): List<DocsItemUi> {
        delay(delayMs)
        if (folder != null) {
            val f = byId(vaultId, folder)
            if (!f.isFolder || inTrash(f)) throw VaultException(VaultErrorKind.NOT_FOUND)
        }
        return synchronized(nodes) { sorted(nodes.filter { it.vaultId == vaultId && it.parent == folder && it.trashed == null }) }
    }

    override suspend fun item(vaultId: String, id: String): DocsItemUi {
        delay(delayMs)
        return ui(byId(vaultId, id))
    }

    override suspend fun search(vaultId: String?, query: String): List<DocsItemUi> {
        delay(delayMs)
        val q = query.trim().lowercase()
        if (q.isEmpty()) return emptyList()
        return synchronized(nodes) { sorted(nodes.filter { (vaultId == null || it.vaultId == vaultId) && !inTrash(it) && it.name.lowercase().contains(q) }) }
    }

    override suspend fun favorites(vaultId: String?): List<DocsItemUi> {
        delay(delayMs)
        return synchronized(nodes) { sorted(nodes.filter { (vaultId == null || it.vaultId == vaultId) && it.favorite && !inTrash(it) }) }
    }

    override suspend fun trash(vaultId: String?): List<DocsItemUi> {
        delay(delayMs)
        return synchronized(nodes) {
            nodes.filter { (vaultId == null || it.vaultId == vaultId) && it.trashed != null }
                .filter { n -> n.parent?.let { p -> nodes.firstOrNull { it.vaultId == n.vaultId && it.id == p } }?.let { inTrash(it) } != true }
                .sortedByDescending { it.trashed }.map { ui(it) }
        }
    }

    override suspend fun createFolder(vaultId: String, folder: String?, name: String): DocsItemUi {
        delay(delayMs)
        val n = checkName(name)
        if (taken(vaultId, folder, n)) throw VaultException(VaultErrorKind.ALREADY_EXISTS)
        calls += "createFolder:$vaultId:$folder:$n"
        return ui(Node(vaultId, newId(), folder, n, true).also { nodes += it })
    }

    override suspend fun rename(vaultId: String, id: String, name: String): DocsItemUi {
        delay(delayMs)
        val x = byId(vaultId, id)
        val n = checkName(name)
        if (taken(vaultId, x.parent, n, except = id)) throw VaultException(VaultErrorKind.ALREADY_EXISTS)
        calls += "rename:$id:$n"
        x.name = n
        return ui(x)
    }

    override suspend fun move(vaultId: String, id: String, folder: String?): DocsItemUi {
        delay(delayMs)
        val x = byId(vaultId, id)
        var p = folder
        while (p != null) {
            if (p == id) throw VaultException(VaultErrorKind.INVALID_INPUT, "a folder cannot go into itself")
            p = byId(vaultId, p).parent
        }
        if (taken(vaultId, folder, x.name, except = id)) throw VaultException(VaultErrorKind.ALREADY_EXISTS)
        calls += "move:$id:$folder"
        x.parent = folder
        return ui(x)
    }

    override suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) {
        delay(delayMs)
        calls += "favorite:$id:$favorite"
        byId(vaultId, id).favorite = favorite
    }

    override suspend fun toTrash(vaultId: String, id: String) {
        delay(delayMs)
        calls += "toTrash:$id"
        byId(vaultId, id).trashed = System.currentTimeMillis()
    }

    override suspend fun restore(vaultId: String, id: String): DocsItemUi {
        delay(delayMs)
        val x = byId(vaultId, id)
        calls += "restore:$id"
        val parentOk = x.parent?.let { p -> nodes.firstOrNull { it.vaultId == vaultId && it.id == p }?.let { !inTrash(it) } } ?: true
        if (!parentOk) x.parent = null
        x.trashed = null
        x.name = free(vaultId, x.parent, x.name).let { if (it == x.name || !taken(vaultId, x.parent, x.name, except = id)) x.name else it }
        return ui(x)
    }

    private fun subtree(vaultId: String, id: String): List<Node> {
        val out = mutableListOf(byId(vaultId, id))
        var i = 0
        while (i < out.size) {
            val f = out[i++]
            out += nodes.filter { it.vaultId == vaultId && it.parent == f.id }
        }
        return out
    }

    override suspend fun deleteForever(vaultId: String, id: String) {
        delay(delayMs)
        calls += "deleteForever:$id"
        val gone = subtree(vaultId, id).toSet()
        nodes.removeAll(gone)
    }

    override suspend fun emptyTrash(vaultId: String?) {
        delay(delayMs)
        calls += "emptyTrash:$vaultId"
        synchronized(nodes) { nodes.removeAll(nodes.filter { (vaultId == null || it.vaultId == vaultId) && inTrash(it) }.toSet()) }
    }

    override suspend fun add(vaultId: String, folder: String?, name: String, mime: String, source: ByteSource, onProgress: (Long) -> Unit): DocsItemUi {
        delay(delayMs)
        val n = checkName(name)
        val have = used()
        // Known size: refused before anything is read.
        if (source.size >= 0 && have + source.size > limit) throw VaultException(VaultErrorKind.DOCS_LIMIT, "$have|$limit|${source.size}")
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(section)
        while (true) {
            // The next section waits a little; a cancelled job stops here and nothing stays.
            delay(sectionDelayMs)
            yield()
            val r = source.read(buf, 0, buf.size)
            if (r < 0) break
            out.write(buf, 0, r)
            if (have + out.size() > limit) throw VaultException(VaultErrorKind.DOCS_LIMIT, "$have|$limit|0")
            onProgress(out.size().toLong())
        }
        calls += "add:$vaultId:$folder:$n:$mime:${out.size()}"
        val node = Node(vaultId, newId(), folder, free(vaultId, folder, n), false, out.toByteArray(), mime)
        nodes += node
        return ui(node)
    }

    override suspend fun read(vaultId: String, id: String, sink: ByteSink, onProgress: (Long) -> Unit) {
        delay(delayMs)
        val x = byId(vaultId, id)
        if (x.isFolder) throw VaultException(VaultErrorKind.INVALID_INPUT, "a folder has no content")
        calls += "read:$id"
        var pos = 0
        do {
            delay(sectionDelayMs)
            // A damaged document: the sections before reached the sink already.
            if (x.damaged && pos > 0) throw VaultException(VaultErrorKind.CORRUPTED, "a document is damaged or missing")
            val end = minOf(x.data.size, pos + section)
            sink.write(x.data.copyOfRange(pos, end))
            pos = end
            onProgress(pos.toLong())
        } while (pos < x.data.size)
        if (x.damaged) throw VaultException(VaultErrorKind.CORRUPTED, "a document is damaged or missing")
    }

    override suspend fun usage(): DocsUsageUi {
        delay(delayMs)
        return DocsUsageUi(used(), limit, nodes.count { !it.isFolder })
    }

    /** Ids the next check reports as damaged. */
    var badOnVerify = emptySet<String>()

    override suspend fun verify(vaultId: String): List<DocsItemUi> {
        delay(delayMs)
        calls += "verify:$vaultId"
        return synchronized(nodes) { sorted(nodes.filter { it.vaultId == vaultId && (it.id in badOnVerify || it.damaged) }) }
    }

    override suspend fun exportVault(vaultId: String, sink: ByteSink, onProgress: (Long, Long) -> Unit) {
        delay(delayMs)
        calls += "exportVault:$vaultId"
        sink.write("GHULZIP".encodeToByteArray())
    }

    override suspend fun shareVault(share: ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit, sink: ByteSink, onProgress: (Long, Long) -> Unit): String {
        delay(delayMs)
        calls += "shareVault:${share.vaultId}:${share.ids.joinToString(",")}:${share.name}"
        sink.write("GHULZIP".encodeToByteArray())
        return "AAAAA-BBBBB-CCCCC-DDDDD-EEEEE-FFFFF-GGGGG-HHHHH-JJJJJ-KKKKK-MMMMM"
    }

    override suspend fun adoptVault(
        file: SuiteFileRef, name: String, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): String {
        delay(delayMs)
        // As the core: the package's documents count towards the limit, before anything is copied.
        val have = used()
        if (have + packageSize > limit) throw VaultException(VaultErrorKind.DOCS_LIMIT, "$have|$limit|$packageSize")
        calls += "adoptVault:${file.name}:$name"
        return "d2"
    }

    /** Bytes of the documents in a GhulDocs package to be added (the limit counts them). */
    var packageSize = 1000L

    override suspend fun replaceVault(
        file: SuiteFileRef, name: String, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): String {
        delay(delayMs)
        // As the core: the package alone counts (the old vault goes); refused, nothing changes.
        if (packageSize > limit) throw VaultException(VaultErrorKind.DOCS_LIMIT, "0|$limit|$packageSize")
        calls += "replaceVault:${file.name}:$name"
        synchronized(nodes) {
            nodes.removeAll { it.vaultId == "d1" }
            nodes += Node("d1", newId(), null, "Aus dem Paket.pdf", false, ByteArray(packageSize.toInt()), "application/pdf")
        }
        return "d1"
    }

    companion object {
        /** A small PNG (a coloured card) made in memory. */
        fun samplePng(): ByteArray {
            val surface = org.jetbrains.skia.Surface.makeRasterN32Premul(320, 200)
            surface.canvas.clear(0xFFE8F0FE.toInt())
            surface.canvas.drawRect(org.jetbrains.skia.Rect.makeXYWH(20f, 20f, 120f, 160f), org.jetbrains.skia.Paint().apply { color = 0xFF2F74E8.toInt() })
            surface.canvas.drawRect(org.jetbrains.skia.Rect.makeXYWH(160f, 40f, 140f, 16f), org.jetbrains.skia.Paint().apply { color = 0xFF6F7187.toInt() })
            surface.canvas.drawRect(org.jetbrains.skia.Rect.makeXYWH(160f, 80f, 100f, 16f), org.jetbrains.skia.Paint().apply { color = 0xFF6F7187.toInt() })
            return surface.makeImageSnapshot().encodeToData(org.jetbrains.skia.EncodedImageFormat.PNG)!!.bytes
        }

        /** A PDF of [pages] pages with a line of text each, made in memory. */
        fun samplePdf(pages: Int): ByteArray {
            val doc = org.apache.pdfbox.pdmodel.PDDocument()
            doc.use {
                repeat(pages) { i ->
                    val page = org.apache.pdfbox.pdmodel.PDPage(org.apache.pdfbox.pdmodel.common.PDRectangle.A4)
                    doc.addPage(page)
                    org.apache.pdfbox.pdmodel.PDPageContentStream(doc, page).use { cs ->
                        cs.setNonStrokingColor(0.18f, 0.45f, 0.91f)
                        cs.addRect(56f, 700f, 480f, 60f)
                        cs.fill()
                        cs.beginText()
                        cs.setFont(org.apache.pdfbox.pdmodel.font.PDType1Font(org.apache.pdfbox.pdmodel.font.Standard14Fonts.FontName.HELVETICA), 18f)
                        cs.setNonStrokingColor(0f, 0f, 0f)
                        cs.newLineAtOffset(56f, 650f)
                        cs.showText("GhulDocs page ${i + 1}")
                        cs.endText()
                    }
                }
                val out = java.io.ByteArrayOutputStream()
                doc.save(out)
                return out.toByteArray()
            }
        }
    }
}
