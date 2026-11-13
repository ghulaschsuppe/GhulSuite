// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Password
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.FoundVaultUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultSourceUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

fun extensionOf(kind: VaultKindUi) = when (kind) {
    VaultKindUi.PASS -> "ghlpss"
    VaultKindUi.TOTP -> "ghltotp"
    VaultKindUi.CAL -> "ghlcal"
    VaultKindUi.DRIVE -> "ghldrv"
    VaultKindUi.MAIL -> "ghlmail"
}

/** Name of the area a vault belongs to. */
fun kindLabel(kind: VaultKindUi) = when (kind) {
    VaultKindUi.PASS -> Res.string.tab_pass
    VaultKindUi.TOTP -> Res.string.tab_auth
    VaultKindUi.CAL -> Res.string.tab_cal
    VaultKindUi.DRIVE -> Res.string.tab_drive
    VaultKindUi.MAIL -> Res.string.tab_mail
}

/** Filter chips "All" + one per vault; hidden when there is only one vault. */
@Composable
fun VaultChips(vaults: List<VaultUi>, selected: String?, onSelect: (String?) -> Unit) {
    if (vaults.size < 2) return
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FilterChip(selected == null, { onSelect(null) }, label = { Text(stringResource(Res.string.vault_all)) })
        for (v in vaults) {
            FilterChip(selected == v.id, { onSelect(v.id) }, label = { Text(v.name) })
        }
    }
}

/**
 * Chooses the vault for a new entry: the filtered vault, the only vault, or asks the user.
 * Offers to create a vault if there is none.
 */
@Composable
fun VaultPickerDialog(state: AppState, kind: VaultKindUi, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    var create by remember { mutableStateOf(false) }
    val vaults = state.vaultsOf(kind).filter { it.available }
    if (create) {
        CreateVaultDialog(state, kind, onCreated = { onPick(it) }, onDismiss = onDismiss)
        return
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.vault_pick_title)) },
        text = {
            Column {
                if (vaults.isEmpty()) Hint(stringResource(Res.string.vault_none_yet))
                for (v in vaults) {
                    ListItem(headlineContent = { Text(v.name) }, modifier = Modifier.clickable { onPick(v.id) })
                }
                ListItem(
                    headlineContent = { Text(stringResource(Res.string.vault_new)) },
                    leadingContent = { Icon(Icons.Filled.Add, null) },
                    modifier = Modifier.clickable { create = true },
                )
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Creates a vault; [kind] fixed or chosen by the user when null. */
@Composable
fun CreateVaultDialog(state: AppState, kind: VaultKindUi?, onCreated: (String) -> Unit, onDismiss: () -> Unit) {
    var chosen by remember { mutableStateOf(kind ?: VaultKindUi.PASS) }
    var name by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.vault_new)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (kind == null) {
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                        VaultKindUi.entries.forEachIndexed { i, k ->
                            SegmentedButton(chosen == k, { chosen = k }, SegmentedButtonDefaults.itemShape(i, 3)) {
                                Text(stringResource(kindLabel(k)))
                            }
                        }
                    }
                }
                Hint(stringResource(Res.string.vault_new_text))
                OutlinedTextField(
                    name, { name = it; error = null }, label = { Text(stringResource(Res.string.l_vault_name)) },
                    placeholder = { Text(stringResource(Res.string.vault_name_example)) }, singleLine = true,
                )
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = { state.backend.createVault(chosen, name.trim()) }) { id ->
                    state.enableModule(chosen)
                    state.changed()
                    onCreated(id)
                }
            }, enabled = !state.busy && name.isNotBlank()) { Text(stringResource(Res.string.a_create)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Settings section: list, create, rename, export, add and remove vaults. */
@Composable
fun VaultsSection(state: AppState) {
    val backend = state.backend
    var create by remember { mutableStateOf(false) }
    var menuFor by remember { mutableStateOf<String?>(null) }
    var renaming by remember { mutableStateOf<VaultUi?>(null) }
    var removing by remember { mutableStateOf<VaultUi?>(null) }
    val exported = stringResource(Res.string.msg_exported)
    val added = stringResource(Res.string.msg_vault_added)
    val save = rememberSaveFile { ok -> if (ok) state.message(exported) }
    var foreign by remember { mutableStateOf<ForeignRequest?>(null) }
    val noneFound = stringResource(Res.string.msg_no_new_vaults)
    val addFile = rememberOpenFile { file ->
        val name = file.name.substringBeforeLast('.')
        val source = VaultSourceUi.Bytes(file.bytes)
        state.run(block = {
            try {
                val id = backend.addVaultFile(file.bytes, name)
                backend.listVaults().firstOrNull { it.id == id }?.kind to null
            } catch (e: VaultException) {
                if (e.kind != VaultErrorKind.DIFFERENT_PROTECTION) throw e
                val info = backend.inspectFile(file.bytes)
                null to ForeignRequest(source, if (info.kind == "pass") VaultKindUi.PASS else VaultKindUi.TOTP, info.protection, name)
            }
        }) { (kind, request) ->
            if (request != null) {
                foreign = request
            } else {
                kind?.let { state.enableModule(it) }
                state.changed()
                state.message(added)
            }
        }
    }

    SectionCard(stringResource(Res.string.set_vaults)) {
        Hint(stringResource(Res.string.set_vaults_hint))
        for (v in state.vaults) {
            Box {
                ListItem(
                    headlineContent = { Text(v.name) },
                    supportingContent = {
                        Column {
                            Text(
                                if (v.available) {
                                    stringResource(
                                        Res.string.vault_summary,
                                        stringResource(kindLabel(v.kind)),
                                        v.entries.toString(),
                                        v.file,
                                    )
                                } else {
                                    stringResource(Res.string.vault_missing, v.file)
                                },
                            )
                            if (v.outdated) Text(stringResource(Res.string.vault_outdated), color = warnColor())
                        }
                    },
                    leadingContent = {
                        Icon(
                            when {
                                !v.available -> Icons.Filled.Warning
                                v.kind == VaultKindUi.PASS -> Icons.Filled.Password
                                v.kind == VaultKindUi.CAL -> Icons.Filled.CalendarMonth
                                else -> Icons.Filled.Key
                            },
                            null,
                        )
                    },
                    trailingContent = {
                        IconButton(onClick = { menuFor = v.id }) { Icon(Icons.Filled.MoreVert, stringResource(Res.string.a_more)) }
                    },
                )
                DropdownMenu(expanded = menuFor == v.id, onDismissRequest = { menuFor = null }) {
                    DropdownMenuItem(text = { Text(stringResource(Res.string.a_rename)) }, onClick = { renaming = v; menuFor = null })
                    if (v.available && filePickersAvailable) {
                        DropdownMenuItem(text = { Text(stringResource(Res.string.a_export)) }, onClick = {
                            menuFor = null
                            state.run(block = { backend.exportVault(v.id) }) { data ->
                                save("${v.name.ifBlank { "vault" }}.${extensionOf(v.kind)}", data)
                            }
                        })
                    }
                    if (v.outdated) {
                        DropdownMenuItem(text = { Text(stringResource(Res.string.a_accept_state)) }, onClick = {
                            menuFor = null
                            state.run(block = { backend.acceptVaultState(v.id) }) { state.changed() }
                        })
                    }
                    DropdownMenuItem(text = { Text(stringResource(Res.string.a_remove)) }, onClick = { removing = v; menuFor = null })
                }
            }
        }
        OutlinedButton(onClick = { create = true }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.vault_new)) }
        if (filePickersAvailable) {
            OutlinedButton(onClick = addFile, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.vault_add_file)) }
            Hint(stringResource(Res.string.vault_add_file_hint))
        }
        OutlinedButton(onClick = {
            state.foundPostponed = false
            state.scanFound { if (it.isEmpty()) state.message(noneFound) }
        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.vault_scan)) }
    }

    foreign?.let { r -> ForeignVaultDialog(state, r, onDone = { foreign = null }) }

    if (create) CreateVaultDialog(state, null, onCreated = { create = false }, onDismiss = { create = false })
    renaming?.let { v ->
        var name by remember(v.id) { mutableStateOf(v.name) }
        AlertDialog(
            onDismissRequest = { renaming = null },
            title = { Text(stringResource(Res.string.a_rename)) },
            text = { OutlinedTextField(name, { name = it }, label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true) },
            confirmButton = {
                TextButton(onClick = {
                    renaming = null
                    state.run(block = { backend.renameVault(v.id, name.trim()) }) { state.changed() }
                }, enabled = name.isNotBlank()) { Text(stringResource(Res.string.a_save)) }
            },
            dismissButton = { TextButton(onClick = { renaming = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    removing?.let { v ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text(stringResource(Res.string.vault_remove_title, v.name)) },
            text = { Text(stringResource(Res.string.vault_remove_text), color = warnColor()) },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    state.run(block = { backend.removeVault(v.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** A vault with its own protection that the user wants to add. */
class ForeignRequest(val source: VaultSourceUi, val kind: VaultKindUi, val protection: ProtectionUi, val name: String)

/** Offers vault files found in the suite's folder that are not part of the suite yet. */
@Composable
fun FoundVaultsDialog(state: AppState) {
    var foreign by remember { mutableStateOf<ForeignRequest?>(null) }
    foreign?.let { r ->
        ForeignVaultDialog(state, r, onDone = { foreign = null })
        return
    }
    AlertDialog(
        onDismissRequest = { state.foundPostponed = true },
        title = { Text(stringResource(Res.string.found_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.found_text))
                for (f in state.found) {
                    FoundRow(state, f, onForeign = {
                        foreign = ForeignRequest(VaultSourceUi.InFolder(f.file), f.kind, f.protection, f.file.substringBeforeLast('.'))
                    })
                }
                BusyBanner(state)
            }
        },
        confirmButton = { TextButton(onClick = { state.foundPostponed = true }) { Text(stringResource(Res.string.a_later)) } },
    )
}

@Composable
private fun FoundRow(state: AppState, f: FoundVaultUi, onForeign: () -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        Text(f.file, style = androidx.compose.material3.MaterialTheme.typography.titleSmall)
        Hint(
            stringResource(kindLabel(f.kind)) + " · " +
                stringResource(Res.string.found_own_protection),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onForeign, enabled = !state.busy) { Text(stringResource(Res.string.a_open_with_password)) }
            TextButton(onClick = {
                state.run(block = { state.backend.ignoreFoundVault(f.file) }) { state.scanFound() }
            }, enabled = !state.busy) { Text(stringResource(Res.string.a_ignore)) }
        }
    }
}

/** Opens a vault with its own password and key or key file once and adds it to the suite. */
@Composable
fun ForeignVaultDialog(state: AppState, request: ForeignRequest, onDone: () -> Unit) {
    val hasKey = request.protection.securityKeys > 0
    val hasKeyfile = request.protection.keyfiles > 0
    var withKey by remember { mutableStateOf(hasKey) }
    val needsKey = withKey && hasKey
    var name by remember { mutableStateOf(request.name) }
    var password by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val added = stringResource(Res.string.msg_vault_added)
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDone() },
        title = { Text(stringResource(Res.string.foreign_title)) },
        text = {
            // The sheet scrolls itself; a second scroll container inside it crashes (T6).
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.foreign_text))
                OutlinedTextField(name, { name = it }, label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true)
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                if (hasKey && hasKeyfile) FactorChoice(withKey) { withKey = it; error = null }
                if (needsKey) {
                    KeyPinField(state, pin) { pin = it; error = null }
                } else {
                    KeyfilePicker(keyfile) { keyfile = it; error = null }
                }
                Text(stringResource(Res.string.foreign_warning), color = warnColor())
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = {
                    state.backend.adoptVault(
                        request.source, name.ifBlank { request.name }, password,
                        if (needsKey) pin else null, if (needsKey) null else keyfile?.bytes, state.onTouchNeeded,
                    )
                }) {
                    state.enableModule(request.kind)
                    state.changed()
                    state.scanFound()
                    state.message(added)
                    onDone()
                }
            }, enabled = !state.busy && password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null)) {
                Text(stringResource(Res.string.a_take_over))
            }
        },
        dismissButton = { TextButton(onClick = onDone, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}
