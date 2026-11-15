// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.VaultKindUi
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import kotlinx.coroutines.launch
import androidx.compose.ui.test.assertIsNotDisplayed
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.unit.Density
import de.danerd.ghulsuite.ui.downloadFromDrive
import de.danerd.ghulsuite.ui.DevChecks
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.CalView
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.HeaderGuard
import de.danerd.ghulsuite.ui.DesktopBack
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.model.NetBackend
import de.danerd.ghulsuite.model.SecurityIssueKindUi
import de.danerd.ghulsuite.model.SecurityIssueUi
import de.danerd.ghulsuite.model.SecurityReportUi
import de.danerd.ghulsuite.ui.SetPage
import de.danerd.ghulsuite.ui.Tab
import de.danerd.ghulsuite.ui.uploadToDrive
import de.danerd.ghulsuite.ui.runAutoArchive
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image
import java.awt.HeadlessException
import java.io.File
import java.util.Locale
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.performTextInput
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Clicks through the real screens with the fake backend of [ScreenshotTest]: the selection of
 * every list, deleting with "Undo", the menus behind the three dots and the questions that are
 * left (vault, area, emptying the trash). Every checked state is also saved as a picture
 * (build/screenshots/ui-*.png) to look at.
 */
@OptIn(ExperimentalTestApi::class)
class InteractionTest {
    private val out = File("build/screenshots").apply { mkdirs() }

    /** The app in English on a phone (412 × 892) or a wide window (1280 × 820), unlocked. */
    private fun app(
        wide: Boolean = false,
        /** Window size in dp instead of the phone or wide window (e.g. a phone lying sideways). */
        size: Pair<Int, Int>? = null,
        /** Font size of the system (1.3: "large", as many set it on their phone). */
        fontScale: Float = 1f,
        setup: AppState.() -> Unit = {},
        prepare: ScreenshotTest.Fake.() -> Unit = {},
        block: DesktopComposeUiTest.(AppState, ScreenshotTest.Fake) -> Unit,
    ) {
        Locale.setDefault(Locale.ENGLISH)
        // As in the development build: dropped follow-up actions and unexpected errors are alarms.
        DevChecks.enabled = true
        DevChecks.alarms.clear()
        // No "Save" of an earlier test holds the lock back.
        HeaderGuard.reset()
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        // Texts that AppState looks up outside of the screen need a display (screen resolution);
        // the tests run without one, so only that failure is let through, every other one counts.
        val errors = java.util.Collections.synchronizedList(mutableListOf<Throwable>())
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined + CoroutineExceptionHandler { _, e -> if (e !is HeadlessException) errors += e })
        val state = AppState(fake, scope).apply(setup)
        val (w, h) = size ?: if (wide) 1280 to 820 else 412 to 892
        runDesktopComposeUiTest(w * 2, h * 2) {
            setContent { CompositionLocalProvider(LocalDensity provides Density(2f, fontScale)) { GhulSuiteApp(state) } }
            waitForIdle()
            block(state, fake)
        }
        assertTrue(errors.isEmpty(), "errors in the app: $errors")
        assertTrue(DevChecks.alarms.isEmpty(), "alarms of the development build: ${DevChecks.alarms.toList()}")
    }

    private fun DesktopComposeUiTest.shot(name: String) {
        waitForIdle()
        val bitmap = captureToImage().asSkiaBitmap()
        File(out, "ui-$name.png").writeBytes(Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)!!.bytes)
    }

    private fun DesktopComposeUiTest.text(t: String): SemanticsNodeInteraction = onAllNodesWithText(t, useUnmergedTree = true).onFirst()
    private fun DesktopComposeUiTest.desc(d: String): SemanticsNodeInteraction = onAllNodesWithContentDescription(d).onFirst()
    private fun DesktopComposeUiTest.shown(t: String) = onAllNodesWithText(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.waitFor(t: String) = waitUntil(timeoutMillis = 10_000) { shown(t) }
    private fun DesktopComposeUiTest.waitGone(t: String) = waitUntil(timeoutMillis = 10_000) { !shown(t) }
    /** F103: every deletion asks first; answers the question with [button]. */
    private fun DesktopComposeUiTest.confirm(button: String? = null) {
        waitUntil(timeoutMillis = 10_000) { shown("Delete?") || shown("Move to the trash?") }
        val b = button ?: if (shown("Move to the trash?")) "Move to trash" else "Delete"
        onAllNodes(hasText(b) and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitUntil(timeoutMillis = 10_000) { !shown("Delete?") && !shown("Move to the trash?") }
    }

    private fun DesktopComposeUiTest.longPress(t: String) {
        waitFor(t)
        text(t).performTouchInput { longClick() }
        waitForIdle()
    }

    /**
     * The same in every area: a long press selects (the header shows "1 selected"), delete hides the
     * item at once with "Undo"; Undo brings it back and nothing reaches the backend. Deleting
     * again and waiting carries it out.
     */
    private fun DesktopComposeUiTest.deleteUndoDelete(
        state: AppState, item: String, delete: String, message: String, name: String, called: () -> Boolean,
    ) {
        state.undoMillis = 60_000
        longPress(item)
        waitFor("1 selected")
        shot("$name-selected")
        desc(delete).performClick()
        // F103: nothing happens before the question is answered; "Cancel" keeps everything.
        waitUntil(timeoutMillis = 10_000) { shown("Delete?") || shown("Move to the trash?") }
        assertFalse(shown("Undo"))
        onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitForIdle()
        assertTrue(shown(item), "$name: Cancel keeps it")
        assertFalse(called(), "$name: Cancel must not reach the backend")
        // The selection is still there: delete again and answer "Delete".
        desc(delete).performClick()
        confirm()
        waitFor("Undo")
        assertTrue(shown(message), "message \"$message\" after deleting in $name")
        waitGone(item)
        shot("$name-undo")
        text("Undo").performClick()
        waitFor(item)
        assertFalse(called(), "$name: Undo must not reach the backend")
        // Without Undo it is carried out.
        state.undoMillis = 300
        longPress(item)
        desc(delete).performClick()
        confirm()
        waitUntil(timeoutMillis = 10_000) { called() }
    }

    @Test
    fun passSelectionMenuAndUndo() = app { state, fake ->
        longPress("Codeberg")
        waitFor("1 selected")
        // One entry chosen: copy and edit are in the sheet behind "More".
        desc("More").performClick()
        waitFor("Copy password")
        assertTrue(shown("Edit"))
        shot("pass-selection-sheet")
        text("Copy password").performClick()
        waitGone("1 selected")
        deleteUndoDelete(state, "Codeberg", "Delete", "1 deleted", "pass") { "deleteEntry:p1:1" in fake.calls }
        // The menu of an open entry.
        waitFor("Jira")
        text("Jira").performClick()
        waitFor("Username")
        desc("More").performClick()
        waitFor("Delete")
        shot("pass-detail-sheet")
    }

    /**
     * F103: swiping a row to the left only asks; the account stays until "Delete" is tapped
     * (bug: the swipe deleted it at once, without a question).
     */
    @Test
    fun authSwipeAsksFirst() = app(setup = { tab = Tab.AUTH }) { state, fake ->
        state.undoMillis = 60_000
        waitFor("GitHub")
        text("GitHub").performTouchInput { swipeLeft() }
        waitFor("Delete?")
        assertFalse(shown("Undo"), "nothing deleted before the answer")
        shot("auth-swipe-ask")
        onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitGone("Delete?")
        assertTrue(shown("GitHub"), "Cancel keeps the account")
        text("GitHub").performTouchInput { swipeLeft() }
        confirm()
        waitFor("Undo")
        waitGone("GitHub")
        assertFalse("deleteTotp:t1:b" in fake.calls, "Undo is still possible")
    }

    @Test
    fun authSelectionMenuAndUndo() = app(setup = { tab = Tab.AUTH }) { state, fake ->
        longPress("GitHub")
        waitFor("1 selected")
        desc("More").performClick()
        waitFor("Export as QR code")
        assertTrue(shown("Add to favourites"))
        shot("auth-selection-sheet")
        text("Add to favourites").performClick()
        waitGone("1 selected")
        deleteUndoDelete(state, "GitHub", "Delete", "1 deleted", "auth") { "deleteTotp:t1:b" in fake.calls }
    }

    /**
     * Share links always come from GhulDrive's server through its module: the form offers expiry
     * and download limits, and no choice of server.
     */
    @Test
    fun shareFormHasExpiryAndLimitsWithoutServerChips() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val net = fake.net as FakeNet
        longPress("Notizen.txt")
        waitFor("1 selected")
        desc("Share").performClick()
        waitFor("Share link")
        waitFor("Encrypted link with expiry, download limit and password")
        text("Share link").performClick()
        waitFor("Create link")
        for (chip in listOf("1 day", "7 days", "30 days", "No expiry", "Unlimited", "1× (one-time)", "3×", "5×")) {
            assertTrue(shown(chip), "missing: $chip")
        }
        // No server chips, no "until withdrawn" of the old share folders.
        for (name in listOf("Heimserver", "Webspace", "Server")) {
            assertTrue(onAllNodes(hasText(name) and hasAnyAncestor(isDialog())).fetchSemanticsNodes().isEmpty(), "no choice of server: $name")
        }
        assertFalse(shown("Valid until you withdraw it"))
        // One file: its name is the title, no title field.
        assertFalse(shown("Title of the link"))
        shot("share-form")
        text("1 day").performClick()
        text("3×").performClick()
        text("Create link").performClick()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it.startsWith("shareDrive:") } }
        assertEquals("shareDrive:f2::1:3:false", net.calls.last { it.startsWith("shareDrive:") })
    }

    /** Several files: the link gets a title, and the recipient gets every file on its own. */
    @Test
    fun shareOfSeveralFilesHasATitle() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val net = fake.net as FakeNet
        longPress("Notizen.txt")
        waitFor("1 selected")
        text("Steuer 2025.pdf").performClick()
        waitFor("2 selected")
        desc("Share").performClick()
        waitFor("Share link")
        text("Share link").performClick()
        waitFor("Title of the link")
        assertTrue(shown("The recipient gets every file on its own."))
        onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog())).onFirst().performTextReplacement("Unterlagen")
        text("Create link").performScrollTo().performClick()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it.startsWith("shareDrive:") } }
        assertTrue(net.calls.last { it.startsWith("shareDrive:") }.contains(":Unterlagen:7:0:false"), net.calls.toString())
    }

    /** Without the module on GhulDrive's server, "Share link" says so and opens its installation. */
    @Test
    fun shareSheetWithoutModuleOffersInstallation() = app(setup = { tab = Tab.DRIVE }, prepare = { (net as FakeNet).missingModule = "s2" }) { state, _ ->
        longPress("Notizen.txt")
        waitFor("1 selected")
        desc("Share").performClick()
        waitFor("Needs the GhulSuite server module. Tap to install it.")
        shot("share-sheet-no-module")
        text("Share link").performClick()
        waitFor("Install and check")
        assertEquals(Screen.ModuleInstall("s2"), state.stack.lastOrNull())
        assertFalse(shown("Create link"))
    }

    @Test
    fun driveSelectionSheetsAndTrash() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        longPress("Notizen.txt")
        waitFor("1 selected")
        // In "My files" the heading says where; no path line with the same words above it.
        assertEquals(1, onAllNodesWithText("My files", ignoreCase = true).fetchSemanticsNodes().size)
        // Three actions: all of them icons in the header.
        assertTrue(onAllNodesWithContentDescription("Share").fetchSemanticsNodes().isNotEmpty())
        desc("Cancel").performClick()
        waitGone("1 selected")
        // Into the trash at once (F96: the count follows at once); "Undo" takes it out again.
        state.undoMillis = 60_000
        longPress("Notizen.txt")
        desc("Move to trash").performClick()
        confirm("Move to trash")
        waitFor("Undo")
        assertTrue(shown("Moved to trash"))
        waitGone("Notizen.txt")
        assertTrue("driveDelete:f2" in net.calls)
        shot("drive-undo")
        text("Undo").performClick()
        waitFor("Notizen.txt")
        assertTrue("driveRestore:f2" in net.calls)
        // The sheet of a file.
        waitFor("Steuer 2025.pdf")
        text("Steuer 2025.pdf").performClick()
        waitFor("Move to trash")
        shot("drive-file-sheet")
    }

    /**
     * Opening a folder on a slow server: the list of the folder before is not shown under the new
     * path while the new one loads (it looked like the inside of the folder just opened).
     */
    @Test
    fun driveFolderDoesNotShowTheOldListWhileLoading() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val net = fake.net as FakeNet
        waitFor("Fotos")
        net.driveListDelayMs = 1500
        text("Fotos").performClick()
        waitFor("Loading …")
        assertTrue(onAllNodesWithText("Notizen.txt").fetchSemanticsNodes().isEmpty())
        assertTrue(onAllNodesWithText("Verträge").fetchSemanticsNodes().isEmpty())
        waitUntil(timeoutMillis = 10_000) { onAllNodesWithText("Strand.jpg").fetchSemanticsNodes().isNotEmpty() }
        net.driveListDelayMs = 0
    }

    /**
     * The line under "GhulDrive" names the server at once, also while Drive's index still comes
     * from a slow server (bug: after a restart it stayed empty until the index had arrived).
     */
    @Test
    fun driveHeaderNamesTheServerWhileTheIndexLoads() {
        val gate = java.util.concurrent.CountDownLatch(1)
        try {
            app(setup = { tab = Tab.DRIVE }, prepare = { (net as FakeNet).driveStatusGate = gate }) { _, _ ->
                waitUntil(timeoutMillis = 10_000) { onAllNodesWithText("· My files", substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
                assertEquals(1L, gate.count, "the index was still loading")
                gate.countDown()
            }
        } finally {
            gate.countDown()
        }
    }

    /** F96: the number of files follows a change at once (bug: only after leaving and opening the app). */
    @Test
    fun driveFileCountFollowsChanges() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val part = { t: String -> onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        (fake.net as FakeNet).driveFiles = 4
        // Refreshing the list (as after an upload) also counts again.
        text("Up to date").performClick()
        waitUntil(timeoutMillis = 10_000) { part("4 files") }
        // Into the trash: the number drops at once, not only when "Undo" is gone.
        text("Notizen.txt").performTouchInput { longClick() }
        waitFor("1 selected")
        desc("Move to trash").performClick()
        confirm("Move to trash")
        waitUntil(timeoutMillis = 2_000) { part("3 files") }
        assertTrue("driveDelete:f2" in (fake.net as FakeNet).calls)
    }

    /**
     * F99: several files at once (picked together or shared from another app): one after the
     * other with "2 of 3" in the bar, the count follows each file, a failed file does not stop the
     * others, every file is closed again.
     */
    @Test
    fun driveUploadsSeveralFilesInARow() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        val part = { t: String -> onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        val closed = java.util.Collections.synchronizedList(mutableListOf<String>())
        fun src(name: String) = object : de.danerd.ghulsuite.model.ByteSource {
            var left = 3000
            override val name = name
            override val size = 3000L
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                if (left <= 0) return -1
                val n = minOf(length, left); left -= n; return n
            }
            override fun close() { closed += name }
        }
        net.driveUploadMs = 1500
        net.driveUploadFails = setOf("b.txt")
        runOnIdle { state.driveIncoming = listOf(src("a.txt"), src("b.txt"), src("c.txt")) }
        // The strip under the header: what runs and how far ("0 of 3 done · a.txt").
        waitUntil(timeoutMillis = 10_000) { part("Uploading 3 files") && part("0 of 3 done") }
        shot("drive-upload-several")
        waitUntil(timeoutMillis = 10_000) { part("2 of 3 done") }
        // a.txt is in, the count follows at once (b fails, c still runs).
        waitUntil(timeoutMillis = 10_000) { part("4 files") }
        waitUntil(timeoutMillis = 15_000) { part("2 files uploaded") }
        assertTrue("driveUpload::a.txt" in net.calls && "driveUpload::c.txt" in net.calls)
        assertFalse(net.calls.any { it.endsWith(":b.txt") })
        waitUntil(timeoutMillis = 10_000) { part("5 files") }
        assertTrue(part("b.txt: "), "the failed file is named")
        assertTrue(closed.containsAll(listOf("a.txt", "b.txt", "c.txt")))
    }

    /** F96: the server goes away while several files go to the trash: what moved shows as moved. */
    @Test
    fun driveTrashFailingHalfwayShowsWhatIsTrue() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val net = fake.net as FakeNet
        val part = { t: String -> onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        net.driveDeleteFails = "f2"
        longPress("Steuer 2025.pdf")
        text("Notizen.txt").performClick()
        waitFor("2 selected")
        desc("Move to trash").performClick()
        confirm("Move to trash")
        // Steuer went to the trash, Notizen did not: the list and the count say so at once (not
        // only when the time for "Undo" is over), the error too.
        waitGone("Steuer 2025.pdf")
        waitUntil(timeoutMillis = 2_000) { part("2 files") }
        assertTrue(shown("Notizen.txt"))
        waitUntil(timeoutMillis = 10_000) { part("connection reset") || part("No connection") || part("Connection") }
    }

    /** Windows: Esc, Alt+Left and the mouse's back button go back one step (bug: they did nothing). */
    @Test
    fun windowsBackGoesBackOneStep() = app(wide = true, setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, _ ->
        waitFor("Abendessen mit Jana")
        // Nothing to go back to: nothing happens (the window does not close).
        assertFalse(runOnIdle { DesktopBack.dispatch() })
        text("Abendessen mit Jana").performClick()
        waitFor("Trattoria Da Luca")
        assertTrue(runOnIdle { DesktopBack.dispatch() })
        waitUntil(timeoutMillis = 10_000) { state.stack.isEmpty() }
        waitFor("Kundentermin Müller GmbH")
        // A selection is left first, the page stays.
        text("Abendessen mit Jana").performTouchInput { longClick() }
        waitFor("1 selected")
        assertTrue(runOnIdle { DesktopBack.dispatch() })
        waitGone("1 selected")
        assertTrue(state.stack.isEmpty())
        // The keys and the mouse button that mean "back".
        assertTrue(DesktopBack.isBackKey(androidx.compose.ui.input.key.Key.Escape, alt = false))
        assertTrue(DesktopBack.isBackKey(androidx.compose.ui.input.key.Key.DirectionLeft, alt = true))
        assertFalse(DesktopBack.isBackKey(androidx.compose.ui.input.key.Key.DirectionLeft, alt = false))
        // Esc held down repeats "pressed": still only one step back.
        text("Abendessen mit Jana").performClick()
        waitFor("Trattoria Da Luca")
        val esc = androidx.compose.ui.input.key.Key.Escape
        runOnIdle {
            assertTrue(DesktopBack.onKey(esc, false, androidx.compose.ui.input.key.KeyEventType.KeyDown))
            DesktopBack.onKey(esc, false, androidx.compose.ui.input.key.KeyEventType.KeyDown)
            DesktopBack.onKey(esc, false, androidx.compose.ui.input.key.KeyEventType.KeyUp)
        }
        waitUntil(timeoutMillis = 10_000) { state.stack.isEmpty() }
        assertEquals(Tab.CAL, state.tab)
        val released = java.awt.event.MouseEvent.MOUSE_RELEASED
        assertTrue(DesktopBack.isBackButton(released, 4))
        assertFalse(DesktopBack.isBackButton(released, 1))
        assertFalse(DesktopBack.isBackButton(java.awt.event.MouseEvent.MOUSE_PRESSED, 4))
    }

    /** F91: without a connection GhulDrive says so and still lists the folders it already knows. */
    @Test
    fun driveOfflineKeepsTheKnownFolders() = app(setup = { tab = Tab.DRIVE }, prepare = { (net as FakeNet).driveOffline = true }) { _, _ ->
        waitUntil(timeoutMillis = 10_000) { onAllNodesWithText("Timed out.", substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        waitFor("Try again")
        waitFor("Fotos")
        waitFor("Steuer 2025.pdf")
    }

    @Test
    fun driveEmptyTrashAsks() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        // Emptying the trash still asks, with a red button.
        val net = fake.net as FakeNet
        waitFor("Trash")
        assertEquals(1, onAllNodesWithTag(de.danerd.ghulsuite.ui.FAB_TAG).fetchSemanticsNodes().size)
        text("Trash").performClick()
        waitFor("Alter Entwurf.docx")
        // Nothing can be added to the trash: no "Add" button there.
        waitUntil(timeoutMillis = 5_000) { onAllNodesWithTag(de.danerd.ghulsuite.ui.FAB_TAG).fetchSemanticsNodes().isEmpty() }
        text("Empty").performClick()
        waitFor("Delete for good")
        shot("drive-empty-trash")
        text("Delete for good").performClick()
        waitUntil(timeoutMillis = 10_000) { "driveEmptyTrash" in net.calls }
    }

    /**
     * A row of the list stands for a whole conversation: deleting it deletes all its mails (bug: only
     * the newest one, the older one then came back as its own row), and "Select all" counts rows.
     */
    @Test
    fun mailConversationRowActsOnAllItsMails() = app(setup = { tab = Tab.MAIL }, prepare = { net = FakeNet(conversation = true) }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        assertTrue(onAllNodesWithText("Re: Treffen am Freitag").fetchSemanticsNodes().isEmpty())
        state.undoMillis = 300
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        desc("All").performClick()
        // Three rows (the conversation counts once), not four mails.
        waitFor("3 selected")
        desc("Cancel").performClick()
        waitGone("3 selected")
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        desc("Delete").performClick()
        confirm()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":3") } && net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":5") } }
        assertTrue(onAllNodesWithText("Re: Treffen am Freitag").fetchSemanticsNodes().isEmpty())
    }

    /** With every row deleted the list says "No messages." and no longer explains swiping rows. */
    @Test
    fun mailSwipeHintOnlyWithRows() = app(setup = { tab = Tab.MAIL }, prepare = { net = FakeNet(conversation = true) }) { state, _ ->
        val hint = "Swipe: right toggles read, left does the action from Options (archive, delete or read)."
        waitFor("Treffen am Freitag")
        assertTrue(shown(hint))
        state.undoMillis = 10_000
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        desc("All").performClick()
        waitFor("3 selected")
        desc("Delete").performClick()
        confirm()
        waitFor("No messages.")
        assertFalse(shown(hint), "swipe hint below an empty list")
        shot("mail-all-deleted")
    }

    /**
     * The selection holds the conversation: after switching to "Unread" the row stays selected and
     * acts on what the row now shows (the unread mail), not on nothing and not on hidden mails.
     */
    @Test
    fun mailSelectionFollowsTheFilterChips() = app(setup = { tab = Tab.MAIL }, prepare = { net = FakeNet(conversation = true) }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        state.undoMillis = 300
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        text("Unread").performClick()
        waitFor("1 selected")
        desc("Delete").performClick()
        confirm()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":3") } }
        assertFalse(net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":5") }, "the read mail was not shown, so it is not deleted")
    }

    /**
     * The mail search runs by itself after typing (bug: only with the search key, so after typing
     * and closing the keyboard the whole inbox stayed, as if nothing matched the words).
     */
    @Test
    fun mailSearchRunsWithoutTheSearchKey() = app(setup = { tab = Tab.MAIL }) { _, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        onNode(hasSetTextAction() and hasText("Search mails", substring = true)).performTextInput("nichts-dergleichen")
        waitFor("No messages found.")
        assertTrue(net.calls.contains("mailSearch:nichts-dergleichen"))
        assertTrue(onAllNodesWithText("Treffen am Freitag").fetchSemanticsNodes().isEmpty())
        // A refresh (new mail, the button) repeats the search, it does not bring the folder back
        // under the words.
        desc("Refresh").performClick()
        waitUntil(timeoutMillis = 10_000) { net.calls.count { it == "mailSearch:nichts-dergleichen" } >= 2 }
        waitFor("No messages found.")
        assertTrue(onAllNodesWithText("Treffen am Freitag").fetchSemanticsNodes().isEmpty())
        // Emptying the field shows the folder again.
        onNode(hasSetTextAction() and hasText("nichts-dergleichen", substring = true)).performTextReplacement("")
        waitFor("Treffen am Freitag")
    }

    /** A failed search shows its error alone: not the folder under the words, no endless loading. */
    @Test
    fun mailSearchFailureShowsTheErrorNotTheFolder() = app(setup = { tab = Tab.MAIL }) { _, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        net.mailSearchFails = true
        onNode(hasSetTextAction() and hasText("Search mails", substring = true)).performTextInput("Treffen")
        waitFor("Try again")
        assertTrue(onAllNodesWithText("Treffen am Freitag").fetchSemanticsNodes().isEmpty())
        assertTrue(onAllNodesWithText("Loading …").fetchSemanticsNodes().isEmpty())
        // Try again repeats the search, which now finds the mail.
        net.mailSearchFails = false
        text("Try again").performClick()
        waitFor("Treffen am Freitag")
    }

    @Test
    fun mailSelectionReaderAndUndo() = app(setup = { tab = Tab.MAIL }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        // Five actions: read, delete and "More" for the rest.
        desc("More").performClick()
        waitFor("Archive")
        assertTrue(shown("Move"))
        shot("mail-selection-sheet")
        text("Flag").performClick()
        waitGone("1 selected")
        deleteUndoDelete(state, "Treffen am Freitag", "Delete", "Moved to trash", "mail") { net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":3") } }
        // The menu of an open mail.
        waitFor("Notiz an mich")
        text("Notiz an mich").performClick()
        waitFor("Reply")
        desc("More").performClick()
        waitFor("Move")
        shot("mail-reader-sheet")
    }

    @Test
    fun mailEncryptedSubject() = app(setup = { tab = Tab.MAIL }, prepare = { net = FakeNet(encryptedMail = true) }) { _, _ ->
        // An encrypted mail keeps its subject inside: the list says "Encrypted message" once, never "...".
        waitFor("Jana Beispiel")
        assertTrue(shown("Encrypted message"))
        assertFalse(shown("..."), "the outer subject \"...\" is not shown")
        assertTrue(onAllNodesWithText("Encrypted message", useUnmergedTree = true).fetchSemanticsNodes().size == 1, "said once per mail")
        shot("mail-encrypted")
    }

    @Test
    fun calSelectionEventAndUndo() = app(setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, fake ->
        val cal = fake.cal as FakeCal
        waitFor("Abendessen mit Jana")
        deleteUndoDelete(state, "Abendessen mit Jana", "Delete", "Event deleted", "cal") { cal.calls.any { it.startsWith("deleteEvent:9:") } }
        // The menu of an open event.
        waitFor("Kundentermin Müller GmbH")
        text("Kundentermin Müller GmbH").performClick()
        waitFor("Besprechungsraum 2")
        desc("More").performClick()
        waitFor("Duplicate")
        shot("cal-event-sheet")
    }

    @Test
    fun vaultAndAreaAskFirst() = app(setup = { push(Screen.SettingsPage(SetPage.VAULTS)) }) { _, fake ->
        waitFor("Privat")
        desc("More").performClick()
        waitFor("Rename")
        shot("vault-sheet")
        text("Delete").performClick()
        waitFor("Delete “Privat”?")
        shot("vault-delete")
        assertFalse(fake.calls.any { it.startsWith("removeVault") }, "a vault is only deleted after confirming")
    }

    /** F105: setting up GhulMail under "Areas" opens the sheet for the mail account at once. */
    @Test
    fun mailAreaOpensTheAccountSheet() = app(
        setup = { push(Screen.SettingsPage(SetPage.MODULES)) },
        prepare = { vaults = vaults.filter { it.kind != VaultKindUi.MAIL }; net = FakeNet(configured = false) },
    ) { state, _ ->
        waitFor("GhulMail")
        text("GhulMail").performClick()
        waitFor("Set up GhulMail")
        assertFalse(shown("Add e-mail account"))
        onAllNodes(hasText("Set up") and hasClickAction()).onFirst().performClick()
        waitFor("Add e-mail account")
        assertTrue(state.mailAccountSheet != null)
        shot("mail-area-account-sheet")
    }

    /** F105: showing GhulMail again while it has no account opens the account sheet as well. */
    @Test
    fun mailShownAgainOpensTheAccountSheet() = app(
        setup = { backend.settings.moduleGhulMail = false; push(Screen.SettingsPage(SetPage.MODULES)) },
        prepare = { net = FakeNet(configured = false) },
    ) { state, _ ->
        waitFor("GhulMail")
        text("GhulMail").performClick()
        waitFor("Show on this device")
        assertTrue(state.mailAccountSheet == null)
        text("Show on this device").performClick()
        waitFor("Add e-mail account")
        assertTrue(state.mailAccountSheet != null)
    }

    /**
     * F103: "Delete" in an open mail asks first; "Cancel" keeps the mail open and deletes nothing
     * (bug: it went to the trash at once).
     */
    @Test
    fun mailReaderDeleteAsks() = app(setup = { tab = Tab.MAIL }) { state, fake ->
        val net = fake.net as FakeNet
        state.undoMillis = 300
        waitFor("Treffen am Freitag")
        text("Treffen am Freitag").performClick()
        waitFor("Reply")
        desc("Delete").performClick()
        waitFor("Move to the trash?")
        onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitGone("Move to the trash?")
        assertTrue(shown("Reply"), "Cancel keeps the mail open")
        Thread.sleep(600)
        assertFalse(net.calls.any { it.startsWith("mailDelete:") }, net.calls.toString())
        desc("Delete").performClick()
        confirm()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it.startsWith("mailDelete:") && it.endsWith(":3") } }
    }

    @Test
    fun areaRemoveAsks() = app(setup = { push(Screen.SettingsPage(SetPage.CAL)) }) { _, _ ->
        waitFor("Remove area")
        text("Remove area").performScrollTo().performClick()
        waitFor("Remove GhulCal?")
        shot("area-remove")
    }

    /** A count before a noun reads "1 entry" for one and "2 entries" otherwise (plural texts). */
    @Test
    fun countsUseTheSingularForOne() {
        fun weak(vaultId: String, id: String, title: String) =
            SecurityIssueUi(vaultId, "Arbeit", id, title, SecurityIssueKindUi.WEAK)
        app(setup = { passVault = "p2" }, prepare = { report = SecurityReportUi(80, 1, listOf(weak("p2", "2", "Jira"))) }) { _, _ ->
            // The section label is upper-cased.
            waitFor("1 ENTRY")
            waitFor("1 entry needs attention")
            assertFalse(shown("1 ENTRIES"))
            shot("pass-one-entry")
        }
        app(prepare = { report = SecurityReportUi(60, 5, listOf(weak("p2", "2", "Jira"), weak("p1", "1", "Codeberg"))) }) { _, _ ->
            waitFor("5 ENTRIES")
            waitFor("2 entries need attention")
        }
    }

    /**
     * A double tap on "Save" in an editor must not lock the app with the lock that is behind "Save";
     * a tap on the lock after "Back" (no Save) locks at once. The grace is set long and then off,
     * so the test does not depend on how fast the machine is.
     */
    @Test
    fun lockRightAfterSavingIsIgnored() = app(setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, fake ->
        try {
            HeaderGuard.grace = kotlin.time.Duration.parse("60s")
            waitFor("Abendessen mit Jana")
            text("Abendessen mit Jana").performClick()
            waitFor("Trattoria Da Luca")
            desc("Edit").performClick()
            waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEdit }
            text("Save").performClick()
            waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEvent }
            // The second tap of the double tap lands on the lock of the event screen.
            desc("Lock now").performClick()
            waitForIdle()
            assertEquals(0, fake.locks, "the lock took the second tap on Save")
            assertTrue(state.stack.lastOrNull() is Screen.CalEvent)
            // Once the grace is over, the lock works as before.
            HeaderGuard.grace = kotlin.time.Duration.ZERO
            desc("Lock now").performClick()
            waitUntil(timeoutMillis = 10_000) { fake.locks == 1 }
        } finally {
            HeaderGuard.reset()
        }
    }

    /** Without "Save" before it, the lock locks at once, also right after going back. */
    @Test
    fun lockAfterBackLocksAtOnce() = app(setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, fake ->
        try {
            HeaderGuard.grace = kotlin.time.Duration.parse("60s")
            waitFor("Abendessen mit Jana")
            text("Abendessen mit Jana").performClick()
            waitFor("Trattoria Da Luca")
            desc("Edit").performClick()
            waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEdit }
            state.back()
            waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEvent }
            desc("Lock now").performClick()
            waitUntil(timeoutMillis = 10_000) { fake.locks == 1 }
        } finally {
            HeaderGuard.reset()
        }
    }

    /**
     * A day whose only event is late in the evening: the day view shows it without scrolling by
     * hand (bug: "1 event" above an empty grid from 6 to 17 o'clock), also the week view; the
     * switch between the views stays usable.
     */
    @Test
    fun dayViewShowsAnEveningEvent() = app(setup = { tab = Tab.CAL; calView = CalView.DAY; calSel = de.danerd.ghulsuite.ui.today().plus(75) }) { state, _ ->
        waitFor("Spätvorstellung Kino")
        waitUntil(timeoutMillis = 10_000) {
            runCatching { text("Spätvorstellung Kino").assertIsDisplayed() }.isSuccess
        }
        // The view switch stays in sight (bug: scrolling the whole page pushed it away).
        text("Day").assertIsDisplayed()
        shot("cal-day-evening")
        state.calView = CalView.WEEK
        waitUntil(timeoutMillis = 10_000) {
            runCatching { text("Spätvorstellung Kino").assertIsDisplayed() }.isSuccess
        }
        text("Day").assertIsDisplayed()
        text("Day").performClick()
        waitUntil(timeoutMillis = 10_000) { state.calView == CalView.DAY }
    }

    /** The strength of a new password shows once something is typed: an empty field is not "very weak". */
    @Test
    fun passwordStrengthOnlyAfterTyping() = app(setup = { tab = Tab.SETTINGS; push(Screen.ChangePassword) }) { _, _ ->
        waitFor("New master password")
        assertFalse(shown("very weak"))
        assertFalse(shown("weak"))
        // Current, new, repeat: the second field is the new password.
        onAllNodes(hasSetTextAction())[1].performTextInput("kurz")
        waitFor("weak")
    }

    /** F90: the start-area hint names the areas, not the internal tab codes. */
    @Test
    fun startHintNamesTheAreas() = app(setup = { tab = Tab.SETTINGS; push(Screen.SettingsPage(SetPage.LOOK)) }) { _, _ ->
        val part = { t: String -> onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        waitUntil(timeoutMillis = 10_000) { part("Order: GhulPass, GhulAuth") }
        assertFalse(part("PASS, AUTH"))
    }

    /** The sidebar names the key for locking (Ctrl+L) on Windows only; a phone lying sideways has no keyboard. */
    @Test
    fun sidebarNamesTheLockKeyOnlyOnWindows() {
        app(wide = true) { _, _ ->
            waitFor("Lock")
            assertFalse(shown("Ctrl+L"))
        }
        app(wide = true, prepare = { desktop = true }) { _, _ ->
            waitFor("Ctrl+L")
        }
    }

    /** Wide window: a message shows once, not in the column and in the window at the same time. */
    @Test
    fun wideWindowShowsAMessageOnce() = app(wide = true) { state, _ ->
        waitFor("GhulAuth")
        state.scope.launch { state.message("Nachricht für den Test") }
        waitFor("Nachricht für den Test")
        waitForIdle()
        assertEquals(1, onAllNodesWithText("Nachricht für den Test", useUnmergedTree = true).fetchSemanticsNodes().size)
    }

    /**
     * Wide window: switching the area in the sidebar while an editor has unsaved input asks first
     * (as "back" does) instead of dropping the input.
     */
    @Test
    fun sidebarAsksBeforeDroppingUnsavedInput() = app(wide = true, setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, _ ->
        waitFor("Abendessen mit Jana")
        text("Abendessen mit Jana").performClick()
        waitFor("Trattoria Da Luca")
        desc("Edit").performClick()
        waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEdit }
        onNode(hasSetTextAction() and hasText("Abendessen mit Jana")).performTextInput(" neu")
        waitUntil(timeoutMillis = 10_000) { state.editorDirty }
        text("GhulAuth").performClick()
        waitFor("Discard changes?")
        text("Keep editing").performClick()
        waitGone("Discard changes?")
        assertTrue(state.stack.lastOrNull() is Screen.CalEdit)
        assertEquals(Tab.CAL, state.tab)
        text("GhulAuth").performClick()
        waitFor("Discard changes?")
        text("Discard").performClick()
        waitUntil(timeoutMillis = 10_000) { state.tab == Tab.AUTH && state.stack.isEmpty() }
    }

    /**
     * A phone lying sideways with large font: the week view keeps hours to look at and shows the
     * evening event (bug: the head left the hours almost no room); the page scrolls instead.
     */
    @Test
    fun lowLandscapeWeekKeepsItsHours() = app(size = 915 to 412, fontScale = 1.3f, setup = { tab = Tab.CAL; calView = CalView.WEEK; calSel = de.danerd.ghulsuite.ui.today().plus(75) }) { _, _ ->
        waitFor("Spätvorstellung Kino")
        text("Day").assertIsDisplayed()
        // Without scrolling: the hours under the head show the evening event.
        waitUntil(timeoutMillis = 10_000) { runCatching { text("Spätvorstellung Kino").assertIsDisplayed() }.isSuccess }
        shot("landscape-phone-week")
    }

    /** F93: with large text "all day" stays on one line in the event list (it broke into two). */
    @Test
    fun largeTextKeepsAllDayOnOneLine() = app(fontScale = 1.4f, setup = { tab = Tab.CAL; calView = CalView.LIST }) { _, _ ->
        waitFor("all day")
        val node = onAllNodesWithText("all day", useUnmergedTree = true).fetchSemanticsNodes().first()
        val lineHeightPx = 17f * 1.4f * node.layoutInfo.density.density
        assertTrue(node.size.height < lineHeightPx * 1.5f, "one line, was ${node.size.height} px for a line of $lineHeightPx px")
    }

    /**
     * A phone lying sideways (915 × 412): the areas in the sidebar on the left, and "Lock" at its
     * end stays reachable although the screen is low (bug: it was cut off).
     */
    @Test
    fun lowLandscapeSidebarKeepsLockReachable() = app(size = 915 to 412) { _, fake ->
        waitFor("GhulAuth")
        assertFalse(shown("Ctrl+L"))
        text("Lock").performScrollTo().assertIsDisplayed().performClick()
        waitUntil(timeoutMillis = 10_000) { fake.locks == 1 }
        shot("landscape-phone")
    }

    /**
     * A wide window: the areas in the sidebar, an opened event takes the place of the list (no
     * third column that mostly stood empty) and "back" returns to the list.
     */
    @Test
    fun wideWindowOpensEventsInPlaceOfTheList() {
        app(wide = true, setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, _ ->
            waitFor("Abendessen mit Jana")
            assertFalse(shown("No event open"))
            shot("wide-cal-list")
            text("Abendessen mit Jana").performClick()
            waitFor("Trattoria Da Luca")
            assertTrue(state.stack.lastOrNull() is Screen.CalEvent)
            // The list is not beside it any more (kept alive, but not shown).
            onAllNodesWithText("Kundentermin Müller GmbH", useUnmergedTree = true).onFirst().assertIsNotDisplayed()
            shot("wide-cal-event")
            state.back()
            waitFor("Kundentermin Müller GmbH")
            // The sidebar switches the area and closes what was open.
            text("Abendessen mit Jana").performClick()
            waitFor("Trattoria Da Luca")
            text("GhulAuth").performClick()
            waitUntil(timeoutMillis = 10_000) { state.tab == Tab.AUTH && state.stack.isEmpty() }
            assertFalse(shown("No account open"))
            shot("wide-auth")
        }
    }

    /** A test updater: finds [found] on the pre-release channel only, records what it was asked. */
    private class FakeUpdater(val found: de.danerd.ghulsuite.model.UpdateUi?) : de.danerd.ghulsuite.model.AppUpdater {
        val checks = java.util.Collections.synchronizedList(mutableListOf<Boolean>())
        val installed = java.util.Collections.synchronizedList(mutableListOf<String>())
        override val available = true
        override val installs = true
        override suspend fun check(prerelease: Boolean) = found.takeIf { prerelease }.also { checks += prerelease }
        override suspend fun install(update: de.danerd.ghulsuite.model.UpdateUi): de.danerd.ghulsuite.model.UpdateStep {
            installed += update.version
            return de.danerd.ghulsuite.model.UpdateStep.STARTED
        }
    }

    @Test
    fun updatesByHandOnBothChannels() {
        val dev = de.danerd.ghulsuite.model.UpdateUi("0.6.0-dev.3", "Mail archive faster", "https://github.com/x/y/releases/v", 31_000_000)
        // Stable: nothing newer, the app says so; no automatic check when switched off.
        val stable = FakeUpdater(dev)
        app(setup = { push(Screen.SettingsPage(SetPage.ABOUT)) }, prepare = {
            updater = stable
            settings.updateAuto = false
            settings.updateDev = false
        }) { state, _ ->
            waitFor("Check for updates now")
            assertTrue(stable.checks.isEmpty(), "no automatic check while switched off")
            shot("about-updates")
            text("Check for updates now").performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { stable.checks.isNotEmpty() && !state.updateBusy }
            assertEquals(listOf(false), stable.checks.toList())
            assertNull(state.update)
        }
        // Pre-release channel, checked automatically after unlocking: the dialog offers it, "Install" installs.
        val pre = FakeUpdater(dev)
        app(prepare = {
            updater = pre
            settings.updateAuto = true
            settings.updateDev = true
            settings.updateLastCheck = "0"
        }) { state, _ ->
            waitFor("Pre-release 0.6.0-dev.3")
            assertTrue(shown("Mail archive faster"))
            shot("update-dialog")
            text("Install").performClick()
            waitUntil(timeoutMillis = 10_000) { pre.installed.isNotEmpty() }
            assertEquals(listOf("0.6.0-dev.3"), pre.installed.toList())
            assertNull(state.update)
        }
        // Without an updater (test versions, store installs): only the hint.
        app(setup = { push(Screen.SettingsPage(SetPage.ABOUT)) }) { _, _ ->
            waitFor("This version does not update itself: it is a test version, an own build or was installed from a store, which then brings the updates.")
        }
    }

    /**
     * The servers cannot be read: the page says why and offers to try again, instead of an empty
     * list and "Off" (looked as if the server was gone) or an error with no way out.
     */
    @Test
    fun serversPageLoadErrorOffersRetry() {
        app(setup = { push(Screen.SettingsPage(SetPage.SERVERS)) }, prepare = { (net as FakeNet).serversFailures = 1 }) { _, _ ->
            waitFor("Try again")
            assertTrue(onAllNodesWithText("Off").fetchSemanticsNodes().isEmpty())
            assertTrue(onAllNodesWithText("Add server").fetchSemanticsNodes().isEmpty())
            text("Try again").performClick()
            waitFor("Heimserver")
            waitGone("Try again")
        }
    }

    /**
     * An answer to a mail without a Message-ID is still titled "Reply" (bug: "New mail"), and
     * attaching is offered once, not in the header and again below the text.
     */
    @Test
    fun replyWithoutMessageIdIsTitledReply() {
        app(setup = { push(Screen.MailCompose(de.danerd.ghulsuite.ui.MailDraft("m1", to = "erika@example.org", subject = "Re: Treffen"))) }) { _, _ ->
            waitFor("Reply")
            assertTrue(onAllNodesWithText("New mail").fetchSemanticsNodes().isEmpty())
            // Without a display (CI) there is no file picker and no button at all; never two.
            assertTrue(onAllNodesWithContentDescription("Attach file").fetchSemanticsNodes().size <= 1)
        }
    }

    /** Only the sync status cannot be read: the servers are shown, the sync card does not claim "Off". */
    @Test
    fun syncStatusErrorDoesNotSayOff() {
        app(setup = { push(Screen.SettingsPage(SetPage.SERVERS)) }, prepare = { (net as FakeNet).syncStatusFails = true }) { _, fake ->
            waitFor("Heimserver")
            waitFor("Try again")
            assertTrue(onAllNodesWithText("Off").fetchSemanticsNodes().isEmpty())
            (fake.net as FakeNet).syncStatusFails = false
            text("Try again").performScrollTo().performClick()
            waitFor("Synchronise now")
            waitGone("Try again")
        }
    }

    /** Choosing the sync server must stick: it is saved and shown (bug: it stayed "Off"). */
    @Test
    fun choosingTheSyncServerSticks() {
        app(setup = { push(Screen.SettingsPage(SetPage.SERVERS)) }, prepare = { (net as FakeNet).syncServer = null }) { _, fake ->
            val net = fake.net as FakeNet
            waitFor("Off")
            text("Off").performScrollTo().performClick()
            waitFor("Heimserver")
            onAllNodesWithText("Heimserver", useUnmergedTree = true).onLast().performClick()
            waitUntil(timeoutMillis = 10_000) { net.calls.contains("setSyncServer:s1") }
            assertEquals("s1", net.syncServer)
            // The row now names the server, not "Off".
            waitUntil(timeoutMillis = 10_000) { !shown("Off") }
            shot("sync-server-chosen")
            // With a server, the controls of the synchronisation appear.
            waitFor("Synchronise now")
        }
    }

    /**
     * Tapping the next folder while the last one is still loading: the loading is cancelled, and
     * that is not an error (bug: "The coroutine scope left the composition" under the button).
     */
    @Test
    fun browsingFoldersQuicklyShowsNoError() {
        app(setup = { push(Screen.ModuleInstall("s2")) }) { _, fake ->
            val net = fake.net as FakeNet
            waitFor("public_html")
            net.browseDelayMs = 1500
            text("public_html").performClick()
            // The old list is still shown while the folder loads: on to the next one at once.
            text("ghulsuite").performClick()
            waitFor("/public_html/ghulsuite")
            // Both slow listings are done (the first one answers only now with its cancellation).
            waitUntil(timeoutMillis = 10_000) { net.slowBrowsesDone >= 2 }
            net.browseDelayMs = 0
            waitFor("blog")
            Thread.sleep(200)
            waitForIdle()
            assertTrue(onAllNodesWithText("composition", substring = true, useUnmergedTree = true).fetchSemanticsNodes().isEmpty())
            shot("module-quick-browse")
        }
    }

    /**
     * The module does not answer at the usual addresses: HTTPS is red with the addresses tried,
     * the folder step grey "not checked" (not a red result nobody saw). With the address typed
     * in, the next try works.
     */
    @Test
    fun moduleInstallHttpsFailsThenWorksWithAddress() {
        app(setup = { push(Screen.ModuleInstall("s2")) }) { _, fake ->
            val net = fake.net as FakeNet
            waitFor("public_html")
            text("public_html").performClick()
            waitFor("/public_html")
            onNodeWithText("Install and check").performScrollTo().performClick()
            waitFor("Not checked, because the HTTPS step failed")
            assertTrue(shown("Module file uploaded"))
            assertTrue(shown("public_html/ghulsuite.php"))
            assertTrue(shown("No answer at https://example.org/ghulsuite.php, https://www.example.org/ghulsuite.php. Enter the address of the module above and try again."))
            assertTrue(onAllNodesWithText("skipped").fetchSemanticsNodes().isEmpty())
            assertFalse(shown("Done"))
            assertTrue("moduleInstall:s2:public_html:" in net.calls, net.calls.toString())
            shot("module-https-failed")
            onAllNodes(hasSetTextAction()).onFirst().performTextReplacement("https://example.org/web/ghulsuite.php")
            onNodeWithText("Install and check").performScrollTo().performClick()
            waitFor("The server module runs at https://example.org/web/ghulsuite.php. You can now create share links.")
            assertTrue("moduleInstall:s2:public_html:https://example.org/web/ghulsuite.php" in net.calls, net.calls.toString())
            assertFalse(shown("Not checked, because the HTTPS step failed"))
            shot("module-installed")
        }
    }

    /**
     * The module answers, but PHP may not write in the GhulSuite folder, or the web server hands
     * out the GhulSuite folder itself: each says what is wrong and what to do; nothing is done.
     */
    @Test
    fun moduleInstallExplainsNoWriteAndPublicFolder() {
        app(setup = { push(Screen.ModuleInstall("s2")) }, prepare = {
            (net as FakeNet).moduleAnswersWithoutAddress = true
            (net as FakeNet).moduleFailure = "no-write"
        }) { _, fake ->
            val net = fake.net as FakeNet
            waitFor("public_html")
            text("public_html").performClick()
            waitFor("/public_html")
            onNodeWithText("Install and check").performScrollTo().performClick()
            waitFor("PHP may read the GhulSuite folder but not write in its shares folder. Then downloads cannot be counted. Allow the web server to write there (file permissions), then check again.")
            assertFalse(shown("Done"))
            assertFalse(shown("PHP may not read the GhulSuite folder on this hosting (e.g. because of open_basedir). Share links are not possible there."))
            shot("module-no-write")
            net.moduleFailure = "private"
            onNodeWithText("Install and check").performScrollTo().performClick()
            waitFor("GhulSuite folder not reachable from the web")
            assertTrue(shown("The web server hands out the GhulSuite folder itself (https://example.org/ghulsuite/shares/check-1). The app removed the module again. Move the GhulSuite folder out of the website, then install again."))
            assertFalse(shown("Done"))
            shot("module-private-failed")
        }
    }

    /** The folder picker for the GhulSuite folder warns inside a website's folder, not next to it. */
    @Test
    fun folderPickerWarnsInsideTheWebsite() {
        for (p in listOf("public_html", "public_html/ghulsuite", "htdocs/x", "www", "example.org", "kunden/example.org/ghul")) {
            assertTrue(de.danerd.ghulsuite.ui.inWebsite(p), p)
        }
        for (p in listOf("", "ghulsuite", "daten/ghulsuite", "backup_alt", "websites-alt")) {
            assertFalse(de.danerd.ghulsuite.ui.inWebsite(p), p)
        }
    }

    /** A download that is cancelled or fails leaves no half file behind; a complete one stays. */
    @Test
    fun cancelledOrFailedDownloadLeavesNoHalfFile() {
        app { state, fake ->
            val net = fake.net as FakeNet
            class Sink : de.danerd.ghulsuite.model.ByteSink {
                @Volatile var discarded = false
                @Volatile var closed = false
                override fun write(data: ByteArray) {}
                override fun close() { closed = true }
                override fun discard() { discarded = true }
            }
            val item = de.danerd.ghulsuite.model.DriveItemUi("f1", null, "Film.mp4", false, 10, 0)
            net.downloadStops = "hang"
            val cancelled = Sink()
            runOnIdle { state.downloadFromDrive(item, cancelled) }
            waitFor("Cancel")
            onAllNodesWithText("Cancel").onFirst().performClick()
            waitUntil(timeoutMillis = 10_000) { cancelled.discarded && cancelled.closed }
            net.downloadStops = "fail"
            val failed = Sink()
            runOnIdle { state.downloadFromDrive(item, failed) }
            waitUntil(timeoutMillis = 10_000) { failed.discarded && failed.closed }
            net.downloadStops = null
            val complete = Sink()
            runOnIdle { state.downloadFromDrive(item, complete) }
            waitUntil(timeoutMillis = 10_000) { complete.closed && state.jobs.list.none { it.active } }
            assertFalse(complete.discarded, "a complete download stays")
        }
    }

    /**
     * The list of servers: per server its login, what it is used for and its module. The module is
     * installed from there (happy path: it answers at the usual address), updated, and removed
     * only after asking.
     */
    @Test
    fun serverListInstallsUpdatesAndRemovesTheModule() {
        app(setup = { push(Screen.SettingsPage(SetPage.SERVERS)) }, prepare = { (net as FakeNet).moduleAnswersWithoutAddress = true }) { state, fake ->
            val net = fake.net as FakeNet
            waitFor("Webspace")
            assertTrue(shown("Server module: version 1, version 2 is available"))
            assertTrue(shown("Server module: not installed (only needed for share links)"))
            assertTrue(shown("Used for: Synchronisation, GhulDrive"))
            assertTrue(shown("Not used yet"))
            shot("servers-modules")
            // Install on the web hosting: the folder, then "Install and check" (address left empty).
            text("Install module").performScrollTo().performClick()
            waitFor("Install and check")
            assertEquals(Screen.ModuleInstall("s2"), state.stack.lastOrNull())
            waitFor("public_html")
            text("public_html").performClick()
            waitFor("/public_html")
            onNodeWithText("Install and check").performScrollTo().performClick()
            waitFor("The server module runs at https://example.org/ghulsuite.php. You can now create share links.")
            assertTrue("moduleInstall:s2:public_html:" in net.calls, net.calls.toString())
            assertTrue(shown("Answers over HTTPS") && shown("Reads the GhulSuite folder"))
            assertTrue(shown("GhulSuite folder not reachable from the web"))
            assertFalse(shown("Not checked, because the HTTPS step failed"))
            text("Done").performScrollTo().performClick()
            waitFor("Webspace")
            assertTrue(state.stack.lastOrNull() is Screen.SettingsPage)
            // Removing asks first; "Cancel" removes nothing.
            text("Remove module").performScrollTo().performClick()
            waitFor("Remove the server module from Heimserver?")
            assertTrue(shown("Share links of this server stop working at once. Your files stay."))
            onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
            waitGone("Remove the server module from Heimserver?")
            assertFalse(net.calls.any { it.startsWith("moduleRemove:") })
            text("Remove module").performScrollTo().performClick()
            waitFor("Remove the server module from Heimserver?")
            onAllNodes(hasText("Remove") and hasAnyAncestor(isDialog())).onFirst().performClick()
            waitUntil(timeoutMillis = 10_000) { "moduleRemove:s1" in net.calls }
            // Update the older module (last: its message stays at the bottom for a while).
            text("Update module").performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { "moduleUpdate:s1" in net.calls }
            waitFor("Server module updated (version 2)")
        }
    }

    /**
     * Deleting a shared file in GhulDrive: the question says that its link ends; "Cancel" ends
     * nothing, "Move to trash" ends the link first and then moves the file.
     */
    @Test
    fun driveDeleteOfSharedFileEndsItsLinks() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        val net = fake.net as FakeNet
        // A file without a link: the question has no extra line.
        longPress("Notizen.txt")
        waitFor("1 selected")
        desc("Move to trash").performClick()
        waitFor("Move to the trash?")
        assertFalse(shown("1 active share link will end."))
        onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitGone("Move to the trash?")
        if (shown("1 selected")) desc("Cancel").performClick()
        waitGone("1 selected")
        // The shared file.
        longPress("Steuer 2025.pdf")
        waitFor("1 selected")
        desc("Move to trash").performClick()
        waitFor("1 active share link will end.")
        shot("drive-delete-shared")
        onAllNodes(hasText("Cancel") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitGone("Move to the trash?")
        assertFalse(net.calls.any { it.startsWith("shareDelete:") }, net.calls.toString())
        assertFalse("driveDelete:f1" in net.calls)
        if (!shown("1 selected")) longPress("Steuer 2025.pdf")
        waitFor("1 selected")
        desc("Move to trash").performClick()
        waitFor("1 active share link will end.")
        confirm("Move to trash")
        waitUntil(timeoutMillis = 10_000) { "driveDelete:f1" in net.calls }
        val calls = net.calls.toList()
        assertTrue("shareDelete:a1" in calls, calls.toString())
        assertTrue(calls.indexOf("shareDelete:a1") < calls.indexOf("driveDelete:f1"), "the link ends first: $calls")
    }

    /** GhulDrive with a server of its own (no synchronisation): the card and the header name it. */
    @Test
    fun driveWithItsOwnServerShowsIt() {
        app(setup = { tab = Tab.DRIVE }, prepare = {
            val n = net as FakeNet
            n.syncServer = null
            n.driveOwnServer = ServerUi("d1", "Drive-NAS", "nas.example.org", 22, "max", "drive", true, "SHA256:x", "/srv/drive",
                ServerKindUi.SFTP)
        }) { _, _ ->
            waitFor("Drive-NAS")
            assertFalse(shown("No server yet"))
            assertFalse(shown("Not set up"))
            shot("drive-own-server")
        }
    }

    // --- Background jobs and the mail archive in GhulDrive ----------------------------------------

    private fun DesktopComposeUiTest.part(t: String) = onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    /** Selects every mail of the inbox and taps "Archive in GhulDrive" in the selection's sheet. */
    private fun DesktopComposeUiTest.archiveAllSelected() {
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.SELECT_ALL_TAG).onFirst().performClick()
        waitFor("3 selected")
        desc("More").performClick()
        waitFor("Archive in GhulDrive")
        text("Archive in GhulDrive").performClick()
    }

    private fun DesktopComposeUiTest.dialogButton(label: String) =
        onAllNodes(hasText(label) and hasAnyAncestor(isDialog())).onFirst().performClick()

    /**
     * Archiving the selected mails asks first (copies, the provider keeps the mails), then runs in
     * the background with its progress under the header, and ends with how many were archived and
     * how many were in the archive already.
     */
    @Test
    fun archiveSelectedMailsShowsProgressAndSkipped() = app(setup = { tab = Tab.MAIL }, prepare = {
        val n = net as FakeNet
        n.archiveDelayMs = 700
        // One of them is in the archive already.
        n.archivedToDrive += "m1/INBOX/2"
    }) { _, fake ->
        val net = fake.net as FakeNet
        archiveAllSelected()
        waitFor("Archive in GhulDrive?")
        assertTrue(shown("3 selected mails are copied."))
        assertTrue(part("The mails stay at your provider"))
        assertFalse(net.calls.any { it.startsWith("archiveToDrive") }, "nothing before the answer")
        shot("archive-ask")
        dialogButton("Archive")
        waitUntil(timeoutMillis = 10_000) { part("Archiving 3 mails") }
        waitUntil(timeoutMillis = 10_000) { part("1 of 3 done") }
        shot("archive-progress")
        waitFor("2 mails archived in GhulDrive. 1 was already in the archive.")
        shot("archive-done")
        for (uid in listOf(1, 2, 3)) assertTrue("archiveToDrive:m1:INBOX:$uid" in net.calls, net.calls.toString())
        // Nothing changed at the provider.
        assertFalse(net.calls.any { it.startsWith("mailDelete") || it.startsWith("mailMove") })
        waitUntil(timeoutMillis = 5_000) { !part("Archiving 3 mails") }
    }

    /** "Cancel" in the strip stops the archiving; the message says how far it got. */
    @Test
    fun archiveCancelSaysHowFar() = app(setup = { tab = Tab.MAIL }, prepare = { (net as FakeNet).archiveDelayMs = 1500 }) { _, fake ->
        val net = fake.net as FakeNet
        archiveAllSelected()
        waitFor("Archive in GhulDrive?")
        dialogButton("Archive")
        waitUntil(timeoutMillis = 10_000) { part("1 of 3 done") }
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Archiving cancelled: 1 of 3 mails done.")
        Thread.sleep(2_000)
        assertEquals(1, net.calls.count { it.startsWith("archiveToDrive") }, net.calls.toString())
        assertFalse(part("Archiving 3 mails"), "the strip is gone")
    }

    /** "Archive older than 6 months" in the folder menu hands the right day to the core. */
    @Test
    fun archiveOlderThanPassesBeforeDays() = app(setup = { tab = Tab.MAIL }) { _, fake ->
        val net = fake.net as FakeNet
        waitFor("Treffen am Freitag")
        desc("Folder actions").performClick()
        waitFor("Archive older than …")
        assertTrue(shown("Archive this folder in GhulDrive"))
        shot("archive-folder-menu")
        text("Archive older than …").performClick()
        waitFor("6 months")
        assertTrue(shown("1 month") && shown("1 year") && shown("2 years"))
        shot("archive-older-choices")
        text("6 months").performClick()
        waitFor("Archive in GhulDrive?")
        assertTrue(shown("All mails in “Inbox” older than 6 months are copied."))
        dialogButton("Archive")
        val before = de.danerd.ghulsuite.ui.today().plusMonths(-6).days
        waitUntil(timeoutMillis = 10_000) { "archiveUids:m1:INBOX:$before" in net.calls }
        waitUntil(timeoutMillis = 10_000) { part("mails archived in GhulDrive.") || part("mail archived in GhulDrive.") }
    }

    /** Without GhulDrive the archive explains that it needs GhulDrive and leads there; nothing starts. */
    @Test
    fun archiveWithoutDriveExplains() = app(setup = { tab = Tab.MAIL }, prepare = { (net as FakeNet).driveMissing = true }) { state, fake ->
        val net = fake.net as FakeNet
        longPress("Treffen am Freitag")
        waitFor("1 selected")
        desc("More").performClick()
        waitFor("Archive in GhulDrive")
        text("Archive in GhulDrive").performClick()
        waitFor("GhulDrive needed")
        assertTrue(part("Set up GhulDrive first"))
        assertFalse(shown("Archive in GhulDrive?"))
        shot("archive-needs-drive")
        dialogButton("Set up GhulDrive")
        waitUntil(timeoutMillis = 10_000) { state.tab == Tab.DRIVE }
        assertFalse(net.calls.any { it.startsWith("archiveToDrive") || it.startsWith("archiveUids") }, net.calls.toString())
        assertTrue(state.jobs.list.isEmpty())
    }

    /**
     * The archive in the mailbox sheet: years as rows, inside them the mails read from the file
     * names (other files left out, the user's own folder shown as folder); a mail opens in the
     * reader through mailParseEml; a file that is no mail says so.
     */
    @Test
    fun archiveViewListsYearsAndOpensMails() = app(setup = { tab = Tab.MAIL }, prepare = { (net as FakeNet).archiveExists = true }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("Privat · Inbox")
        text("Privat · Inbox").performClick()
        waitFor("Archive (GhulDrive)")
        text("Archive (GhulDrive)").performClick()
        waitFor("2025")
        assertTrue(shown("2024") && shown("Steuern"))
        shot("archive-years")
        text("2025").performClick()
        waitFor("Steuerbescheid 2024")
        assertTrue(shown("Finanzamt") && shown("Urlaub im Mai") && shown("Erika Musterfrau"))
        assertFalse(part("Notizen zum Archiv"), "other files are left out")
        assertFalse(part("1a2b3c4d"), "the code is not shown")
        shot("archive-year")
        text("Steuerbescheid 2024").performClick()
        waitUntil(timeoutMillis = 10_000) { "mailParseEml:m1" in net.calls }
        waitFor("Anbei Ihr Bescheid.")
        assertTrue("driveDownload:e1" in net.calls)
        assertTrue(shown("Reply") && shown("Forward"))
        assertTrue(shown("Bescheid.pdf"))
        shot("archive-read")
        // Reply works as for a mail at the provider.
        text("Reply").performClick()
        waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.MailCompose }
        runOnIdle { state.pop() }
        runOnIdle { state.pop() }
        waitFor("kaputt")
        text("kaputt").performClick()
        waitFor("Cannot be read as a mail.")
        shot("archive-unreadable")
        runOnIdle { state.pop() }
        waitFor("kaputt")
        assertTrue(shown("Cannot be read as a mail."), "the row says it too")
    }

    /**
     * Automatic archiving: the choice in the mail options is stored in the core, and the run after
     * unlocking archives the inbox and Sent (up to the last mail handled), at most once a day.
     */
    @Test
    fun autoArchiveOptionAndDailyRun() = app(setup = { push(Screen.SettingsPage(SetPage.MAIL)) }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("Automatic archiving")
        assertTrue(part("once a day after unlocking"))
        assertEquals(0, net.calls.count { it.startsWith("archiveAutoUids") }, "off: no run")
        text("Automatic archiving").performClick()
        waitFor("Older than 90 days")
        shot("archive-auto-choice")
        text("Older than 90 days").performClick()
        waitUntil(timeoutMillis = 10_000) { "archiveSetAutoDays:90" in net.calls }
        assertEquals(90, net.archiveAutoDaysValue)
        // Switched on: it runs at once (nothing is old enough in the fake for 90 days).
        waitUntil(timeoutMillis = 10_000) { "archiveAutoUids:m1:INBOX" in net.calls && "archiveAutoUids:m1:Sent" in net.calls }
        waitUntil(timeoutMillis = 10_000) { state.backend.settings.archiveAutoLast != "0" }
        shot("archive-auto-on")
        // A day later with 1 day: the next unlock runs it and archives the old mail of both folders.
        net.archiveAutoDaysValue = 1
        state.backend.settings.archiveAutoLast = (de.danerd.ghulsuite.ui.nowSeconds() - 90_000).toString()
        net.calls.clear()
        fake.unlocked.value = false
        waitForIdle()
        fake.unlocked.value = true
        waitUntil(timeoutMillis = 15_000) { "archiveAutoDone:m1:Sent:1:1" in net.calls }
        val calls = net.calls.toList()
        assertTrue("archiveToDrive:m1:INBOX:1" in calls && "archiveToDrive:m1:Sent:1" in calls, calls.toString())
        assertTrue("archiveAutoDone:m1:INBOX:1:1" in calls, calls.toString())
        assertTrue(calls.indexOf("archiveToDrive:m1:INBOX:1") < calls.indexOf("archiveAutoDone:m1:INBOX:1:1"))
        waitFor("Automatic archive: 2 mails copied to GhulDrive.")
        // Unlocked again the same day: no second run.
        net.calls.clear()
        fake.unlocked.value = false
        waitForIdle()
        fake.unlocked.value = true
        waitForIdle()
        Thread.sleep(1_500)
        assertFalse(net.calls.any { it.startsWith("archiveAutoUids") }, net.calls.toString())
    }

    /** An automatic run that is cancelled notes only how far it got, so the next one continues there. */
    @Test
    fun autoArchiveCancelledKeepsItsPlace() = app(prepare = {
        val n = net as FakeNet
        n.archiveAutoDaysValue = 1
        n.archiveDelayMs = 1500
    }) { state, fake ->
        val net = fake.net as FakeNet
        // The old mail of the inbox and of Sent; the inbox is done, then "Cancel".
        waitUntil(timeoutMillis = 10_000) { "archiveToDrive:m1:INBOX:1" in net.calls }
        waitUntil(timeoutMillis = 10_000) { part("1 of 2 done") }
        shot("archive-auto-running")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitUntil(timeoutMillis = 10_000) { !state.jobs.running }
        val calls = net.calls.toList()
        assertTrue("archiveAutoDone:m1:INBOX:1:1" in calls, calls.toString())
        assertFalse(calls.any { it.startsWith("archiveAutoDone:m1:Sent") || it == "archiveToDrive:m1:Sent:1" }, calls.toString())
        // Not done: the next unlock runs it again (the day is not used up).
        assertEquals("0", state.backend.settings.archiveAutoLast)
    }

    /**
     * An upload runs as a background job: leaving GhulDrive (another area, a page on top) does not
     * stop it, its strip stays visible everywhere, and "Cancel" there stops it with how far it got.
     */
    @Test
    fun driveUploadSurvivesLeavingAndCancels() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        val closed = java.util.Collections.synchronizedList(mutableListOf<String>())
        net.driveUploadMs = 1500
        runOnIdle { state.driveIncoming = listOf(fakeSource("a.txt", closed = closed), fakeSource("b.txt", closed = closed), fakeSource("c.txt", closed = closed)) }
        waitUntil(timeoutMillis = 10_000) { part("Uploading 3 files") }
        runOnIdle { state.selectTab(Tab.MAIL) }
        waitFor("Treffen am Freitag")
        assertTrue(part("Uploading 3 files"), "the strip is shown in GhulMail too")
        shot("job-strip-mail")
        runOnIdle { state.push(Screen.SettingsPage(SetPage.LOOK)) }
        waitForIdle()
        waitUntil(timeoutMillis = 10_000) { part("1 of 3 done") }
        assertTrue(part("Uploading 3 files"), "and on a page on top")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Upload cancelled: 1 of 3 files done.")
        assertTrue("driveUpload::a.txt" in net.calls)
        assertFalse(net.calls.any { it.endsWith(":b.txt") || it.endsWith(":c.txt") }, net.calls.toString())
        waitUntil(timeoutMillis = 5_000) { closed.containsAll(listOf("a.txt", "b.txt", "c.txt")) }
    }

    /**
     * The auto-lock waits for a running job and follows when it ends; the user coming back drops
     * it; a lock by hand stops the job at once, and after the next unlock the message says why.
     */
    @Test
    fun autoLockWaitsForJobsAndManualLockCancels() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        net.driveUploadMs = 1200
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        // 1. The timer fires while two files upload: no lock until they are done.
        runOnIdle { state.uploadToDrive(null, listOf(fakeSource("a.txt"), fakeSource("b.txt"))) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.autoLock() }
        Thread.sleep(800)
        assertEquals(0, fake.locks, "not while the upload runs")
        waitUntil(timeoutMillis = 10_000) { !state.jobs.running }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 1 }
        assertTrue("driveUpload::b.txt" in net.calls)
        // 2. The user is back before the end: the waiting lock is dropped.
        runOnIdle { state.uploadToDrive(null, listOf(fakeSource("c.txt"))) }
        runOnIdle { state.autoLock() }
        runOnIdle { state.keepUnlocked() }
        waitUntil(timeoutMillis = 10_000) { "driveUpload::c.txt" in net.calls && !state.jobs.running }
        Thread.sleep(300)
        assertEquals(1, fake.locks)
        // 3. Locked by hand: the job stops at once.
        runOnIdle { state.uploadToDrive(null, listOf(fakeSource("d.txt"), fakeSource("e.txt"))) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.lockNow() }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 2 && !state.jobs.running }
        assertEquals(de.danerd.ghulsuite.ui.JobState.CANCELLED, state.jobs.list.last().state)
        Thread.sleep(1_500)
        assertFalse(net.calls.any { it.endsWith(":d.txt") || it.endsWith(":e.txt") }, net.calls.toString())
        // The fake does not lock by itself: lock and unlock it to see the message after unlocking.
        fake.unlocked.value = false
        waitForIdle()
        assertFalse(part("Upload cancelled"), "not on the lock screen")
        fake.unlocked.value = true
        waitFor("GhulSuite was locked. Upload cancelled: 0 of 2 files done.")
    }

    /** The strip in a wide window (Windows): under the header of the column, with "Cancel". */
    @Test
    fun jobStripInWideWindow() = app(wide = true, setup = { tab = Tab.DRIVE }) { state, fake ->
        (fake.net as FakeNet).driveUploadMs = 2000
        runOnIdle { state.uploadToDrive(null, listOf(fakeSource("Steuer 2025.pdf", 2_400_000), fakeSource("Urlaub.jpg"))) }
        waitUntil(timeoutMillis = 10_000) { part("Uploading 2 files") }
        shot("job-strip-wide")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Upload cancelled: 0 of 2 files done.")
    }

    /**
     * Photo backup: two photos added in the same second and "Cancel" between them. The mark stays
     * one second before, so the next run still takes the second photo (it was skipped for good).
     */
    @Test
    fun photoBackupKeepsPhotosOfTheSameSecond() = app(prepare = {
        settings.photoBackup = true
        (net as FakeNet).photos = listOf("a.jpg" to 1_000L, "b.jpg" to 1_000L, "c.jpg" to 1_005L)
        (net as FakeNet).photoDelayMs = 1_500
    }) { state, fake ->
        val net = fake.net as FakeNet
        waitUntil(timeoutMillis = 10_000) { "photo:a.jpg" in net.calls }
        val job = state.jobs.list.first { it.kind == de.danerd.ghulsuite.ui.JobKind.PHOTOS }
        runOnIdle { state.jobs.cancel(job) }
        waitUntil(timeoutMillis = 10_000) { !job.active }
        assertFalse("photo:b.jpg" in net.calls)
        assertEquals(999, state.backend.settings.photoBackupSince, "the mark stays before the second of a.jpg")
        // The next run takes b.jpg and c.jpg.
        net.photoDelayMs = 0
        fake.unlocked.value = false
        waitForIdle()
        fake.unlocked.value = true
        waitUntil(timeoutMillis = 10_000) { "photo:c.jpg" in net.calls }
        assertTrue("photo:b.jpg" in net.calls, net.calls.toString())
        waitUntil(timeoutMillis = 5_000) { state.backend.settings.photoBackupSince == 1_005 }
    }

    /**
     * The automatic lock that waited for a job follows its end; the job's message (it names the
     * file) is not shown on the way to the lock screen but after the next unlock.
     */
    @Test
    fun messageOfAJobTheLockWaitedForComesAfterUnlock() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        net.driveUploadMs = 1_500
        waitUntil(timeoutMillis = 10_000) { part("3 files") }
        runOnIdle { state.uploadToDrive(null, listOf(fakeSource("secret.txt"))) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.autoLock() }
        waitUntil(timeoutMillis = 10_000) { fake.locks == 1 }
        assertTrue("driveUpload::secret.txt" in net.calls)
        Thread.sleep(800)
        assertFalse(part("secret.txt uploaded"), "nothing on the way to the lock screen")
        fake.unlocked.value = false
        waitForIdle()
        fake.unlocked.value = true
        waitFor("secret.txt uploaded.")
    }

    /** The system notification of a finished job (Windows tray) names no files: only what ended and the counts. */
    @Test
    fun jobEndForNotificationsHasNoNames() = app(setup = { tab = Tab.DRIVE }) { state, fake ->
        val net = fake.net as FakeNet
        net.driveUploadMs = 300
        val ends = java.util.Collections.synchronizedList(mutableListOf<de.danerd.ghulsuite.ui.JobEnd>())
        val collector = CoroutineScope(Dispatchers.Default).launch { de.danerd.ghulsuite.ui.JobStatus.finished.collect { ends += it } }
        try {
            Thread.sleep(200)
            runOnIdle { state.uploadToDrive(null, listOf(fakeSource("secret.txt"))) }
            waitUntil(timeoutMillis = 10_000) { ends.isNotEmpty() }
            runOnIdle { state.uploadToDrive(null, listOf(fakeSource("geheim-1.txt"), fakeSource("geheim-2.txt"))) }
            waitUntil(timeoutMillis = 10_000) { ends.size >= 2 }
        } finally {
            collector.cancel()
        }
        assertTrue("secret.txt" in ends[0].text, ends[0].text)
        assertEquals("Upload finished", ends[0].publicText)
        assertEquals("Upload finished · 2 of 2 done", ends[1].publicText)
        assertFalse(ends.any { it.publicText.contains("secret") || it.publicText.contains("geheim") })
    }

    /** Two quick unlocks (or the option and an unlock) start one automatic archive run, not two. */
    @Test
    fun twoQuickStartsRunOneAutoArchive() = app { state, fake ->
        val net = fake.net as FakeNet
        net.archiveAutoDaysValue = 1
        net.archiveAutoDaysDelayMs = 400
        state.backend.settings.archiveAutoLast = "0"
        runOnIdle {
            state.runAutoArchive()
            state.runAutoArchive()
        }
        waitUntil(timeoutMillis = 10_000) { "archiveAutoDone:m1:Sent:1:1" in net.calls }
        Thread.sleep(1_000)
        assertEquals(1, net.calls.count { it == "archiveAutoUids:m1:INBOX" }, net.calls.toString())
    }
}
