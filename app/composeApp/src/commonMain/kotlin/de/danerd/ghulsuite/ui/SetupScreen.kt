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
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
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

private enum class Flow { START, NEW, OPEN_VAULT, OPEN_FILE, OPEN_SINGLE }

const val KEYFILE_NAME = "ghulsuite.ghlkey"

@Composable
fun ScreenColumn(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 100.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = LocalContentMax.current).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            content()
        }
    }
}

/** First start: set up a new suite, open an existing one or build one around a single vault file. */
@Composable
fun SetupScreen(state: AppState) {
    var flow by remember { mutableStateOf(Flow.START) }
    var vaultFile by remember { mutableStateOf<PickedVaultFile?>(null) }
    /** A vault file was picked: ask what to do with it. */
    var choose by remember { mutableStateOf(false) }
    var vaultInfo by remember { mutableStateOf<ContainerInfoUi?>(null) }
    var suiteFile by remember { mutableStateOf<PickedSuiteFile?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val backend = state.backend
    val scope = rememberCoroutineScope()

    val isSuiteFile = stringResource(Res.string.start_is_suite_file)
    /** A GhulDocs vault saved on its own was picked: what to do with it (no bytes: it is read as a stream). */
    fun docsVaultPicked(ref: de.danerd.ghulsuite.model.SuiteFileRef, protection: de.danerd.ghulsuite.model.ProtectionUi) {
        vaultFile = PickedVaultFile(ref, ByteArray(0))
        vaultInfo = ContainerInfoUi("docs", protection)
        error = null
        choose = true
    }
    val openSuiteFile = rememberOpenSuiteFile { ref ->
        suiteFile = PickedSuiteFile(ref)
        error = null
        flow = Flow.OPEN_FILE
    }
    // With access to write: a vault opened on its own goes back into its file.
    val openFile = rememberOpenSuiteFile { ref ->
        scope.launch {
            // A .ghlzip is a suite file or a GhulDocs vault: only its first entry is read.
            val archive = catching { backend.archiveInspect(ref) }.getOrNull()
            when (archive?.content) {
                de.danerd.ghulsuite.model.ArchiveContentUi.SUITE -> { error = isSuiteFile; return@launch }
                de.danerd.ghulsuite.model.ArchiveContentUi.DOCS_VAULT -> { docsVaultPicked(ref, archive.protection); return@launch }
                null -> {}
            }
            catching {
                val data = backend.readSuiteFile(ref) ?: throw VaultException(VaultErrorKind.STORAGE, "the file cannot be read")
                data to backend.inspectFile(data)
            }
                .onSuccess { (data, info) ->
                    if (info.kind == "suite") {
                        error = isSuiteFile
                    } else {
                        vaultFile = PickedVaultFile(ref, data)
                        vaultInfo = info
                        error = null
                        choose = true
                    }
                }
                .onFailure { error = describe(it) }
        }
    }

    // A suite file handed over by another app (file manager, double click): opened as if picked here.
    LaunchedEffect(Unit) {
        AppLaunch.suiteFile.collect { f ->
            if (f != null && !backend.unlocked.value) {
                AppLaunch.suiteFile.value = null
                suiteFile = f
                error = null
                flow = Flow.OPEN_FILE
            }
        }
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
                if (filePickersAvailable) {
                    StartOption(Icons.Filled.Folder, paletteColor(2), stringResource(Res.string.start_open_suite), stringResource(Res.string.start_open_suite_text)) {
                        error = null
                        openSuiteFile()
                    }
                    StartOption(Icons.Filled.Description, paletteColor(3), stringResource(Res.string.start_open_vault), stringResource(Res.string.start_open_vault_text)) {
                        error = null
                        openFile()
                    }
                }
                HintText(stringResource(Res.string.start_footer))
                ErrorText(error)
                if (state.showStart && backend.exists()) {
                    TextButton(onClick = { state.showStart = false; state.vaultExists = true }) {
                        Text(stringResource(Res.string.start_back_to_suite))
                    }
                }
            }

            Flow.NEW -> NewSuiteWizard(state) { flow = Flow.START }

            Flow.OPEN_FILE -> suiteFile?.let { f ->
                OpenSuiteFile(
                    state, f, onOpened = { suiteFile = null; flow = Flow.START }, onCancel = { suiteFile = null; flow = Flow.START },
                    // A GhulDocs vault saved on its own: open only it, or add it to the own suite.
                    onDocsVault = { protection ->
                        suiteFile = null
                        flow = Flow.START
                        docsVaultPicked(f.source, protection)
                    },
                )
            }

            Flow.OPEN_VAULT -> {
                val file = vaultFile
                val info = vaultInfo
                if (file != null && info != null) {
                    OpenVaultWizard(state, PickedFile(file.ref.name, file.data), info) { flow = Flow.START }
                }
            }

            Flow.OPEN_SINGLE -> {
                val file = vaultFile
                val info = vaultInfo
                if (file != null && info != null) {
                    OpenSingleVault(state, file, info) { flow = Flow.START }
                }
            }
        }
    }

    val picked = vaultFile
    val pickedInfo = vaultInfo
    if (choose && picked != null && pickedInfo != null) {
        VaultFileChoice(
            state, picked, pickedInfo,
            onOnlyThis = { choose = false; flow = Flow.OPEN_SINGLE },
            onNewSuite = { choose = false; flow = Flow.OPEN_VAULT },
            onCancel = { choose = false },
        )
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

/** Finish of a setup around one vault file: only the area of that vault is switched on. */
private fun finishSetup(state: AppState, kind: VaultKindUi) {
    state.pendingAdd = null
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
    ChipRow {
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
