// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import de.danerd.ghulsuite.model.VaultBackend
import de.danerd.ghulsuite.model.FoundVaultUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString

enum class Tab { PASS, AUTH, DRIVE, MAIL, CAL, SETTINGS }

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
    data class ShareSetup(val serverId: String) : Screen
    data object Shares : Screen
    data object Security : Screen
    data class MailRead(val accountId: String, val folder: String, val uid: Long) : Screen
    data class MailCompose(val draft: MailDraft) : Screen
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


/** UI state and navigation of the whole app. */
class AppState(val backend: VaultBackend, val scope: CoroutineScope) {
    var vaultExists by mutableStateOf(backend.exists())

    /** Shows the start screen (open another suite or vault) although a suite exists. */
    var showStart by mutableStateOf(false)

    /** Vaults of the open suite. */
    var vaults by mutableStateOf<List<VaultUi>>(emptyList())
        private set

    /** The vault list was read since unlocking (until then all areas switched on count as there). */
    private var areasKnown by mutableStateOf(false)

    /** After the setup wizard: show the first area once the vault list is read. */
    var openFirstArea = false

    /** The recovery code was never confirmed: a banner asks for a new one (#24). */
    var codeUnconfirmed by mutableStateOf(false)

    /** The area has its file in this suite (it may still be hidden on this device). */
    fun hasArea(kind: VaultKindUi): Boolean = !areasKnown || vaults.any { it.kind == kind }

    /** Vault files in the folder that are not part of the suite yet (offered after unlocking). */
    var found by mutableStateOf<List<FoundVaultUi>>(emptyList())

    /** The user postponed the "new vaults found" question for this session. */
    var foundPostponed by mutableStateOf(false)

    private var outdatedNotified = false

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
            runCatching { backend.cal.importIcs(target, data) }
                .onSuccess { n ->
                    changed()
                    message(getString(Res.string.cal_import_done).replace("%1\$s", n.toString()))
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
        if (!st.moduleGhulCal) {
            backend.cal.clearReminders()
            return
        }
        scope.launch {
            calendars = runCatching { backend.cal.calendars() }.getOrDefault(emptyList())
            val lang = getString(Res.string.cal_lang)
            calRegionName = runCatching { backend.cal.regions(lang) }.getOrDefault(emptyList()).firstOrNull { it.code == st.calRegion }?.name ?: ""
            val rem = runCatching { backend.cal.reminders(localNow(), 30) }.getOrDefault(emptyList())
            val names = loadCalNames()
            backend.cal.scheduleReminders(rem.take(400).map { calAlarm(it, st.calNotifyDetail, names) }, st.calSound)
        }
    }

    var mailFolder by mutableStateOf("INBOX")

    /** The context sheet of the current area is open (vault, storage, mailbox). */
    var contextSheet by mutableStateOf(false)

    /** A synchronisation is running in the background. */
    var syncing by mutableStateOf(false)
        private set

    /** The server has a suite with a newer master password: ask for it. */
    var passwordChangedElsewhere by mutableStateOf(false)
    /** B6: a calendar file another app handed over, waiting for the user to confirm the import. */
    var icsPending by mutableStateOf<ByteArray?>(null)
    /** A key was removed and the code renewed on another device (see [ProtectionChangedDialog]). */
    var protectionChangedElsewhere by mutableStateOf(false)

    private var syncAgain = false

    /**
     * U12: an editor (login, event) holds input that is not saved. "Back" then asks first. Locking
     * cannot wait (security): it drops the input, and after unlocking the app says so.
     */
    var editorDirty by mutableStateOf(false)

    /** U11: opened with the recovery code: the next steps are shown once (see [RecoveredDialog]). */
    var recoveredGuide by mutableStateOf(false)

    /** U13: the last automatic sync found no connection; shown on the area pages until a sync works. */
    var syncOffline by mutableStateOf(false)
    var confirmDiscard by mutableStateOf(false)
    private var discardedOnLock = false

    /** Back from the current screen; an editor with unsaved input asks first. */
    fun back() {
        if (editorDirty) confirmDiscard = true else pop()
    }

    fun discardAndBack() {
        editorDirty = false
        confirmDiscard = false
        pop()
    }

    init {
        scope.launch {
            backend.unlocked.collect { unlocked ->
                if (!unlocked) {
                    if (editorDirty) discardedOnLock = true
                    editorDirty = false
                    recoveredGuide = false
                    confirmDiscard = false
                    dropPending()
                    stack.clear()
                    tab = tabs.first()
                    vaults = emptyList()
                    passVault = null
                    authVault = null
                    found = emptyList()
                    foundPostponed = false
                    outdatedNotified = false
                    passwordChangedElsewhere = false
                    protectionChangedElsewhere = false
                    syncing = false
                    mailUnread = 0
                    areasKnown = false
                    codeUnconfirmed = false
                } else {
                    if (discardedOnLock) {
                        discardedOnLock = false
                        message(getString(Res.string.edit_dropped_on_lock))
                    }
                    refreshVaults()
                    scanFound()
                    autoSync()
                    runPhotoBackup()
                    sendDueMail()
                    runAutoBackup()
                    val start = backend.settings.startTab.let { if (it < 0) backend.settings.lastTab else it }
                    tabs.getOrNull(start)?.let { tab = it }
                    applyLaunch()
                    refreshUnread()
                    if (backend.settings.moduleGhulMail && hasArea(VaultKindUi.MAIL)) {
                        scope.launch { runCatching { backend.net.refreshMailNotifications() } }
                    }
                    refreshCalendar()
                }
            }
        }
        // Going to the background carries out what waits for "Undo": the auto lock may come next.
        scope.launch { AppLaunch.inFront.collect { if (!it) runCatching { settlePending() } } }
        // Area settings go into the area's file (and from there to the other devices).
        backend.settings.onAreaChange = { kind, key, value ->
            if (backend.unlocked.value && vaults.any { it.kind == kind }) {
                scope.launch {
                    runCatching { backend.setAreaOptions(kind, mapOf(key to value)) }.onSuccess { autoSync() }
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
        scope.launch {
            AppLaunch.newMail.collect {
                if (it > 0 && backend.unlocked.value) {
                    mailRefreshTick++
                    refreshUnread()
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
            val accounts = runCatching { backend.net.mailAccounts() }.getOrNull() ?: return@launch
            var sum = 0
            for (a in accounts) sum += runCatching { backend.net.mailUnseen(a.id) }.getOrElse { return@launch }
            mailUnread = sum
        }
    }

    /** Tabs of enabled modules plus settings. */
    /** Tabs of the areas this suite has and this device shows, plus settings. */
    val tabs: List<Tab>
        get() = buildList {
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
            runCatching { backend.printEmergencySheet(code, text) }.onFailure { message(describe(it)) }
        }
    }

    /** Wide window: side navigation, list on the left and the opened screen on the right. */
    var wide by mutableStateOf(false)

    fun push(screen: Screen) {
        // In two panes, opening another entry, mail or settings page from the list replaces the
        // right side instead of piling up screens behind it.
        if (wide && (screen is Screen.EntryDetail || screen is Screen.MailRead || screen is Screen.SettingsPage || screen == Screen.Security ||
                screen == Screen.Shares || screen is Screen.CalEvent || screen is Screen.AddTotp ||
                (screen is Screen.CalEdit && stack.lastOrNull() !is Screen.CalEvent))) {
            stack.clear()
        }
        stack.add(screen)
    }

    /** Switches the area; in two panes the right side is closed at the same time. */
    fun selectTab(t: Tab) {
        if (wide) stack.clear()
        tab = t
        tabs.indexOf(t).takeIf { it >= 0 }?.let { backend.settings.lastTab = it }
    }

    fun pop() {
        if (stack.isNotEmpty()) stack.removeAt(stack.lastIndex)
    }

    fun changed() {
        revision++
        refreshVaults()
        autoSync()
        refreshCalendar()
    }


    /** Synchronises in the background if a sync server is set and automatic sync is on. */
    fun autoSync() {
        if (backend.settings.autoSync) sync(quiet = true)
    }

    /** Synchronises now; errors are shown unless [quiet] (then only real problems). */
    fun sync(quiet: Boolean = false, onDone: (de.danerd.ghulsuite.model.SyncReportUi) -> Unit = {}) {
        if (syncing) {
            syncAgain = true
            return
        }
        scope.launch {
            syncing = true
            try {
                do {
                    syncAgain = false
                    val status = runCatching { backend.net.syncStatus() }.getOrNull()
                    if (status?.serverId == null) {
                        if (!quiet) message(getString(Res.string.sync_not_set))
                        return@launch
                    }
                    val r = backend.net.syncNow()
                    syncOffline = false
                    if (r.downloaded > 0 || r.merged > 0 || r.deleted > 0) {
                        revision++
                        refreshVaults()
                        refreshCalendar()
                    }
                    onDone(r)
                } while (syncAgain)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val kind = (e as? VaultException)?.kind
                when {
                    kind == VaultErrorKind.PASSWORD_CHANGED_ELSEWHERE -> passwordChangedElsewhere = true
                    kind == VaultErrorKind.PROTECTION_CHANGED_ELSEWHERE -> protectionChangedElsewhere = true
                    kind == VaultErrorKind.LOCKED -> {}
                    // Offline is normal for automatic runs: no message, a quiet note on the area pages.
                    quiet && kind == VaultErrorKind.NETWORK -> syncOffline = true
                    else -> message(getString(Res.string.sync_failed, describe(e)))
                }
            } finally {
                syncing = false
            }
        }
    }

    fun refreshVaults() {
        scope.launch {
            vaults = runCatching { backend.listVaults() }.getOrDefault(vaults)
            if (backend.unlocked.value) {
                areasKnown = true
                if (openFirstArea) {
                    openFirstArea = false
                    stack.clear()
                    tab = tabs.first()
                }
                codeUnconfirmed = runCatching { !backend.recoveryConfirmed() }.getOrDefault(false)
                loadAreaOptions()
            }
            val old = vaults.filter { it.outdated }
            if (old.isNotEmpty() && !outdatedNotified) {
                outdatedNotified = true
                message(getString(Res.string.msg_outdated, old.joinToString(", ") { it.name }))
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
            val stored = runCatching { backend.areaOptions(kind) }.getOrNull() ?: continue
            st.applyArea(stored)
            val missing = st.deviceValues(kind).filterKeys { it !in stored }
            if (missing.isNotEmpty()) runCatching { backend.setAreaOptions(kind, missing) }
        }
    }

    /** Looks for vault files in the folder that are not part of the suite. */
    fun scanFound(onDone: (List<FoundVaultUi>) -> Unit = {}) {
        scope.launch {
            found = runCatching { backend.findNewVaults() }.getOrDefault(emptyList())
            onDone(found)
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
        val p = pending ?: return
        pending = null
        hidden = hidden - p.keys
        snackbar.currentSnackbarData?.takeIf { it.visuals.message == p.message }?.dismiss()
    }

    /** Locks the suite after carrying out what waits for "Undo". Every lock button uses this. */
    fun lockNow() {
        scope.launch {
            runCatching { settlePending() }
            backend.lock()
        }
    }

    /** Weekly backup into the folder chosen in Options (if set). */
    fun runAutoBackup() {
        val st = backend.settings
        val folder = st.autoBackupFolder
        if (folder.isEmpty() || nowSeconds() - st.lastAutoBackup < 7 * 86_400) return
        scope.launch {
            runCatching {
                val data = backend.suiteBackup()
                backend.writeToFolder(FolderRef(folder.substringBefore('|'), folder.substringAfter('|')), backupName(), data, keep = 4)
            }.onSuccess { st.lastAutoBackup = nowSeconds().toInt() }
                .onFailure { message(getString(Res.string.backup_auto_failed, describe(it))) }
        }
    }

    private var dueJob: kotlinx.coroutines.Job? = null

    /** Sends scheduled mail that is due, now and every minute while unlocked. */
    fun sendDueMail() {
        dueJob?.cancel()
        dueJob = scope.launch {
            while (backend.unlocked.value) {
                if (backend.settings.moduleGhulMail) {
                    runCatching { backend.net.mailSendDue(nowSeconds() * 1000) }.onSuccess { if (it > 0) revision++ }
                }
                kotlinx.coroutines.delay(60_000)
            }
        }
    }

    private var photoBackupRunning = false

    /** Backs up new photos into GhulDrive (Android, if switched on); runs after unlocking. */
    fun runPhotoBackup() {
        val st = backend.settings
        if (!st.photoBackup || photoBackupRunning || !backend.net.photoBackupAvailable) return
        photoBackupRunning = true
        scope.launch {
            var count = 0
            try {
                if (backend.net.driveStatus().serverId == null) return@launch
                val newest = backend.net.photoBackup(st.photoBackupSince.toLong(), st.photoBackupWifiOnly) { count++ }
                st.photoBackupSince = newest.toInt()
                if (count > 0) {
                    revision++
                    message(getString(Res.string.drive_photo_backup_done, count.toString()))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if ((e as? VaultException)?.kind != VaultErrorKind.NETWORK) message(describe(e))
            } finally {
                photoBackupRunning = false
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
        if (busy) return
        scope.launch {
            busy = true
            touchNeeded = false
            try {
                onSuccess(block())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                onError(describe(e))
            } finally {
                busy = false
                touchNeeded = false
            }
        }
    }
}

suspend fun describe(e: Throwable): String {
    if (e !is VaultException) return getString(Res.string.err_other, e.message ?: e.toString())
    return when (e.kind) {
        VaultErrorKind.UPDATE_REQUIRED -> getString(Res.string.err_update_required)
        VaultErrorKind.NO_KEY -> getString(Res.string.err_no_key)
        VaultErrorKind.PERMISSION_DENIED -> getString(Res.string.err_permission)
        VaultErrorKind.NOT_A_FIDO_KEY -> getString(Res.string.err_not_fido)
        VaultErrorKind.TRANSPORT -> getString(Res.string.err_transport, coreText(e.detail) ?: e.detail)
        VaultErrorKind.TIMEOUT -> getString(Res.string.err_timeout)
        VaultErrorKind.PIN_NOT_SET -> getString(Res.string.err_pin_not_set)
        VaultErrorKind.PIN_INVALID -> getString(Res.string.err_pin_invalid, (e.retries ?: 0).toString())
        VaultErrorKind.PIN_BLOCKED -> getString(Res.string.err_pin_blocked)
        VaultErrorKind.PIN_AUTH_BLOCKED -> getString(Res.string.err_pin_auth_blocked)
        VaultErrorKind.PIN_POLICY -> getString(Res.string.err_pin_policy)
        VaultErrorKind.QUICK_UNAVAILABLE -> getString(Res.string.err_quick_unavailable)
        VaultErrorKind.QUICK_EXPIRED -> getString(Res.string.err_quick_expired)
        VaultErrorKind.QUICK_INVALID -> getString(Res.string.err_quick_invalid, (e.retries ?: 0).toString())
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
        VaultErrorKind.PASSWORD_CHANGED_ELSEWHERE -> getString(Res.string.err_password_elsewhere)
        VaultErrorKind.PROTECTION_CHANGED_ELSEWHERE -> getString(Res.string.err_protection_elsewhere)
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
        password.length < MIN_LENGTH -> getString(Res.string.setup_pw_too_short, MIN_LENGTH.toString())
        backend.passwordScore(password) < MIN_SCORE -> getString(Res.string.setup_pw_too_weak)
        password != repeat -> getString(Res.string.setup_pw_mismatch)
        else -> null
    }
}
