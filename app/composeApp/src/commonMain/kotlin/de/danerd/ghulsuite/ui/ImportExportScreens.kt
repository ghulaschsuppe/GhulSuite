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
import de.danerd.ghulsuite.model.ImportResultUi
import de.danerd.ghulsuite.model.SkippedImportUi
import de.danerd.ghulsuite.model.SkipReasonUi
import de.danerd.ghulsuite.model.KeepSkippedUi
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Warning
import androidx.compose.ui.unit.dp

/** Drop-down to choose a vault; `null` = "do not import". */
@Composable
fun VaultDropdown(label: String, vaults: List<VaultUi>, selected: String?, allowNone: Boolean, onSelect: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val none = stringResource(Res.string.import_none)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Box {
            TextButton(onClick = { open = true }) { Text(vaults.firstOrNull { it.id == selected }?.let { vaultLabel(it, vaults) } ?: none) }
            DropdownMenu(open, { open = false }) {
                if (allowNone) DropdownMenuItem(text = { Text(none) }, onClick = { onSelect(null); open = false })
                for (v in vaults) DropdownMenuItem(text = { Text(vaultLabel(v, vaults)) }, onClick = { onSelect(v.id); open = false })
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
    /** Keep what cannot be converted as notes (on by default: nothing gets lost). */
    var keep by remember { mutableStateOf(true) }
    var result by remember { mutableStateOf<ImportResultUi?>(null) }
    val keepFolder = stringResource(Res.string.import_keep_folder)
    val pick = rememberOpenFile { f ->
        file = f
        result = null
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
            val res = result
            if (res != null) {
                ImportResultCard(res) { state.pop() }
            } else preview?.let { p ->
                // Items whose data would otherwise be lost (not counting 2FA codes that stay in their login).
                val lost = p.skippedItems.count { !it.inEntry }
                SectionCard(stringResource(Res.string.import_found, p.format)) {
                    if (p.entries > 0) Hint(stringResource(Res.string.import_count_entries, p.entries.toString()))
                    if (p.totps > 0) Hint(stringResource(Res.string.import_count_totps, p.totps.toString()))
                    if (p.totpsInLogins > 0) Hint(stringResource(Res.string.import_count_login_totps, p.totpsInLogins.toString()))
                    if (p.entries > 0 || (keep && lost > 0)) {
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
                }
                if (p.skippedItems.isNotEmpty()) {
                    SkippedList(p.skippedItems)
                    if (lost > 0) {
                        SectionCard {
                            SwitchRow(stringResource(Res.string.import_keep), keep) { keep = it }
                            Hint(stringResource(Res.string.import_keep_sub, keepFolder))
                            if (keep && passTarget == null) Text(stringResource(Res.string.import_keep_needs_vault), color = warnColor())
                        }
                    }
                }
                SectionCard {
                    Text(stringResource(Res.string.import_delete_hint), style = MaterialTheme.typography.bodySmall, color = warnColor())
                    val f = file
                    Button(
                        onClick = {
                            if (f == null) return@Button
                            state.run(onError = { error = it }, block = {
                                backend.importData(
                                    f.bytes, f.name,
                                    passTarget.takeIf { p.entries > 0 || (keep && lost > 0) },
                                    totpTarget.takeIf { p.totps > 0 || p.totpsInLogins > 0 },
                                    fromLogins,
                                    if (keep && lost > 0) KeepSkippedUi(
                                        keepFolder,
                                        getString(Res.string.import_keep_reason_label),
                                        getString(Res.string.import_keep_data_label),
                                        getString(Res.string.import_reason_hotp),
                                        getString(Res.string.import_reason_unsupported_short),
                                        getString(Res.string.import_reason_invalid),
                                    ) else null,
                                )
                            }) { r ->
                                state.changed()
                                result = r
                            }
                        },
                        enabled = !state.busy && f != null && (passTarget != null || totpTarget != null),
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(stringResource(Res.string.a_import)) }
                    ErrorText(error)
                }
            }
        }
    }
}

/** Why an item could not be converted, in words. */
@Composable
private fun skipText(x: SkippedImportUi): String = when (x.reason) {
    SkipReasonUi.HOTP -> stringResource(Res.string.import_reason_hotp)
    SkipReasonUi.UNSUPPORTED -> stringResource(Res.string.import_reason_unsupported, x.kind)
    SkipReasonUi.INVALID_CODE -> stringResource(Res.string.import_reason_invalid)
} + if (x.inEntry) " · " + stringResource(Res.string.import_in_entry) else ""

/** The items an import could not convert: name and reason, one per row. */
@Composable
private fun SkippedList(items: List<SkippedImportUi>) {
    SectionLabel(stringResource(Res.string.import_skipped_title, items.size.toString()))
    ListCard {
        items.forEachIndexed { i, x ->
            ListRow(
                x.name.ifBlank { stringResource(Res.string.untitled) }, skipText(x),
                leading = { IconTile(Icons.Filled.Warning, if (x.inEntry) g.faint else warnColor(), size = 36.dp) },
                divider = i > 0,
            )
        }
    }
}

/** What the import did, shown in place (no message box): counts and what was not converted. */
@Composable
private fun ImportResultCard(r: ImportResultUi, onDone: () -> Unit) {
    SectionCard(stringResource(Res.string.import_result_title)) {
        Hint(stringResource(Res.string.import_done, r.entries.toString(), r.totps.toString(), r.duplicates.toString()))
        if (r.kept > 0) Text(stringResource(Res.string.import_result_kept, r.kept.toString()), color = okColor())
    }
    if (r.skippedItems.isNotEmpty()) SkippedList(r.skippedItems)
    Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_finish)) }
}

@Composable
fun ExportScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    // GhulDrive and GhulMail files hold settings and logins only: nothing to export as plain text.
    val vaults = state.vaults.filter { it.available && it.kind != VaultKindUi.DRIVE && it.kind != VaultKindUi.MAIL }
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
                    Hint(stringResource(when (v.kind) {
                        VaultKindUi.PASS -> Res.string.export_format_csv
                        VaultKindUi.TOTP -> Res.string.export_format_otpauth
                        VaultKindUi.CAL, VaultKindUi.DRIVE, VaultKindUi.MAIL -> Res.string.export_format_ics
                    }))
                }
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                ErrorText(error)
                Button(
                    onClick = {
                        val vault = v ?: return@Button
                        state.run(onError = { error = it }, block = { backend.exportPlain(vault.id, password) }) { data ->
                            password = ""
                            val name = vault.name.ifBlank { "ghulsuite" }
                            save(when (vault.kind) {
                                VaultKindUi.PASS -> "$name.csv"
                                VaultKindUi.TOTP -> "$name-2fa.txt"
                                VaultKindUi.CAL, VaultKindUi.DRIVE, VaultKindUi.MAIL -> "$name.ics"
                            }, data)
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
        if (status == AutofillStatus.ON) {
            Text(stringResource(Res.string.set_autofill_how), style = MaterialTheme.typography.labelLarge)
            Hint(stringResource(Res.string.set_autofill_how_field))
            Hint(stringResource(Res.string.set_autofill_how_apps))
            Hint(stringResource(Res.string.set_autofill_how_chrome))
            Hint(stringResource(Res.string.set_autofill_how_manual))
        }
    }
}

/** File name of a backup with a readable UTC date and time, e.g. "GhulSuite-Backup-2026-11-12-1530.zip". */
fun backupName(): String {
    val secs = nowSeconds()
    // Civil date from days since 1970 (Howard Hinnant's algorithm), no platform date API needed.
    val z = secs / 86_400 + 719_468
    val era = (if (z >= 0) z else z - 146_096) / 146_097
    val doe = z - era * 146_097
    val yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365
    val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
    val mp = (5 * doy + 2) / 153
    val day = doy - (153 * mp + 2) / 5 + 1
    val month = if (mp < 10) mp + 3 else mp - 9
    val year = yoe + era * 400 + (if (month <= 2) 1 else 0)
    val minutes = (secs % 86_400) / 60
    fun two(n: Long) = n.toString().padStart(2, '0')
    return "GhulSuite-Backup-$year-${two(month)}-${two(day)}-${two(minutes / 60)}${two(minutes % 60)}.zip"
}

/**
 * Common start of all automatic backups with the same naming ("GhulSuite-Backup-"): only those are
 * rotated, other ZIP files in the folder (older names included) are left alone.
 */
fun backupPrefix(name: String): String = name.split('-').take(2).joinToString("-") + "-"

/** Vault name, with its area when the list mixes GhulPass and GhulAuth vaults ("Personal · GhulAuth"). */
private fun vaultLabel(v: de.danerd.ghulsuite.model.VaultUi, all: List<de.danerd.ghulsuite.model.VaultUi>): String =
    if (all.map { it.kind }.distinct().size > 1) v.name + " · " + (when (v.kind) { VaultKindUi.PASS -> "GhulPass"; VaultKindUi.TOTP -> "GhulAuth"; VaultKindUi.CAL -> "GhulCal"; VaultKindUi.DRIVE -> "GhulDrive"; VaultKindUi.MAIL -> "GhulMail" }) else v.name
