// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import de.danerd.ghulsuite.model.VaultKindUi
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsEnabled
import kotlinx.coroutines.launch
import androidx.compose.ui.test.assertIsNotDisplayed
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.printToString
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithTag
import de.danerd.ghulsuite.ui.SAVE_BUTTON_TAG
import de.danerd.ghulsuite.ui.SAVE_CONFIRM_TAG
import de.danerd.ghulsuite.ui.JOB_CANCEL_TAG
import de.danerd.ghulsuite.ui.saveJob
import de.danerd.ghulsuite.ui.checkAndSave
import androidx.compose.ui.test.onLast
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.unit.Density
import de.danerd.ghulsuite.ui.DevChecks
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.CalView
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.HeaderGuard
import de.danerd.ghulsuite.ui.DesktopBack
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.model.SecurityIssueKindUi
import de.danerd.ghulsuite.model.SecurityIssueUi
import de.danerd.ghulsuite.model.SecurityReportUi
import de.danerd.ghulsuite.ui.SetPage
import de.danerd.ghulsuite.ui.Tab
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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
 * left (vault, area). Every checked state is also saved as a picture
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
        // Nothing handed over in an earlier test (one AppLaunch for the whole process).
        de.danerd.ghulsuite.ui.AppLaunch.suiteFile.value = null
        val fake = ScreenshotTest.Fake(exists = true, unlocked = true).apply(prepare)
        // Texts that AppState looks up outside of the screen need a display (screen resolution);
        // the tests run without one, so only that failure is let through, every other one counts.
        val errors = java.util.Collections.synchronizedList(mutableListOf<Throwable>())
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined + CoroutineExceptionHandler { _, e -> if (e !is HeadlessException) errors += e })
        val state = AppState(fake, scope).apply(setup)
        val (w, h) = size ?: if (wide) 1280 to 820 else 412 to 892
        try {
            runDesktopComposeUiTest(w * 2, h * 2) {
                setContent { CompositionLocalProvider(LocalDensity provides Density(2f, fontScale)) { GhulSuiteApp(state) } }
                waitForIdle()
                block(state, fake)
            }
        } finally {
            // The app of this test ends with it: its collectors of AppLaunch (one object for all
            // tests) would otherwise take what a later test hands over.
            scope.cancel()
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
    private fun DesktopComposeUiTest.waitFor(t: String) {
        try {
            waitUntil(timeoutMillis = 10_000) { shown(t) }
        } catch (e: androidx.compose.ui.test.ComposeTimeoutException) {
            // What was there instead (for the test report).
            File(out, "waitfor-" + t.filter { it.isLetterOrDigit() }.take(40) + ".txt")
                .writeText(onRoot(useUnmergedTree = true).printToString())
            throw e
        }
    }
    private fun DesktopComposeUiTest.waitFor(t: String, substring: Boolean) =
        waitUntil(timeoutMillis = 10_000) { onAllNodesWithText(t, substring = substring, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    private fun DesktopComposeUiTest.waitGone(t: String) = waitUntil(timeoutMillis = 10_000) { !shown(t) }
    private fun DesktopComposeUiTest.dialogButton(t: String) = onAllNodes(hasText(t) and hasAnyAncestor(isDialog())).onFirst().performClick()
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
        waitFor("All codes · 2 accounts")
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
        // The line under the title counts what the list shows (bug: "2 accounts" above one).
        waitFor("All codes · 1 account")
        assertFalse(shown("All codes · 2 accounts"))
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

    /**
     * After a change of the protection the app asks to save the suite file (older copies still open
     * with the old protection); "Save now" opens the page of the suite file.
     */
    @Test
    fun protectionChangeAsksToSave() = app { state, _ ->
        waitForIdle()
        state.protectionChanged()
        waitFor("Save the suite file now")
        shot("protection-save")
        onAllNodes(hasText("Save now") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitGone("Save the suite file now")
        // The save dialog, as the save button opens it.
        assertTrue(state.saveDialog != null)
        assertFalse(state.saveAfterProtection)
    }

    /** A suite file handed over by another app while unlocked: the page of the suite file opens it. */
    @Test
    fun aHandedOverSuiteFileOpensOnTheFilePage() = app { state, _ ->
        waitForIdle()
        de.danerd.ghulsuite.ui.AppLaunch.suiteFile.value = de.danerd.ghulsuite.ui.PickedSuiteFile(
            de.danerd.ghulsuite.model.SuiteFileRef("", "Stick.ghlzip"),
            de.danerd.ghulsuite.model.SuiteFileRef("/nicht/da/Stick.ghlzip", "Stick.ghlzip"),
        )
        waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() == Screen.SettingsPage(SetPage.FILE) }
        waitUntil(timeoutMillis = 10_000) { de.danerd.ghulsuite.ui.AppLaunch.suiteFile.value == null }
        // "Stick.ghlzip is checked completely first …": the form of opening, not the page alone.
        waitFor("Stick.ghlzip is checked completely first", substring = true)
        shot("suite-file-handed-over")
    }

    /** The error report is only shown, to copy (or share or save): nothing is sent by itself. */
    @Test
    fun errorReportIsShownNotSent() = app(setup = { tab = Tab.SETTINGS; push(Screen.SettingsPage(SetPage.ABOUT)) }) { _, _ ->
        waitFor("Show error report")
        text("Show error report").performScrollTo().performClick()
        waitFor("GhulSuite 0.5.0-dev", substring = true)
        assertTrue(shown("Copy"))
        assertFalse(part("Send"), "no upload any more")
        shot("error-report")
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
            assertFalse(shown("1 ENTRIES"))
            shot("pass-one-entry")
            // M33: the check itself runs on its own page.
            text("Security check").performClick()
            waitFor("1 entry needs attention")
        }
        app(prepare = { report = SecurityReportUi(60, 5, listOf(weak("p2", "2", "Jira"), weak("p1", "1", "Codeberg"))) }) { _, fake ->
            waitFor("5 ENTRIES")
            assertFalse(fake.calls.contains("securityReport"), "the list does not run the security check")
            text("Security check").performClick()
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
        // Current, the second factor, new, repeat: the new password is the second field from the end.
        onAllNodes(hasSetTextAction()).fetchSemanticsNodes().size.let { n -> onAllNodes(hasSetTextAction())[n - 2].performTextInput("kurz") }
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
     * Something handed over from outside (here: open GhulAuth) while an editor has unsaved input
     * asks first instead of dropping the input; "Discard" then carries it out.
     */
    @Test
    fun aHandOverWhileEditingAsksFirst() = app(setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, _ ->
        waitFor("Abendessen mit Jana")
        text("Abendessen mit Jana").performClick()
        waitFor("Trattoria Da Luca")
        desc("Edit").performClick()
        waitUntil(timeoutMillis = 10_000) { state.stack.lastOrNull() is Screen.CalEdit }
        onNode(hasSetTextAction() and hasText("Abendessen mit Jana")).performTextInput(" neu")
        waitUntil(timeoutMillis = 10_000) { state.editorDirty }
        runOnIdle { de.danerd.ghulsuite.ui.AppLaunch.openTab.value = Tab.AUTH }
        waitFor("Discard changes?")
        assertTrue(state.stack.lastOrNull() is Screen.CalEdit, "the input is still there")
        text("Discard").performClick()
        waitUntil(timeoutMillis = 10_000) { state.tab == Tab.AUTH && state.stack.isEmpty() }
    }

    /** The area used last is kept by its name (the list of areas changes). */
    @Test
    fun theLastAreaIsKeptByName() = app(setup = { tab = Tab.PASS }) { state, fake ->
        runOnIdle { state.selectTab(Tab.CAL) }
        assertEquals("CAL", fake.settings.lastTabName)
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
        /** The download waits here (null: it does not wait). */
        var downloadGate: kotlinx.coroutines.CompletableDeferred<Unit>? = null
        /** What the app answered right before the hand-over. */
        val asked = java.util.Collections.synchronizedList(mutableListOf<Boolean>())
        override suspend fun install(update: de.danerd.ghulsuite.model.UpdateUi, mayFinish: () -> Boolean): de.danerd.ghulsuite.model.UpdateStep {
            // As the real ones: the download takes a while, then the app is asked once more.
            kotlinx.coroutines.delay(30)
            downloadGate?.await()
            val ok = mayFinish()
            asked += ok
            if (!ok) return de.danerd.ghulsuite.model.UpdateStep.WAIT
            installed += update.version
            return de.danerd.ghulsuite.model.UpdateStep.STARTED
        }
    }

    /**
     * M14: a setting that weakens the protection takes effect only after the master password; a
     * wrong one changes nothing, making it stronger asks nothing.
     */
    @Test
    fun weakeningTheLockAsksForThePassword() = app(setup = { push(Screen.SettingsPage(SetPage.LOCK)) }) { _, fake ->
        waitFor("Lock when the screen turns off")
        assertTrue(fake.settings.lockOnScreenOff)
        text("Lock when the screen turns off").performClick()
        waitFor("Weaken the protection?")
        shot("weaken-ask")
        assertTrue(fake.settings.lockOnScreenOff, "nothing changed before the password")
        gateField().performTextInput("falsch")
        onNodeWithTag("gate-confirm", useUnmergedTree = true).performClick()
        waitFor("Wrong master password.")
        assertTrue(fake.settings.lockOnScreenOff, "a wrong password changes nothing")
        gateField().performTextReplacement(fake.masterPassword)
        onNodeWithTag("gate-confirm", useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 5_000) { !fake.settings.lockOnScreenOff }
        waitGone("Weaken the protection?")
        // Back on: no question.
        text("Lock when the screen turns off").performClick()
        waitUntil(timeoutMillis = 5_000) { fake.settings.lockOnScreenOff }
        assertFalse(shown("Weaken the protection?"))
        // The instant lock keeping the reminders again: asked as well.
        runOnIdle { fake.settings.panicClearReminders = true }
        text("Instant lock also deletes the event reminders").performScrollTo().performClick()
        waitFor("Weaken the protection?")
        assertTrue(fake.settings.panicClearReminders)
        dialogButton("Cancel")
        waitGone("Weaken the protection?")
        assertTrue(fake.settings.panicClearReminders, "cancelled: unchanged")
    }

    /**
     * H6: the app PIN is set up only with both factors (the password alone does not enable
     * "Save"), with the validity chosen in the dialog; that validity is what the core gets.
     */
    @Test
    fun theAppPinNeedsBothFactorsAndItsValidity() = app(setup = { push(Screen.SettingsPage(SetPage.UNLOCK)) }) { _, fake ->
        waitFor("App PIN")
        onAllNodesWithText("App PIN", useUnmergedTree = true).onFirst().performClick()
        waitFor("Repeat app PIN")
        val fields = { onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog())) }
        fields()[0].performTextInput("246813")
        fields()[1].performTextInput("246813")
        fields()[2].performTextInput(fake.masterPassword)
        // A key is chosen (the default), its PIN still missing: not yet.
        onNode(hasText("Save") and hasAnyAncestor(isDialog()) and hasClickAction()).assertIsNotEnabled()
        fields()[3].performTextInput("1234")
        shot("app-pin-proof")
        dialogButton("Save")
        waitUntil(timeoutMillis = 10_000) { fake.quickSetups.isNotEmpty() }
        assertEquals("pin:246813:${fake.settings.quickUnlockHours}", fake.quickSetups.single())
    }

    /** H6: two methods with their own validity: each is shown (and changed) on its own. */
    @Test
    fun eachQuickUnlockMethodShowsItsOwnValidity() = app(
        setup = { push(Screen.SettingsPage(SetPage.UNLOCK)) },
        prepare = {
            quickReady = true
            settings.quickUnlockBiometric = true; settings.quickUnlockPin = true
            settings.quickHoursBio = 24; settings.quickHoursPin = 0; settings.quickUnlockHours = 24
        },
    ) { _, _ ->
        waitFor("Fingerprint valid for")
        assertTrue(shown("App PIN valid for"))
        assertFalse(shown("Valid for"), "no single validity for both")
        shot("quick-validity-per-method")
    }

    /**
     * H3: opened with the recovery code, the lost factors cannot be shown: a change of the
     * protection asks for the new master password alone, and says why.
     */
    @Test
    fun afterTheRecoveryCodeThePasswordAloneIsAsked() = app(
        setup = { push(Screen.SettingsPage(SetPage.FACTORS)) },
        prepare = { recovered = true },
    ) { _, _ ->
        waitFor("New recovery code")
        text("New recovery code").performScrollTo().performClick()
        waitFor("Opened with the recovery code", substring = true)
        assertFalse(shown("Key file"), "no choice of a factor")
        assertEquals(1, onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog())).fetchSemanticsNodes().size, "the password only")
        shot("recovered-password-only")
    }

    /**
     * The quick unlock of an older version: the unlock screen goes to password and second
     * factor, and Options, Unlocking says to set it up again.
     */
    @Test
    fun anOutdatedQuickUnlockAsksToSetItUpAgain() = app(prepare = {
        unlocked.value = false
        quickReady = true
        quickError = de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.QUICK_OUTDATED)
    }) { state, fake ->
        waitFor("Unlock with app PIN")
        onAllNodes(hasSetTextAction()).onFirst().performTextInput("246813")
        text("Unlock with app PIN").performClick()
        waitUntil(timeoutMillis = 10_000) { part("Master password") }
        assertFalse(shown("Unlock with app PIN"))
        assertTrue(part("set up again") || part("QUICK_OUTDATED"), "says why")
        // Unlocked fully: the options say what to do.
        runOnIdle { fake.unlocked.value = true; state.push(Screen.SettingsPage(SetPage.UNLOCK)) }
        waitFor("Quick unlock must be set up again", substring = true)
        assertTrue(fake.settings.quickUnlockHours > 0)
    }

    /** The text field of the master password question (M13, M14). */
    private fun DesktopComposeUiTest.gateField() =
        onAllNodes(hasSetTextAction() and hasAnyAncestor(androidx.compose.ui.test.hasTestTag("gate-password")), useUnmergedTree = true).onFirst()

    /** M13: the secret of a 2FA account as a QR code only after the master password. */
    @Test
    fun aQrCodeOfASecretNeedsThePassword() = app(setup = { tab = Tab.AUTH }) { state, fake ->
        waitFor("Codeberg")
        val before = fake.calls.count { it == "verifyPassword" }
        onAllNodesWithText("Codeberg", useUnmergedTree = true).onFirst().performTouchInput { longClick() }
        waitUntil(timeoutMillis = 10_000) { shown("1 selected") }
        // An icon of the bar, or in "More" when the bar has more actions than room.
        if (onAllNodesWithContentDescription("Export as QR code").fetchSemanticsNodes().isNotEmpty()) {
            onAllNodesWithContentDescription("Export as QR code").onFirst().performClick()
        } else {
            onAllNodesWithContentDescription("More").onFirst().performClick()
            waitFor("Export as QR code")
            text("Export as QR code").performClick()
        }
        waitFor("Confirm with your master password", substring = true)
        assertTrue(fake.calls.none { it.startsWith("totpUri") }, "no secret before the password")
        gateField().performTextInput(fake.masterPassword)
        onNodeWithTag("gate-confirm", useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 5_000) { fake.calls.count { it == "verifyPassword" } > before }
        waitUntil(timeoutMillis = 5_000) { fake.calls.any { it.startsWith("totpUri") } }
        shot("qr-after-password")
    }

    /**
     * M2: after unlocking, the suite file at its place holds a state saved elsewhere since: the
     * app says so (who, when) before anything is edited, and "Open now" opens that file.
     */
    @Test
    fun newerSuiteFileIsOfferedAfterUnlocking() = app(prepare = {
        suiteRef = de.danerd.ghulsuite.model.SuiteFileRef("stick", "GhulSuite.ghlzip")
        target = de.danerd.ghulsuite.model.ArchiveTargetUi.CHANGED_ELSEWHERE
        targetDevice = "Pixel 8"
        targetTime = 1_762_900_000_000
    }) { state, _ ->
        waitFor("The suite file was saved elsewhere")
        assertTrue(part("Pixel 8"))
        shot("newer-suite-file")
        text("Open now").performClick()
        waitUntil(timeoutMillis = 5_000) { state.stack.lastOrNull() == Screen.SettingsPage(SetPage.FILE) }
        assertNull(state.newerSuiteFile)
        // The page of the suite file took the file over (AppLaunch is one for the whole process:
        // nothing may be left for the next test).
        waitUntil(timeoutMillis = 5_000) { de.danerd.ghulsuite.ui.AppLaunch.suiteFile.value == null }
    }

    /** M2: the other device changed the protection and saved: the most important case, said as such. */
    @Test
    fun aFileWithANewerProtectionIsOfferedAfterUnlocking() = app(prepare = {
        suiteRef = de.danerd.ghulsuite.model.SuiteFileRef("stick", "GhulSuite.ghlzip")
        target = de.danerd.ghulsuite.model.ArchiveTargetUi.OTHER_PROTECTION
    }) { state, _ ->
        waitFor("The suite file was saved elsewhere")
        assertTrue(part("saved with a changed protection"))
        text("Later").performClick()
        waitUntil(timeoutMillis = 5_000) { state.newerSuiteFile == null }
    }

    /** M2: the same file as this device's state: nothing is asked. */
    @Test
    fun sameSuiteFileAsksNothing() = app(prepare = {
        suiteRef = de.danerd.ghulsuite.model.SuiteFileRef("stick", "GhulSuite.ghlzip")
        target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME
    }) { state, _ ->
        waitFor("Search entries")
        Thread.sleep(200)
        waitForIdle()
        assertFalse(shown("The suite file was saved elsewhere"))
        assertNull(state.newerSuiteFile)
    }

    /**
     * M12, M18, M9: the reminders handed to the system carry only the chosen detail (below
     * "everything" no place, with "notice only" no title either), and a failed read keeps the
     * reminders already set instead of clearing them.
     */
    @Test
    fun remindersCarryOnlyTheChosenDetail() = app(prepare = {
        (cal as FakeCal).reminderList = listOf(
            de.danerd.ghulsuite.model.CalReminderUi(
                "e1", "Zahnarzt Dr. Weber", "Praxis am Markt", false,
                de.danerd.ghulsuite.ui.dateTime(de.danerd.ghulsuite.ui.today().plus(1), 600),
                de.danerd.ghulsuite.ui.dateTime(de.danerd.ghulsuite.ui.today().plus(1), 645),
                de.danerd.ghulsuite.ui.dateTime(de.danerd.ghulsuite.ui.today().plus(1), 585), 15,
            ),
        )
    }) { state, fake ->
        val cal = fake.cal as FakeCal
        fun scheduleWith(detail: Int): de.danerd.ghulsuite.model.CalAlarmUi {
            val before = cal.scheduled.size
            runOnIdle { fake.settings.calNotifyDetail = detail; state.refreshCalendar() }
            waitUntil(timeoutMillis = 5_000) { cal.scheduled.size > before }
            return cal.scheduled.last().single()
        }
        val notice = scheduleWith(0)
        for (t in listOf(notice.title, notice.text, notice.publicTitle, notice.publicText)) {
            assertFalse(t.contains("Zahnarzt") || t.contains("Praxis"), "notice only: $t")
        }
        val titled = scheduleWith(1)
        assertEquals("Zahnarzt Dr. Weber", titled.title)
        assertFalse(titled.text.contains("Praxis"), "no place below everything: ${titled.text}")
        val full = scheduleWith(2)
        assertTrue(full.text.contains("Praxis"), full.text)
        // Reading fails: what is set stays.
        cal.remindersFail = true
        val before = cal.scheduled.size
        runOnIdle { state.refreshCalendar() }
        Thread.sleep(300)
        waitForIdle()
        assertEquals(before, cal.scheduled.size, "a failed read does not replace the reminders")
    }

    /**
     * M7: input started while the update downloads stops the hand-over (the installer would end
     * the app and lose it); saving does not start while the update is being installed.
     */
    @Test
    fun updateWaitsForInputStartedMeanwhile() {
        val dev = de.danerd.ghulsuite.model.UpdateUi("0.6.0-dev.3", "Calendar faster", "https://github.com/x/y/releases/v", 31_000_000)
        val up = FakeUpdater(dev).apply { downloadGate = kotlinx.coroutines.CompletableDeferred() }
        app(prepare = { updater = up; unsavedChanges = true }) { state, fake ->
            runOnIdle { state.installUpdate(dev) }
            waitUntil(timeoutMillis = 5_000) { state.updateInstalling }
            // Saving now is refused with a word.
            runOnIdle { state.checkAndSave(de.danerd.ghulsuite.model.SuiteFileRef("x", "GhulSuite.ghlzip")) }
            waitFor("The update is being installed; the app ends in a moment. Save afterwards.")
            assertFalse(fake.calls.any { it.startsWith("archiveSave") }, "nothing saved while the update installs")
            // An edit starts meanwhile: the update does not go ahead.
            runOnIdle { state.editorDirty = true }
            up.downloadGate!!.complete(Unit)
            waitUntil(timeoutMillis = 5_000) { up.asked.isNotEmpty() && !state.updateBusy }
            assertEquals(listOf(false), up.asked.toList())
            assertTrue(up.installed.isEmpty())
            assertEquals(dev, state.update, "offered again")
            assertFalse(state.updateInstalling)
            runOnIdle { state.editorDirty = false }
        }
    }

    @Test
    fun updatesByHandOnBothChannels() {
        val dev = de.danerd.ghulsuite.model.UpdateUi("0.6.0-dev.3", "Calendar faster", "https://github.com/x/y/releases/v", 31_000_000)
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
            assertTrue(shown("Calendar faster"))
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
     * Leaving an area with its search focused: the hidden search keeps no focus (bug: Android
     * brought the keyboard back for it in GhulAuth when the delete question closed).
     */
    @Test
    fun leavingAnAreaClearsTheFocusOfItsSearch() {
        app { state, _ ->
            waitFor("Search entries")
            val focusedFields = { onAllNodes(androidx.compose.ui.test.hasSetTextAction() and androidx.compose.ui.test.isFocused(), useUnmergedTree = true).fetchSemanticsNodes().size }
            onAllNodes(androidx.compose.ui.test.hasSetTextAction(), useUnmergedTree = true)[0].performClick()
            waitUntil(timeoutMillis = 5_000) { focusedFields() == 1 }
            runOnIdle { state.tab = Tab.AUTH }
            waitForIdle()
            assertEquals(0, focusedFields())
        }
    }

    // --- Background jobs -----------------------------------------------------------------------

    private fun DesktopComposeUiTest.tagged(t: String) = onAllNodesWithTag(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
    private fun DesktopComposeUiTest.part(t: String) = onAllNodesWithText(t, substring = true, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    /**
     * A file transfer as the app runs it: [files] one after the other ([stepMs] each), the name of
     * the file worked on, how far it got after "Cancel", every file closed in the end.
     */
    private fun AppState.transfer(
        files: List<String>,
        stepMs: Long,
        done: MutableList<String>,
        closed: MutableList<String>? = null,
    ) = jobs.start(
        de.danerd.ghulsuite.ui.JobKind.UPLOAD,
        title = { if (files.size == 1) "Adding ${files[0]}" else "Adding ${files.size} files" },
        cancelled = { j -> "Adding cancelled: ${j.done} of ${files.size} files done." },
        onEnd = { closed?.addAll(files) },
    ) { j ->
        j.total = files.size
        for (f in files) {
            j.item = f
            kotlinx.coroutines.delay(stepMs)
            done += f
            j.done++
        }
        if (files.size == 1) "${files[0]} added." else "${files.size} files added."
    }

    /**
     * A transfer runs as a background job: leaving its area (another area, a page on top) does not
     * stop it, its strip stays visible everywhere, and "Cancel" there stops it with how far it got.
     */
    @Test
    fun jobSurvivesLeavingAndCancels() = app { state, _ ->
        val done = java.util.Collections.synchronizedList(mutableListOf<String>())
        val closed = java.util.Collections.synchronizedList(mutableListOf<String>())
        runOnIdle { state.transfer(listOf("a.txt", "b.txt", "c.txt"), 1500, done, closed) }
        waitUntil(timeoutMillis = 10_000) { part("Adding 3 files") }
        runOnIdle { state.selectTab(Tab.CAL) }
        waitForIdle()
        assertTrue(part("Adding 3 files"), "the strip is shown in GhulCal too")
        shot("job-strip-cal")
        runOnIdle { state.push(Screen.SettingsPage(SetPage.LOOK)) }
        waitForIdle()
        waitUntil(timeoutMillis = 10_000) { part("1 of 3 done") }
        assertTrue(part("Adding 3 files"), "and on a page on top")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Adding cancelled: 1 of 3 files done.")
        assertEquals(listOf("a.txt"), done.toList())
        waitUntil(timeoutMillis = 5_000) { closed.containsAll(listOf("a.txt", "b.txt", "c.txt")) }
    }

    /**
     * The auto-lock waits for a running job and follows when it ends; the user coming back drops
     * it; a lock by hand stops the job at once, and after the next unlock the message says why.
     */
    @Test
    fun autoLockWaitsForJobsAndManualLockCancels() = app { state, fake ->
        val done = java.util.Collections.synchronizedList(mutableListOf<String>())
        // 1. The timer fires while two files are added: no lock until they are done.
        runOnIdle { state.transfer(listOf("a.txt", "b.txt"), 1200, done) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.autoLock() }
        Thread.sleep(800)
        assertEquals(0, fake.locks, "not while the job runs")
        waitUntil(timeoutMillis = 10_000) { !state.jobs.running }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 1 }
        assertTrue("b.txt" in done)
        // 2. The user is back before the end: the waiting lock is dropped.
        runOnIdle { state.transfer(listOf("c.txt"), 1200, done) }
        runOnIdle { state.autoLock() }
        runOnIdle { state.keepUnlocked() }
        waitUntil(timeoutMillis = 10_000) { "c.txt" in done && !state.jobs.running }
        Thread.sleep(300)
        assertEquals(1, fake.locks)
        // 3. Locked by hand: the job stops at once.
        runOnIdle { state.transfer(listOf("d.txt", "e.txt"), 1200, done) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.lockNow() }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 2 && !state.jobs.running }
        assertEquals(de.danerd.ghulsuite.ui.JobState.CANCELLED, state.jobs.list.last().state)
        Thread.sleep(1_500)
        assertFalse("d.txt" in done || "e.txt" in done, done.toString())
        // The fake does not lock by itself: lock and unlock it to see the message after unlocking.
        fake.unlocked.value = false
        waitForIdle()
        assertFalse(part("Adding cancelled"), "not on the lock screen")
        fake.unlocked.value = true
        waitFor("GhulSuite was locked. Adding cancelled: 0 of 2 files done.")
    }

    /** The strip in a wide window (Windows): under the header of the column, with "Cancel". */
    @Test
    fun jobStripInWideWindow() = app(wide = true) { state, _ ->
        val done = java.util.Collections.synchronizedList(mutableListOf<String>())
        runOnIdle { state.transfer(listOf("Steuer 2025.pdf", "Urlaub.jpg"), 2000, done) }
        waitUntil(timeoutMillis = 10_000) { part("Adding 2 files") }
        shot("job-strip-wide")
        onAllNodesWithTag(de.danerd.ghulsuite.ui.JOB_CANCEL_TAG).onFirst().performClick()
        waitFor("Adding cancelled: 0 of 2 files done.")
    }

    /**
     * The automatic lock that waited for a job follows its end; the job's message (it names the
     * file) is not shown on the way to the lock screen but after the next unlock.
     */
    @Test
    fun messageOfAJobTheLockWaitedForComesAfterUnlock() = app { state, fake ->
        val done = java.util.Collections.synchronizedList(mutableListOf<String>())
        runOnIdle { state.transfer(listOf("secret.txt"), 1500, done) }
        waitUntil(timeoutMillis = 5_000) { state.jobs.running }
        runOnIdle { state.autoLock() }
        waitUntil(timeoutMillis = 10_000) { fake.locks == 1 }
        assertTrue("secret.txt" in done)
        Thread.sleep(800)
        assertFalse(part("secret.txt added"), "nothing on the way to the lock screen")
        fake.unlocked.value = false
        waitForIdle()
        fake.unlocked.value = true
        waitFor("secret.txt added.")
    }

    /** The system notification of a finished job (Windows tray) names no files: only what ended and the counts. */
    @Test
    fun jobEndForNotificationsHasNoNames() = app { state, _ ->
        val done = java.util.Collections.synchronizedList(mutableListOf<String>())
        val ends = java.util.Collections.synchronizedList(mutableListOf<de.danerd.ghulsuite.ui.JobEnd>())
        val collector = CoroutineScope(Dispatchers.Default).launch { de.danerd.ghulsuite.ui.JobStatus.finished.collect { ends += it } }
        try {
            Thread.sleep(200)
            runOnIdle { state.transfer(listOf("secret.txt"), 300, done) }
            waitUntil(timeoutMillis = 10_000) { ends.isNotEmpty() }
            runOnIdle { state.transfer(listOf("geheim-1.txt", "geheim-2.txt"), 300, done) }
            waitUntil(timeoutMillis = 10_000) { ends.size >= 2 }
        } finally {
            collector.cancel()
        }
        assertTrue("secret.txt" in ends[0].text, ends[0].text)
        assertEquals("Upload finished", ends[0].publicText)
        assertEquals("Upload finished · 2 of 2 done", ends[1].publicText)
        assertFalse(ends.any { it.publicText.contains("secret") || it.publicText.contains("geheim") })
    }

    /**
     * P5b: a vault opened on its own shows only its area; Options hold only that vault (no options
     * of the own suite), and "Close vault" ends it like locking.
     */
    @Test
    fun singleVaultShowsOnlyItsAreaAndCloses() = app(
        prepare = { single = de.danerd.ghulsuite.model.SuiteFileRef("/x/Privat.ghlpss", "Privat.ghlpss") },
        setup = { single = de.danerd.ghulsuite.ui.SingleUi(VaultKindUi.PASS, de.danerd.ghulsuite.model.SuiteFileRef("/x/Privat.ghlpss", "Privat.ghlpss")) },
    ) { state, fake ->
        waitFor("Codeberg")
        assertEquals(listOf(Tab.PASS, Tab.SETTINGS), state.tabs)
        assertFalse(shown("GhulAuth"), "no other area")
        shot("single-pass")
        text("Options").performClick()
        waitFor("Close vault")
        assertFalse(shown("My suite"), "no options of the own suite")
        assertTrue(shown("Everything is in the file."))
        shot("single-options")
        text("Close vault").performClick()
        waitUntil(timeoutMillis = 10_000) { state.single == null }
        assertFalse(state.backend.unlocked.value)
        assertEquals(1, fake.locks)
    }

    /**
     * P5b: picked entries of one vault are handed on as a vault: name, its own password (twice,
     * checked like a master password), a new key file, and the own master password to confirm.
     */
    @Test
    fun shareAsVaultFromTheSelection() = app { _, fake ->
        longPress("Codeberg")
        waitFor("1 selected")
        desc("More").performClick()
        waitFor("Share as separate vault")
        text("Share as separate vault").performClick()
        waitFor("Password of the new vault")
        shot("share-vault-form")
        val fields = onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))
        fields[0].performTextReplacement("Für Anna")
        fields[1].performTextReplacement("kurz")
        fields[2].performTextReplacement("kurz")
        fields[3].performTextReplacement("Gruener Kaktus singt 1987!")
        // M15: both factors of the own suite (a key chosen: its PIN).
        fields[4].performTextReplacement("1234")
        onAllNodes(hasText("Create") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitForIdle()
        assertFalse(fake.calls.any { it.startsWith("shareAsVault") }, "a weak password is refused first")
        fields[1].performTextReplacement("ein langes neues Passwort")
        fields[2].performTextReplacement("ein langes neues Passwort")
        onAllNodes(hasText("Create") and hasAnyAncestor(isDialog())).onFirst().performClick()
        // The code first: no vault file exists without it having been shown.
        waitFor("The recovery code of the new vault", substring = true)
        assertTrue("shareAsVault:p1:1:Für Anna:true:Gruener Kaktus singt 1987!" in fake.calls, fake.calls.toString())
        shot("share-vault-code")
        onAllNodes(hasText("Next") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitFor("Save key file")
    }

    /** Through steps 1 and 2 of "Set up new" to step 3 ("Create"), from the start screen. */
    private fun DesktopComposeUiTest.toCreateStep() {
        waitFor("Set up new")
        text("Set up new").performClick()
        waitFor("Master password")
        onAllNodes(hasSetTextAction())[0].performTextInput("Kater-Ufer-Mondlicht-73")
        onAllNodes(hasSetTextAction())[1].performTextInput("Kater-Ufer-Mondlicht-73")
        text("Next").performClick()
        waitFor("PIN of the security key")
        // The key's name, then its PIN.
        onAllNodes(hasSetTextAction())[1].performTextInput("1234")
        // The backup is required: a second key (its name, then its PIN).
        onAllNodesWithText("Second key").onFirst().performScrollTo().performClick()
        waitUntil(timeoutMillis = 10_000) { onAllNodes(hasSetTextAction()).fetchSemanticsNodes().size >= 4 }
        onAllNodes(hasSetTextAction())[3].performTextInput("1234")
        // The step is longer now: the button below is scrolled to first.
        text("Next").performScrollTo().performClick()
        waitFor("Create")
    }

    private fun withPickers(block: () -> Unit) {
        System.setProperty("ghul.pickers", "true")
        try {
            block()
        } finally {
            System.clearProperty("ghul.pickers")
            de.danerd.ghulsuite.ui.CreateFilePicker.override = null
        }
    }

    /**
     * "Set up new" over a suite on this device: step 3 says so once (no second red message) and
     * offers three ways; "Set up" itself waits for the choice. "Open this suite" goes to unlocking it.
     */
    @Test
    fun setupOverAnExistingSuiteSaysSoOnceAndOpensIt() = withPickers {
        app(setup = { showStart = true }, prepare = { unlocked.value = false }) { state, fake ->
            toCreateStep()
            val hint = "There already is a suite on this device. Choose what happens to it."
            waitFor(hint)
            assertEquals(1, onAllNodesWithText(hint, useUnmergedTree = true).fetchSemanticsNodes().size)
            assertFalse(shown("There already are GhulSuite files here."))
            assertTrue(shown("Save the existing suite and replace it"))
            assertTrue(shown("Delete the existing suite and replace it"))
            onNode(hasText("Register key") and hasClickAction()).assertIsNotEnabled()
            shot("setup-existing")
            assertEquals(1, onAllNodesWithText(hint, useUnmergedTree = true).fetchSemanticsNodes().size, "still once")
            text("Open this suite").performClick()
            waitUntil(timeoutMillis = 10_000) { !state.showStart }
            waitFor("Unlock")
            assertTrue(fake.existsNow)
            assertTrue(fake.calls.none { it == "deleteSuite" || it == "create" }, fake.calls.toString())
        }
    }

    /**
     * "Save the existing suite and replace it": unlocked with its own password and key, saved
     * where the user picks, and only then deleted and the new suite set up. Cancelled or not
     * written, nothing is deleted; leaving locks the existing suite again.
     */
    @Test
    fun setupSavesTheExistingSuiteBeforeReplacingIt() = withPickers {
        app(setup = { showStart = true }, prepare = { unlocked.value = false; unlockOpens = true }) { _, fake ->
            toCreateStep()
            text("Save the existing suite and replace it").performClick()
            waitFor("Save the existing suite")
            val fields = onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))
            fields[0].performTextInput(fake.masterPassword)
            fields[1].performTextInput("1234")
            shot("setup-save-existing")
            dialogButton("Unlock")
            waitFor("Unlocked. Now choose where the file is saved.")
            assertTrue("unlock" in fake.calls)
            shot("setup-save-unlocked")
            // The save dialog cancelled: nothing saved, nothing deleted.
            de.danerd.ghulsuite.ui.CreateFilePicker.override = { null }
            dialogButton("Save and replace")
            waitFor("Not saved. Nothing was deleted.")
            assertTrue(fake.existsNow)
            assertTrue(fake.calls.none { it.startsWith("archiveSave") || it == "deleteSuite" }, fake.calls.toString())
            // Leaving locks the existing suite again.
            val locks = fake.locks
            dialogButton("Cancel")
            waitGone("Save the existing suite")
            assertEquals(locks + 1, fake.locks)
            assertTrue(fake.existsNow)

            // Not written completely: nothing deleted.
            text("Save the existing suite and replace it").performClick()
            waitFor("Save the existing suite")
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[0].performTextInput(fake.masterPassword)
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[1].performTextInput("1234")
            dialogButton("Unlock")
            waitFor("Unlocked. Now choose where the file is saved.")
            de.danerd.ghulsuite.ui.CreateFilePicker.override = { name -> de.danerd.ghulsuite.model.SuiteFileRef("/stick/$name", name) }
            fake.saveOk = false
            dialogButton("Save and replace")
            waitFor("The file was not written completely. Nothing was deleted.")
            assertTrue(fake.existsNow)
            assertTrue("deleteSuite" !in fake.calls)

            // Written and read back, but the key's second touch breaks off: the file is kept,
            // the deletion is asked for once more (without saving again).
            fake.saveOk = true
            fake.deleteFailsOnce = de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.TIMEOUT)
            dialogButton("Save and replace")
            waitFor("Deleting the existing suite was not confirmed", substring = true)
            assertTrue(fake.existsNow)
            val saves = fake.calls.count { it.startsWith("archiveSave") }
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[0].performTextInput(fake.masterPassword)
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[1].performTextInput("1234")
            dialogButton("Delete and replace")
            waitUntil(timeoutMillis = 10_000) { "create" in fake.calls }
            assertEquals(saves, fake.calls.count { it.startsWith("archiveSave") }, "not saved twice")
            val saved = fake.calls.lastIndexOf("archiveSave:GhulSuite.ghlzip:true")
            assertTrue(saved >= 0 && saved < fake.calls.indexOf("deleteSuite") && fake.calls.indexOf("deleteSuite") < fake.calls.indexOf("create"), fake.calls.toString())
            waitFor("Areas")
            assertFalse(shown("Save the existing suite"))
            shot("setup-replaced")
        }
    }

    /**
     * H6 in the wizard: the fingerprint (preset where there is a sensor) is set up in step 6 with
     * the factor of the suite just made, never with an empty PIN to the key.
     */
    @Test
    fun theWizardSetsUpQuickUnlockWithTheFactorItWasMadeWith() = withPickers {
        app(setup = { showStart = true }, prepare = { unlocked.value = false; existsNow = false; masterPassword = "Kater-Ufer-Mondlicht-73" }) { _, fake ->
            toCreateStep()
            waitUntil(timeoutMillis = 10_000) { onAllNodes(hasText("Register key") and hasClickAction()).fetchSemanticsNodes().isNotEmpty() }
            onNode(hasText("Register key") and hasClickAction()).performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { "create" in fake.calls }
            // Areas, locking, then unlocking (fingerprint preset).
            waitFor("Areas")
            text("Next").performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { part("Step 5 of 8") }
            text("Next").performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { part("Step 6 of 8") }
            text("Next").performScrollTo().performClick()
            waitUntil(timeoutMillis = 10_000) { fake.quickSetups.isNotEmpty() || part("PIN_INVALID") || part("Wrong key PIN") }
            assertEquals(listOf("bio:${fake.settings.quickUnlockHours}:1234:false"), fake.quickSetups.toList())
            waitUntil(timeoutMillis = 10_000) { part("Step 7 of 8") }
        }
    }

    /**
     * "Delete the existing suite and replace it" asks first with a warning and the word typed
     * out; "Cancel" keeps the suite, confirmed it is deleted and the new one set up.
     */
    @Test
    fun setupDeletesTheExistingSuiteOnlyWhenConfirmed() = withPickers {
        app(setup = { showStart = true }, prepare = { unlocked.value = false }) { _, fake ->
            toCreateStep()
            text("Delete the existing suite and replace it").performClick()
            waitFor("Delete the existing suite for good?")
            assertTrue(shown("A suite file (.ghlzip) saved somewhere else is not touched. If you have not saved the suite as a file yet, better choose “Save the existing suite and replace it”."))
            onNode(hasText("Delete and replace") and hasAnyAncestor(isDialog()) and hasClickAction()).assertIsNotEnabled()
            shot("setup-delete-existing")
            dialogButton("Cancel")
            waitGone("Delete the existing suite for good?")
            assertTrue(fake.existsNow)
            assertTrue("deleteSuite" !in fake.calls)

            // H5: only with the proof of the existing suite. A wrong password deletes nothing.
            text("Delete the existing suite and replace it").performClick()
            waitFor("Delete the existing suite for good?")
            assertTrue(part("To delete it you need the existing suite"))
            val fields = { onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog())) }
            fields()[0].performTextInput("Ein falsches Passwort 42!")
            fields()[1].performTextInput("1234")
            fields().onLast().performTextInput("delete")
            dialogButton("Delete and replace")
            // The message (headless tests show the kind of the error, the app its text).
            waitUntil(timeoutMillis = 10_000) { part("Wrong master password") || part("WRONG_PASSWORD") }
            assertTrue(fake.existsNow, "a wrong proof deletes nothing")
            assertTrue("deleteSuite" !in fake.calls)
            // With the recovery code instead.
            dialogButton("With the recovery code")
            fields()[0].performTextInput(fake.recoveryCode.lowercase())
            dialogButton("Delete and replace")
            waitUntil(timeoutMillis = 10_000) { "create" in fake.calls }
            assertTrue(fake.calls.indexOf("deleteSuite") in 0 until fake.calls.indexOf("create"), fake.calls.toString())
            assertTrue(fake.calls.none { it.startsWith("archiveSave") })
            waitFor("Areas")
        }
    }

    /** GhulPass: the selection makes all picked entries favourites at once (as GhulDocs and GhulAuth). */
    @Test
    fun severalEntriesBecomeFavourites() = app(setup = { tab = Tab.PASS }) { _, fake ->
        waitFor("Codeberg")
        onAllNodesWithText("Codeberg", useUnmergedTree = true).onFirst().performTouchInput { longClick() }
        waitUntil(timeoutMillis = 10_000) { shown("1 selected") }
        // All of them (two vaults: no "Share as separate vault"), so the favourite is one of the icons.
        onAllNodesWithTag("sel-all").onFirst().performClick()
        waitUntil(timeoutMillis = 10_000) { onAllNodesWithContentDescription("Add to favourites").fetchSemanticsNodes().isNotEmpty() }
        onAllNodesWithContentDescription("Add to favourites").onFirst().performClick()
        waitUntil(timeoutMillis = 10_000) { fake.calls.count { it.startsWith("setFavorite:") && it.endsWith(":true") } >= 2 }
    }

    /** GhulCal with two GhulCal vaults: the calendars under their vault, a new calendar asks where it goes. */
    @Test
    fun aNewCalendarAsksForItsVault() = app(
        setup = { tab = Tab.CAL; calView = CalView.LIST },
        prepare = { vaults = vaults + de.danerd.ghulsuite.model.VaultUi("c2", de.danerd.ghulsuite.model.VaultKindUi.CAL, "Verein", "verein-c2.ghlcal", true, 0) },
    ) { state, _ ->
        waitForIdle()
        runOnIdle { state.contextSheet = true }
        waitFor("New calendar")
        assertTrue(shown("Kalender"), "the calendars under the name of their vault")
        text("New calendar").performClick()
        waitFor("Which vault?")
        assertTrue(shown("Verein"))
        text("Verein").performClick()
        waitUntil(timeoutMillis = 10_000) { !shown("Which vault?") }
    }

    /**
     * Unsaved changes while locked: the unlock screen says so, but offers no tap that cannot work
     * there (saving needs the open suite); unlocked, the save button is in the header.
     */
    @Test
    fun unsavedChangesWhileLockedSayUnlockFirst() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = { unsavedChanges = true }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { state.lockNow() }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 1 }
        fake.unlocked.value = false
        waitUntil(timeoutMillis = 10_000) { shown("There are changes that are not in the suite file yet. Unlock and tap the save icon at the top.") }
        assertFalse(tagged(SAVE_BUTTON_TAG), "no save button on the unlock screen")
        shot("unsaved-locked")
    }

    /** "Recently used": the pills have room around their round badge (they looked squeezed). */
    @Test
    fun recentlyUsedPills() = app(setup = { tab = Tab.PASS }, prepare = { settings.recentEntries = "p1/1,p2/2" }) { _, _ ->
        waitFor("Codeberg")
        shot("recent-pills")
    }

    private val stick = de.danerd.ghulsuite.model.SuiteFileRef("/stick/GhulSuite.ghlzip", "GhulSuite.ghlzip")

    /** The save button asks, then the suite file is written with its progress, and the button goes. */
    @Test
    fun theSaveButtonAsksAndSaves() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME; savedAtMs = 1_700_000_000_000L }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        onAllNodesWithTag(SAVE_BUTTON_TAG, useUnmergedTree = true).onFirst().performClick()
        waitFor("Write the changes into the suite file now?")
        assertTrue(part("File: GhulSuite.ghlzip"))
        assertTrue(part("Last saved: "))
        shot("save-ask")
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { shown("Saved in GhulSuite.ghlzip.") }
        assertTrue("archiveSave:GhulSuite.ghlzip:false" in fake.calls)
        onNodeWithTag("save-done", useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { !tagged(SAVE_BUTTON_TAG) }
        assertFalse(state.unsaved)
    }

    /**
     * M1: a place that only allows writing over the file (Android without a shared folder, a
     * cloud app): "Save" first says what is at stake; it overwrites only when asked again.
     */
    @Test
    fun overwritingTheOnlyFileIsAskedFirst() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME
        safe = de.danerd.ghulsuite.model.SafeSaveUi.OVERWRITES
    }) { _, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        onAllNodesWithTag(SAVE_BUTTON_TAG, useUnmergedTree = true).onFirst().performClick()
        waitFor("Write the changes into the suite file now?")
        assertTrue(part("Cloud apps also keep older versions"))
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitFor("The suite file is written over directly", substring = true)
        assertTrue(fake.calls.none { it.startsWith("archiveSave") }, "not written before the second answer")
        shot("save-overwrite-risk")
        text("Overwrite anyway").performClick()
        waitUntil(timeoutMillis = 10_000) { "archiveSave:GhulSuite.ghlzip:false" in fake.calls }
    }

    /** Nothing is written by itself: the suite file changes only when the user saves. */
    @Test
    fun nothingIsSavedByItself() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME }) { _, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        Thread.sleep(6_000)
        assertTrue(fake.calls.none { it.startsWith("archiveSave") })
        assertTrue(tagged(SAVE_BUTTON_TAG))
    }

    /**
     * H11: a newer protection at the place: no overwrite, and the dialog says honestly that a side
     * file keeps the old protection, with the way that keeps this device's changes.
     */
    @Test
    fun aNewerProtectionThereSaysHowToKeepTheChanges() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.OTHER_PROTECTION
    }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { state.newerSuiteFile != null }
        text("Later").performClick()
        waitUntil(timeoutMillis = 10_000) { state.newerSuiteFile == null }
        runOnIdle { state.askSave() }
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitFor("Share as separate vault", substring = true)
        assertTrue(part("keeps the old protection"))
        assertFalse(tagged("save-overwrite"), "a newer protection is never overwritten")
        shot("save-conflict-protection")
        assertTrue(fake.calls.none { it.startsWith("archiveSave") })
    }

    /** Saved elsewhere meanwhile: who and when, "save mine as a new file" first, overwriting only when asked. */
    @Test
    fun aFileSavedElsewhereIsOnlyOverwrittenWhenAsked() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.CHANGED_ELSEWHERE
        targetDevice = "Laptop"; targetTime = 1_700_000_000_000L
    }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        // After unlocking the newer file was offered (M2): "Later".
        waitUntil(timeoutMillis = 10_000) { state.newerSuiteFile != null }
        text("Later").performClick()
        waitUntil(timeoutMillis = 10_000) { state.newerSuiteFile == null }
        runOnIdle { state.askSave() }
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitFor("The file was saved elsewhere")
        assertTrue(part("on Laptop."))
        // Saving as a new file needs the file dialogs (not there in the headless tests).
        assertEquals(de.danerd.ghulsuite.ui.filePickersAvailable, shown("Save mine as a new file …"))
        shot("save-conflict")
        assertTrue(fake.calls.none { it.startsWith("archiveSave") })
        onNodeWithTag("save-overwrite", useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { "archiveSave:GhulSuite.ghlzip:false" in fake.calls }
    }

    /** The stick is not there: said in the dialog, with "Try again"; nothing written. */
    @Test
    fun aPlaceThatIsGoneIsSaidInTheDialog() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick; target = null }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { state.askSave() }
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { part("The suite file cannot be reached right now") }
        assertTrue(shown("Try again"))
        assertTrue(fake.calls.none { it.startsWith("archiveSave") })
    }

    /** Saving fails (e.g. the stick is full): the reason in the dialog; still unsaved. */
    @Test
    fun aFailedSaveSaysWhy() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME
        saveError = de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.STORAGE, "no space left on device")
    }) { state, _ ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { state.askSave() }
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitFor("Not saved")
        assertTrue(state.unsaved)
        assertTrue(shown("Try again"))
    }

    /** No suite file yet: the dialog asks for a place (the button shows also then). */
    @Test
    fun withoutASuiteFileThePlaceIsAskedFor() = app(setup = { tab = Tab.PASS; unsaved = true }, prepare = { unsavedChanges = true }) { state, _ ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { state.askSave() }
        waitUntil(timeoutMillis = 10_000) { part("There is no suite file yet.") }
        assertEquals(de.danerd.ghulsuite.ui.filePickersAvailable, shown("Choose a place …"))
        shot("save-no-file")
    }

    /** Changes not in the suite file: the save button with a dot in the header of every area and a dot on Options. */
    @Test
    fun unsavedChangesShowEverywhere() = app(setup = { tab = Tab.PASS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick }) { state, _ ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        shot("save-needed-pass")
        for (t in listOf(Tab.AUTH, Tab.DOCS, Tab.CAL, Tab.SETTINGS)) {
            runOnIdle { state.selectTab(t) }
            waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        }
        onAllNodesWithTag(SAVE_BUTTON_TAG, useUnmergedTree = true).onFirst().performClick()
        waitFor("Write the changes into the suite file now?")
    }

    /**
     * Locked while the suite file is written: the save goes on (no "Cancel", not stopped by the
     * lock), the unlock screen says so; afterwards it counts as saved.
     */
    @Test
    fun aSaveGoesOnWhenTheSuiteIsLocked() = app(setup = { tab = Tab.DOCS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME
        saveGate = kotlinx.coroutines.CompletableDeferred()
    }) { state, fake ->
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { state.askSave() }
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { part("50 %") }
        assertFalse(tagged(JOB_CANCEL_TAG), "saving has no Cancel")
        runOnIdle { state.lockNow() }
        waitUntil(timeoutMillis = 5_000) { fake.locks == 1 }
        fake.unlocked.value = false
        waitUntil(timeoutMillis = 10_000) { part("The suite file is still being saved") }
        shot("saving-locked")
        assertTrue(state.saveJob != null, "not cancelled by locking")
        runOnIdle { fake.saveGate!!.complete(Unit) }
        waitUntil(timeoutMillis = 10_000) { state.saveJob == null }
        assertFalse(state.unsaved)
    }

    /** Not saved for three days: a reminder above the lists; a day is not enough. */
    @Test
    fun aReminderAfterThreeDays() = app(setup = { tab = Tab.PASS; unsaved = true }, prepare = {
        unsavedChanges = true; suiteRef = stick; unsavedSinceMs = 1_700_000_000_000L
    }) { state, _ ->
        runOnIdle { state.clockMs = { 1_700_000_000_000L + 24 * 3_600_000L }; state.readUnsaved() }
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        assertFalse(tagged("save-reminder"))
        runOnIdle { state.clockMs = { 1_700_000_000_000L + 4 * 24 * 3_600_000L }; state.readUnsaved() }
        runOnIdle { state.changed() }
        waitUntil(timeoutMillis = 10_000) { tagged("save-reminder") }
        shot("save-reminder")
    }

    /** Quitting (Windows) with unsaved changes asks; "Quit without saving" quits, nothing is written. */
    @Test
    fun quittingWithUnsavedChangesAsks() = app(setup = { tab = Tab.PASS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick }) { state, fake ->
        var quit = 0
        runOnIdle { state.onQuit = { quit++ } }
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { assertFalse(state.requestQuit()) }
        waitFor("Unsaved changes")
        shot("quit-ask")
        onNodeWithTag("quit-without", useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 5_000) { quit == 1 }
        assertTrue(fake.calls.none { it.startsWith("archiveSave") })
    }

    /** "Save" before quitting: saved, then the app quits by itself. */
    @Test
    fun quittingAfterSaving() = app(setup = { tab = Tab.PASS; unsaved = true }, prepare = { unsavedChanges = true; suiteRef = stick; target = de.danerd.ghulsuite.model.ArchiveTargetUi.SAME }) { state, fake ->
        var quit = 0
        runOnIdle { state.onQuit = { quit++ } }
        waitUntil(timeoutMillis = 10_000) { tagged(SAVE_BUTTON_TAG) }
        runOnIdle { assertFalse(state.requestQuit()) }
        onNodeWithTag("quit-save", useUnmergedTree = true).performClick()
        onNodeWithTag(SAVE_CONFIRM_TAG, useUnmergedTree = true).performClick()
        waitUntil(timeoutMillis = 10_000) { quit == 1 }
        assertTrue("archiveSave:GhulSuite.ghlzip:false" in fake.calls)
    }
}
