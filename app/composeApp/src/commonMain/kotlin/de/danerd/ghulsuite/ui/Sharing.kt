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
import androidx.compose.material.icons.filled.RadioButtonUnchecked
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
import androidx.compose.runtime.produceState
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
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.ModuleCheckUi
import de.danerd.ghulsuite.model.ModuleInstalledUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.rememberResourceEnvironment
import org.jetbrains.compose.resources.stringResource

private val Blue = Color(0xFF2F74E8)

/** Expiry choices in days; 0 = never. */
private val EXPIRY = listOf(1, 7, 30, 0)
private val MAX_DOWNLOADS = listOf(0, 1, 3, 5, 10)
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
        available = enabled && catching { state.backend.net.mailAccounts() }.getOrDefault(emptyList()).isNotEmpty()
    }
    return available && enabled
}

/** Opens the mail module with prefilled content (only called when [rememberMailAvailable]). */
fun AppState.composeMail(subject: String = "", text: String = "", attachments: List<de.danerd.ghulsuite.model.OutgoingAttachmentUi> = emptyList()) {
    push(Screen.MailCompose(MailDraft(mailAccount, subject = subject, text = text, attachments = attachments)))
}

/** What share links still need: nothing, GhulDrive, or the server module on GhulDrive's server. */
sealed interface ShareNeed {
    data object Ready : ShareNeed
    data object Drive : ShareNeed
    data class Module(val serverId: String) : ShareNeed
}

/** Whether share links can be made (null while the core is asked). */
@Composable
fun rememberShareNeed(state: AppState): ShareNeed? {
    var need by remember { mutableStateOf<ShareNeed?>(null) }
    LaunchedEffect(state.liveRevision()) {
        catching { state.backend.net.shareMissingModule() }.onSuccess { missing ->
            need = when {
                missing == null -> ShareNeed.Ready
                missing.isEmpty() -> ShareNeed.Drive
                else -> ShareNeed.Module(missing)
            }
        }
    }
    return need
}

/** Subtitle of a "Share link" row: what it does, or what is still missing for it. */
@Composable
fun shareLinkSub(need: ShareNeed?): String = when (need) {
    is ShareNeed.Module -> stringResource(Res.string.share_link_needs_module)
    ShareNeed.Drive -> stringResource(Res.string.share_link_needs_drive)
    else -> stringResource(Res.string.share_link_sub)
}

/** Share Drive files: to another app, as a link, or as mail attachment. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareSheet(state: AppState, files: List<DriveItemUi>, onDismiss: () -> Unit, onDone: () -> Unit, onZip: (() -> Unit)? = null) {
    val net = state.backend.net
    val need = rememberShareNeed(state)
    var linkMode by remember { mutableStateOf(false) }
    var result by remember { mutableStateOf<ShareInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val mail = rememberMailAvailable(state)
    val onlyFiles = files.none { it.isDir }
    // One file: its name is the link's title. Several or a folder: the user can name the link.
    val single = files.size == 1 && onlyFiles
    var linkTitle by remember { mutableStateOf(if (files.size == 1) files[0].name else "GhulDrive") }
    val title = if (files.size == 1) files[0].name else pluralStringResource(Res.plurals.share_n_items, files.size, files.size.toString())

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
                    shareLinkSub(need),
                    leading = { IconTile(Icons.Filled.Link, paletteColor(5)) },
                    end = { ChevronEnd() },
                    divider = onlyFiles,
                    enabled = need != ShareNeed.Drive,
                ) {
                    when (need) {
                        ShareNeed.Ready -> linkMode = true
                        is ShareNeed.Module -> { onDismiss(); state.push(Screen.ModuleInstall(need.serverId)) }
                        else -> {}
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
            if (!single) {
                OutlinedTextField(linkTitle, { linkTitle = it }, Modifier.fillMaxWidth(), singleLine = true,
                    label = { Text(stringResource(Res.string.share_title_label)) })
                Hint(stringResource(Res.string.share_each_file))
            }
            ShareOptionsForm(state, onError = { error = it }) { options ->
                state.run(onError = { error = it }, block = {
                    net.shareDrive(files.map { it.id }, if (single) null else linkTitle.trim().ifEmpty { "GhulDrive" }, options)
                }) { result = it }
            }
            ErrorText(error)
            OutlinedButton(onClick = { linkMode = false }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_back)) }
        }
    }
}

/** Expiry, download limit and password of a new share link (the server module ends it by itself). */
@Composable
fun ShareOptionsForm(
    state: AppState,
    onError: (String?) -> Unit,
    defaultDays: Int = 7,
    defaultMax: Int = 0,
    onCreate: (ShareOptionsUi) -> Unit,
) {
    var days by remember { mutableStateOf(defaultDays) }
    var max by remember { mutableStateOf(defaultMax) }
    var usePassword by remember { mutableStateOf(false) }
    var password by remember { mutableStateOf("") }

    SectionLabel(stringResource(Res.string.share_expires))
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (d in EXPIRY) FilterChip(days == d, { days = d }, label = { Text(expiryLabel(d)) })
    }
    SectionLabel(stringResource(Res.string.share_max))
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (m in MAX_DOWNLOADS) {
            val label = when (m) {
                0 -> stringResource(Res.string.share_unlimited)
                1 -> stringResource(Res.string.share_once)
                else -> m.toString() + "×"
            }
            FilterChip(max == m, { max = m }, label = { Text(label) })
        }
    }
    SwitchRow(stringResource(Res.string.share_password), usePassword) { usePassword = it }
    if (usePassword) {
        SecretTextField(password, { password = it; onError(null) }, stringResource(Res.string.share_password))
        Hint(stringResource(Res.string.share_password_hint))
    }
    Hint(stringResource(Res.string.share_e2e_hint))
    Button(
        onClick = { onCreate(ShareOptionsUi(password.takeIf { usePassword && it.isNotEmpty() }, days, max)) },
        enabled = !state.busy && (!usePassword || password.length >= 4),
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
    else -> pluralStringResource(Res.plurals.share_days, days, days.toString())
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
        info.maxDownloads > 0 -> " · " + pluralStringResource(Res.plurals.share_count_of, info.maxDownloads, info.downloads.toString(), info.maxDownloads.toString())
        else -> " · " + pluralStringResource(Res.plurals.share_count, info.downloads.pluralCount(), info.downloads.toString())
    }
    return expiry + count
}

/** All own share links: copy, send, withdraw. */
@Composable
fun SharesScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    var shares by remember { mutableStateOf<List<ShareInfoUi>?>(null) }
    var showing by remember { mutableStateOf<ShareInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    val mail = rememberMailAvailable(state)
    val need = rememberShareNeed(state)
    LaunchedEffect(state.liveRevision(), retry) {
        catching { net.shareList() }.onSuccess { shares = it; error = null }.onFailure { error = describe(it); if (shares == null) shares = emptyList() }
    }
    val withdrawnMsg = stringResource(Res.string.share_withdrawn)
    // F103: withdrawing asks first; the link is then gone from the list, "Undo" brings it back.
    fun withdraw(s: ShareInfoUi) = state.askDelete(listOf(s.name)) {
        state.undoable(listOf(hiddenKey("share", "", s.id)), withdrawnMsg) { net.shareDelete(s.id) }
    }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            LoadError(error) { error = null; retry++ }
            val list = shares?.filter { hiddenKey("share", "", it.id) !in state.hidden }
            when {
                list == null -> LoadingHint()
                list.isEmpty() -> {
                    EmptyState(Icons.Filled.Link, stringResource(Res.string.shares_empty), g.ok)
                    when (need) {
                        is ShareNeed.Module -> {
                            Hint(stringResource(Res.string.share_link_needs_module))
                            OutlinedButton(onClick = { state.push(Screen.ModuleInstall(need.serverId)) }, modifier = Modifier.fillMaxWidth()) {
                                Text(stringResource(Res.string.module_install))
                            }
                        }
                        ShareNeed.Drive -> Hint(stringResource(Res.string.share_link_needs_drive))
                        else -> {}
                    }
                }
                else -> ListCard {
                    list.forEachIndexed { i, s ->
                        ListRow(
                            s.name,
                            listOf(formatSize(s.size), shareSummary(s)).filter { it.isNotBlank() }.joinToString(" · "),
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

/**
 * Installs the GhulSuite server module on a server over its own login: choose the website
 * folder, optionally its address, then "Install and check" shows each step.
 */
@Composable
fun ModuleInstallScreen(state: AppState, serverId: String, padding: PaddingValues) {
    val net = state.backend.net
    var server by remember { mutableStateOf<ServerUi?>(null) }
    var path by remember { mutableStateOf("") }
    var dirs by remember { mutableStateOf<List<String>>(emptyList()) }
    var url by remember { mutableStateOf("") }
    var result by remember { mutableStateOf<ModuleInstalledUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var newDir by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { server = catching { net.knownServers() }.getOrDefault(emptyList()).firstOrNull { it.id == serverId } }
    LaunchedEffect(path) {
        error = null
        dirs = catching { net.serverBrowse(serverId, path) }.getOrElse {
            error = describe(it)
            emptyList()
        }
    }
    val r = result
    val done = r?.url != null
    val urlOk = url.isEmpty() || url.startsWith("https://") && url.length > 10
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(server?.let { "${it.name} · ${kindLabel(it.kind)}" }) {
                Hint(stringResource(Res.string.module_intro))
            }
            SectionCard(stringResource(Res.string.module_step1)) {
                Hint(stringResource(Res.string.module_step1_hint))
                Text("/" + path, fontFamily = FontFamily.Monospace, color = g.ok)
                ListCard {
                    if (path.isNotEmpty()) {
                        ListRow("..", leading = { IconTile(Icons.Filled.Folder, MaterialTheme.colorScheme.onSurfaceVariant, size = 34.dp) }) {
                            path = path.substringBeforeLast('/', "")
                            result = null
                        }
                    }
                    for (d in dirs) {
                        ListRow(d, leading = { IconTile(Icons.Filled.Folder, areaColor(Tab.DRIVE), size = 34.dp) }, divider = path.isNotEmpty() || d != dirs.first()) {
                            path = if (path.isEmpty()) d else "$path/$d"
                            result = null
                        }
                    }
                }
                OutlinedButton(onClick = { newDir = true }) { Text(stringResource(Res.string.drive_new_folder)) }
            }
            SectionCard(stringResource(Res.string.module_step2)) {
                OutlinedTextField(url, { url = it.trim(); result = result?.takeIf { it.url != null } }, Modifier.fillMaxWidth(), singleLine = true,
                    label = { Text(stringResource(Res.string.module_url)) },
                    placeholder = { Text("https://example.org/ghulsuite.php") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                Hint(stringResource(if (urlOk) Res.string.module_url_hint else Res.string.core_https_only))
            }
            SectionCard(stringResource(Res.string.module_step3)) {
                r?.checks?.forEach { c -> ModuleCheckRow(c, r.tried) }
                if (done) Banner(Icons.Filled.CheckCircle, stringResource(Res.string.module_ok, r?.url.orEmpty()))
                ErrorText(error)
                if (done) {
                    Button(onClick = { state.pop() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_done)) }
                } else {
                    Button(
                        onClick = {
                            state.run(onError = { error = it }, block = { net.serverModuleInstall(serverId, path, url) }) {
                                result = it
                                if (it.url != null) state.changed()
                            }
                        },
                        enabled = !state.busy && urlOk,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(stringResource(Res.string.module_install_check)) }
                }
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

/** One step of the module installation: green done, red failed (with why), grey not checked. */
@Composable
private fun ModuleCheckRow(c: ModuleCheckUi, tried: List<String>) {
    // "skipped" (SHARE_CHECK_SKIPPED in the core's shares.rs): the https step failed, so this
    // step could not be checked.
    val skipped = !c.ok && c.detail == "skipped"
    Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(
            when { c.ok -> Icons.Filled.CheckCircle; skipped -> Icons.Filled.RadioButtonUnchecked; else -> Icons.Filled.Error },
            null, tint = when { c.ok -> g.ok; skipped -> g.faint; else -> g.danger },
        )
        Column(Modifier.weight(1f)) {
            Text(moduleCheckLabel(c.step), color = if (skipped) g.muted else Color.Unspecified)
            when {
                skipped -> Hint(stringResource(Res.string.share_check_skipped))
                c.ok && c.detail.isNotBlank() -> Hint(c.detail)
                c.ok -> {}
                c.step == "https" -> Hint(stringResource(Res.string.module_https_failed, tried.ifEmpty { listOf(c.detail) }.joinToString(", ")))
                c.step == "php" -> Hint(stringResource(Res.string.module_php_failed))
                // "no-write" (MODULE_NO_WRITE in the core's servers.rs): reads, may not write.
                c.step == "folder" && c.detail == "no-write" -> Hint(stringResource(Res.string.module_folder_no_write))
                c.step == "folder" -> Hint(stringResource(Res.string.module_folder_failed))
                // The address where the web server hands out the GhulSuite folder itself.
                c.step == "private" -> Hint(stringResource(Res.string.module_private_failed, c.detail))
                c.detail.isNotBlank() -> {
                    // Known causes ("server address not found") in the app's language.
                    val env = rememberResourceEnvironment()
                    val detail by produceState(c.detail, c.detail) { value = coreText(env, c.detail) ?: c.detail }
                    Hint(detail)
                }
            }
        }
    }
}

@Composable
private fun moduleCheckLabel(step: String): String = when (step) {
    "upload" -> stringResource(Res.string.module_check_upload)
    "https" -> stringResource(Res.string.module_check_https)
    "php" -> stringResource(Res.string.module_check_php)
    "folder" -> stringResource(Res.string.module_check_folder)
    "private" -> stringResource(Res.string.module_check_private)
    else -> step
}

/** The suite's servers and GhulDrive's own server (kept in the Drive file, not in the list). */
suspend fun de.danerd.ghulsuite.model.NetBackend.knownServers(): List<ServerUi> {
    val list = servers()
    val drive = catching { driveStorage() }.getOrNull()?.server
    return if (drive == null || list.any { it.id == drive.id }) list else list + drive
}

/** Settings → GhulDrive: storage, trash and shares. */
@Composable
fun DriveSettingsSection(state: AppState) {
    val net = state.backend.net
    val settings = state.backend.settings
    var status by remember { mutableStateOf<DriveStatusUi?>(null) }
    LaunchedEffect(state.liveRevision()) { status = catching { net.driveStatus() }.getOrNull() }
    SectionCard(areaGroupTitle(de.danerd.ghulsuite.model.VaultKindUi.DRIVE)) {
        DriveStorageRow(state)
        status?.takeIf { it.serverId != null }?.let { Hint(pluralStringResource(Res.plurals.drive_status, it.files, it.files.toString(), formatSize(it.bytes))) }
        ChoiceRow(
            stringResource(Res.string.drive_trash_days),
            listOf(7 to Res.string.drive_days_7, 30 to Res.string.drive_days_30, 90 to Res.string.drive_days_90, 0 to Res.string.drive_days_never),
            settings.driveTrashDays,
        ) { settings.driveTrashDays = it }
        Hint(stringResource(Res.string.ctx_shares_sub))
        OutlinedButton(onClick = { state.push(Screen.Shares) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.shares_title)) }
        Hint(stringResource(Res.string.drive_crypto_hint))
        // B3: after a reset of the server (old GhulDrive put back) GhulDrive refuses to write.
        var accept by remember { mutableStateOf(false) }
        TextButton(onClick = { accept = true }) { Text(stringResource(Res.string.drive_accept_state)) }
        Hint(stringResource(Res.string.drive_accept_state_hint))
        if (accept) {
            AlertDialog(
                danger = true,
                onDismissRequest = { accept = false },
                title = { Text(stringResource(Res.string.drive_accept_state)) },
                text = { Text(stringResource(Res.string.drive_accept_state_text)) },
                confirmButton = {
                    TextButton(onClick = {
                        accept = false
                        state.run(block = { net.driveAcceptServerState() }) { state.changed() }
                    }) { Text(stringResource(Res.string.a_take_over)) }
                },
                dismissButton = { TextButton(onClick = { accept = false }) { Text(stringResource(Res.string.a_cancel)) } },
            )
        }
    }
}
