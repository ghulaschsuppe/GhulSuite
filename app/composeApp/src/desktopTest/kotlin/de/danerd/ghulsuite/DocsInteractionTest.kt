// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.unit.Density
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.DevChecks
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.HeaderGuard
import de.danerd.ghulsuite.ui.JOB_CANCEL_TAG
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.ui.SetPage
import de.danerd.ghulsuite.ui.Tab
import de.danerd.ghulsuite.ui.addDocs
import de.danerd.ghulsuite.ui.exportDoc
import de.danerd.ghulsuite.ui.formatSizeSi
import de.danerd.ghulsuite.ui.openDocWith
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image
import java.awt.HeadlessException
import java.io.File
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * GhulDocs clicked through on a phone with [FakeDocs] (it behaves like the core: waits, sections,
 * the limit, damaged documents): the list with its folders, adding with progress and "Cancel",
 * the limit, the trash with its questions, viewing, exporting, what a failed read leaves (nothing)
 * and what locking leaves (nothing). Pictures in build/screenshots/ui-docs-*.png.
 */
@OptIn(ExperimentalTestApi::class)
class DocsInteractionTest {
    private val out = File("build/screenshots").apply { mkdirs() }

    private fun app(
        wide: Boolean = false,
        setup: AppState.() -> Unit = { tab = Tab.DOCS },
        prepare: ScreenshotTest.Fake.() -> Unit = {},
        block: DesktopComposeUiTest.(AppState, ScreenshotTest.Fake) -> Unit,
    ) {
        Locale.setDefault(Locale.ENGLISH)
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        HeaderGuard.reset()
        AppLaunch.docsFiles.value = null
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        val errors = java.util.Collections.synchronizedList(mutableListOf<Throwable>())
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined + CoroutineExceptionHandler { _, e -> if (e !is HeadlessException) errors += e })
        val state = AppState(fake, scope).apply(setup)
        val (w, h) = if (wide) 1280 to 820 else 412 to 892
        try {
            runDesktopComposeUiTest(w * 2, h * 2) {
                setContent { CompositionLocalProvider(LocalDensity provides Density(2f, 1f)) { GhulSuiteApp(state) } }
                waitForIdle()
                block(state, fake)
            }
        } finally {
            // The app's state listens to AppLaunch (one for the whole process): an earlier test's
            // state must not take what a later test hands over.
            scope.coroutineContext[kotlinx.coroutines.Job]?.cancel()
        }
        assertTrue(errors.isEmpty(), "errors in the app: $errors")
        assertTrue(DevChecks.alarms.isEmpty(), "alarms of the development build: ${DevChecks.alarms.toList()}")
    }

    private fun DesktopComposeUiTest.shot(name: String) {
        waitForIdle()
        val bitmap = captureToImage().asSkiaBitmap()
        File(out, "ui-docs-$name.png").writeBytes(Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)!!.bytes)
    }

    private fun DesktopComposeUiTest.text(t: String): SemanticsNodeInteraction = onAllNodesWithText(t, useUnmergedTree = true).onFirst()
    private fun DesktopComposeUiTest.desc(d: String): SemanticsNodeInteraction = onAllNodesWithContentDescription(d).onFirst()
    private fun DesktopComposeUiTest.shown(t: String) = onAllNodesWithText(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.part(t: String) = onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.tagged(t: String) = onAllNodesWithTag(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.waitFor(t: String) = waitUntil(timeoutMillis = 10_000) { shown(t) }
    private fun DesktopComposeUiTest.waitPart(t: String) = waitUntil(timeoutMillis = 10_000) { part(t) }
    private fun DesktopComposeUiTest.waitGone(t: String) = waitUntil(timeoutMillis = 10_000) { !shown(t) }
    private fun DesktopComposeUiTest.dialogButton(t: String) = onAllNodes(hasText(t) and hasAnyAncestor(isDialog())).onFirst().performClick()

    private fun DesktopComposeUiTest.longPress(t: String) {
        waitFor(t)
        text(t).performTouchInput { longClick() }
        waitForIdle()
    }

    /** A file of [size] bytes as a picker hands it over; [known]: the picker knows its size. */
    private class Source(override val name: String, private val size0: Int, known: Boolean = true, override val mime: String = "") : ByteSource {
        override val size: Long = if (known) size0.toLong() else -1
        var pos = 0
        var closed = false
        override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
            if (pos >= size0) return -1
            val n = minOf(length, size0 - pos)
            buffer.fill(1, offset, offset + n)
            pos += n
            return n
        }
        override fun close() { closed = true }
    }

    /** Remembers what a job wrote and whether it threw the half file away. */
    private class Recording : ByteSink {
        val data = java.io.ByteArrayOutputStream()
        var closed = false
        var discarded = false
        override fun write(data: ByteArray) = this.data.write(data)
        override fun close() { closed = true }
        override fun discard() { discarded = true; data.reset() }
    }

    /** The list: folders first, then files, with size and date; into a folder and back (breadcrumb). */
    @Test
    fun listFoldersAndBreadcrumb() = app { state, fake ->
        waitFor("Mietvertrag.pdf")
        val y = { t: String -> text(t).fetchSemanticsNode().boundsInRoot.top }
        assertTrue(y("Steuer 2025") < y("Mietvertrag.pdf"), "folders come first")
        assertTrue(y("Versicherungen") < y("Notizen.txt"))
        // The usage over the list: "… of 1 GB" and the number of documents.
        waitPart("of 1 GB")
        assertTrue(part("6 documents"), "the trash counts too")
        assertFalse(shown("Alter Scan.pdf"), "what is in the trash is not in the folder")
        shot("list")
        text("Steuer 2025").performClick()
        waitFor("Lohnsteuerbescheinigung.pdf")
        assertTrue(tagged("docs-breadcrumb"))
        assertFalse(shown("Mietvertrag.pdf"))
        shot("folder")
        // The breadcrumb leads back to the top of the vault.
        onAllNodes(hasText("Dokumente") and hasAnyAncestor(androidx.compose.ui.test.hasTestTag("docs-breadcrumb")), useUnmergedTree = true).onFirst().performClick()
        waitFor("Mietvertrag.pdf")
        assertEquals("d1", state.vaults.first { it.kind == de.danerd.ghulsuite.model.VaultKindUi.DOCS }.id)
        assertTrue(fake.docs.calls.none { it.startsWith("add") })
    }

    /** Search over names, the favourites view, a new folder. */
    @Test
    fun searchFavoritesAndNewFolder() = app { _, fake ->
        waitFor("Mietvertrag.pdf")
        onAllNodes(androidx.compose.ui.test.hasSetTextAction()).onFirst().performTextInput("lohn")
        waitFor("Lohnsteuerbescheinigung.pdf")
        assertTrue(part("in Steuer 2025"), "a search says where")
        assertFalse(shown("Mietvertrag.pdf"))
        onAllNodes(androidx.compose.ui.test.hasSetTextAction()).onFirst().performTextInput("xyz")
        waitFor("Nothing found.")
        onAllNodes(androidx.compose.ui.test.hasSetTextAction()).onFirst().performTextReplacement("")
        waitFor("Notizen.txt")
        onAllNodesWithTag("docs-view-fav").onFirst().performClick()
        waitFor("Mietvertrag.pdf")
        waitGone("Notizen.txt")
        shot("favorites")
        onAllNodesWithTag("docs-crumb-0", useUnmergedTree = true).onFirst().performClick()
        waitFor("Notizen.txt")
        // "+" opens the add sheet: a new folder.
        desc("Add").performClick()
        waitFor("Add to GhulDocs")
        shot("add-sheet")
        text("New folder").performClick()
        waitFor("Name")
        onAllNodes(androidx.compose.ui.test.hasSetTextAction() and hasAnyAncestor(isDialog())).onFirst().performTextInput("Arzt")
        dialogButton("Create")
        waitFor("Arzt")
        assertTrue("createFolder:d1:null:Arzt" in fake.docs.calls, fake.docs.calls.toString())
    }

    /**
     * Adding files runs as a job with progress; "Cancel" keeps nothing of the file being read;
     * several files are added one after the other and closed in the end.
     */
    @Test
    fun addWithProgressAndCancel() = app { state, fake ->
        waitFor("Mietvertrag.pdf")
        fake.docs.sectionDelayMs = 400
        val big = Source("Scan gross.pdf", 2_000_000)
        runOnIdle { state.addDocs("d1", null, listOf(big)) }
        waitPart("Adding Scan gross.pdf")
        waitUntil(timeoutMillis = 10_000) { part(" / ") }
        shot("adding")
        onAllNodesWithTag(JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Adding Scan gross.pdf cancelled. Nothing of it was kept.")
        assertTrue(big.closed)
        assertTrue(fake.docs.calls.none { it.startsWith("add:") }, "nothing of a cancelled file stays")
        assertTrue(fake.docs.nodes.none { it.name.startsWith("Scan gross") })
        // Two files: one after the other, both added, both closed; a taken name gets a number.
        fake.docs.sectionDelayMs = 10
        val a = Source("Notizen.txt", 1000, mime = "text/plain")
        val b = Source("Quittung.jpg", 3000)
        runOnIdle { state.addDocs("d1", null, listOf(a, b)) }
        waitFor("2 files added.")
        assertTrue(a.closed && b.closed)
        waitFor("Notizen (2).txt")
        waitFor("Quittung.jpg")
        assertTrue(fake.docs.calls.any { it.startsWith("add:d1:null:Quittung.jpg:image/jpeg:3000") }, "the type from the name: ${fake.docs.calls}")
    }

    /** GhulDocs full: a clear message, nothing added (size known before, or found while reading). */
    @Test
    fun limitIsAClearMessage() = app { state, fake ->
        waitFor("Mietvertrag.pdf")
        val used = fake.docs.nodes.filter { !it.isFolder }.sumOf { it.data.size.toLong() }
        fake.docs.limit = used + 500
        runOnIdle { state.addDocs("d1", null, listOf(Source("Gross.pdf", 5000))) }
        val msg = "GhulDocs is full: ${formatSizeSi(used)} of ${formatSizeSi(used + 500)} used. Nothing was added."
        waitPart(msg)
        shot("limit")
        // Without a known size: refused as soon as it is too much, and nothing stays.
        runOnIdle { state.addDocs("d1", null, listOf(Source("Unbekannt.bin", 5000, known = false), Source("Noch eine.pdf", 100))) }
        waitPart("Unbekannt.bin, Noch eine.pdf: GhulDocs is full")
        assertTrue(fake.docs.nodes.none { it.name == "Gross.pdf" || it.name == "Unbekannt.bin" || it.name == "Noch eine.pdf" })
    }

    /**
     * The trash: moving there asks first ("Move to the trash?") and has "Undo"; in the trash
     * restore, delete for good (asks) and empty (asks).
     */
    @Test
    fun trashFlowAsksFirst() = app { state, fake ->
        state.undoMillis = 60_000
        longPress("Notizen.txt")
        waitFor("1 selected")
        shot("selected")
        desc("Move to trash").performClick()
        waitFor("Move to the trash?")
        dialogButton("Cancel")
        waitGone("Move to the trash?")
        assertTrue(fake.docs.calls.none { it.startsWith("toTrash") }, "Cancel keeps it")
        desc("Move to trash").performClick()
        waitFor("Move to the trash?")
        dialogButton("Move to trash")
        waitFor("Undo")
        waitGone("Notizen.txt")
        text("Undo").performClick()
        waitFor("Notizen.txt")
        assertTrue(fake.docs.calls.none { it.startsWith("toTrash") }, "Undo keeps it")
        state.undoMillis = 300
        longPress("Notizen.txt")
        desc("Move to trash").performClick()
        dialogButton("Move to trash")
        waitUntil(timeoutMillis = 10_000) { "toTrash:a3" in fake.docs.calls }
        // The trash: both items there, the latest first.
        onAllNodesWithTag("docs-view-trash").onFirst().performClick()
        waitFor("Alter Scan.pdf")
        waitFor("Notizen.txt")
        assertFalse(shown("Add"), "no adding in the trash")
        shot("trash")
        longPress("Notizen.txt")
        desc("Restore").performClick()
        waitUntil(timeoutMillis = 10_000) { "restore:a3" in fake.docs.calls }
        waitGone("Notizen.txt")
        longPress("Alter Scan.pdf")
        desc("Delete for good").performClick()
        waitFor("Delete?")
        dialogButton("Delete")
        waitUntil(timeoutMillis = 10_000) { "deleteForever:a6" in fake.docs.calls }
        // Empty: asks first.
        runOnIdle { fake.docs.nodes.first { it.id == "a5" }.trashed = 1 }
        runOnIdle { state.changed() }
        waitFor("Hausrat.docx")
        onAllNodesWithTag("docs-empty-trash").onFirst().performClick()
        waitFor("Empty the trash")
        shot("empty-trash-ask")
        dialogButton("Delete for good")
        waitUntil(timeoutMillis = 10_000) { fake.docs.calls.any { it.startsWith("emptyTrash") } }
        waitFor("The trash is empty.")
    }

    /** Several picked: favourite and move into a folder (the folder picker). */
    @Test
    fun selectionFavoriteAndMove() = app { _, fake ->
        longPress("Notizen.txt")
        text("Personalausweis.png").performClick()
        waitFor("2 selected")
        desc("Add to favourites").performClick()
        waitUntil(timeoutMillis = 10_000) { "favorite:a3:true" in fake.docs.calls && "favorite:a2:true" in fake.docs.calls }
        longPress("Notizen.txt")
        desc("More").performClick()
        waitFor("Move")
        text("Move").performClick()
        waitFor("Move here")
        shot("move")
        onAllNodes(hasText("Versicherungen") and hasAnyAncestor(isDialog()), useUnmergedTree = true).onFirst().performClick()
        waitFor("Dokumente › Versicherungen")
        dialogButton("Move here")
        waitUntil(timeoutMillis = 10_000) { "move:a3:f2" in fake.docs.calls }
        waitGone("Notizen.txt")
    }

    /** An image and a text are shown in the app (from memory); a PDF page by page. */
    @Test
    fun viewImageTextAndPdf() = app { state, fake ->
        waitFor("Personalausweis.png")
        text("Personalausweis.png").performClick()
        waitUntil(timeoutMillis = 10_000) { tagged("docs-image") }
        assertTrue("read:a2" in fake.docs.calls)
        shot("view-image")
        runOnIdle { state.pop() }
        waitFor("Notizen.txt")
        text("Notizen.txt").performClick()
        waitPart("Zählerstand Strom: 4711")
        shot("view-text")
        runOnIdle { state.pop() }
        waitFor("Mietvertrag.pdf")
        text("Mietvertrag.pdf").performClick()
        waitUntil(timeoutMillis = 10_000) { tagged("docs-pdf") }
        waitPart("2 pages")
        // The pages are drawn from memory once they come into view.
        waitUntil(timeoutMillis = 20_000) { tagged("docs-pdf-page") }
        shot("view-pdf")
        runOnIdle { state.pop() }
        // Another type: no preview, open or export instead.
        waitFor("Versicherungen")
        text("Versicherungen").performClick()
        text("Hausrat.docx").let { waitFor("Hausrat.docx"); it.performClick() }
        waitUntil(timeoutMillis = 10_000) { tagged("docs-no-preview") }
        assertFalse("read:a5" in fake.docs.calls, "nothing is read that cannot be shown")
        shot("view-none")
    }

    /** "Export" asks first (unencrypted), "Cancel" exports nothing; confirmed, the file is written whole. */
    @Test
    fun exportAsksFirst() {
        System.setProperty("ghul.pickers", "true")
        try {
            app { state, fake ->
                waitFor("Notizen.txt")
                text("Notizen.txt").performClick()
                waitPart("Zählerstand Strom")
                onAllNodesWithTag("docs-export").onFirst().performClick()
                waitFor("Export unencrypted?")
                shot("export-ask")
                dialogButton("Cancel")
                waitGone("Export unencrypted?")
                assertEquals(1, fake.docs.calls.count { it == "read:a3" }, "only the read for viewing")
                // Confirmed (the system's save dialog is not there in a test: the job directly).
                val item = runBlockingItem(fake, "a3")
                val sink = Recording()
                runOnIdle { state.exportDoc(item, sink) }
                waitFor("Notizen.txt exported (unencrypted).")
                assertTrue(sink.closed && !sink.discarded)
                assertEquals("Zählerstand Strom: 4711\nWasser: 815\n", sink.data.toString(Charsets.UTF_8))
            }
        } finally {
            System.clearProperty("ghul.pickers")
        }
    }

    private fun runBlockingItem(fake: ScreenshotTest.Fake, id: String) = kotlinx.coroutines.runBlocking { fake.docs.item("d1", id) }

    /**
     * A damaged document: what was written before the damage was found is thrown away, whether
     * exported, handed to another app or shown.
     */
    @Test
    fun partialOutputIsDiscarded() = app { state, fake ->
        waitFor("Versicherungen")
        fake.docs.section = 8_000
        fake.docs.nodes.first { it.id == "a5" }.damaged = true
        val item = runBlockingItem(fake, "a5")
        val sink = Recording()
        runOnIdle { state.exportDoc(item, sink) }
        waitPart("damaged or missing")
        waitUntil(timeoutMillis = 10_000) { sink.discarded }
        assertEquals(0, sink.data.size(), "nothing of the damaged document stays")
        runOnIdle { state.openDocWith(item) }
        waitUntil(timeoutMillis = 10_000) { !state.jobs.running }
        assertTrue(fake.shared.isEmpty(), "no half copy is handed over")
        // Shown: the reason, nothing of the content.
        fake.docs.nodes.first { it.id == "a3" }.damaged = true
        text("Notizen.txt").performClick()
        waitPart("damaged or missing")
        assertFalse(part("Zählerstand"))
        shot("view-damaged")
    }

    /** "Open with app": the whole document as a copy for the other app. */
    @Test
    fun openWithHandsOverACopy() = app { state, fake ->
        waitFor("Notizen.txt")
        val item = runBlockingItem(fake, "a3")
        runOnIdle { state.openDocWith(item) }
        waitUntil(timeoutMillis = 10_000) { fake.shared.isNotEmpty() }
        assertEquals("Notizen.txt", fake.shared[0].first)
        assertEquals("Zählerstand Strom: 4711\nWasser: 815\n", fake.shared[0].second.decodeToString())
    }

    /** Locking leaves nothing of GhulDocs: no page with a document, no chosen vault, no shared files. */
    @Test
    fun lockClearsState() = app { state, fake ->
        waitFor("Personalausweis.png")
        text("Personalausweis.png").performClick()
        waitUntil(timeoutMillis = 10_000) { tagged("docs-image") }
        runOnIdle { state.docsVault = "d1" }
        runOnIdle { state.lockNow() }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 1 }
        fake.unlocked.value = false
        waitForIdle()
        assertTrue(state.stack.isEmpty())
        assertFalse(tagged("docs-image"))
        assertNull(state.docsVault)
        assertNull(state.docsIncoming)
        assertFalse(part("Personalausweis"), "nothing of GhulDocs on the lock screen")
    }

    /** Files shared from another app: GhulDocs asks where they go, then adds them. */
    @Test
    fun sharedFilesAskWhereTheyGo() = app(setup = { tab = Tab.PASS }) { state, fake ->
        waitFor("Codeberg")
        val f = Source("Rechnung.pdf", 2000, mime = "application/pdf")
        runOnIdle { AppLaunch.docsFiles.value = listOf(f) }
        waitFor("Save in GhulDocs")
        assertEquals(Tab.DOCS, state.tab)
        shot("incoming")
        onAllNodesWithTag("docs-incoming-save").onFirst().performClick()
        waitFor("Rechnung.pdf added.")
        assertTrue(fake.docs.calls.any { it.startsWith("add:d1:null:Rechnung.pdf:application/pdf:2000") })
        assertTrue(f.closed)
    }

    /** Options → GhulDocs: usage, a check of every document with its result, emptying the trash. */
    @Test
    fun optionsPageVerifiesAndEmptiesTrash() = app(setup = { tab = Tab.SETTINGS; push(Screen.SettingsPage(SetPage.DOCS)) }) { _, fake ->
        waitPart("of 1 GB")
        waitFor("1 item in the trash, in all GhulDocs vaults.")
        onAllNodesWithTag("docs-verify-d1").onFirst().performClick()
        waitFor("All documents are intact.")
        fake.docs.badOnVerify = setOf("a4")
        onAllNodesWithTag("docs-verify-d1").onFirst().performClick()
        waitFor("1 document is damaged or missing")
        waitPart("Steuer 2025/Lohnsteuerbescheinigung.pdf")
        shot("options")
        onAllNodesWithTag("docs-settings-empty-trash").onFirst().performClick()
        waitFor("Empty the trash")
        dialogButton("Delete for good")
        waitUntil(timeoutMillis = 10_000) { "emptyTrash:null" in fake.docs.calls }
    }

    /** Several GhulDocs vaults: "All vaults" lists them, the chosen one opens; the header chooses too. */
    @Test
    fun severalVaultsAreChosen() = app(prepare = {
        vaults = vaults + de.danerd.ghulsuite.model.VaultUi("d2", de.danerd.ghulsuite.model.VaultKindUi.DOCS, "Steuer", "d2.ghldoc", true, 0)
    }) { state, _ ->
        waitFor("Steuer")
        waitFor("Dokumente")
        assertFalse(shown("Mietvertrag.pdf"))
        shot("vaults")
        text("Dokumente").performClick()
        waitFor("Mietvertrag.pdf")
        assertTrue(tagged("docs-breadcrumb"))
        onAllNodes(hasText("All vaults") and hasAnyAncestor(androidx.compose.ui.test.hasTestTag("docs-breadcrumb")), useUnmergedTree = true).onFirst().performClick()
        waitGone("Mietvertrag.pdf")
        runOnIdle { state.docsVault = "d1" }
        waitFor("Mietvertrag.pdf")
    }
}
