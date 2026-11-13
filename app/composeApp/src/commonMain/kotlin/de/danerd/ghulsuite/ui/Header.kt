// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Report
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Close
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.sp
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.DriveStatusUi
import de.danerd.ghulsuite.model.MailAccountUi
import de.danerd.ghulsuite.model.MailFolderUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

/** The line under the area title: what is shown right now, in the area's colour; tap to switch. */
@Composable
fun AreaContext(state: AppState) {
    val net = state.backend.net
    var label by remember { mutableStateOf("") }
    val all = stringResource(Res.string.vault_all)
    val allAccounts = stringResource(Res.string.ctx_all_codes)
    val notSet = stringResource(Res.string.ctx_not_set_up)
    val suite = stringResource(Res.string.set_my_suite)
    val inbox = stringResource(Res.string.mail_inbox)
    val allVaults = stringResource(Res.string.vault_all_vaults)
    val accountsWord = stringResource(Res.string.auth_count)
    val myFiles = stringResource(Res.string.drive_my_files)
    val allInboxes = stringResource(Res.string.mail_all_inboxes)
    val allCalendars = stringResource(Res.string.cal_all_calendars)
    val someCalendars = stringResource(Res.string.cal_some_calendars)
    LaunchedEffect(state.tab, state.revision, state.passVault, state.authVault, state.mailAccount, state.mailFolder, state.calendars, state.backend.settings.calHidden) {
        label = when (state.tab) {
            Tab.PASS -> state.vaults.firstOrNull { it.id == state.passVault }?.name ?: allVaults
            Tab.AUTH -> {
                val n = runCatching { state.backend.totpCodes(nowSeconds()) }.getOrDefault(emptyList()).count { state.authVault == null || it.vaultId == state.authVault }
                (state.vaults.firstOrNull { it.id == state.authVault }?.name ?: allAccounts) + " · " + accountsWord.replace("%1\$s", n.toString())
            }
            Tab.DRIVE -> {
                val st = runCatching { net.driveStatus() }.getOrNull()
                val srv = runCatching { net.servers() }.getOrDefault(emptyList()).firstOrNull { it.id == st?.serverId }
                if (srv == null) notSet else "${srv.name} · $myFiles"
            }
            Tab.MAIL -> {
                val acc = runCatching { net.mailAccounts() }.getOrDefault(emptyList()).firstOrNull { it.id == state.mailAccount }
                when {
                    state.mailAccount == ALL_INBOXES -> allInboxes
                    acc == null -> notSet
                    else -> "${acc.name} · " + if (state.mailFolder.equals("INBOX", true)) inbox else state.mailFolder
                }
            }
            Tab.CAL -> {
                val cals = state.calendars
                val hidden = state.backend.settings.calHiddenSet()
                val visible = cals.count { it.id !in hidden }
                if (visible == cals.size) allCalendars else someCalendars.replace("%1\$s", visible.toString()).replace("%2\$s", cals.size.toString())
            }
            Tab.SETTINGS -> "$suite · Version " + state.backend.appVersion.substringBefore("-").split('.').take(2).joinToString(".")
        }
    }
    val clickable = state.tab != Tab.SETTINGS
    Row(
        Modifier.then(if (clickable) Modifier.clickable { state.contextSheet = true } else Modifier),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
            color = areaColor(state.tab),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (clickable) Icon(Icons.Filled.KeyboardArrowDown, null, Modifier.padding(start = 2.dp).size(16.dp), tint = areaColor(state.tab))
    }
}

@Composable
private fun CtxRow(title: String, subtitle: String? = null, selected: Boolean = false, divider: Boolean = false, leading: @Composable () -> Unit, onClick: () -> Unit) {
    SheetRow(
        title, subtitle, leading = leading, divider = divider,
        end = { if (selected) Icon(Icons.Filled.Check, null, Modifier.size(20.dp), tint = g.muted) },
        onClick = onClick,
    )
}

/** Bottom sheet behind the context line: vault filter, storage, mailbox – the same pattern everywhere. */
@Composable
fun AreaContextSheet(state: AppState) {
    val close = { state.contextSheet = false }
    when (state.tab) {
        Tab.PASS, Tab.AUTH -> VaultSheet(state, if (state.tab == Tab.PASS) VaultKindUi.PASS else VaultKindUi.TOTP, close)
        Tab.DRIVE -> DriveSheet(state, close)
        Tab.MAIL -> MailboxSheet(state, close)
        Tab.CAL -> CalendarsSheet(state, close)
        Tab.SETTINGS -> close()
    }
}

@Composable
private fun VaultSheet(state: AppState, kind: VaultKindUi, close: () -> Unit) {
    val selected = if (kind == VaultKindUi.PASS) state.passVault else state.authVault
    val pick = { id: String? -> if (kind == VaultKindUi.PASS) state.passVault = id else state.authVault = id; close() }
    GhulSheet(close, stringResource(Res.string.ctx_choose_vault)) {
        SheetList {
            CtxRow(stringResource(Res.string.vault_all_vaults), selected = selected == null, leading = { IconTile(Icons.Filled.Layers, paletteColor(0)) }) { pick(null) }
            for (v in state.vaultsOf(kind)) {
                CtxRow(v.name, stringResource(Res.string.ctx_entries, v.entries.toString()), selected = selected == v.id, divider = true, leading = { Avatar(v.name) }) { pick(v.id) }
            }
            CtxRow(stringResource(Res.string.set_vaults), divider = true, leading = { IconTile(Icons.Filled.Add, paletteColor(5)) }) {
                close(); state.push(Screen.SettingsPage(SetPage.VAULTS))
            }
        }
    }
}

@Composable
private fun DriveSheet(state: AppState, close: () -> Unit) {
    val net = state.backend.net
    var status by remember { mutableStateOf<DriveStatusUi?>(null) }
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    LaunchedEffect(Unit) {
        status = runCatching { net.driveStatus() }.getOrNull()
        servers = runCatching { net.servers() }.getOrDefault(emptyList())
    }
    val srv = servers.firstOrNull { it.id == status?.serverId }
    GhulSheet(close, stringResource(Res.string.ctx_storage)) {
        SheetList {
            if (srv != null) {
                CtxRow(srv.name, srv.location.ifBlank { srv.host } + "/drive", selected = true, leading = { IconTile(Icons.Filled.Dns, paletteColor(1)) }) { close() }
            }
            CtxRow(stringResource(Res.string.shares_title), stringResource(Res.string.ctx_shares_sub), divider = srv != null, leading = { IconTile(Icons.Filled.Link, paletteColor(5)) }) {
                close(); state.push(Screen.Shares)
            }
            CtxRow(stringResource(Res.string.set_servers_sync_share), divider = true, leading = { IconTile(Icons.Filled.Add, paletteColor(5)) }) {
                close(); state.push(Screen.SettingsPage(SetPage.SERVERS))
            }
        }
    }
}

@Composable
private fun MailboxSheet(state: AppState, close: () -> Unit) {
    val net = state.backend.net
    var accounts by remember { mutableStateOf<List<MailAccountUi>>(emptyList()) }
    var folders by remember { mutableStateOf<List<MailFolderUi>>(emptyList()) }
    LaunchedEffect(state.mailAccount) {
        accounts = runCatching { net.mailAccounts() }.getOrDefault(emptyList())
        folders = state.mailAccount?.let { runCatching { net.mailFolders(it) }.getOrDefault(emptyList()) } ?: emptyList()
    }
    val mail = areaColor(Tab.MAIL)
    val t = g
    GhulSheet(close, stringResource(Res.string.ctx_mailbox)) {
        // Accounts and folders as pill rows (.mailbox): the chosen one tinted in the mail colour.
        @Composable
        fun Pill(on: Boolean, leading: @Composable () -> Unit, title: String, sub: String? = null, onClick: () -> Unit) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(24.dp))
                    .background(if (on) mail.copy(alpha = 0.16f) else androidx.compose.ui.graphics.Color.Transparent)
                    .clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                leading()
                Column(Modifier.weight(1f)) {
                    Text(title, color = if (on) mail else t.fg, fontWeight = if (on) FontWeight.Bold else FontWeight.Normal)
                    if (sub != null) Text(sub, fontSize = 14.sp, color = t.muted)
                }
            }
        }
        var drafts by remember { mutableStateOf<List<de.danerd.ghulsuite.model.MailDraftUi>>(emptyList()) }
        var scheduled by remember { mutableStateOf<List<de.danerd.ghulsuite.model.ScheduledMailUi>>(emptyList()) }
        var showDrafts by remember { mutableStateOf(false) }
        var showScheduled by remember { mutableStateOf(false) }
        LaunchedEffect(Unit) {
            drafts = runCatching { net.mailDrafts() }.getOrDefault(emptyList())
            scheduled = runCatching { net.mailScheduled() }.getOrDefault(emptyList())
        }
        if (accounts.size > 1) {
            Pill(state.mailAccount == ALL_INBOXES, { Icon(Icons.Filled.Inbox, null, Modifier.size(20.dp), tint = if (state.mailAccount == ALL_INBOXES) mail else t.fg) },
                stringResource(Res.string.mail_all_inboxes)) {
                state.mailAccount = ALL_INBOXES
                state.mailFolder = "INBOX"
                close()
            }
        }
        for (a in accounts) {
            Pill(a.id == state.mailAccount, { Avatar(a.name, size = 34.dp) }, a.name, a.email) {
                state.mailAccount = a.id
                state.mailFolder = "INBOX"
            }
        }
        if (folders.isNotEmpty()) {
            Text(stringResource(Res.string.ctx_folders).uppercase(), fontSize = 12.sp, letterSpacing = 0.7.sp, fontWeight = FontWeight.SemiBold,
                color = t.muted, modifier = Modifier.padding(start = 14.dp, top = 10.dp))
            for (f in folders) {
                val on = f.name == state.mailFolder
                Pill(on, { Icon(folderIcon(f.role), null, Modifier.size(20.dp), tint = if (on) mail else t.fg) }, folderLabelOf(f)) {
                    state.mailFolder = f.name
                    close()
                }
            }
        }
        Text("", modifier = Modifier.padding(top = 4.dp))
        Pill(showDrafts, { Icon(Icons.Filled.Description, null, Modifier.size(20.dp), tint = t.fg) },
            stringResource(Res.string.mail_drafts_local) + if (drafts.isNotEmpty()) " (${drafts.size})" else "") { showDrafts = !showDrafts }
        if (showDrafts) {
            if (drafts.isEmpty()) HintText(stringResource(Res.string.mail_drafts_none), Modifier.padding(start = 14.dp))
            for (d in drafts) {
                Pill(false, { Avatar(d.to.ifBlank { "?" }, size = 30.dp) }, d.subject.ifBlank { "–" }, d.to.ifBlank { formatDateTime(d.updated) }) {
                    close()
                    state.push(Screen.MailCompose(MailDraft(d.accountId, d.to, d.cc, d.subject, d.text, d.inReplyTo, attachments = d.attachments, draftId = d.id, bcc = d.bcc)))
                }
            }
        }
        Pill(showScheduled, { Icon(Icons.Filled.Schedule, null, Modifier.size(20.dp), tint = t.fg) },
            stringResource(Res.string.mail_scheduled) + if (scheduled.isNotEmpty()) " (${scheduled.size})" else "") { showScheduled = !showScheduled }
        if (showScheduled) {
            if (scheduled.isEmpty()) HintText(stringResource(Res.string.mail_scheduled_none), Modifier.padding(start = 14.dp))
            for (m in scheduled) {
                Row(Modifier.fillMaxWidth().padding(start = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(m.subject.ifBlank { "–" }, fontWeight = FontWeight.W500, maxLines = 1)
                        Text(formatDateTime(m.sendAt) + " · " + m.to.joinToString(", "), fontSize = 13.sp, color = t.muted, maxLines = 1)
                    }
                    BarIcon(Icons.Filled.Close, stringResource(Res.string.a_cancel)) {
                        state.run(block = { net.mailCancelScheduled(m.id) }) { scheduled = scheduled - m }
                    }
                }
            }
        }
        Pill(false, { Icon(Icons.Filled.Add, null, Modifier.size(20.dp), tint = t.fg) }, stringResource(Res.string.mail_account_new)) {
            close(); state.mailAccountSheet = MailSheet(null)
        }
        Pill(false, { Icon(Icons.Filled.Tune, null, Modifier.size(20.dp), tint = t.fg) }, stringResource(Res.string.mail_settings_row)) {
            close(); state.push(Screen.SettingsPage(SetPage.MAIL))
        }
    }
}

@Composable
fun folderLabelOf(f: MailFolderUi): String = when (f.role) {
    "inbox" -> stringResource(Res.string.mail_inbox)
    "sent" -> stringResource(Res.string.mail_sent)
    "drafts" -> stringResource(Res.string.mail_drafts)
    "trash" -> stringResource(Res.string.mail_trash)
    "junk" -> stringResource(Res.string.mail_junk)
    "archive" -> stringResource(Res.string.mail_archive)
    else -> f.name
}
