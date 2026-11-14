// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.DesktopComposeUiTest
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.runDesktopComposeUiTest
import androidx.compose.ui.unit.Density
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.CalView
import de.danerd.ghulsuite.ui.GhulSuiteApp
import de.danerd.ghulsuite.ui.Screen
import de.danerd.ghulsuite.model.NetBackend
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
import kotlin.test.Test
import kotlin.test.assertFalse
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

    @Test
    fun driveEmptyTrashAsks() = app(setup = { tab = Tab.DRIVE }) { _, fake ->
        // Emptying the trash still asks, with a red button.
        val net = fake.net as FakeNet
        waitFor("Trash")
        text("Trash").performClick()
        waitFor("Alter Entwurf.docx")
        text("Empty").performClick()
        waitFor("Delete for good")
        shot("drive-empty-trash")
        text("Delete for good").performClick()
        waitUntil(timeoutMillis = 10_000) { "driveEmptyTrash" in net.calls }
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
}
