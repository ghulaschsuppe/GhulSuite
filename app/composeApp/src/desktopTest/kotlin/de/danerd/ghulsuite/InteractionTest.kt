// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerUi
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.longClick
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
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.model.NetBackend
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
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.onNodeWithText
import de.danerd.ghulsuite.model.ShareCheckUi
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
    private val optionsSet = java.util.Collections.synchronizedList(mutableListOf<de.danerd.ghulsuite.model.MailArchiveOptionsUi>())

    /** The app in English on a phone (412 × 892) or a wide window (1280 × 820), unlocked. */
    private fun app(
        wide: Boolean = false,
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
        val (w, h) = if (wide) 1280 to 820 else 412 to 892
        runDesktopComposeUiTest(w * 2, h * 2) {
            setContent { CompositionLocalProvider(LocalDensity provides Density(2f)) { GhulSuiteApp(state) } }
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
        deleteUndoDelete(state, "Notizen.txt", "Move to trash", "Moved to trash", "drive") { "driveDelete:f2" in net.calls }
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

    /**
     * Mail archive: archived mails appear below the provider's, open in their own reader and go into
     * the archive's trash with Undo. The first removal at the provider waits for "Remove";
     * tapping beside the question removes nothing.
     */
    @Test
    fun mailArchiveListReaderAndFirstRemoval() = app(setup = { tab = Tab.MAIL }) { state, fake ->
        val net = fake.net as FakeNet
        waitFor("ARCHIVE · 3")
        assertTrue(shown("Steuerbescheid 2024"))
        shot("mail-archive-list")
        text("Steuerbescheid 2024").performClick()
        waitFor("Bescheid.pdf")
        assertTrue(shown("Archived"))
        shot("mail-archive-read")
        state.undoMillis = 300
        desc("Delete").performClick()
        waitUntil(timeoutMillis = 10_000) { net.calls.any { it == "archiveDelete:k1" } }
        // The first removal at the provider: tapping beside it is "later".
        state.archiveAsk = "m1" to 42
        waitFor("Everything safely archived")
        shot("mail-archive-first-removal")
        text("Later").performClick()
        waitGone("Everything safely archived")
        assertFalse(net.calls.any { it.startsWith("archiveConfirm") }, "nothing removed without the answer")
        state.archiveAsk = "m1" to 42
        waitFor("Remove 42 mails")
        text("Remove 42 mails").performClick()
        waitUntil(timeoutMillis = 10_000) { net.calls.contains("archiveConfirm:m1") }
    }

    /** The archive options: switching deleting on asks first; "Cancel" changes nothing. */
    @Test
    fun mailArchiveOptionsAskBeforeDeleting() = app(setup = { push(Screen.SettingsPage(SetPage.MAIL)) }, prepare = {
        net = object : NetBackend by FakeNet() {
            override suspend fun archiveStatus() = FakeNet().archiveStatus()?.let { it.copy(options = it.options.copy(deleteAtProvider = false), confirmed = emptyList()) }
            override suspend fun archiveSetOptions(options: de.danerd.ghulsuite.model.MailArchiveOptionsUi) { optionsSet += options }
        }
    }) { _, _ ->
        waitFor("MAIL ARCHIVE")
        waitFor("Delete at the provider after archiving")
        text("Archive now").performScrollTo()
        shot("mail-archive-options")
        text("Delete at the provider after archiving").performScrollTo().performClick()
        waitFor("Delete archived mails at the provider?")
        text("Cancel").performClick()
        waitGone("Delete archived mails at the provider?")
        assertTrue(optionsSet.isEmpty(), "Cancel changes nothing")
        text("Delete at the provider after archiving").performScrollTo().performClick()
        waitFor("Switch on")
        text("Switch on").performClick()
        waitUntil(timeoutMillis = 10_000) { optionsSet.any { it.deleteAtProvider } }
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

    @Test
    fun wideWindowOpensEventsAndAccountsOnTheRight() {
        app(wide = true, setup = { tab = Tab.CAL; calView = CalView.LIST }) { state, _ ->
            waitFor("No event open")
            shot("wide-cal-empty")
            text("Abendessen mit Jana").performClick()
            waitFor("Trattoria Da Luca")
            // The list stays on the left: the event opens next to it.
            assertTrue(shown("Kundentermin Müller GmbH"))
            assertTrue(state.stack.lastOrNull() is Screen.CalEvent)
            shot("wide-cal-event")
        }
        app(wide = true, setup = { tab = Tab.AUTH }) { _, _ ->
            waitFor("No account open")
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
            text("Try again").performClick()
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
        app(setup = { push(Screen.ShareSetup("s1")) }) { _, fake ->
            val net = fake.net as FakeNet
            // The server's share folder is public_html/teilen: its subfolders are listed.
            waitFor("blog")
            net.browseDelayMs = 1500
            text("teilen").performClick()
            // The old list is still shown while the folder loads: on to the next one at once.
            text("blog").performClick()
            waitFor("/public_html/teilen/teilen/blog")
            // Both slow listings are done (the first one answers only now with its cancellation).
            waitUntil(timeoutMillis = 10_000) { net.slowBrowsesDone >= 2 }
            net.browseDelayMs = 0
            waitFor("blog")
            Thread.sleep(200)
            waitForIdle()
            assertTrue(onAllNodesWithText("composition", substring = true, useUnmergedTree = true).fetchSemanticsNodes().isEmpty())
            shot("share-setup-quick-browse")
        }
    }

    /**
     * The address does not answer: the steps that depend on it say "not checked" in grey, not a
     * red result nobody saw (bug: "the folder content is visible" for an address that is not there).
     */
    @Test
    fun shareCheckWithoutAnswerSaysNotChecked() {
        app(setup = { push(Screen.ShareSetup("s1")) }) { _, fake ->
            val net = fake.net as FakeNet
            net.shareChecks = listOf(
                ShareCheckUi("write", true, ""), ShareCheckUi("https", false, "server address not found"),
                ShareCheckUi("listing", false, "skipped"), ShareCheckUi("page", false, "skipped"),
            )
            waitFor("blog")
            onNode(hasSetTextAction() and hasText("https://", substring = true)).performTextReplacement("https://example.invalid/teilen")
            onNodeWithText("Set up and check").performScrollTo().performClick()
            // The core's fixed text in the app's words.
            waitFor("The server address was not found. Check its spelling and the internet connection.")
            assertFalse(shown("server address not found"))
            assertEquals(2, onAllNodesWithText("Not checked, because the HTTPS step failed").fetchSemanticsNodes().size)
            assertTrue(onAllNodesWithText("skipped").fetchSemanticsNodes().isEmpty())
            shot("share-check-not-checked")
        }
    }

    /** GhulDrive with a server of its own (no synchronisation): the card and the header name it. */
    @Test
    fun driveWithItsOwnServerShowsIt() {
        app(setup = { tab = Tab.DRIVE }, prepare = {
            val n = net as FakeNet
            n.syncServer = null
            n.driveOwnServer = ServerUi("d1", "Drive-NAS", "nas.example.org", 22, "max", "drive", true, "SHA256:x", "/srv/drive",
                ServerKindUi.SFTP, "", "")
        }) { _, _ ->
            waitFor("Drive-NAS")
            assertFalse(shown("No server yet"))
            assertFalse(shown("Not set up"))
            shot("drive-own-server")
        }
    }
}
