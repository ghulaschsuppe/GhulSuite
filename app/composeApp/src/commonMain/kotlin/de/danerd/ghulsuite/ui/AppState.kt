// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import de.danerd.ghulsuite.model.VaultBackend
import de.danerd.ghulsuite.model.KeyChangeUi
import de.danerd.ghulsuite.model.UpdateStep
import de.danerd.ghulsuite.model.UpdateUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.first
import org.jetbrains.compose.resources.getPluralString
import org.jetbrains.compose.resources.getString

enum class Tab { PASS, AUTH, DRIVE, MAIL, CAL, SETTINGS }

/** The tab of a vault's area. */
fun tabOf(kind: de.danerd.ghulsuite.model.VaultKindUi): Tab = when (kind) {
    de.danerd.ghulsuite.model.VaultKindUi.PASS -> Tab.PASS
    de.danerd.ghulsuite.model.VaultKindUi.TOTP -> Tab.AUTH
    de.danerd.ghulsuite.model.VaultKindUi.CAL -> Tab.CAL
    de.danerd.ghulsuite.model.VaultKindUi.DRIVE -> Tab.DRIVE
    de.danerd.ghulsuite.model.VaultKindUi.MAIL -> Tab.MAIL
}

/** A vault opened on its own: its kind and the file it is written back into. */
data class SingleUi(val kind: de.danerd.ghulsuite.model.VaultKindUi, val file: de.danerd.ghulsuite.model.SuiteFileRef)

/** How long "Undo" is offered after a deletion or move. */
const val UNDO_MILLIS = 6000L

/** Key of an item for [AppState.hidden]: area, place (vault, folder, view) and id. */
fun hiddenKey(area: String, place: String, id: String) = "$area|$place|$id"

/**
 * Requests from outside the app UI, e.g. a tapped notification ("open GhulMail"). Applied as soon
 * as the suite is unlocked.
 */
object AppLaunch {
    val openTab = kotlinx.coroutines.flow.MutableStateFlow<Tab?>(null)

    /** A tapped reminder: event id and day (`id|YYYY-MM-DD`). */
    val openEvent = kotlinx.coroutines.flow.MutableStateFlow<String?>(null)

    /** Counts new-mail notifications posted in this process: open lists and the badge refresh. */
    val newMail = kotlinx.coroutines.flow.MutableStateFlow(0)

    /** B22: counts changes of the device's time zone noticed while running (Windows): reminders are planned anew. */
    val timeZoneChanged = kotlinx.coroutines.flow.MutableStateFlow(0)

    /** Counts how often the app came to the front (Android: activity resumed). */
    val resumed = kotlinx.coroutines.flow.MutableStateFlow(0)

    /** Whether the app is in front (Android: between onResume and onPause; elsewhere always). */
    val inFront = kotlinx.coroutines.flow.MutableStateFlow(true)

    /** A mail handed over by another app (mailto link, "send by e-mail"). */
    val compose = kotlinx.coroutines.flow.MutableStateFlow<MailDraft?>(null)

    /** An appointment handed over by another app ("add to calendar"). */
    val newEvent = kotlinx.coroutines.flow.MutableStateFlow<de.danerd.ghulsuite.model.CalEventUi?>(null)

    /** A calendar file (.ics) opened with GhulSuite: imported into the default calendar. */
    val icsFile = kotlinx.coroutines.flow.MutableStateFlow<ByteArray?>(null)

    /** Files shared from another app with "Save in GhulDrive" (opened only when uploaded). */
    val driveFiles = kotlinx.coroutines.flow.MutableStateFlow<List<de.danerd.ghulsuite.model.ByteSource>?>(null)

    /** A tapped mail notification: the new mail, or the account's inbox. */
    val openMail = kotlinx.coroutines.flow.MutableStateFlow<MailTarget?>(null)
}

/** The e-mail account sheet: [id] null adds an account, else edits that one. */
data class MailSheet(val id: String?)

/** What a mail notification opens: [uid] 0 = the inbox of [accountId], else that mail in it. */
data class MailTarget(val accountId: String, val uid: Long)

/** Screens shown on top of the tabs. */
sealed interface Screen {
    data class EntryDetail(val vaultId: String, val id: String) : Screen
    data class EntryEdit(val vaultId: String, val id: String?, val type: String) : Screen
    /** [mode]: 0 scan, 1 manual, 2 otpauth link, -1 default. */
    data class AddTotp(val vaultId: String, val mode: Int = -1) : Screen
    data object ChangePassword : Screen
    data object Import : Screen
    data object Export : Screen
    data class RecoveryCode(val code: String) : Screen
    data object AddServer : Screen
    data class SettingsPage(val page: SetPage) : Screen
    /** Installing the server module (share links) on a server. */
    data class ModuleInstall(val serverId: String) : Screen
    data object Shares : Screen
    data object Security : Screen
    data class MailRead(val accountId: String, val folder: String, val uid: Long) : Screen
    data class MailCompose(val draft: MailDraft) : Screen
    /** The mail archive of an account in GhulDrive. */
    data class MailArchive(val accountId: String) : Screen
    /** An archived mail (an .eml file in GhulDrive). */
    data class MailArchiveRead(val accountId: String, val fileId: String, val name: String) : Screen
    data class CalEvent(val id: String, val day: String) : Screen
    data class CalEdit(val draft: de.danerd.ghulsuite.model.CalEventUi) : Screen
}

/** Content prefilled when writing, replying or forwarding. */
data class MailDraft(
    val accountId: String?,
    val to: String = "",
    val cc: String = "",
    val subject: String = "",
    val text: String = "",
    val inReplyTo: String = "",
    val attachments: List<de.danerd.ghulsuite.model.OutgoingAttachmentUi> = emptyList(),
    /** Stored draft this mail continues (empty: new). */
    val draftId: String = "",
    val bcc: String = "",
)


/**
 * UI state and navigation of the whole app. Runs the background work after unlocking (automatic
 * archive, scheduled mail, update check) and owns the hooks of [JobStatus].
 */
class AppState(val backend: VaultBackend, val scope: CoroutineScope) {
    /** A vault opened on its own instead of the own suite (see [SingleVault]). */
    var single by mutableStateOf<SingleUi?>(null)

    /** The vault opened on its own could not be written back into its file. */
    var singleSave by mutableStateOf(de.danerd.ghulsuite.model.SingleSaveUi.SAVED)

    /** See [singleSave]. */
    val singleNotSaved: Boolean get() = singleSave != de.danerd.ghulsuite.model.SingleSaveUi.SAVED

    /** A vault file waiting to be added to the own suite once it is unlocked. */
    var pendingAdd by mutableStateOf<de.danerd.ghulsuite.model.PickedFile?>(null)

    var vaultExists by mutableStateOf(backend.exists())

    /** Shows the start screen (open another suite or vault) although a suite exists. */
    var showStart by mutableStateOf(false)

    /** Vaults of the open suite. */
    var vaults by mutableStateOf<List<VaultUi>>(emptyList())
        private set

    /** Long work in the background (uploads, downloads, mail archive); declared before init. */
    val jobs = Jobs(this)

    /**
     * The language of the screens (set by the root screen). Texts looked up outside of a screen use
     * it; without a screen the lookup would need the display (and fail where there is none).
     */
    val resEnv = kotlinx.coroutines.flow.MutableStateFlow<org.jetbrains.compose.resources.ResourceEnvironment?>(null)

    private suspend fun env(): org.jetbrains.compose.resources.ResourceEnvironment? =
        resEnv.value ?: kotlinx.coroutines.withTimeoutOrNull(5_000) { resEnv.first { it != null } }

    /** A text in the language of the screens. */
    suspend fun str(res: org.jetbrains.compose.resources.StringResource, vararg args: Any): String =
        env()?.let { getString(it, res, *args) } ?: getString(res, *args)

    /** A counted text ("1 file", "3 files") in the language of the screens. */
    suspend fun plural(res: org.jetbrains.compose.resources.PluralStringResource, count: Int, vararg args: Any): String =
        env()?.let { getPluralString(it, res, count, *args) } ?: getPluralString(res, count, *args)

    /** Messages of jobs that locking stopped: shown after the next unlock (not on the lock screen). */
    private val afterUnlock = mutableListOf<String>()

    fun noteAfterUnlock(text: String) {
        afterUnlock += text
    }

    /** An automatic archive run is being started (set before its first suspension: one run at a time). */
    internal var autoArchiveStarting = false

    /** GhulDrive's content changed in the background (an upload finished): the list reads it again. */
    var driveTick by mutableStateOf(0)

    /** The mail archive in GhulDrive changed (archived, deleted from it). */
    var archiveTick by mutableStateOf(0)

    /** Archived mails (GhulDrive file ids) that could not be read as a mail. */
    var archiveUnreadable by mutableStateOf<Set<String>>(emptySet())

    /** The folder menu of GhulMail (header "⋮") is open. */
    var mailFolderMenu by mutableStateOf(false)

    /** Folders opened in the mail archive (id and name below the account's folder); kept while a mail is read. */
    var archivePath by mutableStateOf<List<Pair<String, String>>>(emptyList())

    /** The vault list was read since unlocking (until then all areas switched on count as there). */
    private var areasKnown by mutableStateOf(false)

    /** After the setup wizard: show the first area once the vault list is read. */
    var openFirstArea = false

    /** The recovery code was never confirmed: a banner asks for a new one (#24). */
    var codeUnconfirmed by mutableStateOf(false)
    /** A running key change after a change of the protection (null: none, or not known yet). */
    var keyChange by mutableStateOf<KeyChangeUi?>(null)
    /** The user put the key change's steps by hand off until the next unlock. */
    var keyChangeLater by mutableStateOf(false)

    /** The area has its file in this suite (it may still be hidden on this device). */
    fun hasArea(kind: VaultKindUi): Boolean = !areasKnown || vaults.any { it.kind == kind }

    /** Vault filter of the GhulPass and GhulAuth lists (null = all vaults). */
    var passVault by mutableStateOf<String?>(null)
    var authVault by mutableStateOf<String?>(null)
    var tab by mutableStateOf(Tab.PASS)
    val stack = mutableStateListOf<Screen>()
    val snackbar = SnackbarHostState()
    var busy by mutableStateOf(false)
        private set
    var touchNeeded by mutableStateOf(false)
        private set

    /** Incremented whenever vault content changes, so lists reload. */
    var revision by mutableStateOf(0)
        private set

    val onTouchNeeded: () -> Unit = { touchNeeded = true }

    /** GhulMail: the account shown and whether any account exists (for the write button). */
    var mailAccount by mutableStateOf<String?>(null)
    var mailReady by mutableStateOf(false)

    /** GhulDrive is set up and can take an upload from the "+" button; a tick asks for one. */
    var driveReady by mutableStateOf(false)
    var driveUploadTick by mutableStateOf(0)

    /**
     * A deletion waiting for "Delete?" to be answered (F103: every deletion asks first). [extra]:
     * one more line under the question (e.g. which share links end with it).
     */
    /** A question before deleting; while not [ready] (still checking, e.g. share links) it cannot be confirmed. */
    class DeleteAsk(val names: List<String>, val trash: Boolean, val action: () -> Unit, val extra: String? = null, val ready: Boolean = true)

    // Declared before init (the lock listener there clears it).
    var deleteAsk by mutableStateOf<DeleteAsk?>(null)

    /** Files shared from another app into GhulDrive ("Save in GhulDrive"), waiting for GhulDrive. */
    var driveIncoming by mutableStateOf<List<de.danerd.ghulsuite.model.ByteSource>?>(null)
    /** GhulDrive shows its trash: nothing can be added there, so there is no "Add" button. */
    var driveTrashShown by mutableStateOf(false)

    /** Header actions of the tabs: list/grid views, mail refresh, unread count for the badge. */
    var driveGrid by mutableStateOf(false)
    var authGrid by mutableStateOf(false)
    var mailRefreshTick by mutableStateOf(0)

    /** Open e-mail account sheet (shown above everything, from any place that offers it). */
    var mailAccountSheet by mutableStateOf<MailSheet?>(null)
    var mailUnread by mutableStateOf(0)
    /** Messages of the open mailbox (for conversations in the reading view). */
    var mailLoaded by mutableStateOf<List<MailItem>>(emptyList())

    // --- GhulCal
    var calView by mutableStateOf(CalView.entries[backend.settings.calStartView.coerceIn(0, 3)])
    var calSel by mutableStateOf(today())
    /** Incremented to focus the search field of the list view. */
    var calSearchTick by mutableStateOf(0)
    /** The first calendar vault was created in this session (only once). */
    var calSetupDone = false
    /** Calendars of all calendar vaults (refreshed with the vaults). */
    var calendars by mutableStateOf<List<de.danerd.ghulsuite.model.CalCalendarUi>>(emptyList())
    val calNames: Map<String, String> get() = calendars.associate { it.id to it.name }
    var calRegionName by mutableStateOf("")
        private set
    var calQuickOpen by mutableStateOf(false)

    /** Calendar for new events: the chosen default, else the first one. */
    val calDefaultCalendar: String
        get() = calendars.firstOrNull { it.id == backend.settings.calDefault }?.id ?: calendars.firstOrNull()?.id ?: ""

    /** Imports a calendar file opened with GhulSuite into the default calendar. */
    fun importIcs(data: ByteArray) {
        scope.launch {
            // Right after unlocking the calendars may still be loading.
            var waited = 0
            while (calDefaultCalendar.isEmpty() && waited < 50) { kotlinx.coroutines.delay(100); waited++ }
            val target = calDefaultCalendar
            // U15: without any calendar the import says so instead of doing nothing.
            if (target.isEmpty()) return@launch message(getString(Res.string.cal_import_no_calendar))
            catching { backend.cal.importIcs(target, data) }
                .onSuccess { n ->
                    changed()
                    message(getPluralString(Res.plurals.cal_import_done, n, n.toString()))
                }
                .onFailure { message(describe(it)) }
        }
    }

    /** Shows a sample reminder with the chosen lock-screen detail (options). */
    fun calTestReminder(title: String) {
        val st = backend.settings
        scope.launch {
            val now = localNow()
            val start = fromAbsMinutes(absMinutes(now) + 15)
            val r = de.danerd.ghulsuite.model.CalReminderUi("", title, getString(Res.string.cal_test_place), false, start,
                fromAbsMinutes(absMinutes(start) + 45), now, 15)
            backend.cal.showReminder(calAlarm(r, st.calNotifyDetail, loadCalNames()), st.calSound)
        }
    }

    /** ‹ › in the calendar: one month, week or day back or forward. */
    fun calNav(n: Int) {
        val s = calSel
        calSel = when (calView) {
            CalView.MONTH -> {
                val d = s.firstOfMonth.plusMonths(n)
                if (d.y == today().y && d.m == today().m) today() else d
            }
            CalView.WEEK -> s.plus(7 * n)
            else -> s.plus(n)
        }
    }

    /** Reloads the calendars and hands the reminders of the next 30 days to the system. */
    fun refreshCalendar() {
        val st = backend.settings
        val alone = single != null
        if (!st.moduleGhulCal && !alone) {
            backend.cal.clearReminders()
            return
        }
        scope.launch {
            calendars = catching { backend.cal.calendars() }.getOrDefault(emptyList())
            val lang = getString(Res.string.cal_lang)
            calRegionName = catching { backend.cal.regions(lang) }.getOrDefault(emptyList()).firstOrNull { it.code == st.calRegion }?.name ?: ""
            // A calendar opened on its own does not replace the reminders of the own suite.
            if (alone) return@launch
            val rem = catching { backend.cal.reminders(localNow(), 30) }.getOrDefault(emptyList())
            val names = loadCalNames()
            backend.cal.scheduleReminders(rem.take(400).map { calAlarm(it, st.calNotifyDetail, names) }, st.calSound)
        }
    }

    var mailFolder by mutableStateOf("INBOX")

    /** The context sheet of the current area is open (vault, storage, mailbox). */
    var contextSheet by mutableStateOf(false)

    /** B6: a calendar file another app handed over, waiting for the user to confirm the import. */
    var icsPending by mutableStateOf<ByteArray?>(null)

    /**
     * U12: an editor (login, event) holds input that is not saved. "Back" then asks first. Locking
     * cannot wait (security): it drops the input, and after unlocking the app says so.
     */
    var editorDirty by mutableStateOf(false)

    /** U11: opened with the recovery code: the next steps are shown once (see [RecoveredDialog]). */
    var recoveredGuide by mutableStateOf(false)

    /** A newer version was found: the update dialog shows it. */
    var update by mutableStateOf<UpdateUi?>(null)
    /** Looking for or loading an update. */
    var updateBusy by mutableStateOf(false)

    /**
     * Looks for a newer version on the chosen channel. Automatically at most once a day after
     * unlocking (and only if switched on); [manual] from the options, with a message either way.
     */
    fun checkUpdate(manual: Boolean) {
        val up = backend.updater
        val s = backend.settings
        if (!up.available || updateBusy) return
        val now = nowSeconds()
        if (!manual && (!s.updateAuto || now - (s.updateLastCheck.toLongOrNull() ?: 0) < 86_400)) return
        updateBusy = true
        scope.launch {
            try {
                val found = up.check(s.updateDev)
                s.updateLastCheck = now.toString()
                if (found != null) update = found
                else if (manual) message(getString(Res.string.upd_none, backend.appVersion))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Automatically: silent, tried again after the next unlock.
                if (manual) message(describe(e))
            } finally {
                updateBusy = false
            }
        }
    }

    /** Downloads the update, checks it and hands it to the installer of the system. */
    fun installUpdate(u: UpdateUi) {
        if (updateBusy) return
        update = null
        updateBusy = true
        // The note must not hold up the installation.
        scope.launch { catching { message(getString(Res.string.upd_loading)) } }
        scope.launch {
            try {
                if (backend.updater.install(u) == UpdateStep.ALLOW_FIRST) {
                    update = u
                    message(getString(Res.string.upd_allow))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                message(describe(e))
            } finally {
                updateBusy = false
            }
        }
    }

    var confirmDiscard by mutableStateOf(false)
    private var discardedOnLock = false

    /** Back from the current screen; an editor with unsaved input asks first. */
    fun back() {
        if (editorDirty) confirmDiscard = true else pop()
    }

    fun discardAndBack() {
        editorDirty = false
        confirmDiscard = false
        val tab = pendingTab
        pendingTab = null
        // Leaving through the sidebar: the editor goes and the chosen area opens.
        if (tab != null) selectTab(tab) else pop()
    }

    /** The area chosen in the sidebar while an editor had unsaved input (opens after "discard"). */
    private var pendingTab: Tab? = null

    /** "Keep editing" in the question about unsaved input: nothing changes. */
    fun keepEditing() {
        confirmDiscard = false
        pendingTab = null
    }

    init {
        scope.launch {
            backend.unlocked.collect { unlocked ->
                if (!unlocked) {
                    // Locked from anywhere (panic, notification, screen off): running jobs stop now.
                    jobs.cancelAll(byLock = true)
                    jobs.lockWhenDone = false
                    mailFolderMenu = false
                    archivePath = emptyList()
                    archiveUnreadable = emptySet()
                    if (editorDirty) discardedOnLock = true
                    editorDirty = false
                    recoveredGuide = false
                    confirmDiscard = false
                    pendingTab = null
                    driveIncoming = null
                    deleteAsk = null
                    dropPending()
                    stack.clear()
                    tab = tabs.first()
                    vaults = emptyList()
                    passVault = null
                    authVault = null
                    mailUnread = 0
                    areasKnown = false
                    codeUnconfirmed = false
                    keyChange = null
                    keyChangeLater = false
                    if (single != null) {
                        // The vault opened on its own is closed: back to the own suite (locked) or the start.
                        single = null
                        unsaved = false
                        vaultExists = backend.exists()
                    }
                } else {
                    if (discardedOnLock) {
                        discardedOnLock = false
                        message(getString(Res.string.edit_dropped_on_lock))
                    }
                    afterUnlock.toList().also { afterUnlock.clear() }.forEach { message(it) }
                    refreshVaults()
                    if (single != null) {
                        refreshCalendar()
                        return@collect
                    }
                    sendDueMail()
                    runAutoArchive()
                    // A key change that stopped (no network, app closed) continues.
                    continueKeyChange()
                    // GhulDrive tidies up its server quietly (blocks nothing uses any more, once a day).
                    if (backend.settings.moduleGhulDrive && hasArea(VaultKindUi.DRIVE)) {
                        scope.launch { catching { backend.net.driveTidy() } }
                    }
                    val start = backend.settings.startTab.let { if (it < 0) backend.settings.lastTab else it }
                    tabs.getOrNull(start)?.let { tab = it }
                    applyLaunch()
                    refreshUnread()
                    if (backend.settings.moduleGhulMail && hasArea(VaultKindUi.MAIL)) {
                        scope.launch { catching { backend.net.refreshMailNotifications() } }
                    }
                    refreshCalendar()
                    checkUpdate(manual = false)
                }
            }
        }
        // The notification's "Cancel" (Android) stops the running jobs; the automatic lock of
        // Android asks the jobs first.
        JobStatus.cancelAll = { jobs.cancelAll() }
        JobStatus.autoLock = { autoLock() }
        JobStatus.keepUnlocked = { keepUnlocked() }
        // Going to the background carries out what waits for "Undo": the auto lock may come next.
        scope.launch { AppLaunch.inFront.collect { if (!it) catching { settlePending() } } }
        // Area settings go into the area's file (they travel with the suite file).
        backend.settings.onAreaChange = { kind, key, value ->
            if (backend.unlocked.value && single == null && vaults.any { it.kind == kind }) {
                scope.launch {
                    catching { backend.setAreaOptions(kind, mapOf(key to value)) }
                }
            }
        }
        scope.launch { AppLaunch.openTab.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.timeZoneChanged.collect { if (it > 0 && backend.unlocked.value) refreshCalendar() } }
        scope.launch { AppLaunch.openEvent.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.openMail.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.compose.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.newEvent.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.icsFile.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.driveFiles.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch {
            AppLaunch.newMail.collect {
                if (it > 0 && backend.unlocked.value) {
                    mailRefreshTick++
                    refreshUnread()
                }
            }
        }
        // A vault opened on its own goes back into its file soon after every change, whatever made it.
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(1500)
                // Changed elsewhere: the user decides (overwrite or save as), not this loop. Gone or
                // failed: tried again (written as soon as the storage is back and unchanged).
                if (single != null && backend.unlocked.value && !busy &&
                    singleSave != de.danerd.ghulsuite.model.SingleSaveUi.CHANGED_ELSEWHERE &&
                    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { backend.archiveUnsaved() }
                ) {
                    singleSave = catching { backend.saveSingle() }.getOrDefault(de.danerd.ghulsuite.model.SingleSaveUi.FAILED)
                    unsaved = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { backend.archiveUnsaved() }
                }
            }
        }
        // Unread count for the navigation badge (and the window icon on Windows) while unlocked.
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(5 * 60 * 1000L)
                if (backend.unlocked.value) refreshUnread()
            }
        }
    }

    private fun applyLaunch() {
        AppLaunch.openEvent.value?.let { ev ->
            AppLaunch.openEvent.value = null
            if (Tab.CAL in tabs) {
                stack.clear()
                tab = Tab.CAL
                stack.add(Screen.CalEvent(ev.substringBefore('|'), ev.substringAfter('|', "")))
            }
        }
        AppLaunch.openMail.value?.let { m ->
            AppLaunch.openMail.value = null
            if (Tab.MAIL in tabs) {
                // Always the inbox of that account, whatever folder was open before.
                stack.clear()
                tab = Tab.MAIL
                mailAccount = m.accountId
                mailFolder = "INBOX"
                if (m.uid > 0) stack.add(Screen.MailRead(m.accountId, "INBOX", m.uid))
            }
        }
        AppLaunch.compose.value?.let { draft ->
            AppLaunch.compose.value = null
            if (Tab.MAIL in tabs) {
                stack.clear()
                tab = Tab.MAIL
                stack.add(Screen.MailCompose(draft))
            }
        }
        AppLaunch.newEvent.value?.let { ev ->
            AppLaunch.newEvent.value = null
            if (Tab.CAL in tabs) {
                stack.clear()
                tab = Tab.CAL
                stack.add(Screen.CalEdit(ev.copy(calendarId = ev.calendarId.ifEmpty { calDefaultCalendar })))
            }
        }
        AppLaunch.driveFiles.value?.let { files ->
            AppLaunch.driveFiles.value = null
            if (Tab.DRIVE in tabs) {
                stack.clear()
                tab = Tab.DRIVE
                driveIncoming = files
                scope.launch {
                    // GhulDrive without storage: the files wait; say what to do.
                    // Only when GhulDrive surely has none (not when the server is just unreachable).
                    catching { backend.net.driveStatus() }.onSuccess { if (it.serverId == null) message(getString(Res.string.drive_upload_not_set_up)) }
                }
            } else {
                scope.launch { message(getString(Res.string.drive_upload_not_set_up)) }
            }
        }
        AppLaunch.icsFile.value?.let { data ->
            AppLaunch.icsFile.value = null
            if (Tab.CAL in tabs) {
                stack.clear()
                tab = Tab.CAL
                icsPending = data
            }
        }
        val t = AppLaunch.openTab.value ?: return
        AppLaunch.openTab.value = null
        if (t !in tabs) return
        stack.clear()
        tab = t
    }

    /** Sums the unread messages of all inboxes; keeps the old count when offline. */
    fun refreshUnread() {
        if (!backend.settings.moduleGhulMail) return
        scope.launch {
            val accounts = catching { backend.net.mailAccounts() }.getOrNull() ?: return@launch
            var sum = 0
            for (a in accounts) sum += catching { backend.net.mailUnseen(a.id) }.getOrElse { return@launch }
            mailUnread = sum
        }
    }

    /** Tabs of enabled modules plus settings. */
    /** Tabs of the areas this suite has and this device shows, plus settings. */
    val tabs: List<Tab>
        get() = buildList {
            single?.let {
                add(tabOf(it.kind))
                add(Tab.SETTINGS)
                return@buildList
            }
            if (backend.settings.moduleGhulPass && hasArea(VaultKindUi.PASS)) add(Tab.PASS)
            if (backend.settings.moduleGhulAuth && hasArea(VaultKindUi.TOTP)) add(Tab.AUTH)
            if (backend.settings.moduleGhulDrive && hasArea(VaultKindUi.DRIVE)) add(Tab.DRIVE)
            if (backend.settings.moduleGhulMail && hasArea(VaultKindUi.MAIL)) add(Tab.MAIL)
            if (backend.settings.moduleGhulCal && hasArea(VaultKindUi.CAL)) add(Tab.CAL)
            add(Tab.SETTINGS)
        }

    /** Opens the print dialog for the emergency sheet of [code]. */
    fun printSheet(code: String) {
        scope.launch {
            val text = de.danerd.ghulsuite.model.EmergencySheetText(
                title = getString(Res.string.sheet_title),
                intro = getString(Res.string.sheet_intro),
                codeLabel = getString(Res.string.sheet_code_label),
                created = getString(Res.string.sheet_created, formatDate(nowSeconds() * 1000)),
                steps = listOf(
                    getString(Res.string.sheet_step1),
                    getString(Res.string.sheet_step2),
                    getString(Res.string.sheet_step3),
                    getString(Res.string.sheet_step4),
                ),
                warning = getString(Res.string.sheet_warning),
            )
            catching { backend.printEmergencySheet(code, text) }.onFailure { message(describe(it)) }
        }
    }

    /** Wide window: the areas in a sidebar on the left, everything else in the rest of the width. */
    var wide by mutableStateOf(false)

    fun push(screen: Screen) {
        stack.add(screen)
    }

    /**
     * Switches the area. With the sidebar the areas stay in reach while a page is open: switching
     * closes it (on phones the bar at the bottom is hidden then).
     */
    fun selectTab(t: Tab) {
        // An editor with unsaved input asks first, as "back" does; a new recovery code cannot be
        // left before it is confirmed (as with "back").
        if (wide && stack.lastOrNull() is Screen.RecoveryCode) return
        if (wide && editorDirty && stack.isNotEmpty()) {
            pendingTab = t
            confirmDiscard = true
            return
        }
        if (wide) stack.clear()
        tab = t
        tabs.indexOf(t).takeIf { it >= 0 }?.let { backend.settings.lastTab = it }
    }

    fun pop() {
        if (stack.isNotEmpty()) stack.removeAt(stack.lastIndex)
    }

    /** The working copy has changes that are not in the suite file yet (see [SuiteFileSection]). */
    var unsaved by mutableStateOf(false)

    /** Writes the vault opened on its own back into its file now ("Try again", "Overwrite", "Write anew"). */
    fun saveSingle(mode: de.danerd.ghulsuite.model.SingleWrite = de.danerd.ghulsuite.model.SingleWrite.CHECKED) {
        if (single == null) return
        run(block = { backend.saveSingle(mode) }) { r ->
            singleSave = r
            unsaved = backend.archiveUnsaved()
        }
    }

    /** Something changed: reload. */
    fun changed() {
        unsaved = backend.archiveUnsaved()
        revision++
        refreshVaults()
        refreshCalendar()
    }

    /**
     * After a change of the protection (password, recovery code, a key removed): an older copy
     * of the suite file is shut out of GhulDrive and the servers (new keys there).
     */
    fun protectionChanged() {
        changed()
        continueKeyChange()
    }

    /** Continues a running key change as far as it goes now; shows what is left. */
    fun continueKeyChange() {
        scope.launch {
            // Only then the network: most of the time no change runs.
            if (catching { backend.net.keyChangeStatus() }.getOrNull()?.running != true) {
                keyChange = null
                return@launch
            }
            keyChange = catching { backend.net.keyChangeContinue() }.getOrNull()?.takeIf { it.running }
        }
    }

    fun refreshVaults() {
        scope.launch {
            vaults = catching { backend.listVaults() }.getOrDefault(vaults)
            if (backend.unlocked.value) {
                areasKnown = true
                if (openFirstArea) {
                    openFirstArea = false
                    stack.clear()
                    tab = tabs.first()
                }
                codeUnconfirmed = catching { !backend.recoveryConfirmed() }.getOrDefault(false)
                // The options of a vault opened on its own are not this device's.
                if (single == null) loadAreaOptions()
            }
            if (passVault != null && vaults.none { it.id == passVault }) passVault = null
            if (authVault != null && vaults.none { it.id == authVault }) authVault = null
        }
    }

    /**
     * Takes over the settings stored in the area files. Settings an area's file does not have yet
     * are filled in once from this device (suites from before #24).
     */
    private suspend fun loadAreaOptions() {
        val st = backend.settings
        for (kind in vaults.map { it.kind }.distinct()) {
            val stored = catching { backend.areaOptions(kind) }.getOrNull() ?: continue
            st.applyArea(stored)
            val missing = st.deviceValues(kind).filterKeys { it !in stored }
            if (missing.isNotEmpty()) catching { backend.setAreaOptions(kind, missing) }
        }
    }

    fun vaultsOf(kind: VaultKindUi): List<VaultUi> = vaults.filter { it.kind == kind }

    /** Switches on the module of a vault the user just added. */
    fun enableModule(kind: VaultKindUi) {
        when (kind) {
            VaultKindUi.PASS -> backend.settings.moduleGhulPass = true
            VaultKindUi.TOTP -> backend.settings.moduleGhulAuth = true
            VaultKindUi.CAL -> backend.settings.moduleGhulCal = true
            VaultKindUi.DRIVE -> backend.settings.moduleGhulDrive = true
            VaultKindUi.MAIL -> backend.settings.moduleGhulMail = true
        }
    }

    fun message(text: String) {
        scope.launch { snackbar.showSnackbar(text) }
    }

    // --- Undo -------------------------------------------------------------------------------------

    /** A deletion or move that is shown as done but not carried out yet ("Undo" in the snackbar). */
    private class Pending(val keys: Set<String>, val message: String, val action: suspend () -> Unit, val after: () -> Unit)

    private var pending: Pending? = null

    /** "Undo" in the language shown (set by the screen, which knows it without a lookup). */
    var undoLabel = ""

    /** How long "Undo" is offered (tests shorten or lengthen it). */
    var undoMillis = UNDO_MILLIS

    /**
     * Items that are gone from their lists while their deletion waits for "Undo". Lists leave out
     * what is in here: [hiddenKey] builds the keys (area, place and id, so a file in the trash is
     * not the same key as the file in its folder).
     */
    var hidden by mutableStateOf<Set<String>>(emptySet())
        private set

    /** Whether there is an action waiting for "Undo" (tests, and the lock that carries it out first). */
    val hasPending: Boolean get() = pending != null

    /**
     * Deletes or moves something the way every area does it: the items with [keys] disappear at
     * once, the snackbar shows [message] with "Undo" for a few seconds. Undo brings them back and
     * nothing happened; otherwise [action] runs when the snackbar ends, when the next such action
     * starts (the earlier one first), or before the app locks itself or goes to the background. If
     * the suite was locked in another way (panic lock, auto lock) the action is dropped and the
     * items simply stay: nothing is lost and nothing is half deleted. [after] runs once it is done.
     */
    fun undoable(keys: Collection<String>, message: String, after: () -> Unit = {}, action: suspend () -> Unit) {
        val p = Pending(keys.toSet(), message, action, after)
        scope.launch {
            settlePending()
            pending = p
            hidden = hidden + p.keys
            snackbar.currentSnackbarData?.dismiss()
            val undo = kotlinx.coroutines.coroutineScope {
                val timer = launch {
                    kotlinx.coroutines.delay(undoMillis)
                    snackbar.currentSnackbarData?.takeIf { it.visuals.message == p.message && it.visuals.actionLabel != null }?.dismiss()
                }
                val r = snackbar.showSnackbar(p.message, undoLabel.ifEmpty { getString(Res.string.a_undo) }, duration = androidx.compose.material3.SnackbarDuration.Indefinite)
                timer.cancel()
                r == androidx.compose.material3.SnackbarResult.ActionPerformed
            }
            settle(p, undo)
        }
    }

    /**
     * Something already done that "Undo" takes back (F96: moving to GhulDrive's trash, so the
     * number of files is right at once; the trash keeps the file anyway). The snackbar shows
     * [message] with "Undo" for a few seconds; Undo runs [undo], then [afterUndo].
     */
    fun doneWithUndo(message: String, undo: suspend () -> Unit, afterUndo: () -> Unit = {}) {
        scope.launch {
            settlePending()
            snackbar.currentSnackbarData?.dismiss()
            val undone = kotlinx.coroutines.coroutineScope {
                val timer = launch {
                    kotlinx.coroutines.delay(undoMillis)
                    snackbar.currentSnackbarData?.takeIf { it.visuals.message == message && it.visuals.actionLabel != null }?.dismiss()
                }
                val r = snackbar.showSnackbar(message, undoLabel.ifEmpty { getString(Res.string.a_undo) }, duration = androidx.compose.material3.SnackbarDuration.Indefinite)
                timer.cancel()
                r == androidx.compose.material3.SnackbarResult.ActionPerformed
            }
            if (!undone || !backend.unlocked.value) return@launch
            // Also after a failure part of it may be back: show what is true.
            val failure = catching { undo() }.exceptionOrNull()
            afterUndo()
            failure?.let { message(describe(it)) }
        }
        doneUndoMessage = message
    }

    /** The message of [doneWithUndo] (a lock takes it away: "Undo" cannot work after it). */
    private var doneUndoMessage: String? = null


    /**
     * Asks before deleting (or moving to a trash) [names]; [action] runs only after "Delete".
     * Every area deletes through this; "Undo" afterwards stays as it was. [extra] adds a line to
     * the question.
     */
    fun askDelete(names: List<String>, trash: Boolean = false, extra: String? = null, action: () -> Unit) {
        if (names.isEmpty()) return
        deleteAsk = DeleteAsk(names, trash, action, extra)
    }

    /** Carries out the action that waits for "Undo" now (before the next one, before locking). */
    suspend fun settlePending() {
        val p = pending ?: return
        snackbar.currentSnackbarData?.takeIf { it.visuals.message == p.message && it.visuals.actionLabel != null }?.dismiss()
        settle(p, undo = false)
    }

    private suspend fun settle(p: Pending, undo: Boolean) {
        if (pending !== p) return
        pending = null
        if (undo || !backend.unlocked.value) {
            hidden = hidden - p.keys
            return
        }
        try {
            p.action()
        } catch (e: CancellationException) {
            hidden = hidden - p.keys
            throw e
        } catch (e: Exception) {
            // Not done: the items come back and the error says why.
            hidden = hidden - p.keys
            message(describe(e))
            revision++
            return
        }
        p.after()
        changed()
        // The keys stay hidden until the lists have read the new state, so nothing flickers back.
        scope.launch {
            kotlinx.coroutines.delay(1500)
            hidden = hidden - p.keys
        }
    }

    /** Drops what waits for "Undo" (the suite was locked from outside): the items stay. */
    private fun dropPending() {
        doneUndoMessage?.let { m -> snackbar.currentSnackbarData?.takeIf { it.visuals.message == m }?.dismiss() }
        doneUndoMessage = null
        val p = pending ?: return
        pending = null
        hidden = hidden - p.keys
        snackbar.currentSnackbarData?.takeIf { it.visuals.message == p.message }?.dismiss()
    }

    /**
     * Locks the suite after carrying out what waits for "Undo". Every lock button uses this. A
     * running job stops at once (its message follows after the next unlock).
     */
    fun lockNow() {
        jobs.lockWhenDone = false
        jobs.cancelAll(byLock = true)
        // A state whose screens are gone (its scope ended) still locks.
        if (!scope.coroutineContext[kotlinx.coroutines.Job].let { it == null || it.isActive }) {
            backend.lock()
            return
        }
        scope.launch {
            catching { settlePending() }
            backend.lock()
        }
    }

    /**
     * An automatic lock (time away, idle, screen off, Windows locked): quiet jobs stop at once;
     * jobs the user started are waited for while they make progress (see [Jobs.autoLock]). A lock
     * by hand stops every job at once ([lockNow]).
     */
    fun autoLock() = jobs.autoLock()

    /** The user is back before the jobs ended: the waiting automatic lock is dropped. */
    fun keepUnlocked() {
        jobs.lockWhenDone = false
    }

    private var dueJob: kotlinx.coroutines.Job? = null

    /** Sends scheduled mail that is due, now and every minute while unlocked. */
    fun sendDueMail() {
        dueJob?.cancel()
        dueJob = scope.launch {
            while (backend.unlocked.value) {
                if (backend.settings.moduleGhulMail) {
                    catching { backend.net.mailSendDue(nowSeconds() * 1000) }.onSuccess { if (it > 0) revision++ }
                }
                kotlinx.coroutines.delay(60_000)
            }
        }
    }

    fun messageRes(res: org.jetbrains.compose.resources.StringResource) {
        scope.launch { snackbar.showSnackbar(getString(res)) }
    }

    fun copy(label: String, value: String) {
        backend.copySensitive(label, value)
        scope.launch { snackbar.showSnackbar(getString(Res.string.msg_copied, label)) }
    }

    /**
     * Runs [block] with a busy indicator. Errors are turned into localized text and passed to
     * [onError] (default: snackbar).
     */
    fun <T> run(
        onError: (String) -> Unit = ::message,
        block: suspend () -> T,
        onSuccess: (T) -> Unit = {},
    ) {
        if (busy) {
            // A second tap while the first one still runs is meant to be dropped. An action the app
            // starts itself while another one is running would be lost without a word (F49).
            if (inCallback > 0) DevChecks.alarm("Folgeaktion verworfen, die App war noch beschäftigt")
            return
        }
        // Busy at once, in the same tap: a second tap right after it must find the app busy, also
        // before the coroutine had its first turn (else Save tapped twice saved twice).
        busy = true
        touchNeeded = false
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            var failure: String? = null
            val result = try {
                block()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failure = describe(e)
                null
            } finally {
                busy = false
                touchNeeded = false
            }
            // Only once the action is done: a follow-up action started here (run inside onSuccess,
            // e.g. "check GhulDrive first, then add a server") must not find the app busy,
            // or it would be dropped without a word. The same for onError.
            failure?.let { text ->
                callback { onError(text) }
                return@launch
            }
            try {
                @Suppress("UNCHECKED_CAST")
                callback { onSuccess(result as T) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                onError(describe(e))
            }
        }
    }

    /** Depth of onSuccess/onError calls of [run] running now (for [DevChecks]). */
    private var inCallback = 0

    private inline fun callback(f: () -> Unit) {
        inCallback++
        try {
            f()
        } finally {
            inCallback--
        }
    }
}

/**
 * The text shown for an error. Never throws itself: if even the translated text cannot be read
 * (e.g. the resources need a display that is not there), the raw message is shown, so a failure
 * while loading a page still ends as a message and not as a crash.
 */
suspend fun describe(e: Throwable): String = try {
    describeText(e)
} catch (c: CancellationException) {
    throw c
} catch (_: Exception) {
    // describeText has already told DevChecks about errors that are not a VaultException
    // (a VaultException is an expected error of the core, nothing to report).
    e.message ?: e.toString()
}

private suspend fun describeText(e: Throwable): String {
    if (e !is VaultException) {
        DevChecks.shown(e)
        return getString(Res.string.err_other, e.message ?: e.toString())
    }
    return when (e.kind) {
        VaultErrorKind.UPDATE_REQUIRED -> getString(Res.string.err_update_required)
        VaultErrorKind.NO_KEY -> getString(Res.string.err_no_key)
        VaultErrorKind.PERMISSION_DENIED -> getString(Res.string.err_permission)
        VaultErrorKind.NOT_A_FIDO_KEY -> getString(Res.string.err_not_fido)
        VaultErrorKind.TRANSPORT -> getString(Res.string.err_transport, coreText(e.detail) ?: e.detail)
        VaultErrorKind.TIMEOUT -> getString(Res.string.err_timeout)
        VaultErrorKind.PIN_NOT_SET -> getString(Res.string.err_pin_not_set)
        VaultErrorKind.PIN_INVALID -> getPluralString(Res.plurals.err_pin_invalid, e.retries ?: 0, (e.retries ?: 0).toString())
        VaultErrorKind.PIN_BLOCKED -> getString(Res.string.err_pin_blocked)
        VaultErrorKind.PIN_AUTH_BLOCKED -> getString(Res.string.err_pin_auth_blocked)
        VaultErrorKind.PIN_POLICY -> getString(Res.string.err_pin_policy)
        VaultErrorKind.QUICK_UNAVAILABLE -> getString(Res.string.err_quick_unavailable)
        VaultErrorKind.QUICK_EXPIRED -> getString(Res.string.err_quick_expired)
        VaultErrorKind.QUICK_INVALID -> getPluralString(Res.plurals.err_quick_invalid, e.retries ?: 0, (e.retries ?: 0).toString())
        VaultErrorKind.BIOMETRIC_CANCELLED -> getString(Res.string.err_bio_cancelled)
        VaultErrorKind.BIOMETRIC_CHANGED -> getString(Res.string.err_bio_changed)
        VaultErrorKind.HMAC_UNSUPPORTED -> getString(Res.string.err_hmac)
        VaultErrorKind.KEY_NOT_REGISTERED -> getString(Res.string.err_key_not_registered)
        VaultErrorKind.NOT_CONFIRMED -> getString(Res.string.err_not_confirmed)
        VaultErrorKind.KEY_PROTOCOL -> getString(Res.string.err_key_protocol, coreText(e.detail) ?: e.detail)
        VaultErrorKind.WRONG_PASSWORD -> getString(Res.string.err_wrong_password)
        VaultErrorKind.WRONG_SECOND_FACTOR -> getString(Res.string.err_wrong_code)
        VaultErrorKind.INVALID_RECOVERY_CODE -> getString(Res.string.err_invalid_code)
        VaultErrorKind.LOCKED -> getString(Res.string.err_locked)
        VaultErrorKind.CORRUPTED -> getString(Res.string.err_corrupted, coreText(e.detail) ?: e.detail)
        VaultErrorKind.TAMPERED -> getString(Res.string.err_tampered)
        VaultErrorKind.PROTECTION_OUTDATED -> getString(Res.string.err_protection_outdated)
        VaultErrorKind.SINGLE_BUSY -> getString(Res.string.single_busy)
        VaultErrorKind.SINGLE_PENDING -> getString(Res.string.single_pending_first)
        VaultErrorKind.STORAGE -> getString(Res.string.err_storage, coreText(e.detail) ?: e.detail)
        // U5: the core's known details are complete sentences in the app's language.
        VaultErrorKind.INVALID_INPUT -> coreText(e.detail) ?: getString(Res.string.err_input, e.detail)
        VaultErrorKind.SECURITY_KEY_REQUIRED -> getString(Res.string.err_key_required)
        VaultErrorKind.KEYFILE_REQUIRED -> getString(Res.string.err_keyfile_required)
        VaultErrorKind.WRONG_KEYFILE -> getString(Res.string.err_wrong_keyfile)
        VaultErrorKind.DIFFERENT_PROTECTION -> getString(Res.string.err_different_protection)
        VaultErrorKind.ALREADY_EXISTS -> getString(Res.string.err_already_exists)
        VaultErrorKind.LAST_FACTOR -> getString(Res.string.err_last_factor)
        VaultErrorKind.IMPORT_ENCRYPTED -> getString(Res.string.err_import_encrypted)
        VaultErrorKind.IMPORT_UNKNOWN -> getString(Res.string.err_import_unknown)
        VaultErrorKind.NETWORK -> getString(Res.string.err_network, coreText(e.detail) ?: e.detail)
        VaultErrorKind.HOST_KEY_UNKNOWN -> getString(Res.string.err_host_key_unknown, e.detail)
        VaultErrorKind.HOST_KEY_CHANGED -> getString(Res.string.err_host_key_changed, e.detail)
        VaultErrorKind.CERT_UNTRUSTED -> getString(Res.string.err_cert_untrusted, e.detail)
        VaultErrorKind.SERVER_AUTH -> getString(Res.string.err_server_auth)
        VaultErrorKind.DRIVE_ROLLED_BACK -> getString(Res.string.err_drive_rolled_back)
        VaultErrorKind.SHARE_NOT_DELETED -> getString(Res.string.err_share_not_deleted)
        VaultErrorKind.PHP_NOT_EXECUTED -> getString(Res.string.err_php_not_executed)
        VaultErrorKind.NOT_CONFIGURED -> getString(Res.string.err_not_configured)
        VaultErrorKind.NOT_FOUND -> getString(Res.string.err_not_found)
        VaultErrorKind.OTHER -> getString(Res.string.err_other, coreText(e.detail) ?: e.detail)
    }
}

/** Rules for the master password (always combined with a security key or key file). */
object PasswordPolicy {
    const val MIN_LENGTH = 10
    const val MIN_SCORE = 3

    /** Returns the error text or null if the password is acceptable. */
    suspend fun check(backend: VaultBackend, password: String, repeat: String): String? = when {
        password.length < MIN_LENGTH -> getPluralString(Res.plurals.setup_pw_too_short, MIN_LENGTH, MIN_LENGTH.toString())
        backend.passwordScore(password) < MIN_SCORE -> getString(Res.string.setup_pw_too_weak)
        password != repeat -> getString(Res.string.setup_pw_mismatch)
        else -> null
    }

    /** The same check with the texts the screen looked up already. */
    fun check(backend: VaultBackend, password: String, repeat: String, tooShort: String, tooWeak: String, mismatch: String): String? = when {
        password.length < MIN_LENGTH -> tooShort
        backend.passwordScore(password) < MIN_SCORE -> tooWeak
        password != repeat -> mismatch
        else -> null
    }
}
