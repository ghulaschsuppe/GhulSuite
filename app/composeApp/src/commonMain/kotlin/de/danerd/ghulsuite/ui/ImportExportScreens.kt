// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import de.danerd.ghulsuite.model.ImportPreviewUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.pluralStringResource
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
        TextButton(onClick = { open = true }) { Text(vaults.firstOrNull { it.id == selected }?.let { vaultLabel(it, vaults) } ?: none) }
    }
    // The choice opens as a sheet like every other choice in the app.
    if (open) {
        GhulSheet({ open = false }, label) {
            SheetList {
                if (allowNone) SheetRow(none, end = { if (selected == null) SheetCheck() }) { onSelect(null); open = false }
                vaults.forEachIndexed { i, v ->
                    SheetRow(vaultLabel(v, vaults), leading = { Avatar(v.name) }, divider = allowNone || i > 0,
                        end = { if (v.id == selected) SheetCheck() }) { onSelect(v.id); open = false }
                }
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
                    if (p.entries > 0) Hint(pluralStringResource(Res.plurals.import_count_entries, p.entries, p.entries.toString()))
                    if (p.totps > 0) Hint(pluralStringResource(Res.plurals.import_count_totps, p.totps, p.totps.toString()))
                    if (p.totpsInLogins > 0) Hint(pluralStringResource(Res.plurals.import_count_login_totps, p.totpsInLogins, p.totpsInLogins.toString()))
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
        Hint(
            pluralStringResource(
                Res.plurals.import_done, r.duplicates,
                pluralStringResource(Res.plurals.n_entries, r.entries, r.entries.toString()),
                pluralStringResource(Res.plurals.n_2fa_accounts, r.totps, r.totps.toString()),
                r.duplicates.toString(),
            ),
        )
        if (r.kept > 0) Text(pluralStringResource(Res.plurals.import_result_kept, r.kept, r.kept.toString()), color = okColor())
    }
    if (r.skippedItems.isNotEmpty()) SkippedList(r.skippedItems)
    Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_finish)) }
}

@Composable
fun ExportScreen(state: AppState, padding: PaddingValues) {
    val backend = state.backend
    // GhulDocs exports its documents one by one (and the vault as a .ghldocs under Options, Vaults).
    val vaults = state.vaults.filter { it.available && it.kind != VaultKindUi.DOCS }
    var target by remember { mutableStateOf(vaults.firstOrNull()?.id) }
    var password by remember { mutableStateOf("") }
    // M29: off by default (another password manager takes the file as it is).
    var spreadsheetSafe by remember { mutableStateOf(false) }
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
                        VaultKindUi.CAL, VaultKindUi.DOCS -> Res.string.export_format_ics
                    }))
                }
                if (v?.kind == VaultKindUi.PASS) {
                    SwitchRow(stringResource(Res.string.export_spreadsheet_safe), spreadsheetSafe) { spreadsheetSafe = it }
                    Hint(stringResource(Res.string.export_spreadsheet_safe_hint))
                }
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                ErrorText(error)
                Button(
                    onClick = {
                        val vault = v ?: return@Button
                        state.run(onError = { error = it }, block = { backend.exportPlain(vault.id, password, spreadsheetSafe && vault.kind == VaultKindUi.PASS) }) { data ->
                            password = ""
                            val name = vault.name.ifBlank { "ghulsuite" }
                            save(when (vault.kind) {
                                VaultKindUi.PASS -> "$name.csv"
                                VaultKindUi.TOTP -> "$name-2fa.txt"
                                VaultKindUi.CAL, VaultKindUi.DOCS -> "$name.ics"
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

/** Settings section: import and export. */
@Composable
fun ImportExportSection(state: AppState) {
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

/** Vault name, with its area when the list mixes GhulPass and GhulAuth vaults ("Personal · GhulAuth"). */
private fun vaultLabel(v: de.danerd.ghulsuite.model.VaultUi, all: List<de.danerd.ghulsuite.model.VaultUi>): String =
    if (all.map { it.kind }.distinct().size > 1) v.name + " · " + (when (v.kind) { VaultKindUi.PASS -> "GhulPass"; VaultKindUi.TOTP -> "GhulAuth"; VaultKindUi.DOCS -> "GhulDocs"; VaultKindUi.CAL -> "GhulCal" }) else v.name
