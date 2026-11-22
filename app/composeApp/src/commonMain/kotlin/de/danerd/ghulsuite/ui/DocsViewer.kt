// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.DriveFileMove
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.DocsItemUi
import de.danerd.ghulsuite.model.MemorySink
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.jetbrains.compose.resources.stringResource

/** Largest documents the app reads into memory to show them (beyond: open with an app or export). */
const val DOC_VIEW_MAX_IMAGE = 40L * 1024 * 1024
const val DOC_VIEW_MAX_PDF = 40L * 1024 * 1024
const val DOC_VIEW_MAX_TEXT = 2L * 1024 * 1024

/** Longest side of an image as it is shown (a photo of a phone camera is far larger than any screen). */
private const val IMAGE_MAX_SIDE = 4096

/** Characters of a text shown at most (the rest is left out, with a note). */
private const val TEXT_MAX_CHARS = 200_000

private fun maxFor(kind: DocViewKind): Long = when (kind) {
    DocViewKind.IMAGE -> DOC_VIEW_MAX_IMAGE
    DocViewKind.PDF -> DOC_VIEW_MAX_PDF
    DocViewKind.TEXT -> DOC_VIEW_MAX_TEXT
    DocViewKind.NONE -> 0
}

/** What the viewer shows: being read, read (one of the kinds), or why not. */
private sealed interface Shown {
    data object Reading : Shown
    class Picture(val image: ImageBitmap) : Shown
    class Pages(val pdf: PdfView) : Shown
    class Plain(val text: String, val cut: Boolean) : Shown
    /** No preview: [reason] says why (type, size, damaged). */
    class None(val reason: String) : Shown
}

/**
 * A document of GhulDocs, shown in the app: read into memory (decrypted part by part, every part
 * checked), never written to the disk. Images and PDF pages are drawn from memory, texts shown as
 * text. Leaving the page (also by locking) wipes what was read. Export and "Open with" are in the
 * header.
 */
@Composable
fun DocViewScreen(state: AppState, vaultId: String, id: String, padding: PaddingValues) {
    val docs = state.backend.docs
    var item by remember { mutableStateOf<DocsItemUi?>(null) }
    LaunchedEffect(id, state.liveRevision()) {
        catching { docs.item(vaultId, id) }.onSuccess { item = it }.onFailure {
            // Gone (deleted, in the trash elsewhere): back to the list with the reason.
            state.message(describe(it))
            state.pop()
        }
    }
    val it = item
    Column(Modifier.padding(padding)) {
        DocViewHeader(state, it)
        if (it == null) {
            Column(Modifier.padding(16.dp)) { LoadingHint() }
            return@Column
        }
        key(it.id, it.size) { DocBody(state, it) }
    }
}

@Composable
private fun DocViewHeader(state: AppState, item: DocsItemUi?) {
    var menu by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    var askExport by remember { mutableStateOf(false) }
    val docs = state.backend.docs
    /** The document whose export waits for the system's save dialog. */
    val exporting = remember { arrayOfNulls<DocsItemUi>(1) }
    val create = rememberCreateStream(onFailed = { state.reportSaveFile(SaveFileResult.FAILED) }) { sink: ByteSink? ->
        val x = exporting[0]
        exporting[0] = null
        if (sink == null) return@rememberCreateStream
        if (x == null) {
            // Nothing to write into the file just made (recreated while the dialog was open): no
            // empty file is left behind, and it is said.
            runCatching { sink.close() }
            runCatching { sink.discard() }
            state.reportSaveFile(SaveFileResult.FAILED)
            return@rememberCreateStream
        }
        state.exportDoc(x, sink)
    }
    val trashMsg = stringResource(Res.string.msg_moved_to_trash)
    GhulTopBar(
        item?.name.orEmpty(), onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = backIcon(),
        actions = {
            if (item != null) {
                BarIcon(if (item.favorite) Icons.Filled.Star else Icons.Filled.StarBorder, stringResource(Res.string.chip_favorites),
                    tint = if (item.favorite) starColor() else Color.Unspecified) {
                    state.run(block = { docs.setFavorite(item.vaultId, item.id, !item.favorite) }) { state.changed() }
                }
                BarIcon(Icons.AutoMirrored.Filled.OpenInNew, stringResource(Res.string.docs_open_with)) { state.openDocWith(item) }
                BarIcon(Icons.Filled.MoreVert, stringResource(Res.string.a_more)) { menu = true }
            }
        },
    )
    if (item == null) return
    if (menu) {
        GhulSheet({ menu = false }, item.name) {
            SheetList {
                SheetRow(stringResource(Res.string.docs_open_with), stringResource(Res.string.docs_open_with_sub),
                    leading = { IconTile(Icons.AutoMirrored.Filled.OpenInNew, g.accent) }) { menu = false; state.openDocWith(item) }
                if (filePickersAvailable) {
                    SheetRow(stringResource(Res.string.docs_export), stringResource(Res.string.docs_export_sub), divider = true,
                        leading = { IconTile(Icons.Filled.Download, g.accent) }) { menu = false; askExport = true }
                }
                SheetRow(stringResource(Res.string.a_rename), divider = true, leading = { IconTile(Icons.Filled.Edit, g.accent) }) { menu = false; renaming = true }
                SheetRow(stringResource(Res.string.a_move), divider = true, leading = { IconTile(Icons.AutoMirrored.Filled.DriveFileMove, g.accent) }) { menu = false; moving = true }
                SheetRow(stringResource(Res.string.a_to_trash), color = g.danger, divider = true, leading = { IconTile(Icons.Filled.Delete, g.danger) }) {
                    menu = false
                    state.askDelete(listOf(item.name), trash = true) {
                        state.pop()
                        state.undoable(listOf(hiddenKey("docs", "files", item.vaultId + "/" + item.id)), trashMsg) { docs.toTrash(item.vaultId, item.id) }
                    }
                }
            }
        }
    }
    if (askExport) {
        AlertDialog(
            onDismissRequest = { askExport = false },
            icon = { Icon(Icons.Filled.Warning, null, tint = warnColor()) },
            title = { Text(stringResource(Res.string.docs_export_title)) },
            text = { Text(stringResource(Res.string.docs_export_text, item.name)) },
            confirmButton = {
                TextButton(onClick = {
                    askExport = false
                    exporting[0] = item
                    create(item.name, item.mime.ifBlank { mimeOf(item.name) })
                }, modifier = Modifier.testTag("docs-export-confirm")) { Text(stringResource(Res.string.docs_export_yes)) }
            },
            dismissButton = { TextButton(onClick = { askExport = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (renaming) {
        NameDialog(stringResource(Res.string.a_rename), item.name, onDismiss = { renaming = false }) { name ->
            renaming = false
            state.run(block = { docs.rename(item.vaultId, item.id, name) }) { state.changed() }
        }
    }
    if (moving) {
        FolderPickerDialog(
            state, fixedVault = item.vaultId, title = stringResource(Res.string.docs_move_title, item.name),
            confirm = stringResource(Res.string.docs_move_here), onDismiss = { moving = false },
        ) { _, picked ->
            moving = false
            val target = picked.lastOrNull()?.first
            if (target != item.parent) state.run(block = { docs.move(item.vaultId, item.id, target) }) { state.changed() }
        }
    }
}

@Composable
private fun DocBody(state: AppState, item: DocsItemUi) {
    val kind = docViewKind(item)
    val tooLarge = kind != DocViewKind.NONE && item.size > maxFor(kind)
    val noPreview = stringResource(Res.string.docs_no_preview)
    val tooLargeText = stringResource(Res.string.docs_too_large_to_show, formatSizeSi(maxFor(kind)))
    val pdfNot = stringResource(Res.string.docs_pdf_not_here)
    val imageNot = stringResource(Res.string.docs_image_not_read)
    var progress by remember { mutableStateOf(0L) }
    /** What was read: wiped when the page goes (also by locking, which clears every page). */
    val read = remember { arrayOfNulls<ByteArray>(1) }
    var shown by remember { mutableStateOf<Shown>(Shown.Reading) }
    DisposableEffect(item.id) {
        onDispose {
            read[0]?.fill(0)
            read[0] = null
            (shown as? Shown.Pages)?.pdf?.close()
            shown = Shown.Reading
        }
    }
    LaunchedEffect(item.id, item.size) {
        if (kind == DocViewKind.NONE || tooLarge) {
            shown = Shown.None(if (tooLarge) tooLargeText else noPreview)
            return@LaunchedEffect
        }
        val sink = MemorySink(maxFor(kind))
        try {
            state.backend.docs.read(item.vaultId, item.id, sink) { progress = it }
        } catch (e: CancellationException) {
            sink.discard()
            throw e
        } catch (e: Exception) {
            // A damaged document: what was read up to there is incomplete and goes at once.
            sink.discard()
            shown = Shown.None(describe(e))
            return@LaunchedEffect
        }
        val bytes = sink.bytes()
        // While the bytes are decoded they belong to this coroutine: the page going away (which
        // cancels it) must not wipe them under the decoder's feet, and what was opened meanwhile
        // (a PDF renderer holding the document) is closed here, not dropped.
        var opened: Shown? = null
        try {
            withContext(Dispatchers.Default) {
                opened = when (kind) {
                    DocViewKind.IMAGE -> decodeImage(bytes, IMAGE_MAX_SIDE)?.let { Shown.Picture(it) } ?: Shown.None(imageNot)
                    DocViewKind.PDF -> openPdf(bytes)?.let { Shown.Pages(it) } ?: Shown.None(pdfNot)
                    DocViewKind.TEXT -> {
                        val text = bytes.decodeToString()
                        Shown.Plain(text.take(TEXT_MAX_CHARS), text.length > TEXT_MAX_CHARS)
                    }
                    DocViewKind.NONE -> Shown.None(noPreview)
                }
            }
        } catch (e: CancellationException) {
            // withContext ends only after its block did: nothing reads the bytes any more.
            (opened as? Shown.Pages)?.pdf?.close()
            bytes.fill(0)
            throw e
        }
        shown = opened ?: Shown.None(noPreview)
        // An image is drawn now: its bytes are not needed any more. A PDF reads its bytes while its
        // pages are drawn (Windows) and a text is shown from them: those go when the page goes.
        if (kind == DocViewKind.IMAGE) bytes.fill(0) else read[0] = bytes
    }
    val info = listOfNotNull(
        formatSizeSi(item.size),
        // The type in short ("PDF", "DOCX"): a media type can be very long.
        item.name.substringAfterLast('.', "").uppercase().takeIf { it.isNotEmpty() && it.length <= 5 }
            ?: item.mime.substringAfter('/').takeIf { it.isNotBlank() && it.length <= 16 },
        stringResource(Res.string.docs_added_on, formatDate(item.created)),
        if (item.path.isNotBlank()) stringResource(Res.string.docs_in, item.path.replace("/", " › ")) else null,
    ).joinToString(" · ")
    when (val s = shown) {
        Shown.Reading -> Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            HintText(info)
            val p = if (item.size > 0) (progress.toFloat() / item.size).coerceIn(0f, 1f) else 0f
            androidx.compose.foundation.layout.Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = g.muted)
                Text(stringResource(Res.string.docs_reading, (p * 100).toInt().toString()), fontSize = 14.sp, color = g.muted)
            }
        }
        is Shown.Picture -> ScreenColumn {
            HintText(info)
            Image(
                s.image, item.name,
                Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(g.surface).testTag("docs-image"),
                contentScale = ContentScale.FillWidth,
            )
            ViewerActions(state, item)
        }
        is Shown.Plain -> ScreenColumn {
            HintText(info)
            SelectionContainer {
                Text(
                    s.text, fontFamily = FontFamily.Monospace, fontSize = 13.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(g.surface).padding(12.dp).testTag("docs-text"),
                )
            }
            if (s.cut) HintText(stringResource(Res.string.docs_text_cut))
            ViewerActions(state, item)
        }
        is Shown.Pages -> PdfPages(state, item, s.pdf, info)
        is Shown.None -> ScreenColumn {
            HintText(info)
            SectionCard {
                val (icon, c) = docKind(item)
                IconTile(icon, c, size = 56.dp)
                Text(s.reason, modifier = Modifier.testTag("docs-no-preview"))
                HintText(stringResource(Res.string.docs_open_hint))
            }
            ViewerActions(state, item)
        }
    }
}

/** "Open with" and "Export" under a document (also where the app cannot show it). */
@Composable
private fun ViewerActions(state: AppState, item: DocsItemUi) {
    var askExport by remember { mutableStateOf(false) }
    val exporting = remember { arrayOfNulls<DocsItemUi>(1) }
    val create = rememberCreateStream(onFailed = { state.reportSaveFile(SaveFileResult.FAILED) }) { sink: ByteSink? ->
        val x = exporting[0]
        exporting[0] = null
        if (sink == null) return@rememberCreateStream
        if (x == null) {
            runCatching { sink.close() }
            runCatching { sink.discard() }
            state.reportSaveFile(SaveFileResult.FAILED)
            return@rememberCreateStream
        }
        state.exportDoc(x, sink)
    }
    ButtonRow {
        if (filePickersAvailable) {
            OutlinedButton(onClick = { askExport = true }, modifier = Modifier.weight(1f).testTag("docs-export")) {
                Icon(Icons.Filled.Download, null, Modifier.size(18.dp)); Text(stringResource(Res.string.docs_export))
            }
        }
        Button(onClick = { state.openDocWith(item) }, modifier = Modifier.weight(1f).testTag("docs-open-with")) {
            Icon(Icons.AutoMirrored.Filled.OpenInNew, null, Modifier.size(18.dp)); Text(stringResource(Res.string.docs_open_with))
        }
    }
    HintText(stringResource(Res.string.docs_copy_hint))
    if (state.backend.openCopiesOnThisPc) HintText(stringResource(Res.string.docs_copy_hint_portable))
    if (askExport) {
        AlertDialog(
            onDismissRequest = { askExport = false },
            icon = { Icon(Icons.Filled.Warning, null, tint = warnColor()) },
            title = { Text(stringResource(Res.string.docs_export_title)) },
            text = { Text(stringResource(Res.string.docs_export_text, item.name)) },
            confirmButton = {
                TextButton(onClick = {
                    askExport = false
                    exporting[0] = item
                    create(item.name, item.mime.ifBlank { mimeOf(item.name) })
                }, modifier = Modifier.testTag("docs-export-confirm")) { Text(stringResource(Res.string.docs_export_yes)) }
            },
            dismissButton = { TextButton(onClick = { askExport = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** The pages of a PDF, each drawn when it comes into view, as wide as the column. */
@Composable
private fun PdfPages(state: AppState, item: DocsItemUi, pdf: PdfView, info: String) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val widthPx = with(LocalDensity.current) { (minOf(maxWidth, LocalContentMax.current) - 32.dp).roundToPx() }
        LazyColumn(
            Modifier.fillMaxSize().testTag("docs-pdf"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 100.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { HintText(info + " · " + pdfPagesText(pdf.pageCount)) }
            items((0 until pdf.pageCount).toList()) { index ->
                val page by produceState<ImageBitmap?>(null, index, widthPx) {
                    value = withContext(Dispatchers.Default) { pdf.render(index, widthPx) }
                }
                val p = page
                if (p == null) {
                    androidx.compose.foundation.layout.Box(Modifier.fillMaxWidth().aspectRatio(0.707f).clip(RoundedCornerShape(12.dp)).background(Color.White))
                } else {
                    Image(p, null, Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).testTag("docs-pdf-page"), contentScale = ContentScale.FillWidth)
                }
            }
            item { ViewerActions(state, item) }
        }
    }
}

@Composable
private fun pdfPagesText(n: Int): String = org.jetbrains.compose.resources.pluralStringResource(Res.plurals.docs_pages, n, n.toString())
