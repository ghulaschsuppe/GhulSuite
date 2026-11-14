// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AddCircle
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ContainerInfoUi
import de.danerd.ghulsuite.model.KdfChoice
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.NewVaultUi
import de.danerd.ghulsuite.model.OpenVaultUi
import de.danerd.ghulsuite.model.PasswordOptionsUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.SetupUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

private enum class Flow { START, NEW, OPEN_VAULT, FETCH }

const val KEYFILE_NAME = "ghulsuite.ghlkey"

@Composable
fun ScreenColumn(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 100.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = 640.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            content()
        }
    }
}

/** First start: set up a new suite, open an existing one or build one around a single vault file. */
@Composable
fun SetupScreen(state: AppState) {
    var flow by remember { mutableStateOf(Flow.START) }
    var vaultFile by remember { mutableStateOf<PickedFile?>(null) }
    var vaultInfo by remember { mutableStateOf<ContainerInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val backend = state.backend
    val scope = rememberCoroutineScope()

    val noSuiteHere = stringResource(Res.string.start_no_suite_in_folder)
    val isSuiteFile = stringResource(Res.string.start_is_suite_file)
    val pickFolder = rememberPickFolder { folder ->
        backend.useStorage(folder)
        if (backend.exists()) {
            state.showStart = false
            state.vaultExists = true
        } else {
            error = noSuiteHere
        }
    }
    val openFile = rememberOpenFile { file ->
        scope.launch {
            runCatching { backend.inspectFile(file.bytes) }
                .onSuccess { info ->
                    if (info.kind == "suite") {
                        error = isSuiteFile
                    } else {
                        vaultFile = file
                        vaultInfo = info
                        error = null
                        flow = Flow.OPEN_VAULT
                    }
                }
                .onFailure { error = describe(it) }
        }
    }

    val restoredMsg = stringResource(Res.string.backup_restored)
    val restoreBackup = rememberOpenFile { file ->
        state.run(onError = { error = it }, block = { backend.restoreBackup(file.bytes) }) {
            state.message(restoredMsg)
            state.showStart = false
            state.vaultExists = true
        }
    }

    // U6: a backup never goes over an existing suite: there the app offers another (empty) folder.
    var restoreAsk by remember { mutableStateOf(false) }
    val folderHasSuite = stringResource(Res.string.core_folder_has_suite)
    val pickRestoreFolder = rememberPickFolder { folder ->
        backend.useStorage(folder)
        if (backend.exists()) error = folderHasSuite else restoreBackup()
    }
    if (restoreAsk) {
        AlertDialog(
            onDismissRequest = { restoreAsk = false },
            title = { Text(stringResource(Res.string.restore_exists_title)) },
            text = { Text(stringResource(Res.string.restore_exists_text)) },
            confirmButton = {
                TextButton(onClick = { restoreAsk = false; pickRestoreFolder() }) { Text(stringResource(Res.string.restore_choose_folder)) }
            },
            dismissButton = { TextButton(onClick = { restoreAsk = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }

    PlatformBackHandler(enabled = flow != Flow.START) { flow = Flow.START }

    ScreenColumn {
        // The wizard has its own header (title, step, progress).
        if (flow != Flow.NEW) {
            BrandHeader(if (flow == Flow.START) stringResource(Res.string.start_tagline) else null)
            BusyBanner(state)
        }
        when (flow) {
            Flow.START -> {
                StartOption(Icons.Filled.Add, paletteColor(0), stringResource(Res.string.start_new), stringResource(Res.string.start_new_text)) {
                    error = null; flow = Flow.NEW
                }
                StartOption(Icons.Filled.Cloud, paletteColor(1), stringResource(Res.string.start_fetch), stringResource(Res.string.start_fetch_text)) {
                    error = null
                    flow = Flow.FETCH
                }
                if (filePickersAvailable) {
                    StartOption(Icons.Filled.Folder, paletteColor(2), stringResource(Res.string.start_open_suite), stringResource(Res.string.start_open_suite_text)) {
                        error = null
                        pickFolder()
                    }
                    StartOption(Icons.Filled.Description, paletteColor(3), stringResource(Res.string.start_open_vault), stringResource(Res.string.start_open_vault_text)) {
                        error = null
                        openFile()
                    }
                }
                if (filePickersAvailable) {
                    StartOption(Icons.Filled.Restore, paletteColor(5), stringResource(Res.string.backup_restore), stringResource(Res.string.backup_restore_sub)) {
                        error = null
                        if (backend.exists()) restoreAsk = true else restoreBackup()
                    }
                }
                HintText(stringResource(Res.string.start_footer))
                ErrorText(error)
                if (state.showStart && backend.exists()) {
                    TextButton(onClick = { state.showStart = false; state.vaultExists = true }) {
                        Text(stringResource(Res.string.start_back_to_suite, storageLabel(backend.storageName())))
                    }
                }
            }

            Flow.NEW -> NewSuiteWizard(state) { flow = Flow.START }

            Flow.FETCH -> FetchFromServer(state) { flow = Flow.START }

            Flow.OPEN_VAULT -> {
                val file = vaultFile
                val info = vaultInfo
                if (file != null && info != null) {
                    OpenVaultWizard(state, file, info) { flow = Flow.START }
                }
            }
        }
    }
}

@Composable
private fun StartOption(icon: ImageVector, color: androidx.compose.ui.graphics.Color, title: String, text: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(g.surface).clickable(onClick = onClick)
            .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IconTile(icon, color)
        Column(Modifier.weight(1f)) {
            Text(title, fontWeight = FontWeight.W500)
            Text(text, fontSize = 14.sp, color = g.muted)
        }
        ChevronEnd()
    }
}

/** Shows where the suite is stored and lets the user choose a folder. */
@Composable
fun StorageChooser(state: AppState) {
    val backend = state.backend
    var name by remember { mutableStateOf(backend.storageName()) }
    var exists by remember { mutableStateOf(backend.exists()) }
    val pickFolder = rememberPickFolder { folder ->
        backend.useStorage(folder)
        name = backend.storageName()
        exists = backend.exists()
    }
    Text(stringResource(Res.string.storage_title), style = MaterialTheme.typography.titleSmall)
    Hint(stringResource(Res.string.storage_current, storageLabel(name)))
    Hint(stringResource(Res.string.storage_hint))
    if (filePickersAvailable) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = {
                backend.useStorage(null)
                name = backend.storageName()
                exists = backend.exists()
            }) { Text(stringResource(Res.string.storage_app)) }
            OutlinedButton(onClick = pickFolder) { Text(stringResource(Res.string.storage_folder)) }
        }
    }
    if (exists) {
        Text(stringResource(Res.string.storage_suite_exists), color = warnColor(), style = MaterialTheme.typography.bodyMedium)
        TextButton(onClick = { state.showStart = false; state.vaultExists = true }) {
            Text(stringResource(Res.string.storage_open_existing))
        }
    }
}

/** Finish of a setup around one vault file: only the area of that vault is switched on. */
private fun finishSetup(state: AppState, kind: VaultKindUi) {
    state.backend.settings.moduleGhulPass = kind == VaultKindUi.PASS
    state.backend.settings.moduleGhulAuth = kind == VaultKindUi.TOTP
    state.enableModule(kind)
    state.showStart = false
    state.vaultExists = true
    state.refreshVaults()
}

/** Builds a new suite around a single vault file. */
@Composable
private fun OpenVaultWizard(state: AppState, file: PickedFile, info: ContainerInfoUi, onCancel: () -> Unit) {
    val backend = state.backend
    // U10: the kind comes from the file itself (a calendar file is no GhulAuth vault).
    val kind = vaultKindOf(info.kind)
    val hasKey = info.protection.securityKeys > 0
    val hasKeyfile = info.protection.keyfiles > 0
    var withKey by remember { mutableStateOf(hasKey) }
    val needsKey = withKey && hasKey
    var name by remember { mutableStateOf(file.name.substringBeforeLast('.')) }
    var password by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var code by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val keptCode = stringResource(Res.string.msg_code_kept)

    val current = code
    if (current != null) {
        RecoveryCodeSteps(state, current) { finishSetup(state, kind) }
        return
    }
    SectionCard(stringResource(Res.string.open_vault_title)) {
        Hint(
            stringResource(
                Res.string.open_vault_text, file.name,
                stringResource(kindLabel(kind)),
            ),
        )
        OutlinedTextField(
            name, { name = it }, label = { Text(stringResource(Res.string.l_vault_name)) },
            singleLine = true, modifier = Modifier.fillMaxWidth(),
        )
        SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
        if (needsKey) {
            Hint(stringResource(Res.string.open_vault_needs_key))
            KeyPinField(state, pin) { pin = it; error = null }
        }
        if (hasKey && hasKeyfile) FactorChoice(withKey) { withKey = it; error = null }
        if (!needsKey) KeyfilePicker(keyfile) { keyfile = it; error = null }
        Hint(stringResource(Res.string.open_vault_new_code))
        StorageChooser(state)
        ErrorText(error)
        ButtonRow {
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
            Button(
                modifier = Modifier.weight(1f),
                onClick = {
                    state.run(onError = { error = it }, block = {
                        backend.createFromVault(
                            OpenVaultUi(
                                file = file.bytes,
                                name = name.ifBlank { getString(Res.string.vault_default_name) },
                                password = password,
                                keyPin = if (needsKey) pin else null,
                                keyfile = if (needsKey) null else keyfile?.bytes,
                                recoveryLabel = getString(Res.string.setup_recovery_label),
                            ),
                            state.onTouchNeeded,
                        )
                    }) { newCode ->
                        password = ""
                        pin = ""
                        if (newCode == null) {
                            state.message(keptCode)
                            finishSetup(state, kind)
                        } else {
                            code = newCode
                        }
                    }
                },
                enabled = !state.busy && password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null),
            ) { Text(stringResource(Res.string.a_open)) }
        }
    }
}

/** Selectable card with title and explanation (used like a radio button). */
@Composable
fun ChoiceCard(selected: Boolean, title: String, text: String, onSelect: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onSelect).padding(vertical = 4.dp),
        verticalAlignment = Alignment.Top,
    ) {
        RadioButton(selected, onSelect)
        Column(Modifier.weight(1f).padding(top = 12.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Hint(text)
        }
    }
}

/** Security key or key file, when the suite has both. */
@Composable
fun FactorChoice(withKey: Boolean, onChange: (Boolean) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(withKey, { onChange(true) }, label = { Text(stringResource(Res.string.factor_key)) })
        FilterChip(!withKey, { onChange(false) }, label = { Text(stringResource(Res.string.factor_keyfile)) })
    }
}

/** Button to choose the key file plus the chosen file's name. */
@Composable
fun KeyfilePicker(selected: PickedFile?, onPicked: (PickedFile) -> Unit) {
    val open = rememberOpenFile(onPicked)
    Hint(stringResource(Res.string.keyfile_needed))
    OutlinedButton(onClick = open, modifier = Modifier.fillMaxWidth()) {
        Text(
            if (selected == null) {
                stringResource(Res.string.keyfile_choose)
            } else {
                stringResource(Res.string.keyfile_chosen, selected.name)
            },
        )
    }
}

/** Shows the new recovery code, then asks for its third group. */
@Composable
fun RecoveryCodeSteps(state: AppState, code: String, onDone: () -> Unit) {
    var confirming by remember { mutableStateOf(false) }
    var noted by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    if (!confirming) {
        SectionCard(stringResource(Res.string.setup_code_title)) {
            Hint(stringResource(Res.string.setup_code_text))
            RecoveryCodeView(code)
            PrintSheetButton(state, code)
            // The whole line toggles the box, not only the small square.
            Row(Modifier.fillMaxWidth().clickable { noted = !noted }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(noted, { noted = it })
                Text(stringResource(Res.string.setup_code_noted))
            }
            Button(onClick = { confirming = true }, enabled = noted, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.a_next))
            }
        }
    } else {
        SectionCard(stringResource(Res.string.setup_confirm_title)) {
            Hint(stringResource(Res.string.setup_confirm_text))
            OutlinedTextField(
                confirm, { confirm = it.uppercase(); error = null },
                label = { Text(stringResource(Res.string.l_code_group_3)) },
                singleLine = true, modifier = Modifier.fillMaxWidth(),
            )
            ErrorText(error)
            val wrong = stringResource(Res.string.setup_confirm_wrong)
            ButtonRow {
                OutlinedButton(onClick = { confirming = false }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
                // The core checks the group and only then marks the code as confirmed.
                Button(modifier = Modifier.weight(1f), onClick = {
                    state.run(onError = { error = it }, block = { state.backend.confirmRecoveryCode(2, confirm) }) { ok ->
                        if (ok) {
                            state.codeUnconfirmed = false
                            onDone()
                        } else {
                            error = wrong
                        }
                    }
                }, enabled = !state.busy && confirm.isNotBlank()) { Text(stringResource(Res.string.a_finish)) }
            }
        }
    }
}

@Composable
fun PrintSheetButton(state: AppState, code: String) {
    OutlinedButton(onClick = { state.printSheet(code) }, modifier = Modifier.fillMaxWidth()) {
        Text(stringResource(Res.string.a_print_sheet))
    }
    Text(stringResource(Res.string.sheet_print_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
fun RecoveryCodeView(code: String) {
    val groups = code.split('-')
    SectionCard {
        for (row in groups.chunked(3)) {
            Text(
                row.joinToString("  "),
                fontFamily = FontFamily.Monospace,
                fontSize = 20.sp,
                fontWeight = FontWeight.SemiBold,
                letterSpacing = 1.sp,
            )
        }
    }
}
