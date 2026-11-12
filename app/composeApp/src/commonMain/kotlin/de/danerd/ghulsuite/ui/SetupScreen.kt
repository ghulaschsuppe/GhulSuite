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

private enum class NewStep { PROTECTION, PASSWORD, KEYFILE, VAULTS, KEY, CODE }

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

    PlatformBackHandler(enabled = flow != Flow.START) { flow = Flow.START }

    ScreenColumn {
        BrandHeader(if (flow == Flow.START) stringResource(Res.string.start_tagline) else null)
        BusyBanner(state)
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
                        restoreBackup()
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
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = g.faint)
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

/** Finish of every setup: switch on the modules of the vaults and leave the start screen. */
private fun finishSetup(state: AppState, pass: Boolean, totp: Boolean) {
    state.backend.settings.moduleGhulPass = pass
    state.backend.settings.moduleGhulAuth = totp
    state.showStart = false
    state.vaultExists = true
    state.refreshVaults()
}

@Composable
private fun NewSuiteWizard(state: AppState, onCancel: () -> Unit) {
    val backend = state.backend
    var step by remember { mutableStateOf(NewStep.PROTECTION) }
    // Second factor: security key (recommended) or key file. Both can be added or swapped later.
    var useKey by remember { mutableStateOf(true) }
    val useKeyfile = !useKey
    var kdf by remember { mutableStateOf<KdfChoice?>(null) }
    var password by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<ByteArray?>(null) }
    var keyfileSaved by remember { mutableStateOf(false) }
    var pass by remember { mutableStateOf(true) }
    var passName by remember { mutableStateOf("") }
    var totp by remember { mutableStateOf(true) }
    var totpName by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var newPin by remember { mutableStateOf("") }
    var keyLabel by remember { mutableStateOf("") }
    var keyStatus by remember { mutableStateOf<KeyStatus?>(null) }
    var code by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(Unit) {
        kdf = runCatching { backend.calibrateKdf() }.getOrElse { KdfChoice(64 * 1024, 3, 0) }
        keyLabel = getString(Res.string.setup_default_key_label)
        passName = getString(Res.string.vault_default_name)
        totpName = getString(Res.string.vault_default_name)
    }

    val saveKeyfile = rememberSaveFile { ok -> keyfileSaved = ok; if (ok) error = null }
    val notSaved = stringResource(Res.string.keyfile_not_saved)
    val openKeyfile = rememberOpenFile { file -> keyfile = file.bytes; keyfileSaved = true; error = null }

    fun create() {
        val choice = kdf ?: KdfChoice(64 * 1024, 3, 0)
        state.run(onError = { error = it }, block = {
            val vaults = buildList {
                if (pass) add(NewVaultUi(VaultKindUi.PASS, passName.ifBlank { getString(Res.string.vault_default_name) }))
                if (totp) add(NewVaultUi(VaultKindUi.TOTP, totpName.ifBlank { getString(Res.string.vault_default_name) }))
            }
            backend.create(
                SetupUi(
                    password = password,
                    keyPin = if (useKey) pin else null,
                    keyLabel = keyLabel.ifBlank { getString(Res.string.setup_default_key_label) },
                    keyfile = if (useKeyfile) keyfile else null,
                    keyfileLabel = getString(Res.string.keyfile_default_label),
                    recoveryLabel = getString(Res.string.setup_recovery_label),
                    kdf = choice,
                    vaults = vaults,
                ),
                state.onTouchNeeded,
            )
        }) {
            code = it
            password = ""
            repeat = ""
            pin = ""
            keyfile = null
            step = NewStep.CODE
        }
    }

    when (step) {
        NewStep.PROTECTION -> SectionCard(stringResource(Res.string.setup_protection_title)) {
            Hint(stringResource(Res.string.setup_protection_text))
            ChoiceCard(useKey, stringResource(Res.string.setup_use_key), stringResource(Res.string.setup_use_key_on)) { useKey = true }
            if (filePickersAvailable) {
                ChoiceCard(!useKey, stringResource(Res.string.setup_use_keyfile), stringResource(Res.string.setup_use_keyfile_text)) {
                    useKey = false
                }
            }
            Hint(stringResource(Res.string.setup_swap_later))
            Hint(stringResource(Res.string.setup_recovery_always))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = onCancel) { Text(stringResource(Res.string.a_back)) }
                Button(onClick = { step = NewStep.PASSWORD }) { Text(stringResource(Res.string.a_next)) }
            }
        }

        NewStep.PASSWORD -> SectionCard(stringResource(Res.string.setup_pw_title)) {
            Hint(stringResource(Res.string.setup_pw_text))
            SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
            StrengthMeter(backend.passwordScore(password))
            SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
            TextButton(onClick = {
                password = backend.generatePassword(PasswordOptionsUi(length = 24))
                repeat = ""
            }) { Text(stringResource(Res.string.setup_pw_suggest)) }
            ErrorText(error)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { step = NewStep.PROTECTION; error = null }) { Text(stringResource(Res.string.a_back)) }
                Button(onClick = {
                    scope.launch {
                        error = PasswordPolicy.check(backend, password, repeat)
                        if (error == null) step = if (useKeyfile) NewStep.KEYFILE else NewStep.VAULTS
                    }
                }) { Text(stringResource(Res.string.a_next)) }
            }
        }

        NewStep.KEYFILE -> SectionCard(stringResource(Res.string.keyfile_title)) {
            Hint(stringResource(Res.string.keyfile_text))
            Button(onClick = {
                val data = keyfile ?: backend.generateKeyfile().also { keyfile = it }
                saveKeyfile(KEYFILE_NAME, data)
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_create_save)) }
            OutlinedButton(onClick = openKeyfile, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.keyfile_use_existing))
            }
            if (keyfileSaved) Text(stringResource(Res.string.keyfile_ready), color = okColor())
            ErrorText(error)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { step = NewStep.PASSWORD }) { Text(stringResource(Res.string.a_back)) }
                Button(onClick = {
                    if (keyfile != null && keyfileSaved) step = NewStep.VAULTS else error = notSaved
                }) { Text(stringResource(Res.string.a_next)) }
            }
        }

        NewStep.VAULTS -> SectionCard(stringResource(Res.string.setup_vaults_title)) {
            Hint(stringResource(Res.string.setup_vaults_text))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(pass, { pass = it })
                Text(stringResource(Res.string.tab_pass) + " – " + stringResource(Res.string.module_pass_text))
            }
            if (pass) {
                OutlinedTextField(
                    passName, { passName = it }, label = { Text(stringResource(Res.string.l_vault_name)) },
                    singleLine = true, modifier = Modifier.fillMaxWidth(),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(totp, { totp = it })
                Text(stringResource(Res.string.tab_auth) + " – " + stringResource(Res.string.module_auth_text))
            }
            if (totp) {
                OutlinedTextField(
                    totpName, { totpName = it }, label = { Text(stringResource(Res.string.l_vault_name)) },
                    singleLine = true, modifier = Modifier.fillMaxWidth(),
                )
            }
            Hint(stringResource(Res.string.setup_vaults_more))
            StorageChooser(state)
            ErrorText(error)
            if (!useKey && kdf == null) Hint(stringResource(Res.string.setup_calibrating))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { step = if (useKeyfile) NewStep.KEYFILE else NewStep.PASSWORD; error = null }) {
                    Text(stringResource(Res.string.a_back))
                }
                if (useKey) {
                    Button(onClick = { step = NewStep.KEY }) { Text(stringResource(Res.string.a_next)) }
                } else {
                    Button(onClick = { create() }, enabled = !state.busy && kdf != null) { Text(stringResource(Res.string.a_setup)) }
                }
            }
        }

        NewStep.KEY -> SectionCard(stringResource(Res.string.setup_key_title)) {
            Hint(stringResource(Res.string.setup_key_text))
            OutlinedTextField(
                keyLabel, { keyLabel = it }, label = { Text(stringResource(Res.string.l_key_label)) },
                singleLine = true, modifier = Modifier.fillMaxWidth(),
            )
            if (!backend.keyPinInApp) {
                KeyPinField(state, pin) { pin = it; error = null }
            } else if (keyStatus?.pinSet == false) {
                Hint(stringResource(Res.string.setup_key_no_pin))
                SecretTextField(newPin, { newPin = it }, stringResource(Res.string.l_new_pin))
                OutlinedButton(onClick = {
                    state.run(onError = { error = it }, block = { backend.setKeyPin(newPin); backend.keyInfo() }) {
                        keyStatus = it
                        pin = newPin
                        newPin = ""
                    }
                }, enabled = !state.busy && newPin.length >= (keyStatus?.minPinLength ?: 4)) {
                    Text(stringResource(Res.string.a_set_pin))
                }
            } else {
                KeyPinField(state, pin) { pin = it; error = null }
            }
            ErrorText(error)
            keyStatus?.let { Hint(stringResource(Res.string.setup_key_detected, it.product)) }
            if (kdf == null) Hint(stringResource(Res.string.setup_calibrating))
            // The system dialog (Windows) checks the key itself; no separate check there.
            if (backend.keyPinInApp) OutlinedButton(onClick = {
                state.run(onError = { error = it }, block = { backend.keyInfo() }) { keyStatus = it; error = null }
            }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_check_key)) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { step = NewStep.VAULTS; error = null }) { Text(stringResource(Res.string.a_back)) }
                Button(onClick = { create() }, enabled = !state.busy && keyPinOk(state, pin) && kdf != null) {
                    Text(stringResource(Res.string.a_register_key))
                }
            }
        }

        NewStep.CODE -> RecoveryCodeSteps(state, code) { finishSetup(state, pass, totp) }
    }
}

/** Builds a new suite around a single vault file. */
@Composable
private fun OpenVaultWizard(state: AppState, file: PickedFile, info: ContainerInfoUi, onCancel: () -> Unit) {
    val backend = state.backend
    val kind = if (info.kind == "pass") VaultKindUi.PASS else VaultKindUi.TOTP
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
        RecoveryCodeSteps(state, current) { finishSetup(state, kind == VaultKindUi.PASS, kind == VaultKindUi.TOTP) }
        return
    }
    SectionCard(stringResource(Res.string.open_vault_title)) {
        Hint(
            stringResource(
                Res.string.open_vault_text, file.name,
                stringResource(if (kind == VaultKindUi.PASS) Res.string.tab_pass else Res.string.tab_auth),
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
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onCancel) { Text(stringResource(Res.string.a_back)) }
            Button(
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
                            finishSetup(state, kind == VaultKindUi.PASS, kind == VaultKindUi.TOTP)
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
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { confirming = false }) { Text(stringResource(Res.string.a_back)) }
                Button(onClick = {
                    val group = code.split('-').getOrNull(2).orEmpty()
                    if (confirm.trim().replace('O', '0') == group.replace('O', '0')) onDone() else error = wrong
                }) { Text(stringResource(Res.string.a_finish)) }
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
