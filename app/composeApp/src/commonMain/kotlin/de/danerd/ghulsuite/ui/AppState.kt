// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import de.danerd.ghulsuite.model.VaultBackend
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

/** The areas in the order they are named everywhere (passwords, 2FA, documents, calendar), then Options. */
enum class Tab { PASS, AUTH, DOCS, CAL, SETTINGS }

/** The tab of a vault's area. */
fun tabOf(kind: de.danerd.ghulsuite.model.VaultKindUi): Tab = when (kind) {
    de.danerd.ghulsuite.model.VaultKindUi.PASS -> Tab.PASS
    de.danerd.ghulsuite.model.VaultKindUi.TOTP -> Tab.AUTH
    de.danerd.ghulsuite.model.VaultKindUi.DOCS -> Tab.DOCS
    de.danerd.ghulsuite.model.VaultKindUi.CAL -> Tab.CAL
}

/** A vault opened on its own: its kind and the file it is written back into. */
data class SingleUi(val kind: de.danerd.ghulsuite.model.VaultKindUi, val file: de.danerd.ghulsuite.model.SuiteFileRef)

/** How long "Undo" is offered after a deletion or move. */
const val UNDO_MILLIS = 6000L

/** Key of an item for [AppState.hidden]: area, place (vault, folder, view) and id. */
fun hiddenKey(area: String, place: String, id: String) = "$area|$place|$id"

/**
 * Requests from outside the app UI, e.g. a tapped reminder or a file opened with GhulSuite.
 * Applied as soon as the suite is unlocked.
 */
object AppLaunch {
    val openTab = kotlinx.coroutines.flow.MutableStateFlow<Tab?>(null)

    /** A tapped reminder: event id and day (`id|YYYY-MM-DD`). */
    val openEvent = kotlinx.coroutines.flow.MutableStateFlow<String?>(null)

    /** B22: counts changes of the device's time zone noticed while running (Windows): reminders are planned anew. */
    val timeZoneChanged = kotlinx.coroutines.flow.MutableStateFlow(0)

    /** Counts how often the app came to the front (Android: activity resumed). */
    val resumed = kotlinx.coroutines.flow.MutableStateFlow(0)

    /** Whether the app is in front (Android: between onResume and onPause; elsewhere always). */
    val inFront = kotlinx.coroutines.flow.MutableStateFlow(true)

    /** An appointment handed over by another app ("add to calendar"). */
    val newEvent = kotlinx.coroutines.flow.MutableStateFlow<de.danerd.ghulsuite.model.CalEventUi?>(null)

    /** A suite file (.ghlzip) opened with GhulSuite from another app (file manager, double click). */
    val suiteFile = kotlinx.coroutines.flow.MutableStateFlow<PickedSuiteFile?>(null)

    /** r10 N1: a file handed over by a second start that is not opened (a network path): said once. */
    val refusedPath = kotlinx.coroutines.flow.MutableStateFlow<String?>(null)

    /** A calendar file (.ics) opened with GhulSuite: imported into the default calendar. */
    val icsFile = kotlinx.coroutines.flow.MutableStateFlow<ByteArray?>(null)

    /**
     * Files shared with GhulSuite from other apps ("Save in GhulDocs", Android): GhulDocs asks
     * where they go once the suite is unlocked. Only read when they are added (encrypted then).
     */
    val docsFiles = kotlinx.coroutines.flow.MutableStateFlow<List<de.danerd.ghulsuite.model.ByteSource>?>(null)
}

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
    data class SettingsPage(val page: SetPage) : Screen
    data object Security : Screen
    data class CalEvent(val id: String, val day: String) : Screen
    data class CalEdit(val draft: de.danerd.ghulsuite.model.CalEventUi) : Screen
    /** A document of GhulDocs, shown in the app (from memory, nothing on the disk). */
    data class DocView(val vaultId: String, val id: String) : Screen
    /** Scanning a document into a GhulDocs folder ([folder] null: the top of the vault). */
    data class DocScan(val vaultId: String, val folder: String?) : Screen
}

/**
 * UI state and navigation of the whole app. Runs the background work after unlocking (update
 * check, reminders) and owns the hooks of [JobStatus].
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

    /** Long work in the background; declared before init. */
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

    /** [str] and [plural] for code outside of the app state (reminders). */
    val texts: Texts = object : Texts {
        // Without arguments the text stays as it is (a template like "%1$s, %2$s" for the calendar).
        override suspend fun s(res: org.jetbrains.compose.resources.StringResource, vararg args: Any) =
            if (args.isEmpty()) env()?.let { getString(it, res) } ?: getString(res) else str(res, *args)
        override suspend fun p(res: org.jetbrains.compose.resources.PluralStringResource, count: Int, vararg args: Any) = plural(res, count, *args)
    }

    /** A counted text ("1 file", "3 files") in the language of the screens. */
    suspend fun plural(res: org.jetbrains.compose.resources.PluralStringResource, count: Int, vararg args: Any): String =
        env()?.let { getPluralString(it, res, count, *args) } ?: getPluralString(res, count, *args)

    /** Messages of jobs that locking stopped: shown after the next unlock (not on the lock screen). */
    private val afterUnlock = mutableListOf<String>()

    fun noteAfterUnlock(text: String) {
        afterUnlock += text
    }

    /** The vault list was read since unlocking (until then all areas switched on count as there). */
    private var areasKnown by mutableStateOf(false)

    /** After the setup wizard: show the first area once the vault list is read. */
    var openFirstArea = false

    /** The recovery code was never confirmed: a banner asks for a new one (#24). */
    var codeUnconfirmed by mutableStateOf(false)
    /** After a change of the protection: asks to save the suite file (older copies are outdated). */
    var saveAfterProtection by mutableStateOf(false)

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

    /** How far the running action is (0 to 1), null when it does not say (shown with the busy indicator). */
    var progress by mutableStateOf<Float?>(null)
        private set

    /** For actions that report bytes done and in all (saving or opening a suite file); total 0: not known. */
    val onProgress: (Long, Long) -> Unit = { done, total ->
        progress = if (total > 0) (done.toFloat() / total).coerceIn(0f, 1f) else null
    }

    /** Incremented whenever vault content changes, so lists reload. */
    var revision by mutableStateOf(0)
        private set

    val onTouchNeeded: () -> Unit = { touchNeeded = true }

    /**
     * A deletion waiting for "Delete?" (or "Move to the trash?") to be answered (F103: every
     * deletion asks first).
     */
    class DeleteAsk(val names: List<String>, val action: () -> Unit, val trash: Boolean = false)


    // Declared before init (the lock listener there clears it).
    var deleteAsk by mutableStateOf<DeleteAsk?>(null)

    /** Header action of GhulAuth: list or grid view. */
    var authGrid by mutableStateOf(false)

    // --- GhulDocs
    /** The GhulDocs vault shown (null: all, or the only one). */
    var docsVault by mutableStateOf<String?>(null)
    /** Grows with every tap on "+" in GhulDocs: its screen opens the add sheet. */
    var docsAddTick by mutableStateOf(0)
    /** GhulDocs shows its trash: nothing can be added there, so there is no "+". */
    var docsTrashShown by mutableStateOf(false)
    /** Files shared from other apps, waiting for the user to choose a vault and folder. */
    var docsIncoming by mutableStateOf<List<de.danerd.ghulsuite.model.ByteSource>?>(null)
    /** A GhulDocs vault saved on its own, waiting to be added to the own suite once it is unlocked. */
    var pendingDocsAdd by mutableStateOf<de.danerd.ghulsuite.model.SuiteFileRef?>(null)

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
            val r = de.danerd.ghulsuite.model.CalReminderUi("", title, str(Res.string.cal_test_place), false, start,
                fromAbsMinutes(absMinutes(start) + 45), now, 15)
            backend.cal.showReminder(calAlarm(r, st.calNotifyDetail, loadCalNames(texts), texts), st.calSound)
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
            val lang = str(Res.string.cal_lang)
            calRegionName = catching { backend.cal.regions(lang) }.getOrDefault(emptyList()).firstOrNull { it.code == st.calRegion }?.name ?: ""
            // A calendar opened on its own does not replace the reminders of the own suite.
            if (alone) return@launch
            // M9: reading failed (a damaged calendar, the core busy): the reminders already set
            // stay as they are, instead of being replaced by an empty list.
            val rem = catching { backend.cal.reminders(localNow(), 30) }.getOrElse { return@launch }
            val names = loadCalNames(texts)
            // The neutral "open GhulSuite" one first, so it is never cut off by the limit.
            val openApp = openAppAlarm(today(), calendars.isNotEmpty(), texts)
            backend.cal.scheduleReminders(listOfNotNull(openApp) + rem.take(399).map { calAlarm(it, st.calNotifyDetail, names, texts) }, st.calSound)
        }
    }

    /** The context sheet of the current area is open (vault filter, calendars). */
    var contextSheet by mutableStateOf(false)

    /** B6: a calendar file another app handed over, waiting for the user to confirm the import. */
    var icsPending by mutableStateOf<ByteArray?>(null)

    /**
     * U12: an editor (login, event) holds input that is not saved. "Back" then asks first. Locking
     * cannot wait (security): it drops the input, and after unlocking the app says so.
     */
    var editorDirty by mutableStateOf(false)

    /**
     * Device test: when the master password was last confirmed for weakening a setting (in memory
     * only; a lock and a restart forget it). Within [WEAKENING_VALID_MS] the next weakening step
     * does not ask again. [gateClock]: milliseconds of a monotonic clock (tests set it).
     */
    var weakeningConfirmedAt: Long? = null
    var gateClock: () -> Long = { monotonicMillis() }

    /** The lock generation of the backend at the confirmation: any lock since ends the window at once. */
    var weakeningConfirmedLocks = -1L

    fun noteWeakeningConfirmed() {
        weakeningConfirmedAt = gateClock()
        weakeningConfirmedLocks = backend.lockCount
    }

    fun weakeningStillConfirmed(): Boolean =
        weakeningConfirmedLocks == backend.lockCount && backend.unlocked.value &&
            weakeningConfirmedAt?.let { gateClock() - it in 0 until WEAKENING_VALID_MS } == true

    /** r11 M2: the idle lock comes within a minute and takes unsaved input with it (said once). */
    var idleWarning by mutableStateOf(false)

    /** A scan in progress (photos and pages in memory only); wiped when it ends or the suite locks. */
    var scanSession: ScanSession? = null

    /** U11: opened with the recovery code: the next steps are shown once (see [RecoveredDialog]). */
    var recoveredGuide by mutableStateOf(false)

    /**
     * The suite moved to another place (written there and checked): the file at the old place,
     * with the hint and "Delete old file" (null: nothing to say). New place: [movedTo].
     */
    var oldSuiteFile by mutableStateOf<de.danerd.ghulsuite.model.SuiteFileRef?>(null)
    var movedTo by mutableStateOf<de.danerd.ghulsuite.model.SuiteFileRef?>(null)
    /** Whether the old file may be deleted (checked: only a state of this suite's own line). */
    var oldDeletable by mutableStateOf(false)
    /** A conflict was shown in this save dialog (then no "Delete old file", review H2). */
    var saveConflictSeen = false

    /** A safe place chosen in the save dialog, not saved there yet: its new folder grant, to give back. */
    var pendingPlace: de.danerd.ghulsuite.model.SuiteFileRef? = null
    var pendingPlaceRelease: (() -> Unit)? = null

    fun releasePendingPlace() {
        pendingPlaceRelease?.invoke()
        pendingPlaceRelease = null
        pendingPlace = null
    }

    /** r9 N-D2: a suite file from another app, opened: "save here from now on?" (null: nothing asked). */
    var saveTargetOffer by mutableStateOf<de.danerd.ghulsuite.model.SuiteFileRef?>(null)

    /** H4: an archive waits for "open anyway" before it goes to another app. */
    var archiveAsk by mutableStateOf<de.danerd.ghulsuite.model.DocsItemUi?>(null)

    /** Opened with the recovery code (until the lock): a banner says what to do before locking (H3). */
    var recoveredOpen by mutableStateOf(false)

    /** A newer version was found: the update dialog shows it. */
    var update by mutableStateOf<UpdateUi?>(null)
    /** Looking for or loading an update. */
    var updateBusy by mutableStateOf(false)
    /** M7: an update is loaded for installing (the installer ends the app): saving waits. */
    var updateInstalling by mutableStateOf(false)
        private set

    /** Nothing would be lost if the app ended now: no save, no other job, no unsaved input. */
    private fun mayEndForUpdate(): Boolean = saveJob == null && !jobs.running && !editorDirty

    /**
     * M2: the suite file at the remembered place was saved elsewhere since this device's state
     * (another device, another stick): who saved it and when. Shown as a question before the user
     * edits; null when there is nothing to say.
     */
    var newerSuiteFile by mutableStateOf<de.danerd.ghulsuite.model.ArchiveTargetInfoUi?>(null)

    /** Files whose newer state was answered with "Later" (until the app starts anew), by their head. */
    private val newerLater = mutableSetOf<String>()

    private fun newerKey(ref: de.danerd.ghulsuite.model.SuiteFileRef, info: de.danerd.ghulsuite.model.ArchiveTargetInfoUi) =
        "${ref.id}|${info.target}|${info.device}|${info.time}"

    /** "Later": not asked again for this state of the file until the app starts anew. */
    fun newerLater() {
        val info = newerSuiteFile ?: return
        backend.suiteFile()?.let { newerLater += newerKey(it, info) }
        newerSuiteFile = null
    }

    /** M2: reads the header of the remembered suite file in the background after unlocking. */
    fun checkNewerSuiteFile() {
        val ref = backend.suiteFile() ?: return
        scope.launch {
            val info = catching { backend.archiveTargetInfo(ref) }.getOrNull() ?: return@launch
            // Locked meanwhile, or another file opened: the answer no longer fits.
            // CHANGED_ELSEWHERE: saved from another state; OTHER_PROTECTION: saved with a protection
            // this device does not know yet (the other device changed it), surely newer.
            val elsewhere = info.target == de.danerd.ghulsuite.model.ArchiveTargetUi.CHANGED_ELSEWHERE ||
                info.target == de.danerd.ghulsuite.model.ArchiveTargetUi.OTHER_PROTECTION
            if (elsewhere && newerKey(ref, info) !in newerLater && backend.unlocked.value &&
                backend.suiteFile() == ref && single == null
            ) {
                newerSuiteFile = info
            }
        }
    }

    /** "Open it now": the page of the suite file opens it, with password and second factor (M2). */
    fun openNewerSuiteFile() {
        newerSuiteFile = null
        val ref = backend.suiteFile() ?: return
        AppLaunch.suiteFile.value = PickedSuiteFile(ref)
    }

    /** M24: an update of an earlier run that failed to install says so once. */
    fun reportFailedUpdate() {
        val code = catching { backend.updater.takeFailedInstall() }.getOrNull() ?: return
        scope.launch {
            catching {
                message(
                    when (code) {
                        // The user cancelled the installer.
                        "1602" -> getString(Res.string.upd_install_cancelled)
                        "hash" -> getString(Res.string.upd_install_hash)
                        "error" -> getString(Res.string.upd_install_check_failed)
                        // The script never ran (see DesktopUpdater): stopped before it started.
                        "pending" -> getString(Res.string.upd_install_blocked)
                        else -> getString(Res.string.upd_install_failed, code)
                    },
                )
            }
        }
    }

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
        // Installing ends the app: not while the suite file is saved, files are added or an
        // entry is edited (that would be lost). The working copy itself is safe.
        if (!mayEndForUpdate()) {
            scope.launch { catching { message(getString(Res.string.upd_wait)) } }
            return
        }
        update = null
        updateBusy = true
        updateInstalling = true
        // The note must not hold up the installation.
        scope.launch { catching { message(getString(Res.string.upd_loading)) } }
        scope.launch {
            try {
                // Asked again right before the app ends (M7): the download took a while.
                when (backend.updater.install(u) { mayEndForUpdate() }) {
                    UpdateStep.ALLOW_FIRST -> {
                        update = u
                        message(getString(Res.string.upd_allow))
                    }
                    UpdateStep.WAIT -> {
                        update = u
                        message(getString(Res.string.upd_wait))
                    }
                    UpdateStep.STARTED -> {}
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                message(describe(e))
            } finally {
                updateBusy = false
                updateInstalling = false
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
        // Something from outside waited for this: it replaces the editor.
        if (launchWaiting) {
            pendingTab = null
            applyLaunch()
            return
        }
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

    /** The working copy has changes that are not in the suite file yet (see [readUnsaved]). */
    var unsaved by mutableStateOf(false)
    /** Which areas have them (see VaultBackend.archiveUnsavedAreas; empty: not known or none). */
    var unsavedAreas by mutableStateOf<List<String>>(emptyList())
    /** When the first of them was made (ms since 1970; 0: none). */
    var unsavedSince by mutableStateOf(0L)

    /**
     * Something is not in the suite file: the save button shows in the header (beside "Lock")
     * wherever the user is, and a dot on Options. Not for a vault opened on its own (that is
     * written back by itself).
     */
    val saveNeeded: Boolean
        get() = unsaved && single == null

    /** The time (ms since 1970); tests may replace it. */
    var clockMs: () -> Long by mutableStateOf({ nowSeconds() * 1000 })

    /**
     * M32: a password or 2FA account was added or changed since the last save: the reminder above
     * the lists shows at once (not only after days). Gone with the next save.
     */
    var savePrompt by mutableStateOf(false)

    /** After a new or changed password or 2FA account (see [savePrompt]). */
    fun promptSave() {
        if (single == null) savePrompt = true
    }

    /** Unsaved for [REMIND_UNSAVED_MS] or longer: a reminder above the lists. */
    val saveOverdue: Boolean
        get() = saveNeeded && unsavedSince > 0 && clockMs() - unsavedSince >= REMIND_UNSAVED_MS

    /** The dialog "Write the changes into the suite file now?" is open (see [SaveSuiteDialog]). */
    var saveDialog by mutableStateOf<SaveRequest?>(null)
    /** Where saving stands in that dialog. */
    var saveStep by mutableStateOf<SaveStep>(SaveStep.Ask)

    /** Reads anew what is unsaved and since when (also while locked). */
    fun readUnsaved() {
        unsaved = backend.archiveUnsaved()
        unsavedAreas = if (unsaved) backend.archiveUnsavedAreas() else emptyList()
        unsavedSince = if (unsaved) backend.archiveUnsavedSince() else 0L
        if (!unsaved) savePrompt = false
    }

    /** Windows: the window is to close while changes are not in the suite file (see [QuitDialog]). */
    var quitAsk by mutableStateOf(false)
    /** Quit once the save that runs (or is started now) is done. */
    var quitAfterSave = false
    /** Ends the app (Windows: locks and closes the window); null where the app is not quit this way. */
    var onQuit: (() -> Unit)? = null

    /**
     * The window is to close: true if it may at once; else [QuitDialog] asks first (changes not
     * in the suite file, or a save that still runs and would leave a temporary file behind).
     */
    fun requestQuit(): Boolean {
        // An entry being edited is not applied yet: asked about as well.
        if (saveJob == null && !(unsaved && single == null) && !editorDirty) return true
        quitAsk = true
        return false
    }

    /** Opens the save dialog ([then]: runs after it was saved, e.g. opening another file). */
    fun askSave(then: (() -> Unit)? = null) {
        // Open already (e.g. Ctrl+S while a file is checked or a conflict is shown): it stays as it is.
        if (saveDialog != null) return
        if (saveJob == null) saveStep = SaveStep.Ask
        saveDialog = SaveRequest(then)
    }

    /**
     * Saving makes sense here: unlocked, the own suite, and not on a page that edits something
     * (what is not applied yet would not be in the file). The save button, the side bar and Ctrl+S.
     */
    val canSaveHere: Boolean
        get() {
            val top = stack.lastOrNull()
            return single == null && top !is Screen.EntryEdit && top !is Screen.CalEdit && top !is Screen.DocScan
        }

    init {
        PasswordPolicy.apply(backend.securityRules())
        scope.launch {
            backend.unlocked.collect { unlocked ->
                if (!unlocked) {
                    // Locked from anywhere (panic, notification, screen off): running jobs stop now.
                    jobs.cancelAll(byLock = true)
                    jobs.lockWhenDone = false
                    if (editorDirty) discardedOnLock = true
                    editorDirty = false
                    newerSuiteFile = null
                    // What is unsaved is read anew (the suite may be gone); the save dialog closes
                    // (a save that runs goes on, its job says how it ended).
                    saveDialog = null
                    if (saveJob == null) {
                        saveStep = SaveStep.Ask
                        // Quitting waited for a save that was not started: it does not any more.
                        quitAfterSave = false
                    }
                    readUnsaved()
                    recoveredGuide = false
                    recoveredOpen = false
                    // A master password confirmed for weakening a setting does not outlive a lock.
                    weakeningConfirmedAt = null
                    // Sheets of the screens before do not come back after unlocking (maybe in another suite).
                    calQuickOpen = false
                    contextSheet = false
                    icsPending = null
                    launchWaiting = false
                    confirmDiscard = false
                    pendingTab = null
                    deleteAsk = null
                    // r10 N2: a question about another file's place does not wait across a lock.
                    saveTargetOffer = null
                    oldSuiteFile = null
                    dropPending()
                    // A scan in progress: its photos and pages are overwritten now, not saved.
                    scanSession?.wipe()
                    scanSession = null
                    stack.clear()
                    tab = tabs.first()
                    vaults = emptyList()
                    passVault = null
                    authVault = null
                    docsVault = null
                    docsTrashShown = false
                    // Files shared from another app stay waiting (they were never decrypted); the
                    // question where they go comes again after unlocking.
                    docsIncoming?.let { list -> if (AppLaunch.docsFiles.value == null) AppLaunch.docsFiles.value = list }
                    docsIncoming = null
                    areasKnown = false
                    codeUnconfirmed = false
                    saveAfterProtection = false
                    if (single != null) {
                        // The vault opened on its own is closed: back to the own suite (locked) or the start.
                        single = null
                        // The own suite again: its unsaved changes count.
                        readUnsaved()
                        vaultExists = backend.exists()
                    }
                } else {
                    if (discardedOnLock) {
                        discardedOnLock = false
                        message(getString(Res.string.edit_dropped_on_lock))
                    }
                    afterUnlock.toList().also { afterUnlock.clear() }.forEach { message(it) }
                    // Changes from before (the app ended before saving, or saving failed): the save button shows.
                    readUnsaved()
                    refreshVaults()
                    if (single != null) {
                        refreshCalendar()
                        return@collect
                    }
                    val st = backend.settings
                    val start = if (st.startTab < 0) tabs.firstOrNull { it.name == st.lastTabName } else tabs.getOrNull(st.startTab)
                    start?.let { tab = it }
                    applyLaunch()
                    recoveredOpen = catching { backend.openedWithRecoveryCode() }.getOrDefault(false)
                    refreshCalendar()
                    checkUpdate(manual = false)
                    reportFailedUpdate()
                    checkNewerSuiteFile()
                }
            }
        }
        // The notification's "Cancel" (Android) stops the running jobs; the automatic lock of
        // Android asks the jobs first.
        JobStatus.cancelAll = { jobs.cancelAll() }
        JobStatus.autoLock = { autoLock() }
        JobStatus.autoLockFirm = { autoLock(firm = true) }
        JobStatus.userJobProgressing = { jobs.progressing() }
        JobStatus.keepUnlocked = { keepUnlocked() }
        // Going to the background carries out what waits for "Undo": the auto lock may come next.
        scope.launch {
            AppLaunch.inFront.collect {
                if (!it) {
                    // Review M1: away (the screen off, another app): the password window ends.
                    weakeningConfirmedAt = null
                    catching { settlePending() }
                }
            }
        }
        // Area settings go into the area's file (they travel with the suite file).
        backend.settings.onAreaChange = { kind, key, value ->
            if (backend.unlocked.value && single == null && vaults.any { it.kind == kind }) {
                scope.launch {
                    catching { backend.setAreaOptions(kind, mapOf(key to value)) }
                    // Written into the area's file: a change for the suite file.
                    readUnsaved()
                }
            }
        }
        scope.launch { AppLaunch.openTab.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        // "Keep editing" and the editor ended (applied or left): what came from outside follows now.
        scope.launch {
            androidx.compose.runtime.snapshotFlow { editorDirty }.collect { dirty ->
                if (!dirty && launchWaiting && backend.unlocked.value) applyLaunch()
            }
        }
        scope.launch { AppLaunch.timeZoneChanged.collect { if (it > 0 && backend.unlocked.value) refreshCalendar() } }
        scope.launch { AppLaunch.openEvent.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.newEvent.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch { AppLaunch.icsFile.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        scope.launch {
            AppLaunch.refusedPath.collect { path ->
                if (path != null) {
                    AppLaunch.refusedPath.value = null
                    message(getString(Res.string.file_network_path, path.substringAfterLast('\\').substringAfterLast('/')))
                }
            }
        }
        scope.launch { AppLaunch.docsFiles.collect { if (it != null && backend.unlocked.value) applyLaunch() } }
        // A suite file handed over: unlocked, the page of the suite file opens it; locked, the start
        // screen does (as "Open another suite or vault" would).
        scope.launch {
            AppLaunch.suiteFile.collect {
                if (it == null) return@collect
                if (backend.unlocked.value) applyLaunch() else if (vaultExists) showStart = true
            }
        }
        // A vault opened on its own goes back into its file soon after every change, whatever made it.
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(1500)
                // Changed elsewhere: the user decides (overwrite or save as), not this loop. Gone or
                // failed: tried again (written as soon as the storage is back and unchanged).
                // Not while files are added (a GhulDocs vault is written with its documents): once after.
                if (single != null && backend.unlocked.value && !busy && !jobs.running &&
                    singleSave != de.danerd.ghulsuite.model.SingleSaveUi.CHANGED_ELSEWHERE &&
                    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { backend.archiveUnsaved() }
                ) {
                    singleSave = catching { backend.saveSingle() }.getOrDefault(de.danerd.ghulsuite.model.SingleSaveUi.FAILED)
                    unsaved = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { backend.archiveUnsaved() }
                }
            }
        }
    }

    /** Something came from outside while an editor had unsaved input: it waits for "discard" (or the editor's end). */
    private var launchWaiting = false

    private fun launchPending(): Boolean = AppLaunch.openEvent.value != null || AppLaunch.newEvent.value != null ||
        AppLaunch.icsFile.value != null || AppLaunch.openTab.value != null ||
        (AppLaunch.suiteFile.value != null && single == null) || (AppLaunch.docsFiles.value != null && single == null)

    private fun applyLaunch() {
        // An editor with unsaved input is not closed unasked (a reminder tapped, a file handed over).
        if (editorDirty && launchPending()) {
            launchWaiting = true
            confirmDiscard = true
            return
        }
        launchWaiting = false
        AppLaunch.openEvent.value?.let { ev ->
            AppLaunch.openEvent.value = null
            // The neutral "open GhulSuite" reminder has no event: it only opens the app.
            if (ev != OPEN_APP_ALARM_KEY && Tab.CAL in tabs) {
                stack.clear()
                tab = Tab.CAL
                stack.add(Screen.CalEvent(ev.substringBefore('|'), ev.substringAfter('|', "")))
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
        if (AppLaunch.suiteFile.value != null && single == null && stack.lastOrNull() != Screen.SettingsPage(SetPage.FILE)) {
            push(Screen.SettingsPage(SetPage.FILE))
        }
        AppLaunch.icsFile.value?.let { data ->
            AppLaunch.icsFile.value = null
            if (Tab.CAL in tabs) {
                stack.clear()
                tab = Tab.CAL
                icsPending = data
            }
        }
        AppLaunch.docsFiles.value?.let { files ->
            // A vault opened on its own is no place for them: they wait for the own suite.
            if (single == null) {
                AppLaunch.docsFiles.value = null
                if (Tab.DOCS in tabs) {
                    stack.clear()
                    tab = Tab.DOCS
                    docsIncoming = (docsIncoming.orEmpty() + files)
                } else {
                    files.forEach { runCatching { it.close() } }
                    scope.launch { message(getString(Res.string.docs_incoming_no_area)) }
                }
            }
        }
        val t = AppLaunch.openTab.value ?: return
        AppLaunch.openTab.value = null
        if (t !in tabs) return
        stack.clear()
        tab = t
    }

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
            if (backend.settings.moduleGhulDocs && hasArea(VaultKindUi.DOCS)) add(Tab.DOCS)
            if (backend.settings.moduleGhulCal && hasArea(VaultKindUi.CAL)) add(Tab.CAL)
            add(Tab.SETTINGS)
        }

    /** Opens the print dialog for the emergency sheet of [code]. */
    fun printSheet(code: String) {
        scope.launch {
            // M31: enough for someone else (an heir): where to get the app, what else is needed.
            // In the wizard there is no file yet (step 8 comes after the sheet): the suggested name, said as such.
            val file = backend.suiteFile()?.name ?: getString(Res.string.sheet_default_name, SUITE_FILE_NAME)
            val text = de.danerd.ghulsuite.model.EmergencySheetText(
                title = getString(Res.string.sheet_title),
                intro = getString(Res.string.sheet_intro, file),
                codeLabel = getString(Res.string.sheet_code_label),
                created = getString(Res.string.sheet_created, formatDate(nowSeconds() * 1000)),
                steps = listOf(
                    getString(Res.string.sheet_need, file),
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
        // By its name (the list of tabs changes); not for a vault opened on its own.
        if (single == null) backend.settings.lastTabName = t.name
    }

    fun pop() {
        if (stack.isNotEmpty()) stack.removeAt(stack.lastIndex)
    }

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
        readUnsaved()
        revision++
        refreshVaults()
        refreshCalendar()
    }

    /**
     * After a change of the protection (password, recovery code, a key removed): the suite file
     * is to be saved again (older copies still open with the old protection).
     */
    fun protectionChanged() {
        changed()
        saveAfterProtection = true
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
            if (docsVault != null && vaults.none { it.id == docsVault }) docsVault = null
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
        readUnsaved()
    }

    fun vaultsOf(kind: VaultKindUi): List<VaultUi> = vaults.filter { it.kind == kind }

    /** Switches on the module of a vault the user just added. */
    fun enableModule(kind: VaultKindUi) {
        when (kind) {
            VaultKindUi.PASS -> backend.settings.moduleGhulPass = true
            VaultKindUi.TOTP -> backend.settings.moduleGhulAuth = true
            VaultKindUi.DOCS -> backend.settings.moduleGhulDocs = true
            VaultKindUi.CAL -> backend.settings.moduleGhulCal = true
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
     * Asks before deleting [names]; [action] runs only after "Delete". Every area deletes through
     * this; "Undo" afterwards stays as it was.
     */
    fun askDelete(names: List<String>, trash: Boolean = false, action: () -> Unit) {
        if (names.isEmpty()) return
        deleteAsk = DeleteAsk(names, action, trash)
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

    /**
     * Locks the suite after carrying out what waits for "Undo". Every lock button uses this. A
     * running job stops at once (its message follows after the next unlock).
     */
    fun lockNow() {
        weakeningConfirmedAt = null
        jobs.lockWhenDone = false
        jobs.firmLock = false
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
    fun autoLock(firm: Boolean = false) = jobs.autoLock(firm)

    /**
     * The user is back before the jobs ended: the waiting automatic lock is dropped. One from the
     * screen going off ([Jobs.firmLock]) is not: it locks now (r9 dieb N3).
     */
    fun keepUnlocked() {
        if (jobs.lockWhenDone && jobs.firmLock) {
            lockNow()
            return
        }
        jobs.lockWhenDone = false
    }

    fun messageRes(res: org.jetbrains.compose.resources.StringResource) {
        scope.launch { snackbar.showSnackbar(getString(res)) }
    }

    fun copy(label: String, value: String) {
        try {
            backend.copySensitive(label, value)
        } catch (e: VaultException) {
            scope.launch { snackbar.showSnackbar(describe(e)) }
            return
        }
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
        progress = null
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            var failure: String? = null
            val result = try {
                block()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failure = describe(e)
                null
            } catch (e: OutOfMemoryError) {
                // M6 (r5-g3): a file too large for this device's memory ends the action, not the app.
                failure = catching { getString(Res.string.err_out_of_memory) }.getOrDefault("Out of memory")
                null
            } finally {
                busy = false
                touchNeeded = false
                progress = null
            }
            // Only once the action is done: a follow-up action started here (run inside onSuccess)
            // must not find the app busy,
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
        VaultErrorKind.OLD_FORMAT -> getString(Res.string.err_old_format)
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
        VaultErrorKind.QUICK_OUTDATED -> getString(Res.string.err_quick_outdated)
        VaultErrorKind.DELETE_INCOMPLETE -> getString(Res.string.err_delete_incomplete)
        // M19: after the second wrong PIN no attempt is left (the PIN is gone). Without a count
        // (the fingerprint, or the keystore could not say): a text without one (r8 N5).
        VaultErrorKind.QUICK_INVALID -> when (val left = e.retries) {
            null -> getString(Res.string.err_quick_invalid_plain)
            0 -> getString(Res.string.err_quick_pin_used_up)
            else -> getPluralString(Res.plurals.err_quick_invalid, left, left.toString())
        }
        VaultErrorKind.BIOMETRIC_CANCELLED -> getString(Res.string.err_bio_cancelled)
        VaultErrorKind.BIOMETRIC_LOCKOUT -> getString(Res.string.err_bio_lockout)
        VaultErrorKind.BIOMETRIC_CHANGED -> getString(Res.string.err_bio_changed)
        VaultErrorKind.HMAC_UNSUPPORTED -> getString(Res.string.err_hmac)
        VaultErrorKind.KEY_NOT_REGISTERED -> getString(Res.string.err_key_not_registered)
        VaultErrorKind.NOT_CONFIRMED -> getString(Res.string.err_not_confirmed)
        VaultErrorKind.KEY_PROTOCOL -> getString(Res.string.err_key_protocol, coreText(e.detail) ?: e.detail)
        VaultErrorKind.WRONG_PASSWORD -> getString(Res.string.err_wrong_password)
        VaultErrorKind.WRONG_SECOND_FACTOR -> getString(Res.string.err_wrong_code)
        VaultErrorKind.INVALID_RECOVERY_CODE -> getString(Res.string.err_invalid_code)
        VaultErrorKind.LOCKED -> getString(Res.string.err_locked)
        VaultErrorKind.DEVICE_LOCKED -> getString(Res.string.err_device_locked)
        VaultErrorKind.CLIPBOARD_BUSY -> getString(Res.string.err_clipboard_busy)
        VaultErrorKind.TRY_AGAIN -> getString(Res.string.err_try_again)
        VaultErrorKind.CORRUPTED -> getString(Res.string.err_corrupted, coreText(e.detail) ?: e.detail)
        VaultErrorKind.TAMPERED -> getString(Res.string.err_tampered)
        VaultErrorKind.PROTECTION_OUTDATED -> getString(Res.string.err_protection_outdated)
        VaultErrorKind.SINGLE_BUSY -> getString(Res.string.single_busy)
        VaultErrorKind.SINGLE_PENDING -> getString(Res.string.single_pending_first)
        VaultErrorKind.STORAGE -> coreText(e.detail) ?: getString(Res.string.err_storage, e.detail)
        // U5: the core's known details are complete sentences in the app's language.
        VaultErrorKind.INVALID_INPUT -> coreText(e.detail) ?: getString(Res.string.err_input, e.detail)
        VaultErrorKind.SECURITY_KEY_REQUIRED -> getString(Res.string.err_key_required)
        VaultErrorKind.KEYFILE_REQUIRED -> getString(Res.string.err_keyfile_required)
        VaultErrorKind.WRONG_KEYFILE -> getString(Res.string.err_wrong_keyfile)
        VaultErrorKind.DIFFERENT_PROTECTION -> getString(Res.string.err_different_protection)
        VaultErrorKind.ALREADY_EXISTS -> getString(Res.string.err_already_exists)
        VaultErrorKind.LAST_FACTOR -> getString(Res.string.err_last_factor)
        VaultErrorKind.BACKUP_FACTOR_REQUIRED -> getString(Res.string.err_backup_factor)
        VaultErrorKind.IMPORT_ENCRYPTED -> getString(Res.string.err_import_encrypted)
        VaultErrorKind.IMPORT_UNKNOWN -> getString(Res.string.err_import_unknown)
        VaultErrorKind.NETWORK -> getString(Res.string.err_network, coreText(e.detail) ?: e.detail)
        VaultErrorKind.CERT_UNTRUSTED -> getString(Res.string.err_cert_untrusted, e.detail)
        VaultErrorKind.NOT_CONFIGURED -> getString(Res.string.err_not_configured)
        VaultErrorKind.NOT_FOUND -> getString(Res.string.err_not_found)
        VaultErrorKind.DOCS_LIMIT -> docsLimitText(e.detail, DocsAdding.ANY) { res, args -> getString(res, *args) }
        // H12: the core checks the room before it writes (needed 0: the system said "full" while writing).
        VaultErrorKind.NO_SPACE -> {
            val (free, needed) = e.detail.split('|').map { it.toLongOrNull() ?: 0L } + listOf(0L, 0L)
            if (needed > 0) getString(Res.string.err_no_space, formatSize(needed), formatSize(free)) else getString(Res.string.err_no_space_unknown)
        }
        VaultErrorKind.OTHER -> getString(Res.string.err_other, coreText(e.detail) ?: e.detail)
    }
}

/** Rules for the master password (always combined with a security key or key file). */
object PasswordPolicy {
    // M42: the rules come from the core (security_rules), which checks them itself as well; set
    // when the app state is made (these defaults are the core's values).
    var MIN_LENGTH = 10
        private set
    var MIN_SCORE = 3
        private set
    var MIN_APP_PIN = 8
        private set

    fun apply(rules: de.danerd.ghulsuite.model.SecurityRulesUi) {
        MIN_LENGTH = rules.passwordMinLength
        MIN_SCORE = rules.passwordMinScore
        MIN_APP_PIN = rules.appPinMinLength
    }

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

/** What was to be added when GhulDocs had no room: it names it in the message. */
enum class DocsAdding { FILE, PACKAGE, ANY }

/**
 * The text for [VaultErrorKind.DOCS_LIMIT] (detail `used|limit|needed|suite`, bytes): how much it
 * needs and how much is free when that is the reason, else that the vault (or, with `suite`, all
 * GhulDocs vaults together) is full (too many documents, or a size that was not known before).
 * [get] reads a text in the language of the screens.
 */
suspend fun docsLimitText(
    detail: String,
    adding: DocsAdding,
    get: suspend (org.jetbrains.compose.resources.StringResource, Array<String>) -> String,
): String {
    val parts = detail.split('|')
    val used = parts.getOrNull(0)?.toLongOrNull() ?: 0
    val limit = parts.getOrNull(1)?.toLongOrNull() ?: 1_000_000_000
    val needed = parts.getOrNull(2)?.toLongOrNull() ?: 0
    val suite = parts.getOrNull(3) == "true"
    val free = (limit - used).coerceAtLeast(0)
    if (suite) {
        // All GhulDocs vaults together: another vault would not help, emptying or deleting would.
        // Room for the bytes but not for one more document, or a size not known before: "full".
        return if (needed > free) {
            get(Res.string.err_docs_suite_room, arrayOf(formatSizeSi(needed), formatSizeSi(free), formatSizeSi(limit)))
        } else {
            get(Res.string.err_docs_suite_full, arrayOf(formatSizeSi(used), formatSizeSi(limit)))
        }
    }
    return when {
        // More than a GhulDocs vault holds at all: emptying the trash would not help.
        needed > limit && adding == DocsAdding.PACKAGE ->
            get(Res.string.err_docs_package_too_large, arrayOf(formatSizeSi(needed), formatSizeSi(limit)))
        needed > limit -> get(Res.string.err_docs_too_large, arrayOf(formatSizeSi(limit)))
        needed > free -> get(
            when (adding) {
                DocsAdding.FILE -> Res.string.err_docs_room_file
                DocsAdding.PACKAGE, DocsAdding.ANY -> Res.string.err_docs_room
            },
            arrayOf(formatSizeSi(needed), formatSizeSi(free)),
        )
        used > 0 -> get(Res.string.err_docs_limit, arrayOf(formatSizeSi(used), formatSizeSi(limit)))
        else -> get(Res.string.err_docs_too_large, arrayOf(formatSizeSi(limit)))
    }
}

/** A reminder above the lists when changes were not saved into the suite file for this long (3 days). */
const val REMIND_UNSAVED_MS = 3 * 24 * 60 * 60_000L

/** The save dialog was asked for; [then] runs once it was saved (e.g. "Save first" before opening another file). */
class SaveRequest(val then: (() -> Unit)? = null)
