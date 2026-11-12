// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import de.danerd.ghulsuite.model.AutofillStatus
import de.danerd.ghulsuite.model.ImportPreviewUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

/** Drop-down to choose a vault; `null` = "do not import". */
@Composable
fun VaultDropdown(label: String, vaults: List<VaultUi>, selected: String?, allowNone: Boolean, onSelect: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val none = stringResource(Res.string.import_none)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Box {
            TextButton(onClick = { open = true }) { Text(vaults.firstOrNull { it.id == selected }?.name ?: none) }
            DropdownMenu(open, { open = false }) {
                if (allowNone) DropdownMenuItem(text = { Text(none) }, onClick = { onSelect(null); open = false })
                for (v in vaults) DropdownMenuItem(text = { Text(v.name) }, onClick = { onSelect(v.id); open = false })
            }
        }
    }
}

@Composable
fun ImportScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    var file by remember { mutableStateOf<PickedFile?>(null) }
    var preview by remember { mutableStateOf<ImportPreviewUi?>(null) }
    val passVaults = state.vaultsOf(VaultKindUi.PASS).filter { it.available }
    val totpVaults = state.vaultsOf(VaultKindUi.TOTP).filter { it.available }
    var passTarget by remember { mutableStateOf(passVaults.firstOrNull()?.id) }
    var totpTarget by remember { mutableStateOf(totpVaults.firstOrNull()?.id) }
    var fromLogins by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    val pick = rememberOpenFile { f ->
        file = f
        preview = null
        error = null
        state.run(onError = { error = it }, block = { backend.previewImport(f.bytes, f.name) }) { preview = it }
    }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.import_title)) {
                Hint(stringResource(Res.string.import_text))
                Hint(stringResource(Res.string.import_formats))
                OutlinedButton(onClick = pick, enabled = filePickersAvailable && !state.busy, modifier = Modifier.fillMaxWidth()) {
                    Text(file?.let { stringResource(Res.string.import_file, it.name) } ?: stringResource(Res.string.import_choose))
                }
                ErrorText(error)
            }
            preview?.let { p ->
                SectionCard(stringResource(Res.string.import_found, p.format)) {
                    if (p.entries > 0) Hint(stringResource(Res.string.import_count_entries, p.entries.toString()))
                    if (p.totps > 0) Hint(stringResource(Res.string.import_count_totps, p.totps.toString()))
                    if (p.totpsInLogins > 0) Hint(stringResource(Res.string.import_count_login_totps, p.totpsInLogins.toString()))
                    if (p.skipped > 0) Text(stringResource(Res.string.import_count_skipped, p.skipped.toString()), color = warnColor())
                    if (p.entries > 0) {
                        VaultDropdown(stringResource(Res.string.import_target_pass), passVaults, passTarget, true) { passTarget = it }
                        if (passVaults.isEmpty()) Hint(stringResource(Res.string.import_no_vault))
                    }
                    if (p.totps > 0 || p.totpsInLogins > 0) {
                        VaultDropdown(stringResource(Res.string.import_target_totp), totpVaults, totpTarget, true) { totpTarget = it }
                        if (totpVaults.isEmpty()) Hint(stringResource(Res.string.import_no_vault))
                    }
                    if (p.totpsInLogins > 0 && totpTarget != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Checkbox(fromLogins, { fromLogins = it })
                            Text(stringResource(Res.string.import_from_logins))
                        }
                    }
                    Text(stringResource(Res.string.import_delete_hint), style = MaterialTheme.typography.bodySmall, color = warnColor())
                    val f = file
                    Button(
                        onClick = {
                            if (f == null) return@Button
                            state.run(onError = { error = it }, block = {
                                val r = backend.importData(
                                    f.bytes, f.name,
                                    passTarget.takeIf { p.entries > 0 },
                                    totpTarget.takeIf { p.totps > 0 || p.totpsInLogins > 0 },
                                    fromLogins,
                                )
                                getString(
                                    Res.string.import_done,
                                    r.entries.toString(), r.totps.toString(), r.duplicates.toString(),
                                )
                            }) { msg ->
                                state.changed()
                                state.message(msg)
                                state.pop()
                            }
                        },
                        enabled = !state.busy && f != null && (passTarget != null || totpTarget != null),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(stringResource(Res.string.a_import)) }
                }
            }
        }
    }
}

@Composable
fun ExportScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    val vaults = state.vaults.filter { it.available }
    var target by remember { mutableStateOf(vaults.firstOrNull()?.id) }
    var password by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val saved = stringResource(Res.string.msg_exported_plain)
    val save = rememberSaveFile { ok -> if (ok) state.message(saved) }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.export_title)) {
                Hint(stringResource(Res.string.export_encrypted_hint))
                Text(stringResource(Res.string.export_warning), color = warnColor())
                VaultDropdown(stringResource(Res.string.export_vault), vaults, target, false) { target = it }
                val v = vaults.firstOrNull { it.id == target }
                if (v != null) {
                    Hint(stringResource(if (v.kind == VaultKindUi.PASS) Res.string.export_format_csv else Res.string.export_format_otpauth))
                }
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                ErrorText(error)
                Button(
                    onClick = {
                        val vault = v ?: return@Button
                        state.run(onError = { error = it }, block = { backend.exportPlain(vault.id, password) }) { data ->
                            password = ""
                            val name = vault.name.ifBlank { "ghulsuite" }
                            save(if (vault.kind == VaultKindUi.PASS) "$name.csv" else "$name-2fa.txt", data)
                        }
                    },
                    enabled = !state.busy && v != null && password.isNotEmpty() && filePickersAvailable,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(Res.string.a_export_plain)) }
            }
        }
    }
}

/** Settings section: import, export and autofill. */
@Composable
fun ImportExportSection(state: AppState) {
    val settings = state.backend.settings
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val pickBackupFolder = rememberPickFolder { f ->
        settings.autoBackupFolder = f.uri + "|" + f.name
        settings.lastAutoBackup = 0
        state.runAutoBackup()
    }
    SectionCard(stringResource(Res.string.backup_title)) {
        Hint(stringResource(Res.string.backup_hint))
        if (filePickersAvailable) {
            Button(onClick = {
                state.run(block = { state.backend.suiteBackup() }) { data -> save(backupName(), data) }
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.backup_create)) }
            val folder = settings.autoBackupFolder.substringAfter('|', "")
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(Res.string.backup_auto))
                    Hint(if (folder.isEmpty()) stringResource(Res.string.backup_auto_off) else stringResource(Res.string.backup_auto_on, folder))
                }
                Switch(folder.isNotEmpty(), { on -> if (on) pickBackupFolder() else settings.autoBackupFolder = "" })
            }
            Hint(stringResource(Res.string.backup_restore_hint))
        }
    }
    SectionCard(stringResource(Res.string.set_import_export)) {
        OutlinedButton(onClick = { state.push(Screen.Import) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.a_import))
        }
        OutlinedButton(onClick = { state.push(Screen.Export) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.a_export_plain))
        }
        Hint(stringResource(Res.string.set_import_hint))
    }
}

@Composable
fun AutofillSection(state: AppState) {
    // Re-read while visible: the user comes back from the system settings without any event (F1).
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) { while (true) { kotlinx.coroutines.delay(1500); tick++ } }
    val status = remember(state.revision, tick) { state.backend.autofillStatus() }
    if (status == AutofillStatus.UNSUPPORTED) return
    SectionCard(stringResource(Res.string.set_autofill)) {
        Hint(stringResource(Res.string.set_autofill_text))
        if (status == AutofillStatus.ON) {
            Text(stringResource(Res.string.set_autofill_on), color = okColor())
        } else {
            Text(stringResource(Res.string.set_autofill_off), color = warnColor())
        }
        OutlinedButton(onClick = { state.backend.openAutofillSettings() }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(if (status == AutofillStatus.ON) Res.string.a_autofill_change else Res.string.a_autofill_enable))
        }
        Hint(stringResource(Res.string.set_autofill_hint))
    }
}

/** File name of a backup, e.g. "GhulSuite-Sicherung-2026-11-12.zip". */
fun backupName(): String {
    val d = formatDate(nowSeconds() * 1000).filter { it.isDigit() || it == '.' || it == '/' || it == '-' }.replace('.', '-').replace('/', '-')
    return "GhulSuite-Sicherung-${nowSeconds() / 60}-$d.zip".replace("--", "-")
}
