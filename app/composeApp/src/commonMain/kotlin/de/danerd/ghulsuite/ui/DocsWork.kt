// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DocsItemUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException

// The long work of GhulDocs, each as a background job (see [Jobs]): adding files, exporting a
// document or a vault, a copy for another app. Started from a screen, they go on when the screen
// is left; the strip under the header shows them with their progress and "Cancel".

/** Media types of the usual documents, by the ending of their name (when the system names none). */
private val MIME_BY_EXT = mapOf(
    "pdf" to "application/pdf",
    "jpg" to "image/jpeg", "jpeg" to "image/jpeg", "png" to "image/png", "gif" to "image/gif", "webp" to "image/webp",
    "heic" to "image/heic", "heif" to "image/heif", "bmp" to "image/bmp", "tif" to "image/tiff", "tiff" to "image/tiff",
    "txt" to "text/plain", "md" to "text/markdown", "csv" to "text/csv", "log" to "text/plain", "json" to "application/json",
    "xml" to "application/xml", "html" to "text/html", "htm" to "text/html", "ics" to "text/calendar", "vcf" to "text/vcard",
    "doc" to "application/msword", "docx" to "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "xls" to "application/vnd.ms-excel", "xlsx" to "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "odt" to "application/vnd.oasis.opendocument.text", "ods" to "application/vnd.oasis.opendocument.spreadsheet",
    "zip" to "application/zip", "mp4" to "video/mp4", "mov" to "video/quicktime", "mp3" to "audio/mpeg", "m4a" to "audio/mp4",
)

/** The media type of a file by its name (application/octet-stream when the ending is not known). */
fun mimeOf(name: String): String = MIME_BY_EXT[name.substringAfterLast('.', "").lowercase()] ?: "application/octet-stream"

/** Characters that turn the direction of text (e.g. "rechnung\u202Efdp.exe" shows as "rechnungexe.pdf"). */
private fun isBidiControl(c: Char): Boolean = c in '\u202A'..'\u202E' || c in '\u2066'..'\u2069'

/** White space as the core trims it (Unicode White_Space, like Rust's `str::trim`). */
private fun isCoreSpace(c: Char): Boolean = c in '\u0009'..'\u000D' || c == ' ' || c == '\u0085' || c == '\u00A0' ||
    c == '\u1680' || c in '\u2000'..'\u200A' || c == '\u2028' || c == '\u2029' || c == '\u202F' || c == '\u205F' || c == '\u3000'

/**
 * A name GhulDocs keeps, made the way the core accepts it (`check_name`): slashes and control
 * characters become "_", direction controls are removed, trimmed like the core trims, at most 255
 * characters (the ending kept), never empty, "." or "..".
 */
fun docName(name: String): String {
    val clean = name.filterNot(::isBidiControl)
        .map { if (it == '/' || it == '\\' || it.isISOControl()) '_' else it }.joinToString("")
        .trim(::isCoreSpace)
    if (clean.isEmpty() || clean == "." || clean == "..") return "file"
    if (clean.coreLength() <= 255) return clean
    val dot = clean.lastIndexOf('.').takeIf { it > 0 && clean.length - it <= 16 } ?: clean.length
    var end = 255 - (clean.length - dot)
    // Not between the two halves of a character outside the BMP.
    if (end > 0 && clean[end - 1].isHighSurrogate()) end--
    val cut = (clean.substring(0, end).trimEnd(::isCoreSpace) + clean.substring(dot)).trim(::isCoreSpace)
    return cut.ifEmpty { "file" }
}

/** How many characters (code points) the core counts. */
private fun String.coreLength(): Int = length - count { it.isLowSurrogate() }

/** Whether the app can show a document itself (image, PDF, text). */
fun docViewable(item: DocsItemUi): Boolean = !item.isFolder && docViewKind(item) != DocViewKind.NONE

enum class DocViewKind { IMAGE, PDF, TEXT, NONE }

fun docViewKind(item: DocsItemUi): DocViewKind {
    val m = item.mime.lowercase()
    val ext = item.name.substringAfterLast('.', "").lowercase()
    return when {
        m == "application/pdf" || ext == "pdf" -> DocViewKind.PDF
        m.startsWith("image/") && m != "image/svg+xml" -> DocViewKind.IMAGE
        m.startsWith("text/") || m == "application/json" || m == "application/xml" ||
            ext in setOf("txt", "md", "csv", "log", "json", "xml") -> DocViewKind.TEXT
        else -> DocViewKind.NONE
    }
}

/** "GhulDocs is full: 412 MB of 1 GB used …" in the language of the screens. */
private suspend fun AppState.limitText(e: VaultException): String {
    val used = e.detail.substringBefore('|').toLongOrNull() ?: 0
    val limit = e.detail.substringAfter('|').toLongOrNull() ?: 1_000_000_000
    return if (used > 0) str(Res.string.err_docs_limit, formatSizeSi(used), formatSizeSi(limit))
    else str(Res.string.err_docs_too_large, formatSizeSi(limit))
}

/**
 * Adds files to a GhulDocs folder, one after the other, each encrypted while it is read. A failed
 * file does not stop the others (the message names it); when GhulDocs is full the rest is not
 * tried (each would fail the same way). "Cancel" stops the file being read (nothing of it stays)
 * and the rest. Every file is closed in the end. Returns the job (null: nothing to add).
 */
fun AppState.addDocs(vaultId: String, folder: String?, sources: List<ByteSource>): BgJob? {
    if (sources.isEmpty()) return null
    val docs = backend.docs
    val one = sources.singleOrNull()
    return jobs.start(
        JobKind.ADD,
        title = { if (one != null) str(Res.string.docs_adding, docName(one.name)) else plural(Res.plurals.docs_adding_n, sources.size, sources.size.toString()) },
        cancelled = { j ->
            if (one != null) str(Res.string.docs_add_cancelled_one, docName(one.name))
            else plural(Res.plurals.docs_add_cancelled_n, sources.size, j.done.toString(), sources.size.toString())
        },
        onEnd = { sources.forEach { runCatching { it.close() } } },
    ) { job ->
        job.total = sources.size
        var ok = 0
        val failed = mutableListOf<String>()
        var failure: String? = null
        var full: String? = null
        for (s in sources) {
            val name = docName(s.name)
            if (full != null) {
                failed += name
                job.done++
                continue
            }
            job.item = name
            job.bytes = 0
            job.bytesTotal = s.size.coerceAtLeast(0)
            try {
                docs.add(vaultId, folder, name, s.mime.ifBlank { mimeOf(name) }, s) { job.bytes = it }
                ok++
                // The list and the usage follow after each file.
                changed()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val limit = (e as? VaultException)?.takeIf { it.kind == VaultErrorKind.DOCS_LIMIT }?.let { limitText(it) }
                // One file alone: its error is the message.
                if (one != null) throw if (limit != null) JobFailed(limit) else e
                failed += name
                if (limit != null) full = limit else failure = describe(e)
            } finally {
                runCatching { s.close() }
            }
            job.done++
        }
        if (one != null) return@start str(Res.string.docs_added, docName(one.name))
        // Every file that was not added is named, with the reason (GhulDocs full first).
        listOfNotNull(
            if (ok > 0) plural(Res.plurals.docs_added_n, ok, ok.toString()) else null,
            (full ?: failure)?.let { failed.joinToString(", ") + ": " + it },
        ).joinToString(" ")
    }
}

/**
 * Writes a document decrypted into [sink] (a file the user chose and confirmed). Cancelled or
 * failed (a damaged document), the half written file is removed again.
 */
fun AppState.exportDoc(item: DocsItemUi, sink: ByteSink) {
    var complete = false
    jobs.start(
        JobKind.EXPORT,
        title = { str(Res.string.docs_exporting, item.name) },
        cancelled = { str(Res.string.docs_export_cancelled, item.name) },
        onEnd = {
            runCatching { sink.close() }
            if (!complete) runCatching { sink.discard() }
        },
    ) { job ->
        job.item = item.name
        job.bytesTotal = item.size
        backend.docs.read(item.vaultId, item.id, sink) { job.bytes = it }
        sink.close()
        // The message first: cancelled while it is looked up, the file goes (and so says the message).
        val done = str(Res.string.docs_exported, item.name)
        complete = true
        done
    }
}

/**
 * Hands a document to another app (Android: the share sheet; Windows: the program registered for
 * it) through a temporary copy that is wiped when the app locks. A read that fails removes the
 * half copy and hands nothing over.
 */
fun AppState.openDocWith(item: DocsItemUi) {
    jobs.start(
        JobKind.EXPORT,
        title = { str(Res.string.docs_opening, item.name) },
        cancelled = { str(Res.string.docs_open_cancelled, item.name) },
    ) { job ->
        job.item = item.name
        job.bytesTotal = item.size
        val ok = backend.shareStreamToApps(item.name, item.mime.ifBlank { mimeOf(item.name) }) { sink ->
            backend.docs.read(item.vaultId, item.id, sink) { job.bytes = it }
        }
        if (ok) null else str(Res.string.docs_no_app)
    }
}

/**
 * Saves the GhulDocs vault with its documents as a GhulDocs package (`.ghldocs`) into [sink] (this suite's protection).
 * Cancelled or failed, the half written file is removed.
 */
fun AppState.exportDocsVault(vault: VaultUi, sink: ByteSink) {
    var complete = false
    jobs.start(
        JobKind.EXPORT,
        title = { str(Res.string.docs_vault_exporting, vault.name) },
        cancelled = { str(Res.string.docs_vault_export_cancelled, vault.name) },
        onEnd = {
            runCatching { sink.close() }
            if (!complete) runCatching { sink.discard() }
        },
    ) { job ->
        job.item = vault.name
        backend.docs.exportVault(vault.id, sink) { d, t ->
            job.bytes = d
            job.bytesTotal = t
        }
        sink.close()
        val done = str(Res.string.msg_exported)
        complete = true
        done
    }
}
