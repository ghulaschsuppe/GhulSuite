// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Crop
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.FilterBAndW
import androidx.compose.material.icons.filled.FlashlightOff
import androidx.compose.material.icons.filled.FlashlightOn
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Rotate90DegreesCw
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import de.danerd.ghulsuite.scan.Pt
import de.danerd.ghulsuite.scan.Quad
import de.danerd.ghulsuite.scan.ScanBitmap
import de.danerd.ghulsuite.scan.ScanCamera
import de.danerd.ghulsuite.scan.ScanEngine
import de.danerd.ghulsuite.scan.ScanRendered
import de.danerd.ghulsuite.scan.ScanShot
import de.danerd.ghulsuite.scan.buildPdf
import de.danerd.ghulsuite.scan.detectSheet
import de.danerd.ghulsuite.scan.grayFromArgb
import de.danerd.ghulsuite.scan.moveCorner
import de.danerd.ghulsuite.scan.pdfName
import de.danerd.ghulsuite.scan.scanFileName
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource
import kotlin.math.max
import kotlin.math.min

// Scanning documents into GhulDocs (G7, Android): the camera in the app, several pages one after
// the other, each cropped with a frame of four corners (suggested by [detectSheet]) and
// straightened, optionally the "Document" filter, all pages into one PDF that is added like a
// file. Photos and pages stay in memory only and are overwritten when the scan ends (saved,
// left, locked); see [ScanSession.wipe].

/** The platform's camera and pictures for scanning; null where there is no scanning (Windows). */
val LocalScanEngine = staticCompositionLocalOf { if (docScanAvailable) platformScanEngine() else null }

/** Pages up to which the memory of a phone surely suffices; above, the screen warns. */
const val SCAN_MANY_PAGES = 30

/** No more pages than this in one scan (the memory of the phone). */
const val SCAN_MAX_PAGES = 100

/** The longer side of the photo on the crop screen. */
private const val CROP_PREVIEW_SIDE = 1600

/** The longer side of a page shown large in the review. */
private const val PAGE_PREVIEW_SIDE = 1400

/** One page of a scan: its photo, the corners chosen on it and the finished page. */
class ScanPage(
    val shot: ScanShot,
    val corners: Quad,
    val turns: Int,
    val filter: Boolean,
    val rendered: ScanRendered,
)

enum class ScanStep { CAMERA, CROP, REVIEW }

/**
 * A scan in progress: its pages and what the screen shows. Held by [AppState.scanSession] (not by
 * the screen), so a screen built anew (the phone turned to the wide layout) keeps it; ends with
 * [wipe] when the scan screen is left, saved or the suite locks.
 */
class ScanSession(val vaultId: String, val folder: String?, parent: CoroutineScope) {
    private val job = SupervisorJob(parent.coroutineContext[kotlinx.coroutines.Job])
    val scope = CoroutineScope(parent.coroutineContext + job)
    val camera = ScanCamera()
    val pages = mutableStateListOf<ScanPage>()
    var step by mutableStateOf(ScanStep.CAMERA)
    /** The page a new photo replaces ("Retake"); null: a new page at the end. */
    var replace by mutableStateOf<Int?>(null)
    /** The crop screen shows the photo of this page (to change its corners); null: a new photo. */
    var editing by mutableStateOf<Int?>(null)
    /** The photo on the crop screen. */
    var shot by mutableStateOf<ScanShot?>(null)
    var preview by mutableStateOf<ScanBitmap?>(null)
    var corners by mutableStateOf(Quad.inset())
    /** Whether the sheet was found on the photo of the crop screen (null: still looking). */
    var found by mutableStateOf<Boolean?>(null)
    /** What the detection suggested (to go back to it after "Whole photo"). */
    var suggested by mutableStateOf<Quad?>(null)
    /** The page shown large in the review and its picture. */
    var selected by mutableStateOf(0)
    var shown by mutableStateOf<Pair<ScanRendered, ScanBitmap>?>(null)
    /** The "Document" filter for new pages (switched for all pages at once). */
    var filterAll by mutableStateOf(false)
    var name by mutableStateOf("")
    var busy by mutableStateOf(false)
    var saving by mutableStateOf<BgJob?>(null)
    var error by mutableStateOf<String?>(null)
    var wiped = false
        private set

    /**
     * Overwrites every photo and page and frees the pictures; work still running is stopped and
     * throws its result away. Nothing of the scan is left in memory the app holds.
     */
    fun wipe() {
        if (wiped) return
        wiped = true
        job.cancel()
        scope.cancel()
        val own = shot
        if (own != null && pages.none { it.shot === own }) own.wipe()
        for (p in pages) {
            p.shot.wipe()
            p.rendered.wipe()
        }
        pages.clear()
        shot = null
        preview?.release()
        preview = null
        shown?.second?.release()
        shown = null
        camera.take = null
        name = ""
    }

    /** Wipes [p] unless a page still uses it. */
    fun drop(p: ScanPage) {
        if (pages.none { it.shot === p.shot } && shot !== p.shot) p.shot.wipe()
        if (shown?.first === p.rendered) {
            shown?.second?.release()
            shown = null
        }
        p.rendered.wipe()
    }
}

/** A finished PDF in memory, read by the core while it encrypts it; overwritten when it is closed. */
private class MemorySource(override val name: String, private val data: ByteArray) : ByteSource {
    private var pos = 0
    override val size: Long = data.size.toLong()
    override val mime: String get() = "application/pdf"

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
        if (pos >= data.size) return -1
        val n = min(length, data.size - pos)
        data.copyInto(buffer, offset, pos, pos + n)
        pos += n
        return n
    }

    override fun close() {
        data.fill(0)
        pos = data.size
    }
}

/**
 * Runs [block] (pictures, the PDF) off the main thread. What it makes is never lost: when the
 * caller is cancelled (the scan left, the suite locked) before it takes the result, [release]
 * clears it, so no picture or page stays behind uncleared.
 */
@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
private suspend fun <T> offMain(release: (T) -> Unit, block: () -> T): T {
    val result = CompletableDeferred<T>()
    CoroutineScope(Dispatchers.Default).launch {
        runCatching(block)
            .onSuccess { v -> if (!result.complete(v)) runCatching { release(v) } }
            .onFailure { result.completeExceptionally(it) }
    }
    try {
        return result.await()
    } catch (e: CancellationException) {
        // Cancelled while it ran: it is cleared when it ends. Already done: cleared now.
        if (!result.completeExceptionally(e) && !result.isCancelled) {
            runCatching { result.getCompleted() }.onSuccess { runCatching { release(it) } }
        }
        throw e
    }
}

/** The scan screen: camera, crop, review (see [ScanStep]). */
@Composable
fun DocScanScreen(state: AppState, vaultId: String, folder: String?, padding: PaddingValues) {
    val engine = LocalScanEngine.current
    if (engine == null) {
        LaunchedEffect(Unit) { state.pop() }
        return
    }
    val session = remember(vaultId, folder) {
        state.scanSession?.takeIf { it.vaultId == vaultId && it.folder == folder && !it.wiped }
            ?: ScanSession(vaultId, folder, state.scope).also {
                state.scanSession?.wipe()
                state.scanSession = it
            }
    }
    val suggestedName = remember { scanFileName(localNow()) }
    LaunchedEffect(session) { if (session.name.isEmpty()) session.name = suggestedName }
    DisposableEffect(session) {
        onDispose {
            state.editorDirty = false
            // Left (back, saved, another screen): the scan ends. Only built anew (the layout
            // changed) it goes on.
            if (state.stack.none { it is Screen.DocScan }) {
                session.wipe()
                if (state.scanSession === session) state.scanSession = null
            }
        }
    }
    // U12: pages not saved yet make "back" ask first (and locking says so after unlocking).
    val dirty = (session.pages.isNotEmpty() || session.shot != null) && session.saving == null
    SideEffect { state.editorDirty = dirty }
    // Back inside the scan: from the crop screen to the camera or the review, from the camera to
    // the review; from the review (or the first camera) the scan is left (asking first).
    PlatformBackHandler(enabled = session.step == ScanStep.CROP || (session.step == ScanStep.CAMERA && session.pages.isNotEmpty())) {
        if (session.step == ScanStep.CROP) cancelCrop(session) else session.step = ScanStep.REVIEW
    }
    Column(Modifier.fillMaxSize().padding(padding)) {
        when (session.step) {
            ScanStep.CAMERA -> CameraStep(state, session, engine)
            ScanStep.CROP -> CropStep(state, session, engine)
            ScanStep.REVIEW -> ReviewStep(state, session, engine)
        }
    }
}

private fun cancelCrop(s: ScanSession) {
    val shot = s.shot
    val editing = s.editing
    s.shot = null
    s.preview?.release()
    s.preview = null
    s.editing = null
    if (editing == null) {
        // A new photo that is not used: gone.
        shot?.wipe()
        s.step = ScanStep.CAMERA
    } else {
        s.step = ScanStep.REVIEW
    }
}

/** Opens the crop screen for [shot]: a new photo (the sheet is looked for) or the photo of page [edit]. */
private fun startCrop(s: ScanSession, engine: ScanEngine, shot: ScanShot, edit: Int?) {
    s.preview?.release()
    s.preview = null
    s.shot = shot
    s.editing = edit
    val page = edit?.let { s.pages.getOrNull(it) }
    s.corners = page?.corners ?: Quad.inset()
    s.found = if (page != null) true else null
    s.suggested = page?.corners
    s.error = null
    s.step = ScanStep.CROP
    s.scope.launch {
        try {
            val (bmp, guess) = offMain({ it.first.release() }) {
                val b = engine.preview(shot, CROP_PREVIEW_SIDE)
                try {
                    val guess = if (page == null) {
                        val px = engine.pixels(b)
                        val gray = grayFromArgb(px, b.width, b.height)
                        px.fill(0)
                        detectSheet(gray).also { gray.pixels.fill(0) }
                    } else {
                        null
                    }
                    b to guess
                } catch (e: Throwable) {
                    b.release()
                    throw e
                }
            }
            if (s.wiped || s.shot !== shot) {
                bmp.release()
                return@launch
            }
            s.preview = bmp
            if (guess != null) {
                s.corners = guess.corners
                s.found = guess.found
                s.suggested = guess.corners.takeIf { guess.found }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            if (!s.wiped) {
                s.error = describe(e)
                s.found = false
            }
        }
    }
}

/** The page from the crop screen: drawn, then the camera for the next page (or back to the review). */
private fun applyCrop(state: AppState, s: ScanSession, engine: ScanEngine) {
    val shot = s.shot ?: return
    if (s.busy) return
    val target = s.editing ?: s.replace
    val old = target?.let { s.pages.getOrNull(it) }
    val corners = s.corners
    val turns = old?.turns ?: 0
    val filter = old?.filter ?: s.filterAll
    s.busy = true
    s.scope.launch {
        try {
            val rendered = offMain({ it.wipe() }) { engine.render(shot, corners, turns, filter) }
            if (s.wiped) {
                rendered.wipe()
                return@launch
            }
            val page = ScanPage(shot, corners, turns, filter, rendered)
            if (old != null && target < s.pages.size) {
                s.pages[target] = page
                s.drop(old)
                s.selected = target
            } else {
                s.pages += page
                s.selected = s.pages.lastIndex
            }
            s.shot = null
            s.preview?.release()
            s.preview = null
            val back = s.editing != null || s.replace != null
            s.editing = null
            s.replace = null
            s.step = if (back) ScanStep.REVIEW else ScanStep.CAMERA
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            if (!s.wiped) s.error = state.str(Res.string.scan_render_failed)
        } finally {
            s.busy = false
        }
    }
}

/** Page [i] drawn again (turned, the filter switched). */
private fun redraw(state: AppState, s: ScanSession, engine: ScanEngine, indices: List<Int>, change: (ScanPage) -> Pair<Int, Boolean>) {
    if (s.busy || indices.isEmpty()) return
    s.busy = true
    s.scope.launch {
        try {
            for (i in indices) {
                val p = s.pages.getOrNull(i) ?: continue
                val (turns, filter) = change(p)
                val r = offMain({ it.wipe() }) { engine.render(p.shot, p.corners, turns, filter) }
                if (s.wiped) {
                    r.wipe()
                    return@launch
                }
                val now = s.pages.indexOf(p)
                if (now < 0) {
                    r.wipe()
                    continue
                }
                s.pages[now] = ScanPage(p.shot, p.corners, turns, filter, r)
                s.drop(p)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            if (!s.wiped) s.error = state.str(Res.string.scan_render_failed)
        } finally {
            s.busy = false
        }
    }
}

/** Takes a photo and opens the crop screen with it. */
private fun capture(state: AppState, s: ScanSession, engine: ScanEngine) {
    val take = s.camera.take ?: return
    if (s.busy) return
    s.error = null
    s.scope.launch {
        if (s.replace == null && (s.pages.size >= SCAN_MAX_PAGES || engine.memoryLow())) {
            s.error = state.str(Res.string.scan_memory_full)
            return@launch
        }
        s.busy = true
        try {
            val shot = take()
            if (s.wiped || s.step != ScanStep.CAMERA) {
                shot.wipe()
                return@launch
            }
            startCrop(s, engine, shot, null)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            if (!s.wiped) s.error = state.str(Res.string.scan_capture_failed)
        } finally {
            s.busy = false
        }
    }
}

/** Writes the pages into one PDF (in memory) and adds it to GhulDocs; the scan ends once it is there. */
private fun save(state: AppState, s: ScanSession, fallbackName: String) {
    if (s.busy || s.saving != null || s.pages.isEmpty()) return
    s.busy = true
    s.error = null
    s.scope.launch {
        var pdf: ByteArray? = null
        try {
            val jpegs = s.pages.map { it.rendered.jpeg }
            pdf = offMain({ it.fill(0) }) { buildPdf(jpegs) }
            val usage = catching { state.backend.docs.usage() }.getOrNull()
            if (usage != null && usage.used + pdf.size > usage.limit) {
                s.error = state.str(Res.string.err_docs_limit, formatSizeSi(usage.used), formatSizeSi(usage.limit))
                return@launch
            }
            val name = docName(pdfName(s.name, fallbackName))
            // From here the job owns the PDF (it overwrites it when it closes it).
            val source = MemorySource(name, pdf)
            pdf = null
            val job = state.addDocs(s.vaultId, s.folder, listOf(source)) ?: return@launch
            s.saving = job
            snapshotFlow { job.active }.first { !it }
            if (job.state == JobState.DONE) {
                // Saved: the scan ends (the screen goes, its pages are wiped).
                state.editorDirty = false
                if (state.stack.lastOrNull() is Screen.DocScan) state.pop()
                if (state.scanSession === s) state.scanSession = null
                s.wipe()
            } else {
                // Failed or cancelled: the pages stay, the message said why.
                s.saving = null
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            if (!s.wiped) s.error = describe(e)
        } finally {
            pdf?.fill(0)
            s.busy = false
        }
    }
}

// --- camera --------------------------------------------------------------------------------

@Composable
private fun ScanHeader(
    state: AppState,
    s: ScanSession,
    title: String,
    onBack: () -> Unit,
    close: Boolean,
    /** The line under the title (the page worked on); else how many pages there are. */
    sub: String? = null,
    actions: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {},
) {
    val line = sub ?: if (s.pages.isNotEmpty()) pluralStringResource(Res.plurals.scan_pages_n, s.pages.size, s.pages.size.toString()) else null
    GhulTopBar(
        title, onLock = { state.lockNow() }, onBack = onBack, backIcon = if (close) Icons.Filled.Close else backIcon(),
        context = line?.let {
            {
                ShrinkText(it, androidx.compose.ui.text.TextStyle(fontSize = 13.sp, color = areaColor(Tab.DOCS), fontWeight = FontWeight.SemiBold), 10.sp)
            }
        },
        actions = actions,
        // As in every full-screen editor: the main action instead of the lock (locking from
        // outside, automatically or by a panic, still ends the scan).
        showLock = false,
    )
}

@Composable
private fun CameraStep(state: AppState, s: ScanSession, engine: ScanEngine) {
    val t = g
    val color = areaColor(Tab.DOCS)
    ScanHeader(
        state, s, stringResource(if (s.replace != null) Res.string.scan_retake else Res.string.scan_title),
        onBack = { if (s.replace != null) { s.replace = null; s.step = ScanStep.REVIEW } else if (s.pages.isNotEmpty()) s.step = ScanStep.REVIEW else state.back() },
        close = s.replace == null && s.pages.isEmpty(),
        sub = s.replace?.let { stringResource(Res.string.scan_page_n, it + 1) },
    ) {
        if (s.replace == null) {
            HeaderButton(stringResource(Res.string.scan_done), enabled = s.pages.isNotEmpty() && !s.busy, color = color) { s.step = ScanStep.REVIEW }
        }
    }
    Column(Modifier.fillMaxSize()) {
        Box(Modifier.weight(1f).fillMaxWidth().background(Color.Black), contentAlignment = Alignment.Center) {
            engine.Camera(s.camera, Modifier.fillMaxSize())
            if (s.camera.take != null) {
                Text(
                    stringResource(Res.string.scan_camera_hint),
                    Modifier.align(Alignment.TopCenter).padding(12.dp).clip(RoundedCornerShape(12.dp))
                        .background(Color.Black.copy(alpha = 0.55f)).padding(horizontal = 12.dp, vertical = 8.dp),
                    color = Color.White, fontSize = 13.sp, textAlign = TextAlign.Center,
                )
            }
            if (s.busy) CircularProgressIndicator(color = Color.White)
        }
        Column(Modifier.fillMaxWidth().background(t.bg).navigationBarsPadding().padding(horizontal = 16.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)) {
            s.error?.let { Banner(Icons.Filled.Warning, it) }
            if (s.pages.size >= SCAN_MANY_PAGES) Banner(Icons.Filled.Info, stringResource(Res.string.scan_many_pages))
            if (s.pages.isNotEmpty() && s.replace == null) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    s.pages.forEachIndexed { i, p ->
                        PageThumb(p.rendered.thumb.image, i + 1, selected = false, height = 64) { s.selected = i; s.step = ScanStep.REVIEW }
                    }
                }
            }
            Row(Modifier.fillMaxWidth().height(80.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                    if (s.camera.hasTorch) {
                        val on = s.camera.torch
                        BarIcon(if (on) Icons.Filled.FlashlightOn else Icons.Filled.FlashlightOff, stringResource(Res.string.scan_torch),
                            tint = if (on) color else t.fg) { s.camera.torch = !on }
                    }
                }
                Shutter(enabled = s.camera.take != null && !s.busy, color = color) { capture(state, s, engine) }
                Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
                    if (s.pages.isNotEmpty()) {
                        Text(
                            stringResource(Res.string.scan_page_n, (s.replace ?: s.pages.size) + 1),
                            fontSize = 14.sp, color = t.muted, fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun Shutter(enabled: Boolean, color: Color, onClick: () -> Unit) {
    val label = stringResource(Res.string.scan_shutter)
    Box(
        Modifier.size(72.dp).clip(CircleShape).border(4.dp, if (enabled) color else g.surface3, CircleShape)
            .clickable(enabled = enabled, onClick = onClick)
            .semantics { contentDescription = label; role = Role.Button }
            .testTag("scan-shutter"),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(56.dp).clip(CircleShape).background(if (enabled) color else g.surface3), contentAlignment = Alignment.Center) {
            Icon(Icons.Filled.CameraAlt, null, Modifier.size(26.dp), tint = if (enabled) Color.White else g.faint)
        }
    }
}

/** A small picture of a page with its number; [selected] framed in the area colour. */
@Composable
private fun PageThumb(image: ImageBitmap, number: Int, selected: Boolean, height: Int, onClick: () -> Unit) {
    val color = areaColor(Tab.DOCS)
    val label = stringResource(Res.string.scan_page_n, number)
    val w = (height * image.width / max(1, image.height)).coerceIn(height / 2, height * 2)
    Box(
        Modifier.size(w.dp, height.dp).clip(RoundedCornerShape(8.dp))
            .border(if (selected) 3.dp else 1.dp, if (selected) color else g.line, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick).semantics { contentDescription = label }.testTag("scan-thumb-$number"),
    ) {
        Image(image, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        Text(
            number.toString(),
            Modifier.align(Alignment.BottomStart).padding(3.dp).clip(RoundedCornerShape(6.dp))
                .background(Color.Black.copy(alpha = 0.6f)).padding(horizontal = 5.dp, vertical = 1.dp),
            color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold,
        )
    }
}

// --- crop ----------------------------------------------------------------------------------

@Composable
private fun CropStep(state: AppState, s: ScanSession, engine: ScanEngine) {
    val t = g
    val color = areaColor(Tab.DOCS)
    val number = (s.editing ?: s.replace ?: s.pages.size) + 1
    ScanHeader(state, s, stringResource(Res.string.scan_page_n, number), onBack = { cancelCrop(s) }, close = false, sub = stringResource(Res.string.scan_crop)) {
        HeaderButton(stringResource(Res.string.scan_use), enabled = s.preview != null && !s.busy, color = color) { applyCrop(state, s, engine) }
    }
    Column(Modifier.fillMaxSize()) {
        Box(Modifier.weight(1f).fillMaxWidth().background(Color(0xFF111217)), contentAlignment = Alignment.Center) {
            val p = s.preview
            if (p != null) {
                CropEditor(p.image, s.corners, { s.corners = it }, Modifier.fillMaxSize(), color)
            } else if (s.error == null) {
                CircularProgressIndicator(color = Color.White)
            }
            if (s.busy) CircularProgressIndicator(color = Color.White)
        }
        Column(Modifier.fillMaxWidth().background(t.bg).navigationBarsPadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            s.error?.let { Banner(Icons.Filled.Warning, it) }
            HintText(
                stringResource(
                    when (s.found) {
                        null -> Res.string.scan_detecting
                        true -> Res.string.scan_found
                        false -> Res.string.scan_not_found
                    },
                ) + " " + stringResource(Res.string.scan_crop_hint),
                Modifier.testTag("scan-crop-status"),
            )
            ButtonRow {
                OutlinedButton(onClick = { cancelCrop(s) }, modifier = Modifier.weight(1f), enabled = !s.busy) {
                    Text(stringResource(if (s.editing != null) Res.string.a_cancel else Res.string.scan_retake))
                }
                val whole = s.corners == Quad.FULL
                val back = s.suggested
                OutlinedButton(
                    onClick = { s.corners = if (whole) back ?: Quad.inset() else Quad.FULL },
                    modifier = Modifier.weight(1f), enabled = s.preview != null && !s.busy,
                ) { Text(stringResource(if (whole && back != null) Res.string.scan_suggested else Res.string.scan_whole)) }
            }
        }
    }
}

/**
 * The photo with a frame of four corners the user drags: each corner follows the finger's
 * movement (it does not jump under the finger), a magnifier shows the corner enlarged while it
 * moves, and the frame never folds into a shape that is no sheet ([moveCorner]).
 */
@Composable
fun CropEditor(image: ImageBitmap, corners: Quad, onChange: (Quad) -> Unit, modifier: Modifier = Modifier, color: Color = g.accent) {
    val density = LocalDensity.current
    var dragging by remember { mutableStateOf(-1) }
    val current = rememberUpdatedState(corners)
    val change = rememberUpdatedState(onChange)
    val hint = stringResource(Res.string.scan_crop_hint)
    BoxWithConstraints(modifier) {
        val pad = with(density) { 28.dp.toPx() }
        val boxW = constraints.maxWidth.toFloat()
        val boxH = constraints.maxHeight.toFloat()
        val scale = max(0.01f, min((boxW - 2 * pad) / image.width, (boxH - 2 * pad) / image.height))
        val dw = image.width * scale
        val dh = image.height * scale
        val ox = (boxW - dw) / 2
        val oy = (boxH - dh) / 2
        fun screen(p: Pt) = Offset(ox + p.x * dw, oy + p.y * dh)
        val touch = with(density) { 48.dp.toPx() }
        // At least 24 dp between corners, in pixels of the photo.
        val minSide = with(density) { 24.dp.toPx() } / scale
        Canvas(
            Modifier.fillMaxSize().testTag("scan-crop").semantics { contentDescription = hint }
                .pointerInput(image, dw, dh) {
                    awaitEachGesture {
                        val down = awaitFirstDown(requireUnconsumed = false)
                        val q = current.value
                        val i = (0 until 4).minBy { (screen(q[it]) - down.position).getDistance() }
                        if ((screen(q[i]) - down.position).getDistance() > touch) return@awaitEachGesture
                        down.consume()
                        dragging = i
                        val start = q[i]
                        var moved = Offset.Zero
                        try {
                            while (true) {
                                val ev = awaitPointerEvent()
                                val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                                if (!ch.pressed) break
                                moved += ch.positionChange()
                                ch.consume()
                                val target = Pt(start.x + moved.x / dw, start.y + moved.y / dh)
                                change.value(moveCorner(current.value, i, target, image.width.toFloat(), image.height.toFloat(), minSide))
                            }
                        } finally {
                            dragging = -1
                        }
                    }
                },
        ) {
            drawImage(
                image, srcOffset = IntOffset.Zero, srcSize = IntSize(image.width, image.height),
                dstOffset = IntOffset(ox.toInt(), oy.toInt()), dstSize = IntSize(dw.toInt(), dh.toInt()),
                filterQuality = FilterQuality.Medium,
            )
            val pts = corners.points.map { screen(it) }
            val frame = Path().apply {
                moveTo(pts[0].x, pts[0].y)
                for (k in 1 until 4) lineTo(pts[k].x, pts[k].y)
                close()
            }
            // Outside the frame darker, so the page stands out.
            val shade = Path().apply {
                fillType = PathFillType.EvenOdd
                addRect(Rect(ox, oy, ox + dw, oy + dh))
                addPath(frame)
            }
            drawPath(shade, Color.Black.copy(alpha = 0.5f))
            drawPath(frame, color, style = Stroke(width = 2.dp.toPx()))
            for ((k, p) in pts.withIndex()) {
                val r = (if (k == dragging) 14.dp else 11.dp).toPx()
                drawCircle(Color.White.copy(alpha = 0.35f), radius = r + 6.dp.toPx(), center = p)
                drawCircle(Color.White, radius = r, center = p)
                drawCircle(color, radius = r, center = p, style = Stroke(width = 3.dp.toPx()))
            }
            val d = dragging
            if (d in 0..3) {
                // The magnifier: in the upper corner away from the finger, the photo 3 times larger.
                val lr = 58.dp.toPx()
                val zoom = 3f
                val corner = pts[d]
                val left = corner.x > size.width / 2 || corner.y > size.height / 2
                val c = Offset(if (left) lr + 12.dp.toPx() else size.width - lr - 12.dp.toPx(), lr + 12.dp.toPx())
                val circle = Path().apply { addOval(Rect(c, lr)) }
                drawCircle(Color.Black, radius = lr + 2.dp.toPx(), center = c)
                clipPath(circle) {
                    // The part of the photo around the corner (in photo pixels), clipped to the photo.
                    val px = corners[d].x * image.width
                    val py = corners[d].y * image.height
                    val half = lr / (scale * zoom)
                    val sx0 = max(0f, px - half)
                    val sy0 = max(0f, py - half)
                    val sx1 = min(image.width.toFloat(), px + half)
                    val sy1 = min(image.height.toFloat(), py + half)
                    if (sx1 - sx0 >= 1 && sy1 - sy0 >= 1) {
                        val f = scale * zoom
                        drawImage(
                            image, srcOffset = IntOffset(sx0.toInt(), sy0.toInt()),
                            srcSize = IntSize((sx1 - sx0).toInt(), (sy1 - sy0).toInt()),
                            dstOffset = IntOffset((c.x + (sx0 - px) * f).toInt(), (c.y + (sy0 - py) * f).toInt()),
                            dstSize = IntSize(((sx1 - sx0) * f).toInt(), ((sy1 - sy0) * f).toInt()),
                            filterQuality = FilterQuality.Medium,
                        )
                    }
                    // The two sides at the corner, as they run in the magnifier.
                    fun lens(p: Pt) = Offset(c.x + (p.x * image.width - px) * scale * zoom, c.y + (p.y * image.height - py) * scale * zoom)
                    val here = lens(corners[d])
                    drawLine(color, here, lens(corners[(d + 1) % 4]), strokeWidth = 2.dp.toPx())
                    drawLine(color, here, lens(corners[(d + 3) % 4]), strokeWidth = 2.dp.toPx())
                    drawCircle(color, radius = 4.dp.toPx(), center = here, style = Stroke(width = 2.dp.toPx()))
                }
                drawCircle(Color.White, radius = lr, center = c, style = Stroke(width = 2.dp.toPx()))
            }
        }
    }
}

// --- review --------------------------------------------------------------------------------

@Composable
private fun ReviewStep(state: AppState, s: ScanSession, engine: ScanEngine) {
    val t = g
    val color = areaColor(Tab.DOCS)
    val fallbackName = remember { scanFileName(localNow()) }
    val canSave = s.pages.isNotEmpty() && !s.busy && s.saving == null
    ScanHeader(state, s, stringResource(Res.string.scan_title), onBack = { state.back() }, close = true) {
        HeaderButton(stringResource(Res.string.a_save), enabled = canSave, color = color) { save(state, s, fallbackName) }
    }
    if (s.pages.isEmpty()) {
        LaunchedEffect(Unit) { s.step = ScanStep.CAMERA }
        return
    }
    val sel = s.selected.coerceIn(0, s.pages.lastIndex)
    val page = s.pages[sel]
    // The selected page, decoded from its JPEG for the screen (the others only as small pictures).
    LaunchedEffect(page.rendered) {
        if (s.shown?.first === page.rendered) return@LaunchedEffect
        val bmp = catching { offMain({ it.release() }) { engine.decode(page.rendered.jpeg, PAGE_PREVIEW_SIDE) } }.getOrNull()
            ?: return@LaunchedEffect
        if (s.wiped || s.pages.none { it.rendered === page.rendered }) {
            bmp.release()
            return@LaunchedEffect
        }
        val old = s.shown
        s.shown = page.rendered to bmp
        old?.second?.release()
    }
    val target = targetName(state, s)
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding()
            .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            Modifier.fillMaxWidth().height(340.dp).clip(RoundedCornerShape(16.dp)).background(t.surface2).testTag("scan-page"),
            contentAlignment = Alignment.Center,
        ) {
            val shown = s.shown?.takeIf { it.first === page.rendered }?.second?.image ?: page.rendered.thumb.image
            Image(shown, stringResource(Res.string.scan_page_n, sel + 1), Modifier.fillMaxSize().padding(10.dp), contentScale = ContentScale.Fit)
            if (s.busy) CircularProgressIndicator(color = color)
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            val idle = !s.busy && s.saving == null
            ToolButton(Icons.Filled.Rotate90DegreesCw, stringResource(Res.string.scan_rotate), idle, "scan-rotate", modifier = Modifier.weight(1f)) {
                redraw(state, s, engine, listOf(sel)) { (it.turns + 1) % 4 to it.filter }
            }
            ToolButton(Icons.Filled.FilterBAndW, stringResource(Res.string.scan_filter), idle, "scan-filter", on = page.filter, modifier = Modifier.weight(1f)) {
                redraw(state, s, engine, listOf(sel)) { it.turns to !it.filter }
            }
            ToolButton(Icons.Filled.Crop, stringResource(Res.string.scan_crop_short), idle, "scan-recrop", modifier = Modifier.weight(1f)) { startCrop(s, engine, page.shot, sel) }
            ToolButton(Icons.Filled.CameraAlt, stringResource(Res.string.scan_retake_short), idle, "scan-retake", modifier = Modifier.weight(1f)) {
                s.replace = sel
                s.step = ScanStep.CAMERA
            }
            ToolButton(Icons.Filled.Delete, stringResource(Res.string.a_delete), idle, "scan-delete", danger = true, modifier = Modifier.weight(1f)) {
                val p = s.pages.removeAt(sel)
                s.drop(p)
                s.selected = min(sel, s.pages.lastIndex).coerceAtLeast(0)
            }
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically) {
            s.pages.forEachIndexed { i, p -> PageThumb(p.rendered.thumb.image, i + 1, selected = i == sel, height = 72) { s.selected = i } }
            Box(
                Modifier.size(56.dp, 72.dp).clip(RoundedCornerShape(8.dp)).background(t.surface2)
                    .clickable(enabled = !s.busy && s.saving == null) { s.replace = null; s.step = ScanStep.CAMERA }
                    .testTag("scan-add-page"),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Filled.Add, stringResource(Res.string.scan_add_page), tint = color) }
        }
        SectionCard {
            val all = s.pages.all { it.filter }
            val free = !s.busy && s.saving == null
            Row(
                Modifier.fillMaxWidth().toggleable(all, enabled = free, role = Role.Switch) { on ->
                    s.filterAll = on
                    redraw(state, s, engine, s.pages.indices.filter { s.pages[it].filter != on }) { it.turns to on }
                }.testTag("scan-filter-all"),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(Res.string.scan_filter_all), fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = t.fg)
                    Text(stringResource(Res.string.scan_filter_sub), fontSize = 13.sp, color = t.muted)
                }
                Switch(all, null, enabled = free)
            }
        }
        OutlinedTextField(
            s.name, { s.name = it }, Modifier.fillMaxWidth().testTag("scan-name"),
            label = { Text(stringResource(Res.string.scan_name)) }, singleLine = true, enabled = s.saving == null,
        )
        HintText(stringResource(Res.string.scan_target, target))
        if (s.pages.size >= SCAN_MANY_PAGES) Banner(Icons.Filled.Info, stringResource(Res.string.scan_many_pages))
        s.error?.let { Banner(Icons.Filled.Warning, it) }
        Button(
            onClick = { save(state, s, fallbackName) }, modifier = Modifier.fillMaxWidth().testTag("scan-save"), enabled = canSave,
            colors = androidx.compose.material3.ButtonDefaults.buttonColors(
                containerColor = color, contentColor = t.onAccent, disabledContainerColor = t.surface3, disabledContentColor = t.faint,
            ),
        ) {
            if (s.saving != null) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = t.faint)
            Text(stringResource(if (s.saving != null) Res.string.scan_saving else Res.string.scan_save))
        }
        HintText(stringResource(Res.string.scan_privacy))
        Spacer(Modifier.height(8.dp))
    }
}

/** "Documents › Taxes": the vault and folder the PDF goes into. */
@Composable
private fun targetName(state: AppState, s: ScanSession): String {
    val vault = state.vaults.firstOrNull { it.id == s.vaultId }?.name.orEmpty()
    var path by remember(s.vaultId, s.folder) { mutableStateOf("") }
    LaunchedEffect(s.vaultId, s.folder) {
        val f = s.folder ?: return@LaunchedEffect
        catching { state.backend.docs.item(s.vaultId, f) }.onSuccess { item ->
            path = (if (item.path.isBlank()) "" else item.path.replace("/", " › ") + " › ") + item.name
        }
    }
    return listOf(vault, path).filter { it.isNotEmpty() }.joinToString(" › ").ifEmpty { stringResource(Res.string.tab_docs) }
}

/** A tool under the page: icon over a short label (the tools share the width). */
@Composable
private fun ToolButton(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    enabled: Boolean,
    tag: String,
    on: Boolean = false,
    danger: Boolean = false,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val t = g
    val color = areaColor(Tab.DOCS)
    val fg = when {
        !enabled -> t.faint
        danger -> t.danger
        on -> color
        else -> t.fg
    }
    Column(
        modifier.heightIn(min = 56.dp).clip(RoundedCornerShape(12.dp))
            .clickable(enabled = enabled, onClick = onClick).testTag(tag).padding(vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(
            Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(if (on) color.copy(alpha = 0.16f) else Color.Transparent),
            contentAlignment = Alignment.Center,
        ) { Icon(icon, null, Modifier.size(22.dp), tint = fg) }
        ShrinkText(label, androidx.compose.ui.text.TextStyle(fontSize = 12.sp, color = fg, textAlign = TextAlign.Center), 9.sp)
    }
}
