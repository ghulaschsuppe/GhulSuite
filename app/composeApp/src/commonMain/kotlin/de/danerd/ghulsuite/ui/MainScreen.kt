// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.material.icons.filled.Usb
import androidx.compose.ui.draw.alpha
import androidx.compose.animation.core.animateFloat
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.key
import androidx.compose.ui.layout.Layout
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.GridView
import androidx.compose.material.icons.automirrored.filled.ViewList
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material.icons.outlined.VerifiedUser
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.Warning
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getPluralString
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

/** Root of the app: setup, unlock or the main screens. */
@Composable
fun GhulSuiteApp(state: AppState) {
    GhulTheme(state.backend.settings.theme) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            val unlocked by state.backend.unlocked.collectAsState()
            BoxWithConstraints(Modifier.fillMaxSize()) {
                // Wide windows (Windows, phones and tablets lying sideways): areas in a sidebar on
                // the left, everything else in the rest of the width.
                val wide = maxWidth >= 840.dp
                val undoLabel = stringResource(Res.string.a_undo)
                val env = org.jetbrains.compose.resources.rememberResourceEnvironment()
                SideEffect { state.wide = wide; state.undoLabel = undoLabel; state.resEnv.value = env }
                CompositionLocalProvider(
                    LocalWide provides wide, LocalBusy provides BusyState(state.busy, state.touchNeeded, state.progress), LocalJobs provides state.jobs,
                    LocalSaveButton provides (if (unlocked && state.single == null) ({ SaveButton(state) }) else null),
                ) {
                    // One place for messages: the main screen on phones shows them above its own bar,
                    // everywhere else this outer host shows them (never both, which doubled every message).
                    val mainHasHost = state.vaultExists && !state.showStart && unlocked
                    Scaffold(snackbarHost = { if (!mainHasHost) GhulSnackbarHost(state.snackbar) }, containerColor = MaterialTheme.colorScheme.background) { _ ->
                        when {
                            !state.vaultExists || state.showStart -> Centered(wide) { SetupScreen(state) }
                            !unlocked -> Centered(wide) { UnlockScreen(state) }
                            else -> MainScreen(state, wide, maxWidth)
                        }
                    }
                    // A vault file waiting to be added once the own suite is unlocked.
                    if (unlocked && state.single == null && !state.showStart) PendingAddHandler(state)
                    // Saving into the suite file (the save button, Ctrl+S, before opening another file).
                    if (unlocked) SaveSuiteDialog(state)
                    // M2: the suite file at its place holds a state saved elsewhere since.
                    // Not over another dialog (a save, a file handed over, the steps after the code).
                    if (unlocked && state.single == null && !state.showStart && state.saveDialog == null &&
                        state.pendingAdd == null && state.pendingDocsAdd == null && !state.recoveredGuide
                    ) NewerSuiteFileDialog(state)
                    // Windows: quitting with changes not in the suite file (see AppState.requestQuit).
                    QuitDialog(state)
                    // Changes of a vault opened on its own that did not reach its file.
                    PendingSingleWrites(state)
                    // A running action, in front of the screens (sheets show it themselves).
                    BusyOverlay()
                    // Development build only: alarms of DevChecks, in every screenshot.
                    DevAlarmBar(Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.statusBars))
                    // While unlocking waits for the security key to be plugged in.
                    val waiting by state.backend.waitingForKey.collectAsState()
                    if (waiting) KeyWaitingBanner(Modifier.align(Alignment.BottomCenter))
                }
            }
        }
    }
}

@Composable
private fun KeyWaitingBanner(modifier: Modifier) {
    val t = g
    val pulse = androidx.compose.animation.core.rememberInfiniteTransition()
    val a by pulse.animateFloat(1f, 0.4f, androidx.compose.animation.core.infiniteRepeatable(
        androidx.compose.animation.core.tween(800), androidx.compose.animation.core.RepeatMode.Reverse))
    Row(
        modifier.padding(16.dp).navigationBarsPadding().widthIn(max = 520.dp).fillMaxWidth()
            .clip(RoundedCornerShape(18.dp)).background(t.accentSoft).padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(Icons.Filled.Usb, null, Modifier.size(28.dp).alpha(a), tint = t.accent)
        Column(Modifier.weight(1f)) {
            Text(stringResource(Res.string.key_waiting_title), color = t.accent, fontWeight = FontWeight.SemiBold)
            Text(stringResource(Res.string.key_waiting_text), color = t.muted, fontSize = 13.sp)
        }
    }
}

/** Width of the column beside the sidebar of a wide window. */
private val WIDE_COLUMN = 760.dp

/** Start and unlock: a phone-wide column in the middle of a wide window. */
@Composable
private fun Centered(wide: Boolean, content: @Composable () -> Unit) {
    if (!wide) return content()
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Box(Modifier.widthIn(max = 500.dp).fillMaxHeight()) { content() }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MainScreen(state: AppState, wide: Boolean, width: Dp) {
    var picker by remember { mutableStateOf<String?>(null) }
    var vaultFor by remember { mutableStateOf<VaultKindUi?>(null) }
    var generatorOnly by remember { mutableStateOf(false) }
    var addTotpFor by remember { mutableStateOf<String?>(null) }
    val top = state.stack.lastOrNull()
    val tabs = state.tabs
    // Review N-2: once per unlock, a key without a backup is asked about (not only on its page).
    LaunchedEffect(state.backend.lockCount) {
        state.keyWithoutBackup = if (state.single == null && !state.backend.openedWithRecoveryCode()) {
            catching { protectionOf(state.backend.slots()).needsBackup }.getOrDefault(false)
        } else {
            false
        }
    }
    // An area that is gone (removed, another suite): the first one, after this composition.
    LaunchedEffect(tabs) { if (state.tab !in tabs) state.tab = tabs.first() }
    // Tabs opened since unlocking; a plain set, read together with state.tab.
    val visited = remember { LinkedHashSet<Tab>() }
    visited.add(state.tab)
    // A field of the area left behind (e.g. its search) keeps no focus: hidden areas stay composed,
    // and Android brought the keyboard back for it when a sheet closed (bug: keyboard in GhulAuth).
    val focusManager = androidx.compose.ui.platform.LocalFocusManager.current
    LaunchedEffect(state.tab) { focusManager.clearFocus(force = true) }
    if (state.codeUnconfirmed && top !is Screen.RecoveryCode) CodeReminderDialog(state)
    else if (state.saveAfterProtection && top !is Screen.RecoveryCode) SaveAfterProtectionDialog(state)
    // Review N-b (factors): after the other questions of the unlock, not on top of them.
    else if (state.keyWithoutBackup && top !is Screen.RecoveryCode && state.oldSuiteFile == null && state.saveTargetOffer == null) NoBackupDialog(state)
    // A new recovery code cannot be skipped: it must be confirmed first.
    PlatformBackHandler(enabled = top != null && top !is Screen.RecoveryCode) { state.back() }
    UpdateDialog(state)
    if (state.saveDialog == null && !state.recoveredGuide) OldSuiteFileDialog(state)
    if (state.saveDialog == null && !state.recoveredGuide) state.saveTargetOffer?.let { ref ->
        AlertDialog(
            onDismissRequest = { state.saveTargetOffer = null },
            title = { Text(ref.name) },
            text = { Text(stringResource(Res.string.file_offer_place)) },
            confirmButton = {
                TextButton(onClick = {
                    state.saveTargetOffer = null
                    if (state.backend.keepAccess(ref)) state.backend.setSuiteFile(ref) else state.messageRes(Res.string.file_offer_no_access)
                }, modifier = Modifier.testTag("offer-keep")) { Text(stringResource(Res.string.file_offer_yes)) }
            },
            dismissButton = { TextButton(onClick = { state.saveTargetOffer = null }) { Text(stringResource(Res.string.file_offer_no)) } },
        )
    }
    // r11 M2: an editor with unsaved input defers the idle lock, but not for good: said in time.
    val idleWarningText = stringResource(Res.string.idle_lock_warning)
    androidx.compose.runtime.LaunchedEffect(state.idleWarning) {
        if (state.idleWarning) state.snackbar.showSnackbar(idleWarningText, duration = androidx.compose.material3.SnackbarDuration.Long)
    }
    state.archiveAsk?.let { item ->
        AlertDialog(
            onDismissRequest = { state.archiveAsk = null },
            icon = { Icon(Icons.Filled.Warning, null, tint = warnColor()) },
            title = { Text(item.name) },
            text = { Text(stringResource(Res.string.docs_open_archive)) },
            confirmButton = {
                TextButton(onClick = { state.archiveAsk = null; state.openDocWith(item, archiveConfirmed = true) }, modifier = Modifier.testTag("archive-open")) {
                    Text(stringResource(Res.string.docs_open_with))
                }
            },
            dismissButton = { TextButton(onClick = { state.archiveAsk = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    // After the save dialog (opened with the code from a file: first a new place for it).
    if (state.recoveredGuide && state.saveDialog == null) {
        AlertDialog(
            onDismissRequest = { state.recoveredGuide = false },
            title = { Text(stringResource(Res.string.recovered_title)) },
            text = { Text(stringResource(Res.string.recovered_steps)) },
            confirmButton = {
                TextButton(onClick = {
                    state.recoveredGuide = false
                    state.push(Screen.SettingsPage(SetPage.FACTORS))
                }) { Text(stringResource(Res.string.recovered_go)) }
            },
            dismissButton = { TextButton(onClick = { state.recoveredGuide = false }) { Text(stringResource(Res.string.a_later)) } },
        )
    }
    state.deleteAsk?.takeIf { it.trash || it.names.size < MANY_TO_DELETE }?.let { ask ->
        val one = ask.names.singleOrNull()
        AlertDialog(
            danger = true,
            onDismissRequest = { state.deleteAsk = null },
            title = { Text(stringResource(if (ask.trash) Res.string.ask_trash_title else Res.string.ask_delete_title)) },
            text = {
                Text(
                    if (one != null) stringResource(if (ask.trash) Res.string.ask_trash_named else Res.string.ask_delete_named, one)
                    else pluralStringResource(if (ask.trash) Res.plurals.ask_trash_n else Res.plurals.ask_delete_n, ask.names.size, ask.names.size.toString()),
                )
            },
            confirmButton = {
                TextButton(onClick = { state.deleteAsk = null; ask.action() }, modifier = Modifier.testTag("ask-confirm")) {
                    Text(stringResource(if (ask.trash) Res.string.a_to_trash else Res.string.a_delete))
                }
            },
            dismissButton = { TextButton(onClick = { state.deleteAsk = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    // r9 dieb N7: many entries at once (not into a trash) only with the master password.
    state.deleteAsk?.takeIf { !it.trash && it.names.size >= MANY_TO_DELETE }?.let { ask ->
        PasswordGateDialog(
            state, stringResource(Res.string.ask_delete_title),
            pluralStringResource(Res.plurals.ask_delete_n, ask.names.size, ask.names.size.toString()),
            confirmLabel = stringResource(Res.string.a_delete),
            onDismiss = { state.deleteAsk = null },
        ) {
            state.deleteAsk = null
            ask.action()
        }
    }
    if (state.confirmDiscard) {
        AlertDialog(
            onDismissRequest = { state.keepEditing() },
            title = { Text(stringResource(Res.string.edit_discard_title)) },
            text = { Text(stringResource(Res.string.edit_discard_text)) },
            confirmButton = { TextButton(onClick = { state.discardAndBack() }) { Text(stringResource(Res.string.edit_discard_yes)) } },
            dismissButton = { TextButton(onClick = { state.keepEditing() }) { Text(stringResource(Res.string.edit_keep)) } },
        )
    }

    val onAdd: () -> Unit = {
        val kind = if (state.tab == Tab.PASS) VaultKindUi.PASS else VaultKindUi.TOTP
        val filter = if (kind == VaultKindUi.PASS) state.passVault else state.authVault
        val usable = state.vaultsOf(kind).filter { it.available }
        val target = filter ?: usable.singleOrNull()?.id
        when {
            target == null -> vaultFor = kind
            kind == VaultKindUi.PASS -> picker = target
            else -> addTotpFor = target
        }
    }

    if (!wide) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.background,
            snackbarHost = { GhulSnackbarHost(state.snackbar) },
            topBar = {
                // Entries and events draw their own header (own actions).
                if (top == null) TabTopBar(state)
                else if (!ownHeader(top)) ScreenTopBar(state, top)
            },
            bottomBar = {
                if (top == null) {
                    GhulNavBar(navTabs(state), tabs.indexOf(state.tab)) { state.selectTab(tabs[it]) }
                }
            },
            floatingActionButton = { if (top == null) MainFab(state, onAdd) },
        ) { padding ->
            // The bars already make room for the status and navigation bar: pages that pad for
            // them themselves (ScreenColumn) must not do it a second time.
            Box(Modifier.fillMaxSize().consumeWindowInsets(padding)) {
                TabContents(state, visited, top == null, padding)
                if (top != null) TopScreen(state, top, padding)
            }
        }
    } else {
        // Sidebar | one column in the middle of the rest, in a readable width (as on a phone, only
        // with the areas on the left). An opened entry or page takes the place of the list
        // and "back" returns to it; there is no third column that mostly stood empty.
        val column = minOf(width - 240.dp - 1.dp, WIDE_COLUMN)
        val cols = ((column - 32.dp) / 220.dp).toInt().coerceIn(2, 6)
        Row(Modifier.fillMaxSize()) {
            SideNav(state, tabs)
            Box(Modifier.fillMaxHeight().width(1.dp).background(g.line))
            // F94: the sidebar keeps clear of a camera cut-out on the left; for everything in the
            // column (lists and pages with their own safe-area padding) that side is done.
            Box(
                Modifier.weight(1f).fillMaxHeight().consumeWindowInsets(WindowInsets.safeDrawing.only(WindowInsetsSides.Start)),
                contentAlignment = Alignment.TopCenter,
            ) {
                Box(Modifier.widthIn(max = WIDE_COLUMN).fillMaxSize()) {
                    CompositionLocalProvider(LocalColumns provides cols, LocalContentMax provides Dp.Infinity) {
                        Scaffold(
                            containerColor = MaterialTheme.colorScheme.background,
                            // F94: the sidebar already keeps clear of a camera cut-out on the left;
                            // the column next to it must not indent its content a second time.
                            contentWindowInsets = androidx.compose.material3.ScaffoldDefaults.contentWindowInsets
                                .only(WindowInsetsSides.Vertical + WindowInsetsSides.End),
                            snackbarHost = { GhulSnackbarHost(state.snackbar) },
                            topBar = {
                                if (top == null) TabTopBar(state)
                                else if (!ownHeader(top)) ScreenTopBar(state, top)
                            },
                            floatingActionButton = { if (top == null) MainFab(state, onAdd) },
                        ) { padding ->
                            Box(Modifier.fillMaxSize().consumeWindowInsets(padding)) {
                                TabContents(state, visited, top == null, padding)
                                if (top != null) TopScreen(state, top, padding)
                            }
                        }
                    }
                }
            }
        }
    }
    picker?.let { vaultId ->
        TypePickerDialog(
            onPick = { picker = null; state.push(Screen.EntryEdit(vaultId, null, it)) },
            onDismiss = { picker = null },
            onGenerate = { picker = null; generatorOnly = true },
            onImport = if (filePickersAvailable) ({ picker = null; state.push(Screen.Import) }) else null,
        )
    }
    val noQr = stringResource(Res.string.auth_no_qr)
    val addedMsg = stringResource(Res.string.auth_added)
    val pickQrImage = rememberOpenFile { f ->
        val vaultId = addTotpFor ?: return@rememberOpenFile
        addTotpFor = null
        val text = decodeQrImage(f.bytes)
        when {
            text == null -> state.message(noQr)
            text.startsWith("otpauth-migration://") -> state.run(block = {
                val r = state.backend.importData(text.encodeToByteArray(), "google-authenticator", null, vaultId, false)
                getPluralString(Res.plurals.msg_migration_done, r.duplicates, getPluralString(Res.plurals.n_accounts, r.totps, r.totps.toString()), r.duplicates.toString())
            }) { msg -> state.message(msg); state.changed(); state.promptSave() }
            else -> state.run(block = { state.backend.addTotp(vaultId, state.backend.parseOtpauth(text)) }) { state.message(addedMsg); state.changed(); state.promptSave() }
        }
    }
    addTotpFor?.let { vaultId ->
        val close = { addTotpFor = null }
        GhulSheet(close, stringResource(Res.string.auth_add_title)) {
            SheetList {
                var first = true
                if (qrScannerAvailable) {
                    SheetRow(stringResource(Res.string.auth_add_qr), stringResource(Res.string.auth_add_qr_sub), leading = { IconTile(Icons.Filled.QrCode2, paletteColor(2)) }) {
                        close(); state.push(Screen.AddTotp(vaultId, 0))
                    }
                    first = false
                }
                if (filePickersAvailable) {
                    SheetRow(stringResource(Res.string.auth_add_image), stringResource(Res.string.auth_add_image_sub), leading = { IconTile(Icons.Filled.Image, paletteColor(1)) }, divider = !first) {
                        pickQrImage()
                    }
                    first = false
                }
                SheetRow(stringResource(Res.string.auth_add_manual), leading = { IconTile(Icons.Filled.Edit, paletteColor(5)) }, divider = !first) {
                    close(); state.push(Screen.AddTotp(vaultId, 1))
                }
                SheetRow(stringResource(Res.string.auth_add_uri), leading = { IconTile(Icons.Filled.Link, paletteColor(1)) }, divider = true) {
                    close(); state.push(Screen.AddTotp(vaultId, 2))
                }
                SheetRow(stringResource(Res.string.auth_add_import), stringResource(Res.string.auth_add_import_sub), leading = { IconTile(Icons.Filled.Download, paletteColor(3)) }, divider = true) {
                    close(); state.push(Screen.Import)
                }
            }
        }
    }
    if (generatorOnly) {
        val label = stringResource(Res.string.f_password)
        GeneratorDialog(state, onUse = { state.copy(label, it); generatorOnly = false }, onDismiss = { generatorOnly = false },
            useLabel = stringResource(Res.string.a_copy))
    }
    state.icsPending?.let { IcsImportDialog(state, it) }
    if (state.contextSheet && top == null) AreaContextSheet(state)
    if (state.calQuickOpen && top == null) CalQuickSheet(state) { state.calQuickOpen = false }
    vaultFor?.let { kind ->
        VaultPickerDialog(state, kind, onPick = { id ->
            vaultFor = null
            if (kind == VaultKindUi.PASS) picker = id else addTotpFor = id
        }, onDismiss = { vaultFor = null })
    }
}

private fun ownHeader(top: Screen) = top is Screen.EntryDetail ||
    top is Screen.EntryEdit || top is Screen.CalEvent || top is Screen.CalEdit || top is Screen.DocView || top is Screen.DocScan

@Composable
private fun TabTopBar(state: AppState) {
    Column {
        GhulTopBar(
            title = screenTitle(null, state.tab),
            onLock = { state.lockNow() },
            context = { AreaContext(state) },
            actions = { TabActions(state) },
        )
        RecoveredBanner(state)
        SaveReminder(state)
    }
}

/** H3: opened with the recovery code: until the lock the new password alone repairs the protection. */
@Composable
private fun RecoveredBanner(state: AppState) {
    if (!state.recoveredOpen) return
    Box(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 8.dp).clickable { state.push(Screen.SettingsPage(SetPage.FACTORS)) }.testTag("recovered-banner")) {
        Banner(Icons.Filled.Warning, stringResource(Res.string.recovered_banner))
    }
}

/**
 * Not saved into the suite file for days, or a password or 2FA account was just added or changed
 * (M32: until it is saved, it is only on this device): a reminder under the header of every area
 * (tap: save). It never blocks anything.
 */
@Composable
private fun SaveReminder(state: AppState) {
    if (state.saveJob != null || !state.saveNeeded) return
    val text = when {
        state.saveOverdue -> stringResource(Res.string.save_reminder, formatDate(state.unsavedSince))
        state.savePrompt -> stringResource(Res.string.save_prompt)
        else -> return
    }
    Box(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 8.dp).clickable { state.askSave() }.testTag("save-reminder")) {
        Banner(Icons.Filled.Warning, text)
    }
}

/** The icon that leaves a sub-page, the same for every screen (also those with their own header). */
fun backIcon(): androidx.compose.ui.graphics.vector.ImageVector = Icons.AutoMirrored.Filled.ArrowBack

@Composable
private fun ScreenTopBar(state: AppState, top: Screen) {
    GhulTopBar(
        title = screenTitle(top, state.tab),
        onLock = { state.lockNow() },
        onBack = if (top !is Screen.RecoveryCode) ({ state.pop() }) else null,
        backIcon = backIcon(),
    )
}

@Composable
private fun navTabs(state: AppState): List<NavTab> = state.tabs.map { t ->
    NavTab(
        stringResource(if (t == Tab.SETTINGS) Res.string.tab_settings_short else tabTitle(t)),
        tabIcon(t),
        areaColor(t),
        tag = "tab-" + t.name.lowercase(),
        // Options holds the suite file: a dot while something is not saved in it; an area with
        // changes not in it has its own dot.
        dot = state.saveNeeded && (t == Tab.SETTINGS || areaKey(t) in state.unsavedAreas),
    )
}

/** The name of an area as the core lists unsaved changes ("pass", "auth", "docs", "cal"). */
fun areaKey(t: Tab): String = when (t) {
    Tab.PASS -> "pass"
    Tab.AUTH -> "auth"
    Tab.DOCS -> "docs"
    Tab.CAL -> "cal"
    Tab.SETTINGS -> "suite"
}

/** The unsaved areas for the save dialog ("GhulPass, GhulCal"); null: none listed. */
@Composable
fun unsavedAreasText(areas: List<String>): String? {
    if (areas.isEmpty()) return null
    if ("unknown" in areas) return stringResource(Res.string.save_unsaved_unknown)
    // Review N3: in the order of the areas everywhere (Pass, Auth, Docs, Cal), the suite last.
    val names = listOf(Tab.PASS, Tab.AUTH, Tab.DOCS, Tab.CAL).filter { areaKey(it) in areas }.map { stringResource(tabTitle(it)) } +
        (if ("suite" in areas) listOf(stringResource(Res.string.save_unsaved_suite)) else emptyList())
    return if (names.isEmpty()) null else stringResource(Res.string.save_unsaved_areas, names.joinToString(", "))
}

/** The icon of an area: the same in the navigation, in Options and wherever the area is named. */
fun tabIcon(t: Tab) = when (t) {
    Tab.PASS -> Icons.Outlined.Key
    Tab.AUTH -> Icons.Outlined.VerifiedUser
    Tab.DOCS -> Icons.Outlined.Description
    Tab.CAL -> Icons.Outlined.CalendarMonth
    Tab.SETTINGS -> Icons.Outlined.Tune
}

@Composable
private fun MainFab(state: AppState, onAdd: () -> Unit) {
    if (state.tab == Tab.CAL) {
        GhulFab(areaColor(Tab.CAL), Icons.Filled.Add, stringResource(Res.string.cal_event_short)) { state.calQuickOpen = true }
    }
    if (state.tab == Tab.DOCS && !state.docsTrashShown) {
        GhulFab(areaColor(Tab.DOCS), Icons.Filled.Add, stringResource(Res.string.fab_docs), description = stringResource(Res.string.a_add)) { state.docsAddTick++ }
    }
    if (state.tab == Tab.PASS || state.tab == Tab.AUTH) {
        // Always "+": scanning a QR code is one of the choices behind it. Icon and short text, as in
        // every area; the icon keeps the description "Add".
        GhulFab(
            areaColor(state.tab), Icons.Filled.Add,
            stringResource(if (state.tab == Tab.PASS) Res.string.fab_entry else Res.string.fab_account),
            description = stringResource(Res.string.a_add), onClick = onAdd,
        )
    }
}

/** The areas; visited ones stay composed and are only hidden (lists, scroll position, open folder). */
@Composable
private fun TabContents(state: AppState, visited: Set<Tab>, visible: Boolean, padding: PaddingValues) {
    Box(Modifier.fillMaxSize()) {
        for (t in visited.filter { it in state.tabs }) {
            key(t) {
                val active = visible && state.tab == t
                KeepAlive(active) {
                    CompositionLocalProvider(LocalTabActive provides active) {
                        when (t) {
                            Tab.PASS -> PassListScreen(state, padding)
                            Tab.AUTH -> AuthListScreen(state, padding)
                            Tab.DOCS -> DocsScreen(state, padding)
                            Tab.CAL -> CalScreen(state, padding)
                            Tab.SETTINGS -> SettingsScreen(state, padding)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun TopScreen(state: AppState, top: Screen, padding: PaddingValues) {
    CompositionLocalProvider(LocalPageTitle provides screenTitle(top, state.tab)) {
        when (top) {
            is Screen.EntryDetail -> key(top) { EntryDetailScreen(state, top.vaultId, top.id, padding) }
            is Screen.EntryEdit -> EntryEditScreen(state, top.vaultId, top.id, top.type, padding)
            is Screen.AddTotp -> AddTotpScreen(state, top.vaultId, padding, top.mode)
            Screen.ChangePassword -> ChangePasswordScreen(state, padding)
            Screen.Import -> ImportScreen(state, padding)
            Screen.Export -> ExportScreen(state, padding)
            is Screen.RecoveryCode -> RecoveryCodeScreen(state, top.code, padding)
            is Screen.SettingsPage -> key(top) { SettingsPageScreen(state, top.page, padding) }
            Screen.Security -> SecurityScreen(state, padding)
            is Screen.CalEvent -> key(top) { CalEventScreen(state, top.id, top.day, padding) }
            is Screen.CalEdit -> key(top) { CalEditScreen(state, top.draft, padding) }
            is Screen.DocView -> key(top) { DocViewScreen(state, top.vaultId, top.id, padding) }
            is Screen.DocScan -> DocScanScreen(state, top.vaultId, top.folder, padding)
        }
    }
}

/** Side navigation of wide windows: logo, the areas, lock at the bottom. */
@Composable
private fun SideNav(state: AppState, tabs: List<Tab>) {
    val t = g
    // A phone lying sideways is low: the bar scrolls then, so "Lock" is never cut off; on a tall
    // window "Lock" stays at the bottom.
    // Clear of the status bar, a camera cut-out and the system bars of a phone lying sideways.
    BoxWithConstraints(
        Modifier.width(240.dp).fillMaxHeight().background(t.surface)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Vertical + WindowInsetsSides.Start)),
    ) {
    Column(
        Modifier.verticalScroll(rememberScrollState()).heightIn(min = maxHeight)
            .padding(horizontal = 14.dp, vertical = 18.dp),
    ) {
        Row(Modifier.padding(start = 6.dp, bottom = 22.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                Modifier.size(38.dp).clip(RoundedCornerShape(12.dp))
                    .background(Brush.linearGradient(listOf(Color(0xFF8A72FF), Color(0xFF5B3DF0)))),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Outlined.Shield, null, tint = Color.White, modifier = Modifier.size(22.dp)) }
            Column {
                Text(stringResource(Res.string.app_name), fontSize = 18.sp, fontWeight = FontWeight.Bold, color = t.fg, letterSpacing = (-0.3).sp)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.padding(top = 3.dp)) {
                    for (a in BRAND_AREAS) {
                        Box(Modifier.size(width = 14.dp, height = 3.dp).clip(RoundedCornerShape(2.dp)).background(areaColor(a)))
                    }
                }
            }
        }
        for (tab in navTabs(state).zip(tabs)) {
            val (nav, area) = tab
            val on = state.tab == area
            Row(
                Modifier.fillMaxWidth().testTag(nav.tag).padding(vertical = 2.dp).clip(RoundedCornerShape(14.dp))
                    .background(if (on) nav.color.copy(alpha = 0.16f) else Color.Transparent)
                    .clickable { state.selectTab(area) }
                    .padding(horizontal = 12.dp, vertical = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Icon(nav.icon, null, Modifier.size(22.dp), tint = if (on) nav.color else t.muted)
                Text(
                    stringResource(tabTitle(area)), Modifier.weight(1f), fontSize = 15.sp,
                    fontWeight = if (on) FontWeight.Bold else FontWeight.SemiBold, color = if (on) nav.color else t.fg,
                )
                if (nav.dot) Box(Modifier.size(9.dp).clip(CircleShape).background(t.warn))
            }
        }
        Spacer(Modifier.weight(1f))
        // Changes not in the suite file yet: "Save" above "Lock" (Ctrl+S).
        if ((state.saveNeeded || state.saveJob != null) && state.canSaveHere) {
            Row(
                Modifier.fillMaxWidth().padding(bottom = 8.dp).testTag(SAVE_BUTTON_TAG).clip(RoundedCornerShape(14.dp)).background(t.surface2)
                    .clickable { state.askSave() }.padding(horizontal = 12.dp, vertical = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Icon(Icons.Filled.Save, null, Modifier.size(20.dp), tint = if (state.saveJob != null) t.accent else t.warn)
                Text(stringResource(Res.string.file_save), Modifier.weight(1f), fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = t.fg)
                if (state.backend.isDesktop) Text(stringResource(Res.string.side_save_key), fontSize = 12.sp, color = t.muted)
                if (state.saveJob == null) Box(Modifier.size(9.dp).clip(CircleShape).background(t.warn))
            }
        }
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(t.surface2)
                .clickable { state.lockNow() }.padding(horizontal = 12.dp, vertical = 11.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Icon(Icons.Filled.Lock, null, Modifier.size(20.dp), tint = t.fg)
            Text(stringResource(Res.string.a_lock), Modifier.weight(1f), fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = t.fg)
            // The key only where there is a keyboard for it (Windows), not on a phone lying sideways.
            if (state.backend.isDesktop) Text(stringResource(Res.string.side_lock_key), fontSize = 12.sp, color = t.muted)
        }
    }
    }
}

@Composable
private fun screenTitle(top: Screen?, tab: Tab): String = stringResource(
    when (top) {
                            is Screen.EntryDetail -> Res.string.title_entry
                            is Screen.EntryEdit -> if (top.id == null) Res.string.pass_new_entry else Res.string.a_edit
                            is Screen.AddTotp -> Res.string.auth_add_title
                            Screen.ChangePassword -> Res.string.a_change_password
                            Screen.Import -> Res.string.import_title
                            Screen.Export -> Res.string.export_title
                            is Screen.RecoveryCode -> Res.string.setup_code_title
                            is Screen.SettingsPage -> top.page.title()
                            Screen.Security -> Res.string.sec_title
                            is Screen.CalEvent -> Res.string.cal_event_title
                            is Screen.CalEdit -> if (top.draft.id.isEmpty()) Res.string.cal_new_event else Res.string.cal_edit_event
                            is Screen.DocView -> Res.string.tab_docs
                            is Screen.DocScan -> Res.string.scan_title
                            null -> tabTitle(tab)
                        },
)

/** Icons in the header of a tab (as in the design preview). */
@Composable
private fun TabActions(state: AppState) {
    when (state.tab) {
        Tab.AUTH -> BarIcon(if (state.authGrid) Icons.AutoMirrored.Filled.ViewList else Icons.Filled.GridView, null) { state.authGrid = !state.authGrid }
        Tab.CAL -> {
            CalTodayIcon { state.calSel = today(); if (state.calView == CalView.LIST) state.calView = CalView.MONTH }
            BarIcon(Icons.Filled.Search, stringResource(Res.string.cal_search_short)) { state.calView = CalView.LIST; state.calSearchTick++ }
        }
        else -> {}
    }
}

/**
 * The save button beside "Lock": changes not in the suite file yet (or a save that runs). A tap
 * opens the save dialog. Not on screens that edit something (what is not applied yet would not
 * be in the file).
 */
@Composable
fun SaveButton(state: AppState) {
    if (!state.canSaveHere) return
    val running = state.saveJob != null
    if (!state.saveNeeded && !running) return
    val t = g
    Box(Modifier.testTag(SAVE_BUTTON_TAG)) {
        BarIcon(Icons.Filled.Save, stringResource(Res.string.save_button_desc), tint = if (running) t.accent else t.warn) {
            state.askSave()
        }
        if (!running) Box(Modifier.align(Alignment.TopEnd).padding(top = 6.dp, end = 6.dp).size(9.dp).clip(CircleShape).background(t.warn))
    }
}

/**
 * M2: right after unlocking, before anything is edited: the suite file at the remembered place
 * was saved elsewhere since (another device). Open it now (this device's unsaved changes are
 * asked about by the opening itself) or later.
 */
@Composable
fun NewerSuiteFileDialog(state: AppState) {
    val info = state.newerSuiteFile ?: return
    val name = remember(info) { state.backend.suiteFile()?.name.orEmpty() }
    AlertDialog(
        onDismissRequest = { state.newerLater() },
        title = { Text(stringResource(Res.string.newer_file_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    stringResource(
                        if (info.target == de.danerd.ghulsuite.model.ArchiveTargetUi.OTHER_PROTECTION) Res.string.newer_file_protection else Res.string.newer_file_text,
                        name,
                    ),
                )
                if (info.device.isNotBlank() && info.time > 0) {
                    Text(stringResource(Res.string.file_saved_on, formatDateTime(info.time), info.device))
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { state.openNewerSuiteFile() }, modifier = Modifier.testTag("newer-open")) {
                Text(stringResource(Res.string.newer_file_open))
            }
        },
        dismissButton = { TextButton(onClick = { state.newerLater() }) { Text(stringResource(Res.string.a_later)) } },
    )
}

/**
 * Quitting (Windows) with changes not in the suite file: save first, quit without saving (they
 * stay on this device) or stay. While a save runs: quit once it is done.
 */
@Composable
fun QuitDialog(state: AppState) {
    if (!state.quitAsk) return
    val unlocked by state.backend.unlocked.collectAsState()
    val job = state.saveJob
    // An entry is being edited: apply it first (saving now would not hold it).
    val editing = job == null && unlocked && state.editorDirty
    val close = { state.quitAsk = false }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(stringResource(Res.string.close_title)) },
        text = {
            Text(
                when {
                    job != null -> savePercent(job.progress).let { p ->
                        if (p.isEmpty()) stringResource(Res.string.close_saving_unknown) else stringResource(Res.string.close_saving, p)
                    }
                    editing -> stringResource(Res.string.close_editing)
                    unlocked -> stringResource(Res.string.close_text)
                    else -> stringResource(Res.string.close_text_locked)
                },
            )
        },
        confirmButton = {
            when {
                job != null -> TextButton(onClick = { state.quitAfterSave = true; close() }) { Text(stringResource(Res.string.close_after_save)) }
                editing -> TextButton(onClick = close) { Text(stringResource(Res.string.edit_keep)) }
                unlocked -> TextButton(onClick = { state.quitAfterSave = true; close(); state.askSave() }, modifier = Modifier.testTag("quit-save")) {
                    Text(stringResource(Res.string.file_save))
                }
                // Locked: unlock first (the unlock screen is behind the dialog).
                else -> TextButton(onClick = close) { Text(stringResource(Res.string.a_unlock)) }
            }
        },
        dismissButton = {
            if (job == null) {
                TextButton(onClick = { close(); state.onQuit?.invoke() }, modifier = Modifier.testTag("quit-without")) {
                    Text(stringResource(Res.string.close_without))
                }
            } else {
                TextButton(onClick = close) { Text(stringResource(Res.string.a_cancel)) }
            }
        },
    )
}

/** Whether the composable is the visible tab (top screens are always active). */
val LocalTabActive = compositionLocalOf { true }

/**
 * [AppState.revision] as seen by this screen: a hidden tab keeps its value and catches up when it
 * becomes visible again, so hidden tabs do not reload on every change.
 */
@Composable
fun AppState.liveRevision(): Int {
    val active = LocalTabActive.current
    var seen by remember { mutableStateOf(revision) }
    LaunchedEffect(active, revision) { if (active) seen = revision }
    return seen
}

/** Keeps [content] composed but neither measures nor draws it while not [visible]. */
@Composable
private fun KeepAlive(visible: Boolean, content: @Composable () -> Unit) {
    Layout(content, Modifier.fillMaxSize()) { measurables, constraints ->
        if (!visible) return@Layout layout(0, 0) {}
        val placeables = measurables.map { it.measure(constraints) }
        layout(constraints.maxWidth, constraints.maxHeight) { placeables.forEach { it.place(0, 0) } }
    }
}

internal fun tabTitle(t: Tab) = when (t) {
    Tab.PASS -> Res.string.tab_pass
    Tab.AUTH -> Res.string.tab_auth
    Tab.DOCS -> Res.string.tab_docs
    Tab.CAL -> Res.string.tab_cal
    Tab.SETTINGS -> Res.string.tab_settings
}

/**
 * B6: a calendar file from another app (shared or opened with GhulSuite) is only imported after
 * the user confirmed it, with the number of events and the target calendar.
 */
@Composable
private fun IcsImportDialog(state: AppState, data: ByteArray) {
    val events = remember(data) { Regex("BEGIN:VEVENT").findAll(data.decodeToString()).count() }
    val target = state.calendars.firstOrNull { it.id == state.calDefaultCalendar }?.name ?: ""
    AlertDialog(
        onDismissRequest = { state.icsPending = null },
        title = { Text(stringResource(Res.string.cal_import_confirm_title)) },
        text = { Text(pluralStringResource(Res.plurals.cal_import_confirm_text, events, events.toString(), target)) },
        confirmButton = {
            TextButton(onClick = {
                state.icsPending = null
                state.importIcs(data)
            }, enabled = events > 0) { Text(stringResource(Res.string.a_import)) }
        },
        dismissButton = { TextButton(onClick = { state.icsPending = null }) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Tag of "Delete old file" (after the suite moved to another place). */
const val DELETE_OLD_FILE_TAG = "delete-old-file"

/**
 * The suite moved to another place (written there and checked): where the old file is. Where it
 * holds only a state of this suite's own line (checked, never after a conflict), "Delete old
 * file": asked again naming it, then the master password (it deletes for good), and checked once
 * more by the backend right before. Where the app may not, or the place is not there, it says so.
 */
@Composable
fun OldSuiteFileDialog(state: AppState) {
    val old = state.oldSuiteFile ?: return
    val now = state.movedTo
    var asking by remember(old) { mutableStateOf(false) }
    var gate by remember(old) { mutableStateOf(false) }
    var outcome by remember(old) { mutableStateOf<de.danerd.ghulsuite.model.OldFileDelete?>(null) }
    val check = state.oldCheck
    val close = { state.oldSuiteFile = null; state.movedTo = null; state.oldCheck = de.danerd.ghulsuite.model.OldFileCheck.OTHER }
    val deletedMsg = stringResource(Res.string.old_file_deleted, old.name)
    if (gate) {
        PasswordGateDialog(
            state, stringResource(Res.string.old_file_title),
            stringResource(if (check == de.danerd.ghulsuite.model.OldFileCheck.EARLIER_PROTECTION) Res.string.old_file_ask_wipe else Res.string.old_file_ask, old.name),
            confirmLabel = stringResource(Res.string.old_file_delete_now), onDismiss = { gate = false; asking = false },
        ) {
            gate = false
            asking = false
            state.run(block = { state.backend.deleteOldSuiteFile(old) }) { r ->
                if (r.deleted) {
                    close()
                    state.message(deletedMsg)
                } else {
                    outcome = r
                }
            }
        }
        return
    }
    val o = outcome
    val canDelete = check.deletable && (o == null || o.outcome == de.danerd.ghulsuite.model.OldFileOutcome.FAILED ||
        o.outcome == de.danerd.ghulsuite.model.OldFileOutcome.UNREACHABLE)
    AlertDialog(
        onDismissRequest = close,
        title = { Text(stringResource(Res.string.old_file_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.file_saved_moved, now?.name ?: "", old.name))
                // Review N3: the reason as it is (not "another state" for a stick not plugged in).
                when (check) {
                    de.danerd.ghulsuite.model.OldFileCheck.OTHER -> Text(stringResource(Res.string.old_file_other_state))
                    de.danerd.ghulsuite.model.OldFileCheck.UNREACHABLE -> Text(stringResource(Res.string.old_file_unreachable, old.name))
                    de.danerd.ghulsuite.model.OldFileCheck.GONE -> Text(stringResource(Res.string.old_file_gone))
                    de.danerd.ghulsuite.model.OldFileCheck.EARLIER_PROTECTION -> Text(stringResource(Res.string.old_file_earlier_protection))
                    de.danerd.ghulsuite.model.OldFileCheck.SAME -> {}
                }
                if (asking) Text(stringResource(if (check == de.danerd.ghulsuite.model.OldFileCheck.EARLIER_PROTECTION) Res.string.old_file_ask_wipe else Res.string.old_file_ask, old.name), color = warnColor())
                o?.let {
                    Text(
                        when (it.outcome) {
                            de.danerd.ghulsuite.model.OldFileOutcome.UNREACHABLE -> stringResource(Res.string.old_file_unreachable, it.where)
                            de.danerd.ghulsuite.model.OldFileOutcome.FAILED -> stringResource(Res.string.old_file_failed, it.where)
                            de.danerd.ghulsuite.model.OldFileOutcome.CHANGED -> stringResource(Res.string.old_file_changed)
                            de.danerd.ghulsuite.model.OldFileOutcome.DIFFERENT_STATE, de.danerd.ghulsuite.model.OldFileOutcome.SAME_FILE ->
                                stringResource(Res.string.old_file_other_state)
                            else -> stringResource(Res.string.old_file_by_hand, it.where)
                        },
                        color = warnColor(),
                    )
                }
            }
        },
        confirmButton = {
            if (canDelete) {
                androidx.compose.runtime.CompositionLocalProvider(LocalButtonLook provides ButtonLook.DANGER) {
                    TextButton(onClick = { if (!asking) asking = true else gate = true }, enabled = !state.busy, modifier = Modifier.testTag(DELETE_OLD_FILE_TAG)) {
                        Text(stringResource(if (asking) Res.string.old_file_delete_now else if (o != null) Res.string.old_file_retry else Res.string.old_file_delete))
                    }
                }
            }
        },
        dismissButton = { TextButton(onClick = close) { Text(stringResource(if (canDelete) Res.string.old_file_keep else Res.string.a_ok)) } },
    )
}
