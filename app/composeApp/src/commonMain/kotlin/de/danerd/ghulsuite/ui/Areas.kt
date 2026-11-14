// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.DriveModeUi
import de.danerd.ghulsuite.model.DriveStorageUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

fun areaInfo(kind: VaultKindUi): AreaInfo = AREAS.first { it.kind == kind }

/** Whether this device shows the area (its file stays either way). */
fun moduleShown(state: AppState, kind: VaultKindUi): Boolean {
    val s = state.backend.settings
    return when (kind) {
        VaultKindUi.PASS -> s.moduleGhulPass
        VaultKindUi.TOTP -> s.moduleGhulAuth
        VaultKindUi.DRIVE -> s.moduleGhulDrive
        VaultKindUi.MAIL -> s.moduleGhulMail
        VaultKindUi.CAL -> s.moduleGhulCal
    }
}

/** Card at the top of an area that is not set up yet: icon, title, explanation. */
@Composable
fun AreaSetupCard(tab: Tab, title: String, text: String) {
    SectionCard {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            IconTile(tabIcon(tab), areaColor(tab), size = 56.dp)
            Text(title, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Text(text, fontSize = 14.sp, color = g.muted)
        }
    }
}

/** Title of the group of an area page whose settings live in the area's file. */
@Composable
fun areaGroupTitle(kind: VaultKindUi): String = stringResource(Res.string.area_group_all, stringResource(areaInfo(kind).title))

@Composable
fun deviceGroupTitle(): String = stringResource(Res.string.area_group_device)

/** Note under an area page: what is stored where. */
@Composable
fun AreaFileNote(kind: VaultKindUi) {
    HintText(stringResource(Res.string.area_file_note, areaInfo(kind).ext))
}

/** "Hide on this device" and "Remove area" at the end of an area page. */
@Composable
fun AreaFooter(state: AppState, kind: VaultKindUi) {
    var removing by remember { mutableStateOf(false) }
    val hidden = !moduleShown(state, kind)
    val hiddenMsg = stringResource(Res.string.area_hidden_msg, stringResource(areaInfo(kind).title))
    val shownMsg = stringResource(Res.string.area_shown_msg, stringResource(areaInfo(kind).title))
    SectionLabel(stringResource(Res.string.area_section))
    ListCard {
        ListRow(
            stringResource(if (hidden) Res.string.area_show else Res.string.area_hide), stringResource(Res.string.area_hide_sub),
            trailing = { Icon(if (hidden) Icons.Filled.Visibility else Icons.Filled.VisibilityOff, null, tint = g.muted) },
        ) {
            showModule(state, kind, hidden)
            state.message(if (hidden) shownMsg else hiddenMsg)
            if (!hidden) state.pop()
        }
        ListRow(
            stringResource(Res.string.area_remove), stringResource(Res.string.area_remove_sub, areaInfo(kind).ext),
            trailing = { Icon(Icons.Filled.Delete, null, tint = dangerColor()) }, divider = true,
        ) { removing = true }
    }
    if (removing) RemoveAreaDialog(state, kind, onDone = { removing = false; state.pop() }) { removing = false }
}

/** Asks before an area's file is deleted on every device. */
@Composable
fun RemoveAreaDialog(state: AppState, kind: VaultKindUi, onDone: () -> Unit, onDismiss: () -> Unit) {
    val name = stringResource(areaInfo(kind).title)
    val removed = stringResource(Res.string.area_removed, name)
    // U4: how much goes, a way to save it first, and the name typed out instead of one tap.
    val entries = state.vaultsOf(kind).sumOf { it.entries }
    var typed by remember { mutableStateOf("") }
    AlertDialog(
        danger = true,
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.area_remove_title, name)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Banner(Icons.Filled.Warning, stringResource(Res.string.area_remove_warning, areaInfo(kind).ext))
                if (entries > 0) Text(stringResource(Res.string.remove_count, entries.toString()))
                Text(stringResource(Res.string.area_remove_tip))
                OutlinedButton(onClick = { onDismiss(); state.push(Screen.Export) }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.remove_export_first))
                }
                ConfirmByName(name, typed) { typed = it }
            }
        },
        confirmButton = {
            TextButton(enabled = nameMatches(typed, name), onClick = {
                state.run(block = { state.backend.removeArea(kind) }) {
                    showModule(state, kind, false)
                    state.message(removed)
                    state.changed()
                    onDone()
                }
            }) { Text(stringResource(Res.string.area_remove)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Options → Areas: every area with its state; set up, hide, remove. */
@Composable
fun AreasPage(state: AppState) {
    var sheet by remember { mutableStateOf<VaultKindUi?>(null) }
    ListCard {
        AREAS.forEachIndexed { i, a ->
            val exists = state.vaults.any { it.kind == a.kind }
            val shown = moduleShown(state, a.kind)
            val names = state.vaults.filter { it.kind == a.kind }.joinToString(", ") { it.name + a.ext }
            ListRow(
                stringResource(a.title), if (exists) names else stringResource(a.sub),
                leading = { IconTile(tabIcon(a.tab), areaColor(a.tab)) },
                trailing = {
                    when {
                        !exists -> Tag(stringResource(Res.string.area_tag_not_set_up))
                        !shown -> Tag(stringResource(Res.string.area_tag_hidden), warnColor())
                        else -> Tag(stringResource(Res.string.area_tag_active), okColor(), Icons.Filled.CheckCircle)
                    }
                },
                divider = i > 0,
            ) { sheet = a.kind }
        }
    }
    HintText(stringResource(Res.string.areas_hint))
    sheet?.let { kind -> AreaSheet(state, kind) { sheet = null } }
}

/** One area: set it up (no file yet) or open, hide, remove it. */
@Composable
private fun AreaSheet(state: AppState, kind: VaultKindUi, onDismiss: () -> Unit) {
    val a = areaInfo(kind)
    val exists = state.vaults.any { it.kind == kind }
    val title = stringResource(a.title)
    var removing by remember { mutableStateOf(false) }
    if (removing) {
        RemoveAreaDialog(state, kind, onDone = { removing = false; onDismiss() }) { removing = false }
        return
    }
    if (!exists) {
        var name by remember { mutableStateOf("") }
        var error by remember { mutableStateOf<String?>(null) }
        var storage by remember { mutableStateOf<DriveStorageUi?>(null) }
        var own by remember { mutableStateOf(false) }
        LaunchedEffect(kind) {
            name = getString(when (kind) { VaultKindUi.PASS -> Res.string.area_pass_default; VaultKindUi.TOTP -> Res.string.area_auth_default; else -> Res.string.area_cal_default })
            if (kind == VaultKindUi.DRIVE) storage = runCatching { state.backend.net.driveStorage() }.getOrNull()
        }
        val syncOn = storage?.syncAvailable == true
        var useSync by remember(syncOn) { mutableStateOf(syncOn) }
        val done = stringResource(Res.string.area_set_up_msg, title)
        if (own) {
            DriveServerSheet(state) { own = false; showModule(state, kind, true); onDismiss() }
            return
        }
        GhulSheet(onDismiss, stringResource(Res.string.area_set_up_title, title)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                IconTile(tabIcon(a.tab), areaColor(a.tab), size = 56.dp)
                HintText(stringResource(Res.string.area_set_up_text, stringResource(a.sub), (if (kind in NAMED_AREAS) name else title.removePrefix("Ghul")) + a.ext), Modifier.weight(1f))
            }
            if (kind in NAMED_AREAS) {
                OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true)
            }
            if (kind == VaultKindUi.DRIVE) {
                SheetList {
                    if (syncOn) PickRow(useSync, Icons.Filled.Sync, 2, stringResource(Res.string.drive_like_sync), stringResource(Res.string.drive_like_sync_sub), onClick = { useSync = true })
                    PickRow(!useSync, Icons.Filled.Dns, 1, stringResource(Res.string.wz_own_server), stringResource(Res.string.drive_own_sub), divider = syncOn, onClick = { useSync = false })
                }
            }
            if (kind == VaultKindUi.MAIL) HintText(stringResource(Res.string.area_mail_next))
            ErrorText(error)
            Button(onClick = {
                if (kind == VaultKindUi.DRIVE && !useSync) { own = true; return@Button }
                state.run(onError = { error = it }, block = {
                    state.backend.addArea(kind, if (kind in NAMED_AREAS) name else "")
                    if (kind == VaultKindUi.DRIVE && useSync) state.backend.net.driveUseSync()
                }) {
                    showModule(state, kind, true)
                    state.message(done)
                    state.changed()
                    onDismiss()
                }
            }, enabled = !state.busy && (kind !in NAMED_AREAS || name.isNotBlank()), modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.area_set_up))
            }
        }
        return
    }
    val shown = moduleShown(state, kind)
    GhulSheet(onDismiss, title) {
        SheetList {
            SheetRow(stringResource(Res.string.area_settings)) {
                onDismiss()
                state.push(Screen.SettingsPage(areaPage(kind)))
            }
            SheetRow(stringResource(if (shown) Res.string.area_hide else Res.string.area_show), stringResource(Res.string.area_hide_sub), divider = true,
                end = { Icon(if (shown) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, null, tint = g.muted) }) {
                showModule(state, kind, !shown)
                onDismiss()
            }
            SheetRow(stringResource(Res.string.area_remove), stringResource(Res.string.area_remove_everywhere), divider = true,
                end = { Icon(Icons.Filled.Delete, null, tint = dangerColor()) }) { removing = true }
        }
    }
}

fun areaPage(kind: VaultKindUi): SetPage = when (kind) {
    VaultKindUi.PASS -> SetPage.PASS
    VaultKindUi.TOTP -> SetPage.AUTH
    VaultKindUi.DRIVE -> SetPage.DRIVE
    VaultKindUi.MAIL -> SetPage.MAIL
    VaultKindUi.CAL -> SetPage.CAL
}

/**
 * After unlocking: the recovery code was never confirmed (setup stopped, or a new code was not
 * confirmed). There is no "later": a new code is made, shown and confirmed by typing a group.
 */
@Composable
fun CodeReminderDialog(state: AppState) {
    AlertDialog(
        onDismissRequest = {},
        icon = { Icon(Icons.Filled.Warning, null, tint = warnColor()) },
        title = { Text(stringResource(Res.string.code_unconfirmed)) },
        text = { Text(stringResource(Res.string.code_unconfirmed_text)) },
        confirmButton = {
            TextButton(onClick = {
                state.run(block = { state.backend.newRecoveryCode(getString(Res.string.setup_recovery_label)) }) {
                    state.codeUnconfirmed = false
                    state.push(Screen.RecoveryCode(it))
                }
            }, enabled = !state.busy) { Text(stringResource(Res.string.code_do_now)) }
        },
    )
}

/** GhulDrive's storage on its settings page: like sync or its own server, changeable. */
@Composable
fun DriveStorageRow(state: AppState) {
    val net = state.backend.net
    var storage by remember { mutableStateOf<DriveStorageUi?>(null) }
    var sheet by remember { mutableStateOf(false) }
    var confirmSync by remember { mutableStateOf(false) }
    var own by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision()) { storage = runCatching { net.driveStorage() }.getOrNull() }
    val s = storage ?: return
    val server = s.server
    val sub = when (s.mode) {
        DriveModeUi.NONE -> stringResource(Res.string.ctx_not_set_up)
        DriveModeUi.SYNC -> if (server == null) stringResource(Res.string.drive_sync_gone_short) else stringResource(Res.string.drive_mode_sync, server.name)
        DriveModeUi.OWN, DriveModeUi.SUITE -> stringResource(Res.string.drive_mode_own, server?.name.orEmpty(), server?.let { kindLabel(it.kind) }.orEmpty())
    }
    ListRow(stringResource(Res.string.drive_storage), sub,
        leading = { IconTile(if (s.mode == DriveModeUi.SYNC) Icons.Filled.Sync else Icons.Filled.Dns, paletteColor(if (s.mode == DriveModeUi.SYNC) 2 else 1)) }) { sheet = true }
    if (server?.kind == ServerKindUi.PHP) {
        // The module can be brought up to date through the FTPS login kept with it.
        val updated = stringResource(Res.string.php_module_updated)
        TextButton(onClick = {
            state.run(onError = { error = it }, block = { net.phpModuleUpdate(server.id) }) { v -> state.message(updated.replace("%1\$s", v.toString())) }
        }) { Text(stringResource(Res.string.php_module_update)) }
    }
    ErrorText(error)
    if (confirmSync) {
        AlertDialog(
            onDismissRequest = { confirmSync = false },
            title = { Text(stringResource(Res.string.drive_to_sync_title)) },
            text = { Text(stringResource(Res.string.drive_to_sync_text)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmSync = false
                    state.run(onError = { error = it }, block = { net.driveUseSync() }) { state.changed() }
                }) { Text(stringResource(Res.string.drive_to_sync_yes)) }
            },
            dismissButton = { TextButton(onClick = { confirmSync = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (own) DriveServerSheet(state) { own = false }
    if (sheet) {
        GhulSheet({ sheet = false }, stringResource(Res.string.drive_storage_title)) {
            SheetList {
                PickRow(s.mode == DriveModeUi.SYNC, Icons.Filled.Sync, 2, stringResource(Res.string.drive_like_sync),
                    stringResource(if (s.syncAvailable) Res.string.drive_like_sync_ref else Res.string.wz_drive_sync_first), enabled = s.syncAvailable,
                    onClick = {
                        sheet = false
                        // U14: switching away from a server that holds files asks first (Drive looks empty).
                        if (s.mode != DriveModeUi.SYNC) confirmSync = true
                    })
                PickRow(s.mode == DriveModeUi.OWN || s.mode == DriveModeUi.SUITE, Icons.Filled.Dns, 1, stringResource(Res.string.wz_own_server),
                    stringResource(Res.string.drive_own_in_file), divider = true, onClick = { sheet = false; own = true })
            }
            HintText(stringResource(Res.string.drive_storage_hint))
        }
    }
}

/**
 * Before the sync server is removed or changed while GhulDrive uses it: keep it for GhulDrive
 * (copy of the login into the Drive file) or set GhulDrive up again later. Never silently nowhere.
 */
@Composable
fun DriveFollowsSyncDialog(state: AppState, onProceed: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.drive_follow_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.drive_follow_text))
                TextButton(onClick = { onDismiss(); onProceed() }, modifier = Modifier.fillMaxWidth().padding(top = 4.dp)) {
                    Text(stringResource(Res.string.drive_follow_later))
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(block = { state.backend.net.driveDetachFromSync() }) { onDismiss(); onProceed() }
            }) { Text(stringResource(Res.string.drive_follow_keep)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}
