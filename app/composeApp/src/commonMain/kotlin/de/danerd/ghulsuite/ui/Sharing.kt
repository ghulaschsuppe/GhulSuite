// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.PhoneAndroid
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.size
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.Attachment
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.DriveStatusUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.ShareCheckUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

private val Blue = Color(0xFF2F74E8)

/** Expiry choices in days; 0 = never. */
private val EXPIRY = listOf(1, 7, 30, 0)
private val MAX_DOWNLOADS = listOf(0, 1, 5, 10)
private const val MAIL_ATTACH_LIMIT = 20L * 1024 * 1024

/**
 * GhulMail is a separate module: it only appears where it is switched on and has at least one
 * account. Nothing in Drive or Pass depends on it.
 */
@Composable
fun rememberMailAvailable(state: AppState): Boolean {
    var available by remember { mutableStateOf(false) }
    val enabled = state.backend.settings.moduleGhulMail
    LaunchedEffect(enabled, state.liveRevision()) {
        available = enabled && runCatching { state.backend.net.mailAccounts() }.getOrDefault(emptyList()).isNotEmpty()
    }
    return available && enabled
}

/** Opens the mail module with prefilled content (only called when [rememberMailAvailable]). */
fun AppState.composeMail(subject: String = "", text: String = "", attachments: List<de.danerd.ghulsuite.model.OutgoingAttachmentUi> = emptyList()) {
    push(Screen.MailCompose(MailDraft(mailAccount, subject = subject, text = text, attachments = attachments)))
}

/** Share Drive files: to another app, as a link, or as mail attachment. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareSheet(state: AppState, files: List<DriveItemUi>, onDismiss: () -> Unit, onDone: () -> Unit, onZip: (() -> Unit)? = null) {
    val net = state.backend.net
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var linkMode by remember { mutableStateOf(false) }
    var result by remember { mutableStateOf<ShareInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val mail = rememberMailAvailable(state)
    LaunchedEffect(Unit) { servers = runCatching { net.servers() }.getOrDefault(emptyList()).filter { it.canShare } }
    val onlyFiles = files.none { it.isDir }
    val title = if (files.size == 1) files[0].name else stringResource(Res.string.share_n_items, files.size.toString())

    result?.let { info ->
        ShareResultDialog(state, info, mail, onDismiss = { result = null; onDone() })
        return
    }

    val t = g
    GhulSheet(onDismiss, if (linkMode) stringResource(Res.string.share_link) else stringResource(Res.string.a_share)) {
        HintText(title, Modifier.padding(start = 0.dp))
        if (!linkMode) {
            SheetList {
                if (onlyFiles) {
                    val desktop = state.backend.isDesktop
                    SheetRow(
                        stringResource(if (desktop) Res.string.share_apps_desktop else Res.string.share_apps),
                        stringResource(if (desktop) Res.string.share_apps_desktop_sub else Res.string.share_apps_sub),
                        leading = { IconTile(Icons.Filled.PhoneAndroid, paletteColor(2)) },
                        end = { Tag(stringResource(Res.string.share_always), t.ok) },
                    ) { state.run(onError = { error = it }, block = { net.shareDriveToApps(files) }) { onDone() } }
                }
                SheetRow(
                    stringResource(Res.string.share_link),
                    stringResource(if (servers.isEmpty()) Res.string.share_link_setup else Res.string.share_link_sub),
                    leading = { IconTile(Icons.Filled.Link, paletteColor(5)) },
                    end = { ChevronEnd() },
                    divider = onlyFiles,
                ) {
                    if (servers.isEmpty()) {
                        onDismiss(); state.push(Screen.SettingsPage(SetPage.SERVERS))
                    } else {
                        linkMode = true
                    }
                }
                if (onZip != null) {
                    SheetRow(
                        stringResource(Res.string.drive_zip_title), stringResource(Res.string.drive_zip_sub),
                        leading = { IconTile(Icons.Filled.Archive, paletteColor(9)) },
                        end = { ChevronEnd() },
                        divider = true,
                    ) { onZip() }
                }
                if (onlyFiles && mail) {
                    // Mail servers usually refuse more than about 25 MB; larger files go as a link.
                    val tooBig = files.sumOf { it.size } > MAIL_ATTACH_LIMIT
                    SheetRow(
                        stringResource(Res.string.share_mail_attach),
                        stringResource(if (tooBig) Res.string.share_mail_too_big else Res.string.share_mail_attach_sub),
                        leading = { IconTile(Icons.Filled.Email, paletteColor(3)) },
                        end = { ChevronEnd() },
                        divider = true, enabled = !tooBig,
                    ) {
                        state.run(onError = { error = it }, block = { net.driveFilesForMail(files) }) { list ->
                            onDone()
                            state.composeMail(subject = files.joinToString(", ") { it.name }, attachments = list)
                        }
                    }
                }
            }
            ErrorText(error)
        } else {
            ShareOptionsForm(state, servers, onError = { error = it }) { options ->
                state.run(onError = { error = it }, block = {
                    net.shareDrive(files.map { it.id }, if (files.size == 1) null else "GhulDrive", options)
                }) { result = it }
            }
            ErrorText(error)
            OutlinedButton(onClick = { linkMode = false }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_back)) }
        }
    }
}

/** Server, expiry, password and (PHP module only) download limit of a new share link. */
@Composable
fun ShareOptionsForm(
    state: AppState,
    servers: List<ServerUi>,
    onError: (String?) -> Unit,
    defaultDays: Int = 7,
    defaultMax: Int = 0,
    onCreate: (ShareOptionsUi) -> Unit,
) {
    // A server that can count downloads first: one-time links need it.
    val ordered = remember(servers, defaultMax) { if (defaultMax > 0) servers.sortedBy { it.kind != ServerKindUi.PHP } else servers }
    var serverId by remember(ordered) { mutableStateOf(ordered.firstOrNull()?.id) }
    var days by remember { mutableStateOf(defaultDays) }
    var usePassword by remember { mutableStateOf(false) }
    var password by remember { mutableStateOf("") }
    var max by remember(serverId) { mutableStateOf(if (ordered.firstOrNull { it.id == serverId }?.kind == ServerKindUi.PHP) defaultMax else 0) }
    val server = ordered.firstOrNull { it.id == serverId }
    val counts = server?.kind == ServerKindUi.PHP

    if (servers.size > 1) {
        SectionLabel(stringResource(Res.string.share_server))
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (s in ordered) FilterChip(s.id == serverId, { serverId = s.id }, label = { Text(s.name) })
        }
    }
    SectionLabel(stringResource(Res.string.share_expires))
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (d in EXPIRY) FilterChip(days == d, { days = d }, label = { Text(expiryLabel(d)) })
    }
    if (counts) {
        SectionLabel(stringResource(Res.string.share_max))
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (m in MAX_DOWNLOADS) {
                FilterChip(max == m, { max = m }, label = { Text(if (m == 0) stringResource(Res.string.share_unlimited) else m.toString() + "×") })
            }
        }
    } else {
        Hint(stringResource(Res.string.share_max_php_only))
    }
    SwitchRow(stringResource(Res.string.share_password), usePassword) { usePassword = it }
    if (usePassword) {
        SecretTextField(password, { password = it; onError(null) }, stringResource(Res.string.share_password))
        Hint(stringResource(Res.string.share_password_hint))
    }
    Hint(stringResource(Res.string.share_e2e_hint))
    Button(
        onClick = { serverId?.let { onCreate(ShareOptionsUi(it, password.takeIf { usePassword && it.isNotEmpty() }, days, max)) } },
        enabled = !state.busy && serverId != null && (!usePassword || password.length >= 4),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Icon(Icons.Filled.Link, null, Modifier.size(18.dp))
        Text(stringResource(Res.string.share_create))
    }
}

@Composable
private fun expiryLabel(days: Int): String = when (days) {
    0 -> stringResource(Res.string.share_never)
    1 -> stringResource(Res.string.share_1_day)
    else -> stringResource(Res.string.share_days, days.toString())
}

/** The finished link: copy it, or hand it to GhulMail when that module is available. */
@Composable
fun ShareResultDialog(state: AppState, info: ShareInfoUi, mail: Boolean, onDismiss: () -> Unit) {
    val linkLabel = stringResource(Res.string.share_link)
    val mailText = stringResource(Res.string.share_mail_text, info.name, info.url)
    val t = g
    GhulSheet(onDismiss, stringResource(Res.string.share_ready)) {
        Banner(Icons.Filled.CheckCircle, info.name + " · " + formatSize(info.size) + " · " + shareSummary(info))
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface2).padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(linkLabel, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = t.accent)
                SelectionContainer { Text(info.url, fontFamily = FontFamily.Monospace, fontSize = 14.sp) }
            }
            BarIcon(Icons.Filled.ContentCopy, stringResource(Res.string.a_copy)) { state.copy(linkLabel, info.url) }
        }
        HintText(stringResource(if (info.password) Res.string.share_password_separately else Res.string.share_key_hint), Modifier.padding(start = 0.dp))
        var qr by remember { mutableStateOf(false) }
        if (qr) QrImage(info.url, Modifier.fillMaxWidth().padding(horizontal = 24.dp))
        ButtonRow {
            if (mail) {
                OutlinedButton(onClick = { onDismiss(); state.composeMail(subject = info.name, text = mailText) }, modifier = Modifier.weight(1f), contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp)) {
                    Icon(Icons.Filled.Email, null, Modifier.size(18.dp)); Text(stringResource(Res.string.share_by_mail_short), maxLines = 1)
                }
            }
            OutlinedButton(onClick = { qr = !qr }, contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp)) {
                Icon(Icons.Filled.QrCode2, stringResource(Res.string.share_qr), Modifier.size(18.dp))
            }
            Button(onClick = { state.copy(linkLabel, info.url) }, modifier = Modifier.weight(1f), contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp)) {
                Icon(Icons.Filled.Link, null, Modifier.size(18.dp)); Text(stringResource(Res.string.share_copy_link), maxLines = 1)
            }
        }
        TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_done)) }
    }
}

@Composable
private fun shareSummary(info: ShareInfoUi): String {
    val expiry = if (info.expires == 0L) stringResource(Res.string.share_never) else stringResource(Res.string.share_until, formatDateTime(info.expires))
    val count = when {
        info.downloads < 0 -> ""
        info.maxDownloads > 0 -> " · " + stringResource(Res.string.share_count_of, info.downloads.toString(), info.maxDownloads.toString())
        else -> " · " + stringResource(Res.string.share_count, info.downloads.toString())
    }
    return expiry + count
}

/** All own share links: copy, send, withdraw. */
@Composable
fun SharesScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    var shares by remember { mutableStateOf<List<ShareInfoUi>?>(null) }
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var showing by remember { mutableStateOf<ShareInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    val mail = rememberMailAvailable(state)
    LaunchedEffect(state.liveRevision(), retry) {
        servers = runCatching { net.servers() }.getOrDefault(emptyList())
        runCatching { net.shareList() }.onSuccess { shares = it; error = null }.onFailure { error = describe(it); if (shares == null) shares = emptyList() }
    }
    val withdrawnMsg = stringResource(Res.string.share_withdrawn)
    // Withdrawing asks nothing: the link is gone from the list at once, "Undo" brings it back.
    fun withdraw(s: ShareInfoUi) = state.undoable(listOf(hiddenKey("share", "", s.id)), withdrawnMsg) { net.shareDelete(s.id) }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            LoadError(error) { error = null; retry++ }
            val list = shares?.filter { hiddenKey("share", "", it.id) !in state.hidden }
            when {
                list == null -> LoadingHint()
                list.isEmpty() -> {
                    EmptyState(Icons.Filled.Link, stringResource(Res.string.shares_empty), g.ok)
                    if (servers.none { it.canShare }) {
                        OutlinedButton(onClick = { state.push(Screen.SettingsPage(SetPage.SERVERS)) }, modifier = Modifier.fillMaxWidth()) {
                            Text(stringResource(Res.string.share_setup_title))
                        }
                    }
                }
                else -> ListCard {
                    list.forEachIndexed { i, s ->
                        val server = servers.firstOrNull { it.id == s.serverId }?.name.orEmpty()
                        ListRow(
                            s.name,
                            listOf(formatSize(s.size), server, shareSummary(s)).filter { it.isNotBlank() }.joinToString(" · "),
                            leading = { IconTile(if (s.password) Icons.Filled.Lock else Icons.Filled.Link, g.ok) },
                            trailing = { TextButton(onClick = { withdraw(s) }) { Text(stringResource(Res.string.share_withdraw)) } },
                            divider = i > 0,
                            onClick = { showing = s },
                        )
                    }
                }
            }
            Hint(stringResource(Res.string.shares_hint))
        }
    }
    showing?.let { ShareResultDialog(state, it, mail) { showing = null } }
}

/** Assistant: choose the share folder on the server, enter its web address, check everything. */
@Composable
fun ShareSetupScreen(state: AppState, serverId: String, padding: PaddingValues) {
    val net = state.backend.net
    var server by remember { mutableStateOf<ServerUi?>(null) }
    var path by remember { mutableStateOf("") }
    var dirs by remember { mutableStateOf<List<String>>(emptyList()) }
    var url by remember { mutableStateOf("https://") }
    var checks by remember { mutableStateOf<List<ShareCheckUi>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }
    var newDir by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        server = runCatching { net.servers() }.getOrDefault(emptyList()).firstOrNull { it.id == serverId }
        server?.let { s ->
            if (s.shareDir.isNotEmpty()) path = s.shareDir
            if (s.shareUrl.isNotEmpty()) url = s.shareUrl
        }
    }
    LaunchedEffect(path) {
        error = null
        dirs = try {
            net.serverBrowse(serverId, path)
        } catch (e: Exception) {
            error = e.message
            emptyList()
        }
    }
    val ok = checks.isNotEmpty() && checks.all { it.ok }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.share_setup_step1)) {
                Hint(stringResource(Res.string.share_setup_step1_hint))
                Text("/" + path, fontFamily = FontFamily.Monospace, color = g.ok)
                ListCard {
                    if (path.isNotEmpty()) {
                        ListRow("..", leading = { IconTile(Icons.Filled.Folder, MaterialTheme.colorScheme.onSurfaceVariant, size = 34.dp) }) {
                            path = path.substringBeforeLast('/', "")
                        }
                    }
                    for (d in dirs) {
                        ListRow(d, leading = { IconTile(Icons.Filled.Folder, areaColor(Tab.DRIVE), size = 34.dp) }) {
                            path = if (path.isEmpty()) d else "$path/$d"
                        }
                    }
                }
                OutlinedButton(onClick = { newDir = true }) { Text(stringResource(Res.string.drive_new_folder)) }
            }
            SectionCard(stringResource(Res.string.share_setup_step2)) {
                Hint(stringResource(Res.string.share_setup_step2_hint, "/" + path))
                OutlinedTextField(url, { url = it.trim(); checks = emptyList() }, Modifier.fillMaxWidth(), singleLine = true,
                    label = { Text(stringResource(Res.string.share_setup_url)) },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                Hint(stringResource(Res.string.share_setup_url_example))
            }
            SectionCard(stringResource(Res.string.share_setup_step3)) {
                for (c in checks) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Icon(if (c.ok) Icons.Filled.CheckCircle else Icons.Filled.Error, null, tint = if (c.ok) g.ok else g.danger)
                        Column(Modifier.weight(1f)) {
                            Text(checkLabel(c.step))
                            if (!c.ok && c.detail.isNotBlank()) Hint(c.detail)
                        }
                    }
                }
                if (ok) Hint(stringResource(Res.string.share_setup_ok))
                ErrorText(error)
                Button(
                    onClick = {
                        state.run(onError = { error = it }, block = { net.shareSetup(serverId, path, url) }) {
                            checks = it
                            if (it.all { c -> c.ok }) state.changed()
                        }
                    },
                    enabled = !state.busy && path.isNotEmpty() && url.startsWith("https://") && url.length > 10,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(if (ok) Res.string.share_setup_again else Res.string.share_setup_check)) }
                if (ok) TextButton(onClick = { state.pop() }) { Text(stringResource(Res.string.a_done)) }
            }
        }
    }
    if (newDir) {
        NameDialog(stringResource(Res.string.drive_new_folder), "", onDismiss = { newDir = false }) { name ->
            newDir = false
            path = if (path.isEmpty()) name.trim('/') else "$path/${name.trim('/')}"
        }
    }
}

@Composable
private fun checkLabel(step: String): String = when (step) {
    "write" -> stringResource(Res.string.share_check_write)
    "https" -> stringResource(Res.string.share_check_https)
    "listing" -> stringResource(Res.string.share_check_listing)
    "page" -> stringResource(Res.string.share_check_page)
    else -> step
}

/** Settings → GhulDrive: storage, trash and shares (all devices), photo backup (this device). */
@Composable
fun DriveSettingsSection(state: AppState) {
    val net = state.backend.net
    val settings = state.backend.settings
    var status by remember { mutableStateOf<DriveStatusUi?>(null) }
    LaunchedEffect(state.liveRevision()) { status = runCatching { net.driveStatus() }.getOrNull() }
    SectionCard(areaGroupTitle(de.danerd.ghulsuite.model.VaultKindUi.DRIVE)) {
        DriveStorageRow(state)
        status?.takeIf { it.serverId != null }?.let { Hint(stringResource(Res.string.drive_status, it.files.toString(), formatSize(it.bytes))) }
        ChoiceRow(
            stringResource(Res.string.drive_trash_days),
            listOf(7 to Res.string.drive_days_7, 30 to Res.string.drive_days_30, 90 to Res.string.drive_days_90, 0 to Res.string.drive_days_never),
            settings.driveTrashDays,
        ) { settings.driveTrashDays = it }
        Hint(stringResource(Res.string.ctx_shares_sub))
        OutlinedButton(onClick = { state.push(Screen.Shares) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.shares_title)) }
        Hint(stringResource(Res.string.drive_crypto_hint))
    }
    if (net.photoBackupAvailable) {
        SectionCard(deviceGroupTitle()) {
            val ask = rememberPhotoPermission { granted -> settings.photoBackup = granted; if (granted) state.runPhotoBackup() }
            SwitchRow(stringResource(Res.string.drive_photo_backup), settings.photoBackup) { on -> if (on) ask() else settings.photoBackup = false }
            Hint(stringResource(Res.string.drive_photo_backup_sub))
            SwitchRow(stringResource(Res.string.drive_wifi_only), settings.photoBackupWifiOnly) { settings.photoBackupWifiOnly = it }
        }
    }
}
