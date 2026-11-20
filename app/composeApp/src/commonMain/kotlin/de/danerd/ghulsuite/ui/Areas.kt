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
import androidx.compose.material.icons.filled.Save
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
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

fun areaInfo(kind: VaultKindUi): AreaInfo = AREAS.first { it.kind == kind }

/** Whether this device shows the area (its file stays either way). */
fun moduleShown(state: AppState, kind: VaultKindUi): Boolean {
    val s = state.backend.settings
    return when (kind) {
        VaultKindUi.PASS -> s.moduleGhulPass
        VaultKindUi.TOTP -> s.moduleGhulAuth
        VaultKindUi.DOCS -> s.moduleGhulDocs
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
                if (entries > 0) Text(pluralStringResource(Res.plurals.remove_count, entries, entries.toString()))
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
        val defaultName = stringResource(
            when (kind) {
                VaultKindUi.PASS -> Res.string.area_pass_default
                VaultKindUi.TOTP -> Res.string.area_auth_default
                VaultKindUi.DOCS -> Res.string.area_docs_default
                VaultKindUi.CAL -> Res.string.area_cal_default
            },
        )
        LaunchedEffect(kind) { name = defaultName }
        val done = stringResource(Res.string.area_set_up_msg, title)
        GhulSheet(onDismiss, stringResource(Res.string.area_set_up_title, title)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                IconTile(tabIcon(a.tab), areaColor(a.tab), size = 56.dp)
                HintText(stringResource(Res.string.area_set_up_text, stringResource(a.sub), (if (kind in NAMED_AREAS) name else title.removePrefix("Ghul")) + a.ext), Modifier.weight(1f))
            }
            if (kind in NAMED_AREAS) {
                OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true)
            }
            ErrorText(error)
            Button(onClick = {
                state.run(onError = { error = it }, block = { state.backend.addArea(kind, if (kind in NAMED_AREAS) name else "") }) {
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
    VaultKindUi.DOCS -> SetPage.DOCS
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
                    state.protectionChanged()
                    state.push(Screen.RecoveryCode(it))
                }
            }, enabled = !state.busy) { Text(stringResource(Res.string.code_do_now)) }
        },
    )
}

/**
 * After a change of the protection the suite file must be saved and older copies (other devices,
 * sticks, backups) deleted: they still open with the old protection.
 */
@Composable
fun SaveAfterProtectionDialog(state: AppState) {
    AlertDialog(
        onDismissRequest = { state.saveAfterProtection = false },
        icon = { Icon(Icons.Filled.Save, null, tint = warnColor()) },
        title = { Text(stringResource(Res.string.prot_save_title)) },
        text = { Text(stringResource(Res.string.prot_save_text)) },
        confirmButton = {
            TextButton(onClick = {
                state.saveAfterProtection = false
                state.push(Screen.SettingsPage(SetPage.FILE))
            }) { Text(stringResource(Res.string.prot_save_now)) }
        },
        dismissButton = {
            TextButton(onClick = { state.saveAfterProtection = false }) { Text(stringResource(Res.string.a_later)) }
        },
    )
}
