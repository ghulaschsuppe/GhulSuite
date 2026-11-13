// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.GridView
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.automirrored.filled.ViewList
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Email
import androidx.compose.material.icons.outlined.CalendarMonth
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
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

/** Root of the app: setup, unlock or the main screens. */
@Composable
fun GhulSuiteApp(state: AppState) {
    GhulTheme(state.backend.settings.theme) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            val unlocked by state.backend.unlocked.collectAsState()
            BoxWithConstraints(Modifier.fillMaxSize()) {
                // Wide windows (Windows, tablets in landscape) get side navigation and two panes.
                val wide = maxWidth >= 840.dp
                SideEffect { state.wide = wide }
                CompositionLocalProvider(LocalWide provides wide) {
                    Scaffold(snackbarHost = { SnackbarHost(state.snackbar) }, containerColor = MaterialTheme.colorScheme.background) { _ ->
                        when {
                            !state.vaultExists || state.showStart -> Centered(wide) { SetupScreen(state) }
                            !unlocked -> Centered(wide) { UnlockScreen(state) }
                            else -> MainScreen(state, wide, maxWidth)
                        }
                    }
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
    if (state.tab !in tabs) state.tab = tabs.first()
    // Tabs opened since unlocking; a plain set, read together with state.tab.
    val visited = remember { LinkedHashSet<Tab>() }
    visited.add(state.tab)
    if (state.codeUnconfirmed && top !is Screen.RecoveryCode) CodeReminderDialog(state)
    // A new recovery code cannot be skipped: it must be confirmed first.
    PlatformBackHandler(enabled = top != null && top !is Screen.RecoveryCode) { state.pop() }

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
            snackbarHost = { SnackbarHost(state.snackbar) },
            topBar = {
                // Mail, compose and entry draw their own header (own actions).
                if (top == null) TabTopBar(state)
                else if (!ownHeader(top)) ScreenTopBar(state, top)
            },
            bottomBar = {
                if (top == null && !DriveSelectionBar(state)) {
                    GhulNavBar(navTabs(state), tabs.indexOf(state.tab)) { state.selectTab(tabs[it]) }
                }
            },
            floatingActionButton = { if (top == null) MainFab(state, onAdd) },
        ) { padding ->
            Box(Modifier.fillMaxSize()) {
                TabContents(state, visited, top == null, padding)
                if (top != null) TopScreen(state, top, padding)
            }
        }
    } else {
        // Side navigation | list | opened screen.
        val twoPane = top != null || state.tab == Tab.PASS || state.tab == Tab.MAIL || state.tab == Tab.SETTINGS
        val listWidth = if (width >= 1300.dp) 440.dp else 400.dp
        val rest = width - 240.dp - 1.dp
        val listPane = if (twoPane) listWidth else rest
        val cols = ((listPane - 32.dp) / 220.dp).toInt().coerceIn(2, 6)
        Row(Modifier.fillMaxSize()) {
            SideNav(state, tabs)
            Box(Modifier.fillMaxHeight().width(1.dp).background(g.line))
            Box(Modifier.width(listPane).fillMaxHeight()) {
                CompositionLocalProvider(LocalColumns provides cols) {
                    Scaffold(
                        containerColor = MaterialTheme.colorScheme.background,
                        topBar = { TabTopBar(state) },
                        bottomBar = { DriveSelectionBar(state) },
                        floatingActionButton = { MainFab(state, onAdd) },
                    ) { padding -> TabContents(state, visited, true, padding) }
                }
            }
            if (twoPane) {
                Box(Modifier.fillMaxHeight().width(1.dp).background(g.line))
                Box(Modifier.weight(1f).fillMaxHeight().background(g.surface.copy(alpha = 0.35f)), contentAlignment = Alignment.TopCenter) {
                    if (top == null) {
                        EmptyPane(state.tab)
                    } else {
                        Box(Modifier.widthIn(max = 820.dp).fillMaxSize()) {
                            Scaffold(
                                containerColor = Color.Transparent,
                                topBar = { if (!ownHeader(top)) ScreenTopBar(state, top) },
                            ) { padding -> TopScreen(state, top, padding) }
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
                org.jetbrains.compose.resources.getString(Res.string.msg_migration_done, r.totps.toString(), r.duplicates.toString())
            }) { msg -> state.message(msg); state.changed() }
            else -> state.run(block = { state.backend.addTotp(vaultId, state.backend.parseOtpauth(text)) }) { state.message(addedMsg); state.changed() }
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
    if (state.found.isNotEmpty() && !state.foundPostponed && top == null) FoundVaultsDialog(state)
    if (state.passwordChangedElsewhere) PasswordChangedDialog(state)
    if (state.contextSheet && top == null) AreaContextSheet(state)
    if (state.calQuickOpen && top == null) CalQuickSheet(state) { state.calQuickOpen = false }
    vaultFor?.let { kind ->
        VaultPickerDialog(state, kind, onPick = { id ->
            vaultFor = null
            if (kind == VaultKindUi.PASS) picker = id else addTotpFor = id
        }, onDismiss = { vaultFor = null })
    }
}

private fun ownHeader(top: Screen) = top is Screen.MailRead || top is Screen.MailCompose || top is Screen.EntryDetail ||
    top is Screen.CalEvent || top is Screen.CalEdit

@Composable
private fun TabTopBar(state: AppState) {
    GhulTopBar(
        title = screenTitle(null, state.tab),
        onLock = { state.backend.lock() },
        context = { AreaContext(state) },
        actions = { TabActions(state) },
    )
}

@Composable
private fun ScreenTopBar(state: AppState, top: Screen) {
    val root = state.wide && state.stack.size == 1
    GhulTopBar(
        title = screenTitle(top, state.tab),
        onLock = { state.backend.lock() },
        onBack = if (top !is Screen.RecoveryCode) ({ state.pop() }) else null,
        backIcon = if (root) Icons.Filled.Close else Icons.AutoMirrored.Filled.ArrowBack,
    )
}

/** Selection bar of GhulDrive while files are being selected; false when there is none. */
@Composable
private fun DriveSelectionBar(state: AppState): Boolean {
    val sel = state.driveSelection
    if (state.tab != Tab.DRIVE || sel == null) return false
    SelectionBar(
        stringResource(Res.string.drive_n_selected, sel.count.toString()),
        listOf(
            Triple(Icons.AutoMirrored.Filled.Send, stringResource(Res.string.a_share), sel.onShare),
            Triple(Icons.Filled.Archive, stringResource(Res.string.drive_as_zip), sel.onZip),
            Triple(Icons.Filled.DoneAll, stringResource(Res.string.a_select_all), sel.onAll),
            Triple(Icons.Filled.Delete, stringResource(Res.string.a_delete), sel.onDelete),
            Triple(Icons.Filled.Close, stringResource(Res.string.a_done), sel.onDone),
        ),
        enabled = listOf(sel.count > 0, sel.count > 0, true, sel.count > 0, true),
    )
    return true
}

@Composable
private fun navTabs(state: AppState): List<NavTab> = state.tabs.map { t ->
    NavTab(
        stringResource(if (t == Tab.SETTINGS) Res.string.tab_settings_short else tabTitle(t)),
        tabIcon(t),
        areaColor(t),
        badge = if (t == Tab.MAIL) state.mailUnread else 0,
    )
}

private fun tabIcon(t: Tab) = when (t) {
    Tab.PASS -> Icons.Outlined.Key
    Tab.AUTH -> Icons.Outlined.VerifiedUser
    Tab.DRIVE -> Icons.Outlined.Cloud
    Tab.MAIL -> Icons.Outlined.Email
    Tab.CAL -> Icons.Outlined.CalendarMonth
    Tab.SETTINGS -> Icons.Outlined.Tune
}

@Composable
private fun MainFab(state: AppState, onAdd: () -> Unit) {
    if (state.tab == Tab.MAIL && state.mailReady) {
        GhulFab(areaColor(Tab.MAIL), Icons.Filled.Edit, stringResource(Res.string.mail_write)) {
            state.push(Screen.MailCompose(MailDraft(state.mailAccount)))
        }
    }
    if (state.tab == Tab.DRIVE && state.driveReady && state.driveSelection == null) {
        GhulFab(areaColor(Tab.DRIVE), Icons.Filled.Add, description = stringResource(Res.string.drive_upload_fab)) { state.driveUploadTick++ }
    }
    if (state.tab == Tab.CAL) {
        GhulFab(areaColor(Tab.CAL), Icons.Filled.Add, stringResource(Res.string.cal_event_short)) { state.calQuickOpen = true }
    }
    if (state.tab == Tab.PASS || state.tab == Tab.AUTH) {
        GhulFab(areaColor(state.tab), if (state.tab == Tab.AUTH) Icons.Filled.QrCode2 else Icons.Filled.Add, description = stringResource(Res.string.a_add), onClick = onAdd)
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
                            Tab.DRIVE -> DriveScreen(state, padding)
                            Tab.MAIL -> MailScreen(state, padding)
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
            Screen.AddServer -> AddServerScreen(state, padding)
            is Screen.SettingsPage -> key(top) { SettingsPageScreen(state, top.page, padding) }
            is Screen.ShareSetup -> ShareSetupScreen(state, top.serverId, padding)
            Screen.Shares -> SharesScreen(state, padding)
            Screen.Security -> SecurityScreen(state, padding)
            is Screen.MailAccount -> MailAccountScreen(state, top.id, padding)
            is Screen.MailRead -> key(top) { MailReadScreen(state, top.accountId, top.folder, top.uid, padding) }
            is Screen.MailCompose -> key(top) { MailComposeScreen(state, top.draft, padding) }
            is Screen.CalEvent -> key(top) { CalEventScreen(state, top.id, top.day, padding) }
            is Screen.CalEdit -> key(top) { CalEditScreen(state, top.draft, padding) }
        }
    }
}

/** Side navigation of wide windows: logo, the areas, lock at the bottom. */
@Composable
private fun SideNav(state: AppState, tabs: List<Tab>) {
    val t = g
    Column(Modifier.width(240.dp).fillMaxHeight().background(t.surface).padding(horizontal = 14.dp, vertical = 18.dp)) {
        Row(Modifier.padding(start = 6.dp, bottom = 22.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                Modifier.size(38.dp).clip(RoundedCornerShape(12.dp))
                    .background(Brush.linearGradient(listOf(Color(0xFF8A72FF), Color(0xFF5B3DF0)))),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Outlined.Shield, null, tint = Color.White, modifier = Modifier.size(22.dp)) }
            Column {
                Text(stringResource(Res.string.app_name), fontSize = 18.sp, fontWeight = FontWeight.Bold, color = t.fg, letterSpacing = (-0.3).sp)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.padding(top = 3.dp)) {
                    for (a in listOf(Tab.PASS, Tab.AUTH, Tab.DRIVE, Tab.MAIL, Tab.CAL)) {
                        Box(Modifier.size(width = 14.dp, height = 3.dp).clip(RoundedCornerShape(2.dp)).background(areaColor(a)))
                    }
                }
            }
        }
        for (tab in navTabs(state).zip(tabs)) {
            val (nav, area) = tab
            val on = state.tab == area
            Row(
                Modifier.fillMaxWidth().padding(vertical = 2.dp).clip(RoundedCornerShape(14.dp))
                    .background(if (on) nav.color.copy(alpha = 0.14f) else Color.Transparent)
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
                if (nav.badge > 0) {
                    Box(
                        Modifier.heightIn(min = 20.dp).widthIn(min = 20.dp).clip(RoundedCornerShape(10.dp)).background(areaColor(Tab.MAIL)).padding(horizontal = 6.dp),
                        contentAlignment = Alignment.Center,
                    ) { Text(nav.badge.toString(), color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold) }
                }
            }
        }
        Spacer(Modifier.weight(1f))
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(t.surface2)
                .clickable { state.backend.lock() }.padding(horizontal = 12.dp, vertical = 11.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Icon(Icons.Filled.Lock, null, Modifier.size(20.dp), tint = t.fg)
            Text(stringResource(Res.string.a_lock), Modifier.weight(1f), fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = t.fg)
            Text(stringResource(Res.string.side_lock_key), fontSize = 12.sp, color = t.muted)
        }
    }
}

/** Right side while nothing is open. */
@Composable
private fun EmptyPane(tab: Tab) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        IconTile(tabIcon(tab), areaColor(tab), size = 72.dp)
        Spacer(Modifier.height(18.dp))
        Text(
            stringResource(
                when (tab) {
                    Tab.PASS -> Res.string.pane_pick_entry
                    Tab.MAIL -> Res.string.pane_pick_mail
                    Tab.SETTINGS -> Res.string.pane_pick_setting
                    else -> Res.string.pane_pick_entry
                },
            ),
            fontSize = 17.sp, fontWeight = FontWeight.SemiBold, color = g.fg,
        )
        Spacer(Modifier.height(6.dp))
        Text(stringResource(Res.string.pane_pick_hint), color = g.muted, textAlign = TextAlign.Center)
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
                            Screen.AddServer -> Res.string.server_add_title
                            is Screen.SettingsPage -> top.page.title()
                            is Screen.ShareSetup -> Res.string.share_setup_title
                            Screen.Shares -> Res.string.shares_title
                            Screen.Security -> Res.string.sec_title
                            is Screen.MailAccount -> if (top.id == null) Res.string.mail_account_new else Res.string.mail_account_edit
                            is Screen.MailRead -> Res.string.mail_read_title
                            is Screen.MailCompose -> Res.string.mail_compose_title
                            is Screen.CalEvent -> Res.string.cal_event_title
                            is Screen.CalEdit -> if (top.draft.id.isEmpty()) Res.string.cal_new_event else Res.string.cal_edit_event
                            null -> tabTitle(tab)
                        },
)

/** Icons in the header of a tab (as in the design preview). */
@Composable
private fun TabActions(state: AppState) {
    when (state.tab) {
        Tab.AUTH -> BarIcon(if (state.authGrid) Icons.AutoMirrored.Filled.ViewList else Icons.Filled.GridView, null) { state.authGrid = !state.authGrid }
        Tab.DRIVE -> if (state.driveReady) {
            BarIcon(if (state.driveGrid) Icons.AutoMirrored.Filled.ViewList else Icons.Filled.GridView, null) { state.driveGrid = !state.driveGrid }
        }
        Tab.MAIL -> if (state.mailReady) BarIcon(Icons.Filled.Refresh, stringResource(Res.string.a_refresh)) { state.mailRefreshTick++ }
        Tab.CAL -> {
            CalTodayIcon { state.calSel = today(); if (state.calView == CalView.LIST) state.calView = CalView.MONTH }
            BarIcon(Icons.Filled.Search, stringResource(Res.string.cal_search_short)) { state.calView = CalView.LIST; state.calSearchTick++ }
        }
        else -> {}
    }
}

/** Whether the composable is the visible tab (top screens are always active). */
val LocalTabActive = compositionLocalOf { true }

/**
 * [AppState.revision] as seen by this screen: a hidden tab keeps its value and catches up when it
 * becomes visible again, so hidden tabs do not reload (e.g. mail over the network) on every change.
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

private fun tabTitle(t: Tab) = when (t) {
    Tab.PASS -> Res.string.tab_pass
    Tab.AUTH -> Res.string.tab_auth
    Tab.DRIVE -> Res.string.tab_drive
    Tab.MAIL -> Res.string.tab_mail
    Tab.CAL -> Res.string.tab_cal
    Tab.SETTINGS -> Res.string.tab_settings
}
