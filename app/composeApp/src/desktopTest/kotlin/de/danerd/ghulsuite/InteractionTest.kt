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
import androidx.compose.ui.test.onFirst
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
        assertTrue(state.stack.lastOrNull() == Screen.SettingsPage(SetPage.FILE), state.stack.toString())
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
        waitFor("Hand on as a vault")
        text("Hand on as a vault").performClick()
        waitFor("Password of the new vault")
        shot("share-vault-form")
        val fields = onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))
        fields[0].performTextReplacement("Für Anna")
        fields[1].performTextReplacement("kurz")
        fields[2].performTextReplacement("kurz")
        fields[3].performTextReplacement("Master Passwort!")
        onAllNodes(hasText("Create") and hasAnyAncestor(isDialog())).onFirst().performClick()
        waitForIdle()
        assertFalse(fake.calls.any { it.startsWith("shareAsVault") }, "a weak password is refused first")
        fields[1].performTextReplacement("ein langes neues Passwort")
        fields[2].performTextReplacement("ein langes neues Passwort")
        onAllNodes(hasText("Create") and hasAnyAncestor(isDialog())).onFirst().performClick()
        // The code first: no vault file exists without it having been shown.
        waitFor("The recovery code of the new vault", substring = true)
        assertTrue("shareAsVault:p1:1:Für Anna:true:Master Passwort!" in fake.calls, fake.calls.toString())
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
        text("Next").performClick()
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
            fields[0].performTextInput("Das alte Passwort!")
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
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[0].performTextInput("Das alte Passwort!")
            onAllNodes(hasSetTextAction() and hasAnyAncestor(isDialog()))[1].performTextInput("1234")
            dialogButton("Unlock")
            waitFor("Unlocked. Now choose where the file is saved.")
            de.danerd.ghulsuite.ui.CreateFilePicker.override = { name -> de.danerd.ghulsuite.model.SuiteFileRef("/stick/$name", name) }
            fake.saveOk = false
            dialogButton("Save and replace")
            waitFor("The file was not written completely. Nothing was deleted.")
            assertTrue(fake.existsNow)
            assertTrue("deleteSuite" !in fake.calls)

            // Written and read back: then deleted, then the new suite is set up.
            fake.saveOk = true
            dialogButton("Save and replace")
            waitUntil(timeoutMillis = 10_000) { "create" in fake.calls }
            val saved = fake.calls.lastIndexOf("archiveSave:GhulSuite.ghlzip:true")
            assertTrue(saved >= 0 && saved < fake.calls.indexOf("deleteSuite") && fake.calls.indexOf("deleteSuite") < fake.calls.indexOf("create"), fake.calls.toString())
            waitFor("Areas")
            assertFalse(shown("Save the existing suite"))
            shot("setup-replaced")
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

            text("Delete the existing suite and replace it").performClick()
            waitFor("Delete the existing suite for good?")
            onNode(hasSetTextAction() and hasAnyAncestor(isDialog())).performTextInput("delete")
            dialogButton("Delete and replace")
            waitUntil(timeoutMillis = 10_000) { "create" in fake.calls }
            assertTrue(fake.calls.indexOf("deleteSuite") in 0 until fake.calls.indexOf("create"), fake.calls.toString())
            assertTrue(fake.calls.none { it.startsWith("archiveSave") })
            waitFor("Areas")
        }
    }
}
