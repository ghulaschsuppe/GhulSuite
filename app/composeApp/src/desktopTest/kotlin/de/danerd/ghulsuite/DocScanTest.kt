// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.formatSizeSi
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.unit.Density
import de.danerd.ghulsuite.scan.Quad
import de.danerd.ghulsuite.scan.isValidQuad
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.DevChecks
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.HeaderGuard
import de.danerd.ghulsuite.ui.LocalScanEngine
import de.danerd.ghulsuite.ui.ScanStep
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.ui.Tab
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image
import java.awt.HeadlessException
import java.io.File
import java.util.Locale
import kotlin.math.abs
import kotlin.math.min
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Scanning a document (G7) clicked through on a phone with [FakeScanEngine] (drawn photos, the
 * same page transform as Android) and [FakeDocs]: the camera, the crop screen with the suggested
 * corners and a dragged corner, several pages, turning and the "Document" filter, saving as one
 * PDF into the open folder, the limit of GhulDocs, and that leaving or locking leaves nothing of
 * the photos and pages in memory. Pictures in build/screenshots/ui-scan-*.png.
 */
@OptIn(ExperimentalTestApi::class)
class DocScanTest {
    private val out = File("build/screenshots").apply { mkdirs() }

    private fun app(
        lang: Locale = Locale.ENGLISH,
        prepare: ScreenshotTest.Fake.() -> Unit = {},
        block: DesktopComposeUiTest.(AppState, ScreenshotTest.Fake, FakeScanEngine) -> Unit,
    ) {
        Locale.setDefault(lang)
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        HeaderGuard.reset()
        AppLaunch.docsFiles.value = null
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        val engine = FakeScanEngine()
        val errors = java.util.Collections.synchronizedList(mutableListOf<Throwable>())
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined + CoroutineExceptionHandler { _, e -> if (e !is HeadlessException) errors += e })
        val state = AppState(fake, scope).apply { tab = Tab.DOCS }
        try {
            runDesktopComposeUiTest(412 * 2, 892 * 2) {
                setContent {
                    CompositionLocalProvider(LocalDensity provides Density(2f, 1f), LocalScanEngine provides engine) { GhulSuiteApp(state) }
                }
                waitForIdle()
                block(state, fake, engine)
            }
        } finally {
            scope.coroutineContext[kotlinx.coroutines.Job]?.cancel()
        }
        assertTrue(errors.isEmpty(), "errors in the app: $errors")
        assertTrue(DevChecks.alarms.isEmpty(), "alarms of the development build: ${DevChecks.alarms.toList()}")
    }

    private fun DesktopComposeUiTest.shot(name: String) {
        waitForIdle()
        val bitmap = captureToImage().asSkiaBitmap()
        File(out, "ui-scan-$name.png").writeBytes(Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)!!.bytes)
    }

    private fun DesktopComposeUiTest.text(t: String): SemanticsNodeInteraction = onAllNodesWithText(t, useUnmergedTree = true).onFirst()
    private fun DesktopComposeUiTest.shown(t: String) = onAllNodesWithText(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.part(t: String) = onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.tag(t: String) = onAllNodesWithTag(t, useUnmergedTree = true).onFirst()
    private fun DesktopComposeUiTest.waitFor(t: String) = waitUntil(timeoutMillis = 10_000) { shown(t) }
    private fun DesktopComposeUiTest.waitPart(t: String) = waitUntil(timeoutMillis = 10_000) { part(t) }
    private fun DesktopComposeUiTest.dialogButton(t: String) = onAllNodes(hasText(t) and hasAnyAncestor(isDialog())).onFirst().performClick()

    /** From the GhulDocs list: "+", "Scan document"; the camera is ready. */
    private fun DesktopComposeUiTest.openScanner(state: AppState) {
        waitFor("Mietvertrag.pdf")
        onAllNodesWithContentDescription("Add").onFirst().performClick()
        waitFor("Scan document")
        text("Scan document").performClick()
        waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.DocScan && state.scanSession?.camera?.take != null }
        waitForIdle()
    }

    /** Shutter, then the crop screen with the sheet found. */
    private fun DesktopComposeUiTest.takePhoto(state: AppState) {
        tag("scan-shutter").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.step == ScanStep.CROP && state.scanSession?.found != null && state.scanSession?.preview != null }
        waitForIdle()
    }

    private fun DesktopComposeUiTest.use(state: AppState, pages: Int) {
        text("Use").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.pages?.size == pages && state.scanSession?.busy == false }
        waitForIdle()
    }

    @Test
    fun scanTwoPagesIntoOnePdf() = app { state, fake, engine ->
        openScanner(state)
        shot("1-camera")
        takePhoto(state)
        val s = assertNotNull(state.scanSession)
        // The sheet of the drawn photo is found (corners in parts of the upright 900 x 1200 photo).
        assertEquals(true, s.found)
        val expected = Quad(
            de.danerd.ghulsuite.scan.Pt(160f / 900, 120f / 1200), de.danerd.ghulsuite.scan.Pt(760f / 900, 180f / 1200),
            de.danerd.ghulsuite.scan.Pt(720f / 900, 1080f / 1200), de.danerd.ghulsuite.scan.Pt(110f / 900, 1020f / 1200),
        )
        for (i in 0 until 4) {
            assertTrue(abs(s.corners[i].x - expected[i].x) < 0.015f && abs(s.corners[i].y - expected[i].y) < 0.015f, "corner $i: ${s.corners[i]}")
        }
        assertTrue(shown("Sheet found. Drag the four corners onto the corners of the sheet."))
        shot("2-crop")

        // Drag the top left corner a little: it follows the finger's movement.
        val before = s.corners
        val node = tag("scan-crop").fetchSemanticsNode()
        val preview = assertNotNull(s.preview)
        val pad = 28f * 2
        val scale = min((node.size.width - 2 * pad) / preview.width, (node.size.height - 2 * pad) / preview.height)
        val dw = preview.width * scale
        val dh = preview.height * scale
        val ox = (node.size.width - dw) / 2
        val oy = (node.size.height - dh) / 2
        val tl = Offset(ox + before.tl.x * dw, oy + before.tl.y * dh)
        tag("scan-crop").performTouchInput {
            down(tl + Offset(6f, 4f))
            moveBy(Offset(20f, 0f))
            moveBy(Offset(20f, 30f))
            up()
        }
        waitForIdle()
        val moved = s.corners
        assertTrue(abs((moved.tl.x - before.tl.x) * dw - 40f) < 1.5f, "moved x ${(moved.tl.x - before.tl.x) * dw}")
        assertTrue(abs((moved.tl.y - before.tl.y) * dh - 30f) < 1.5f, "moved y ${(moved.tl.y - before.tl.y) * dh}")
        assertEquals(before.tr, moved.tr)
        // Pulled far across the frame: it stops where the frame is still a valid shape.
        val tl2 = Offset(ox + moved.tl.x * dw, oy + moved.tl.y * dh)
        tag("scan-crop").performTouchInput {
            down(tl2)
            moveBy(Offset(dw * 0.5f, dh * 0.5f))
            moveBy(Offset(dw * 0.4f, dh * 0.4f))
            up()
        }
        waitForIdle()
        assertTrue(isValidQuad(s.corners.scale(preview.width.toFloat(), preview.height.toFloat()), 1f), "folded: ${s.corners}")
        s.corners = before
        use(state, 1)
        assertEquals(ScanStep.CAMERA, s.step)
        assertTrue(shown("1 page"))
        // The second page.
        takePhoto(state)
        use(state, 2)
        shot("3-camera-2-pages")
        text("Done").performClick()
        waitFor("Save as PDF")
        val page = s.pages[0].rendered
        // A straightened page: about the proportions of the sheet (600 x 900 in the photo).
        assertTrue(page.height > page.width * 1.3f && page.height < page.width * 1.7f, "${page.width} x ${page.height}")
        shot("4-review")

        // Turn the second page, then the "Document" filter for all.
        tag("scan-thumb-2").performClick()
        waitForIdle()
        val w = s.pages[1].rendered.width
        tag("scan-rotate").performClick()
        waitUntil(timeoutMillis = 10_000) { s.pages[1].turns == 1 && !s.busy }
        assertEquals(w, s.pages[1].rendered.height)
        tag("scan-filter-all").performClick()
        waitUntil(timeoutMillis = 10_000) { s.pages.all { it.filter } && !s.busy }
        waitForIdle()
        shot("5-review-filter")

        onAllNodes(androidx.compose.ui.test.hasSetTextAction() and (androidx.compose.ui.test.hasTestTag("scan-name") or hasAnyAncestor(androidx.compose.ui.test.hasTestTag("scan-name")))).onFirst()
            .performTextReplacement("Steuer 2025")
        tag("scan-save").performClick()
        waitUntil(timeoutMillis = 15_000) { state.stack.isEmpty() }
        waitFor("Steuer 2025.pdf")
        shot("6-saved")
        val saved = fake.docs.nodes.single { it.name == "Steuer 2025.pdf" }
        assertEquals("application/pdf", saved.mime)
        assertNull(saved.parent)
        org.apache.pdfbox.Loader.loadPDF(saved.data).use { doc ->
            assertEquals(2, doc.numberOfPages)
            // The turned page lies.
            assertTrue(doc.getPage(1).mediaBox.width > doc.getPage(1).mediaBox.height)
        }
        // Nothing of the scan is left: photos and pages overwritten, pictures released.
        assertNull(state.scanSession)
        assertTrue(engine.allWiped(), "something of the scan is left in memory: ${engine.left()}")
        assertFalse(state.editorDirty)
    }

    @Test
    fun scanGoesIntoTheOpenFolder() = app { state, fake, engine ->
        waitFor("Steuer 2025")
        text("Steuer 2025").performClick()
        waitFor("Lohnsteuerbescheinigung.pdf")
        onAllNodesWithContentDescription("Add").onFirst().performClick()
        waitFor("Scan document")
        text("Scan document").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.camera?.take != null }
        assertEquals("f1", state.scanSession?.folder)
        takePhoto(state)
        use(state, 1)
        text("Done").performClick()
        waitFor("Save as PDF")
        waitPart("Dokumente › Steuer 2025")
        tag("scan-save").performClick()
        waitUntil(timeoutMillis = 15_000) { state.stack.isEmpty() }
        val saved = fake.docs.nodes.single { it.name.startsWith("Scan ") && it.name.endsWith(".pdf") }
        assertEquals("f1", saved.parent)
        assertTrue(Regex("Scan \\d{4}-\\d{2}-\\d{2} \\d{2}\\.\\d{2}\\.pdf").matches(saved.name), saved.name)
        assertTrue(engine.allWiped(), "something of the scan is left in memory: ${engine.left()}")
    }

    @Test
    fun lockingDropsTheScan() = app { state, fake, engine ->
        openScanner(state)
        takePhoto(state)
        use(state, 1)
        takePhoto(state)
        // Locked from outside (auto lock, panic) while a photo is on the crop screen.
        fake.unlocked.value = false
        waitUntil(timeoutMillis = 10_000) { state.scanSession == null && state.stack.isEmpty() }
        waitForIdle()
        assertTrue(engine.allWiped(), "something of the scan is left after locking: ${engine.left()}")
        assertTrue(fake.docs.nodes.none { it.name.startsWith("Scan ") }, "nothing is saved")
    }

    @Test
    fun leavingAsksAndDropsTheScan() = app { state, fake, engine ->
        openScanner(state)
        takePhoto(state)
        use(state, 1)
        // "Back" on the camera with a page goes to the review; there "Close" asks first.
        text("Done").performClick()
        waitFor("Save as PDF")
        state.back()
        waitFor("Discard changes?")
        shot("7-discard")
        dialogButton("Discard")
        waitUntil(timeoutMillis = 10_000) { state.stack.isEmpty() && state.scanSession == null }
        assertTrue(engine.allWiped(), engine.left())
        assertTrue(fake.docs.nodes.none { it.name.startsWith("Scan ") })
    }

    @Test
    fun deleteRetakeAndRecrop() = app { state, _, engine ->
        openScanner(state)
        takePhoto(state)
        use(state, 1)
        takePhoto(state)
        use(state, 2)
        text("Done").performClick()
        waitFor("Save as PDF")
        val s = assertNotNull(state.scanSession)
        // Crop page 1 again: its corners come back, "Cancel" keeps the page.
        tag("scan-thumb-1").performClick()
        tag("scan-recrop").performClick()
        waitUntil(timeoutMillis = 10_000) { s.step == ScanStep.CROP && s.preview != null }
        assertEquals(s.pages[0].corners, s.corners)
        text("Cancel").performClick()
        waitUntil(timeoutMillis = 10_000) { s.step == ScanStep.REVIEW }
        assertFalse(s.pages[0].shot.wiped)
        // Retake page 2: the new photo replaces it.
        tag("scan-thumb-2").performClick()
        val old = s.pages[1]
        tag("scan-retake").performClick()
        waitUntil(timeoutMillis = 10_000) { s.step == ScanStep.CAMERA && s.camera.take != null }
        waitFor("Page 2")
        takePhoto(state)
        text("Use").performClick()
        waitUntil(timeoutMillis = 10_000) { s.step == ScanStep.REVIEW && !s.busy }
        assertEquals(2, s.pages.size)
        assertTrue(s.pages[1] !== old && old.shot.wiped && old.rendered.jpeg.all { it == 0.toByte() })
        // Delete page 1.
        tag("scan-thumb-1").performClick()
        tag("scan-delete").performClick()
        waitForIdle()
        assertEquals(1, s.pages.size)
        state.back()
        waitFor("Discard changes?")
        dialogButton("Discard")
        waitUntil(timeoutMillis = 10_000) { state.scanSession == null }
        assertTrue(engine.allWiped(), "something of the scan is left in memory: ${engine.left()}")
    }

    @Test
    fun fullGhulDocsKeepsThePages() = app(prepare = { docs.limit = docs.nodes.sumOf { it.data.size.toLong() } + 1000 }) { state, fake, _ ->
        openScanner(state)
        takePhoto(state)
        use(state, 1)
        text("Done").performClick()
        waitFor("Save as PDF")
        tag("scan-save").performClick()
        // How much the PDF needs and how much is free.
        waitPart("The file needs")
        waitPart("${formatSizeSi(1000)} are free in this GhulDocs vault. Nothing was added.")
        shot("8-full")
        val s = assertNotNull(state.scanSession)
        assertEquals(1, s.pages.size)
        assertFalse(s.pages[0].shot.wiped)
        assertTrue(fake.docs.nodes.none { it.name.startsWith("Scan ") })
    }

    @Test
    fun noMemoryNoPhoto() = app { state, _, engine ->
        openScanner(state)
        engine.lowMemory = true
        tag("scan-shutter").performClick()
        waitPart("Not enough memory")
        assertEquals(ScanStep.CAMERA, state.scanSession?.step)
        assertTrue(engine.shots.isEmpty())
    }

    @Test
    fun germanScreens() = app(lang = Locale.GERMAN) { state, _, _ ->
        waitFor("Mietvertrag.pdf")
        onAllNodesWithContentDescription("Hinzufügen").onFirst().performClick()
        waitFor("Dokument scannen")
        shot("de-0-add-sheet")
        text("Dokument scannen").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.camera?.take != null }
        tag("scan-shutter").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.found != null && state.scanSession?.preview != null }
        shot("de-2-crop")
        text("Übernehmen").performClick()
        waitUntil(timeoutMillis = 10_000) { state.scanSession?.pages?.size == 1 && state.scanSession?.busy == false }
        text("Fertig").performClick()
        waitFor("Als PDF speichern")
        shot("de-4-review")
    }
}
