// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Fingerprint
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Pin
import androidx.compose.material.icons.filled.Save
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.filled.DeleteForever
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.KdfChoice
import de.danerd.ghulsuite.model.KeyStatus
import de.danerd.ghulsuite.model.PasswordOptionsUi
import de.danerd.ghulsuite.model.SetupUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource

/** The seven steps of a new setup, in this order. */
private val WZ_STEPS = listOf(
    Res.string.wz_step_password, Res.string.wz_step_factor, Res.string.wz_step_storage,
    Res.string.wz_step_areas, Res.string.wz_step_lock, Res.string.wz_step_unlock, Res.string.wz_step_code, Res.string.wz_step_file,
)

/** An area as offered in the wizard and in "manage areas". */
class AreaInfo(
    val kind: VaultKindUi,
    val title: StringResource,
    val sub: StringResource,
    /** The tab of the area: its colour and icon come from [areaColor] and [tabIcon], like everywhere. */
    val tab: Tab,
    val ext: String,
)

val AREAS = listOf(
    AreaInfo(VaultKindUi.PASS, Res.string.tab_pass, Res.string.area_pass_sub, Tab.PASS, ".ghlpss"),
    AreaInfo(VaultKindUi.TOTP, Res.string.tab_auth, Res.string.area_auth_sub, Tab.AUTH, ".ghltotp"),
    AreaInfo(VaultKindUi.DOCS, Res.string.tab_docs, Res.string.area_docs_sub, Tab.DOCS, ".ghldoc"),
    AreaInfo(VaultKindUi.CAL, Res.string.tab_cal, Res.string.area_cal_sub, Tab.CAL, ".ghlcal"),
)

/** Areas with a name the user picks (each vault of them has a name of its own). */
val NAMED_AREAS = setOf(VaultKindUi.PASS, VaultKindUi.TOTP, VaultKindUi.DOCS, VaultKindUi.CAL)

private enum class QuickChoice { NONE, BIO, PIN }

/** Step 3 over an existing suite: replaced after saving it, or deleted without. */
private enum class Replace { SAVE, DELETE }


/** Area visibility on this device follows the areas chosen (a file exists = shown). */
fun showModule(state: AppState, kind: VaultKindUi, on: Boolean) {
    val s = state.backend.settings
    when (kind) {
        VaultKindUi.PASS -> s.moduleGhulPass = on
        VaultKindUi.TOTP -> s.moduleGhulAuth = on
        VaultKindUi.DOCS -> s.moduleGhulDocs = on
        VaultKindUi.CAL -> s.moduleGhulCal = on
    }
}

/**
 * New setup in seven steps (#24). The suite is created when leaving step 3 (storage), the steps
 * after it need an open suite (area files). Should the app stop later, the suite
 * exists with an unconfirmed recovery code, and GhulSuite asks for a new code after unlocking.
 */
@Composable
fun NewSuiteWizard(state: AppState, onCancel: () -> Unit) {
    val backend = state.backend
    val settings = backend.settings
    var step by remember { mutableStateOf(1) }
    var created by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    // 1 + 2: protection
    var password by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var useKey by remember { mutableStateOf(true) }
    var keyLabel by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var newPin by remember { mutableStateOf("") }
    var keyStatus by remember { mutableStateOf<KeyStatus?>(null) }
    var keyfile by remember { mutableStateOf<ByteArray?>(null) }
    var keyfileSaved by remember { mutableStateOf(false) }
    var kdf by remember { mutableStateOf<KdfChoice?>(null) }
    // 2: the backup of a security key (required): a key file, or a second key added after setting up.
    var backupKey by remember { mutableStateOf(false) }
    var backupLabel by remember { mutableStateOf("") }
    var backupPin by remember { mutableStateOf("") }
    var backupDone by remember { mutableStateOf(false) }
    var code by remember { mutableStateOf("") }
    // 4: areas
    val areaOn = remember { mutableStateMapOf(VaultKindUi.PASS to true, VaultKindUi.TOTP to true, VaultKindUi.DOCS to true, VaultKindUi.CAL to true) }
    val areaName = remember { mutableStateMapOf<VaultKindUi, String>() }
    /** Area files this wizard created (switched off again on a second pass: removed). */
    val made = remember { mutableStateMapOf<VaultKindUi, Boolean>() }
    // 6: unlock
    var quick by remember { mutableStateOf(if (backend.biometricAvailable()) QuickChoice.BIO else QuickChoice.NONE) }
    var quickPin by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        kdf = catching { backend.calibrateKdf() }.getOrElse { KdfChoice(64 * 1024, 3, 0) }
        keyLabel = state.str(Res.string.setup_default_key_label)
        areaName[VaultKindUi.PASS] = state.str(Res.string.area_pass_default)
        areaName[VaultKindUi.TOTP] = state.str(Res.string.area_auth_default)
        areaName[VaultKindUi.DOCS] = state.str(Res.string.area_docs_default)
        areaName[VaultKindUi.CAL] = state.str(Res.string.area_cal_default)
    }
    val saveKeyfile = rememberSaveFile { ok -> keyfileSaved = ok; if (ok) error = null }
    val openKeyfile = rememberOpenFile { file -> keyfile = file.bytes; keyfileSaved = true; error = null }
    val notSaved = stringResource(Res.string.keyfile_not_saved)
    /** A suite on this device already (step 3 offers to open or replace it). */
    var existing by remember { mutableStateOf(backend.exists()) }
    var replacing by remember { mutableStateOf<Replace?>(null) }
    val oneArea = stringResource(Res.string.wz_one_area)
    val deletedMsg = stringResource(Res.string.wz_deleted)
    val pinRule = stringResource(Res.string.wz_pin_rule)
    val keyPinMissing = stringResource(Res.string.wz_key_pin_missing)

    /**
     * The backup of the security key: confirmed with the key just made (touched again), then the
     * key file is added, or the second key registered (touched after the first one).
     */
    fun addBackup() {
        state.run(onError = { error = it }, block = {
            val proof = de.danerd.ghulsuite.model.FactorProofUi(password, pin, null)
            if (backupKey) {
                backend.addSecurityKey(proof, backupPin, backupLabel.ifBlank { state.str(Res.string.slot_backup_key) }, state.onTouchNeeded)
            } else {
                backend.addKeyfile(proof, keyfile!!, state.str(Res.string.keyfile_default_label), state.onTouchNeeded)
            }
        }) {
            backupDone = true
            pin = ""; backupPin = ""; keyfile = null
            step = 4
        }
    }

    fun create() {
        // The choices on the screen say what to do with it (no second message).
        if (backend.exists()) { existing = true; return }
        val choice = kdf ?: KdfChoice(64 * 1024, 3, 0)
        state.run(onError = { error = it }, block = {
            backend.create(
                SetupUi(
                    password = password,
                    keyPin = if (useKey) pin else null,
                    keyLabel = keyLabel.ifBlank { state.str(Res.string.setup_default_key_label) },
                    keyfile = if (useKey) null else keyfile,
                    keyfileLabel = state.str(Res.string.keyfile_default_label),
                    recoveryLabel = state.str(Res.string.setup_recovery_label),
                    kdf = choice,
                    vaults = emptyList(),
                ),
                state.onTouchNeeded,
            )
        }) {
            code = it
            created = true
            repeat = ""
            // The password stays until quick unlock is set (step 6); a key needs its backup first.
            if (useKey) addBackup() else { keyfile = null; step = 4 }
        }
    }

    /** Creates the chosen area files (and removes those switched off again), then step 5. */
    fun applyAreas() {
        state.run(onError = { error = it }, block = {
            for (a in AREAS) {
                val on = areaOn[a.kind] == true
                if (on && made[a.kind] != true) {
                    val name = if (a.kind in NAMED_AREAS) areaName[a.kind].orEmpty() else ""
                    backend.addArea(a.kind, name)
                    made[a.kind] = true
                } else if (!on && made[a.kind] == true) {
                    backend.removeArea(a.kind)
                    made[a.kind] = false
                }
            }
        }) {
            for (a in AREAS) showModule(state, a.kind, areaOn[a.kind] == true)
            step = 5
        }
    }

    fun next() {
        error = null
        when (step) {
            1 -> scope.launch {
                error = PasswordPolicy.check(backend, password, repeat)
                if (error == null) step = 2
            }
            2 -> if (useKey) {
                // U9: "Next" without the key's PIN says why it does not go on; the backup is required.
                when {
                    !keyPinOk(state, pin) -> error = keyPinMissing
                    !backupKey && (keyfile == null || !keyfileSaved) -> error = notSaved
                    backupKey && !keyPinOk(state, backupPin) -> error = keyPinMissing
                    else -> step = 3
                }
            } else if (keyfile != null && keyfileSaved) step = 3 else error = notSaved
            // Made already, only the backup failed (e.g. the second key was not there): again.
            3 -> if (created && useKey && !backupDone) addBackup() else create()
            4 -> if (AREAS.none { areaOn[it.kind] == true }) error = oneArea else applyAreas()
            5 -> step = 6
            6 -> {
                settings.quickUnlockHours = settings.quickUnlockHours.let { if (it in listOf(4, 24, 72, 168, 0)) it else 24 }
                when (quick) {
                    QuickChoice.NONE -> {
                        backend.disableQuickUnlock()
                        password = ""
                        step = 7
                    }
                    QuickChoice.BIO -> state.run(onError = { error = it }, block = { backend.enableBiometricUnlock(password) }) { password = ""; step = 7 }
                    QuickChoice.PIN -> if (quickPin.length !in 6..12 || !quickPin.all(Char::isDigit)) error = pinRule
                    else state.run(onError = { error = it }, block = { backend.enablePinUnlock(password, quickPin) }) { quickPin = ""; password = ""; step = 7 }
                }
            }
        }
    }

    // Locked after the suite was made (screen off, idle): the wizard (and the recovery code on it)
    // goes; unlocking leads on, and the code is asked for again there (CodeReminderDialog).
    val unlockedNow by backend.unlocked.collectAsState()
    // Only a lock after it was unlocked (not before the suite is made).
    var wasUnlocked by remember { mutableStateOf(false) }
    LaunchedEffect(unlockedNow, created) {
        if (unlockedNow) wasUnlocked = true
        if (created && wasUnlocked && !unlockedNow) {
            code = ""
            state.pendingAdd = null
            state.pendingDocsAdd = null
            state.vaultExists = true
            state.showStart = false
        }
    }

    // Step 8: the suite file, at a place the user chooses (written, read back, checked).
    var fileFailed by remember { mutableStateOf(false) }
    val notWritten = stringResource(Res.string.file_not_written)
    val savedMsg = stringResource(Res.string.file_saved)
    val chooseSuiteFile = rememberCreateSuiteFile { ref ->
        if (ref != null) {
            state.run(onError = { error = it; fileFailed = true }, block = {
                backend.archiveSave(ref, deviceInfo(), created = true, state.onProgress).also { if (it) backend.setSuiteFile(ref) }
            }) { ok ->
                if (!ok) {
                    error = notWritten
                    fileFailed = true
                    return@run
                }
                state.message(savedMsg.replace("%1\$s", ref.name))
                finishWizard(state)
            }
        }
    }

    fun back() {
        error = null
        when {
            step == 1 -> onCancel()
            // The recovery code was confirmed: no way back to it.
            step == 8 -> {}
            // The suite exists from step 4 on: password, factor and storage stay as they are.
            created && step <= 4 -> {}
            else -> step--
        }
    }
    // Always on: from step 4 on "back" does nothing, but it must not reach the screen behind the
    // wizard either (that would drop the new suite's recovery code before it was shown).
    PlatformBackHandler(enabled = true) { back() }

    // Header: title, "Step n of 8 · name", progress bar.
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (!(created && step <= 4) && step != 8) BarIcon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(Res.string.a_back)) { back() }
        Column {
            Text(stringResource(Res.string.wz_title), fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Text(stringResource(Res.string.wz_step_of, step.toString(), stringResource(WZ_STEPS[step - 1])), fontSize = 13.sp, color = g.muted)
        }
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        for (i in 1..WZ_STEPS.size) Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (i <= step) g.accent else g.line))
    }
    Text(stringResource(WZ_STEPS[step - 1]), fontSize = 22.sp, fontWeight = FontWeight.Bold)
    BusyBanner(state)

    when (step) {
        1 -> {
            SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
            SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
            if (password.isNotEmpty()) StrengthMeter(backend.passwordScore(password))
            FilledTonalButton(onClick = {
                password = backend.generatePassword(PasswordOptionsUi(length = 24))
                repeat = ""
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.setup_pw_suggest)) }
            HintText(stringResource(Res.string.wz_pw_hint))
        }

        2 -> {
            ListCard {
                PickRow(useKey, Icons.Filled.Usb, 0, stringResource(Res.string.setup_use_key), stringResource(Res.string.wz_key_sub), onClick = { useKey = true }) {
                    OutlinedTextField(keyLabel, { keyLabel = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(Res.string.l_key_label)) }, singleLine = true)
                    if (!backend.keyPinInApp) {
                        KeyPinField(state, pin) { pin = it; error = null }
                    } else if (keyStatus?.pinSet == false) {
                        Hint(stringResource(Res.string.setup_key_no_pin))
                        SecretTextField(newPin, { newPin = it }, stringResource(Res.string.l_new_pin))
                        OutlinedButton(onClick = {
                            state.run(onError = { error = it }, block = { backend.setKeyPin(newPin); backend.keyInfo() }) {
                                keyStatus = it; pin = newPin; newPin = ""
                            }
                        }, enabled = !state.busy && newPin.length >= (keyStatus?.minPinLength ?: 4)) { Text(stringResource(Res.string.a_set_pin)) }
                    } else {
                        KeyPinField(state, pin) { pin = it; error = null }
                    }
                    if (backend.keyPinInApp) {
                        OutlinedButton(onClick = {
                            state.run(onError = { error = it }, block = { backend.keyInfo() }) { keyStatus = it; error = null }
                        }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_check_key)) }
                        keyStatus?.let { Hint(stringResource(Res.string.setup_key_detected, it.product)) }
                    }
                    // Holding the key to the phone (NFC) only where there is a phone.
                    HintText(stringResource(if (state.backend.isDesktop) Res.string.wz_key_touch_desktop else Res.string.wz_key_touch))
                    // Required: a backup, so access does not hang on one key that could get lost.
                    Text(stringResource(Res.string.wz_backup_title), fontWeight = FontWeight.SemiBold)
                    Hint(stringResource(Res.string.wz_backup_text))
                    ChipRow {
                        FilterChip(!backupKey, { backupKey = false; error = null }, label = { Text(stringResource(Res.string.factor_keyfile)) })
                        FilterChip(backupKey, { backupKey = true; error = null }, label = { Text(stringResource(Res.string.wz_backup_second_key)) })
                    }
                    if (backupKey) {
                        OutlinedTextField(backupLabel, { backupLabel = it }, Modifier.fillMaxWidth(), label = { Text(stringResource(Res.string.l_key_label)) }, singleLine = true)
                        KeyPinField(state, backupPin) { backupPin = it; error = null }
                        Hint(stringResource(Res.string.wz_backup_second_key_hint))
                    } else if (filePickersAvailable) {
                        FilledTonalButton(onClick = {
                            val data = keyfile ?: backend.generateKeyfile().also { keyfile = it }
                            saveKeyfile(KEYFILE_NAME, data)
                        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_create_save)) }
                        OutlinedButton(onClick = openKeyfile, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_use_existing)) }
                        if (keyfileSaved) Text(stringResource(Res.string.keyfile_ready), color = okColor())
                    }
                }
                if (filePickersAvailable) {
                    PickRow(!useKey, Icons.Filled.Description, 9, stringResource(Res.string.setup_use_keyfile), stringResource(Res.string.setup_use_keyfile_text), divider = true, onClick = { useKey = false }) {
                        FilledTonalButton(onClick = {
                            val data = keyfile ?: backend.generateKeyfile().also { keyfile = it }
                            saveKeyfile(KEYFILE_NAME, data)
                        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_create_save)) }
                        OutlinedButton(onClick = openKeyfile, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_use_existing)) }
                        if (keyfileSaved) Text(stringResource(Res.string.keyfile_ready), color = okColor())
                        HintText(stringResource(Res.string.wz_keyfile_hint))
                    }
                }
            }
            HintText(stringResource(Res.string.wz_factor_hint))
        }

        3 -> {
            LaunchedEffect(Unit) { existing = backend.exists() }
            HintText(stringResource(Res.string.wz_create_hint))
            if (existing) {
                // Once, with the three ways on: open it, save it and replace it, delete it and replace it.
                Banner(Icons.Filled.Warning, stringResource(Res.string.wz_exists))
                StartOption(Icons.Filled.Folder, paletteColor(2), stringResource(Res.string.storage_open_existing), stringResource(Res.string.wz_exists_open_sub)) {
                    state.showStart = false; state.vaultExists = true
                }
                if (filePickersAvailable) {
                    StartOption(Icons.Filled.Save, paletteColor(0), stringResource(Res.string.wz_replace_save), stringResource(Res.string.wz_replace_save_sub)) {
                        error = null; replacing = Replace.SAVE
                    }
                }
                StartOption(Icons.Filled.DeleteForever, paletteColor(7), stringResource(Res.string.wz_replace_delete), stringResource(Res.string.wz_replace_delete_sub)) {
                    error = null; replacing = Replace.DELETE
                }
            }
            if (useKey) HintText(stringResource(Res.string.wz_create_touch))
            if (kdf == null) Hint(stringResource(Res.string.setup_calibrating))
        }

        4 -> {
            for (a in AREAS) {
                val on = areaOn[a.kind] == true
                ListCard {
                    Row(
                        Modifier.fillMaxWidth().clickable { areaOn[a.kind] = !on }
                            .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        IconTile(tabIcon(a.tab), areaColor(a.tab))
                        Column(Modifier.weight(1f)) {
                            Text(stringResource(a.title), fontWeight = FontWeight.W500, fontSize = 15.sp, color = if (on) g.fg else g.muted)
                            Text(stringResource(a.sub), fontSize = 14.sp, color = g.muted)
                        }
                        Switch(on, { areaOn[a.kind] = it })
                    }
                    if (on) {
                        Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedTextField(
                                areaName[a.kind].orEmpty(), { areaName[a.kind] = it }, Modifier.fillMaxWidth(),
                                label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true, enabled = made[a.kind] != true,
                            )
                        }
                    }
                }
            }
            HintText(stringResource(Res.string.wz_areas_hint))
        }

        5 -> {
            SectionCard(stringResource(Res.string.set_auto_lock)) {
                Hint(stringResource(Res.string.wz_lock_sub))
                FilterChipRow(AUTO_LOCK_OPTIONS.map { it.first.toString() to stringResource(it.second) }, settings.autoLockSeconds.toString(),
                    { settings.autoLockSeconds = it.toInt() })
            }
            ListCard {
                Column(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
                    SwitchRow(stringResource(if (backend.isDesktop) Res.string.set_lock_session else Res.string.set_lock_screen_off), settings.lockOnScreenOff) { settings.lockOnScreenOff = it }
                    if (!backend.isDesktop) {
                        SwitchRow(stringResource(Res.string.set_shake), settings.shakeToLock) { settings.shakeToLock = it }
                        Hint(stringResource(Res.string.wz_shake_sub))
                    }
                }
            }
            HintText(stringResource(Res.string.wz_lock_hint))
        }

        6 -> {
            HintText(stringResource(Res.string.wz_quick_explain))
            ListCard {
                PickRow(quick == QuickChoice.NONE, Icons.Filled.Usb, 0, stringResource(Res.string.wz_quick_none), stringResource(Res.string.wz_quick_none_sub), onClick = { quick = QuickChoice.NONE })
                PickRow(quick == QuickChoice.BIO, Icons.Filled.Fingerprint, 2, stringResource(Res.string.wz_quick_bio),
                    stringResource(if (backend.biometricAvailable()) Res.string.wz_quick_bio_sub else Res.string.set_quick_bio_unavailable),
                    enabled = backend.biometricAvailable(), divider = true, onClick = { quick = QuickChoice.BIO })
                // U8: Windows has no app PIN (and no fingerprint unlock yet): not offered there.
                if (!backend.isDesktop) {
                    PickRow(quick == QuickChoice.PIN, Icons.Filled.Pin, 5, stringResource(Res.string.wz_quick_pin), stringResource(Res.string.wz_quick_pin_sub), divider = true, onClick = { quick = QuickChoice.PIN }) {
                        SecretTextField(quickPin, { quickPin = it.filter(Char::isDigit).take(12); error = null }, stringResource(Res.string.wz_quick_pin_field), numeric = true)
                    }
                }
            }
            if (backend.isDesktop) Hint(stringResource(Res.string.wz_quick_desktop))
            if (quick != QuickChoice.NONE) {
                SectionCard(stringResource(Res.string.wz_quick_valid)) {
                    Hint(stringResource(Res.string.wz_quick_valid_sub))
                    FilterChipRow(QUICK_HOURS_OPTIONS.map { it.first.toString() to stringResource(it.second) }, settings.quickUnlockHours.toString(),
                        { settings.quickUnlockHours = it.toInt() })
                    if (settings.quickUnlockHours == 0) Hint(stringResource(Res.string.wz_quick_always_note))
                }
            }
            HintText(stringResource(Res.string.set_quick_always_full))
        }

        // The suite file is part of setting up (without it the suite is on this device only).
        7 -> RecoveryCodeSteps(state, code) { if (filePickersAvailable) step = 8 else finishWizard(state) }

        8 -> {
            HintText(stringResource(Res.string.setup_save_text))
            Hint(stringResource(Res.string.save_hint))
            Button(onClick = { chooseSuiteFile(SUITE_FILE_NAME) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().testTag("wz-file")) {
                Text(stringResource(Res.string.save_place))
            }
            // Only after it failed (no stick, no room): on without it, with a clear warning.
            if (fileFailed) {
                Banner(Icons.Filled.Warning, stringResource(Res.string.wz_file_failed))
                OutlinedButton(onClick = { finishWizard(state) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.wz_file_skip))
                }
            }
        }
    }

    when (replacing) {
        Replace.SAVE -> SaveExistingSuiteDialog(state, onReplaced = { replacing = null; existing = false; create() }) { replacing = null }
        Replace.DELETE -> DeleteExistingSuiteDialog(onConfirm = {
            replacing = null
            state.run(onError = { error = it }, block = {
                backend.deleteSuite()
                state.vaultExists = backend.exists()
            }) {
                existing = false
                state.message(deletedMsg)
                create()
            }
        }) { replacing = null }
        null -> {}
    }

    ErrorText(error)
    if (step < 7) {
        ButtonRow {
            if (step > 1 && !(created && step <= 4)) OutlinedButton(onClick = { back() }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
            Button(
                onClick = { next() },
                enabled = !state.busy && (step != 3 || (kdf != null && !existing)),
                modifier = Modifier.weight(1f),
            ) {
                Text(stringResource(when {
                    step == 3 && useKey -> Res.string.a_register_key
                    step == 3 -> Res.string.a_setup
                    else -> Res.string.a_next
                }))
            }
        }
    }
}

/** End of the wizard: leave the start screen and show the areas. */
private fun finishWizard(state: AppState) {
    // The suite was unlocked before its areas existed: start in the first area, not in Options.
    state.openFirstArea = true
    // A vault file waiting for the suite before belongs to that one, not to this new suite.
    state.pendingAdd = null
    state.pendingDocsAdd = null
    state.showStart = false
    state.vaultExists = true
    state.refreshVaults()
    state.changed()
}

/** Selectable row (radio) with icon tile; [more] opens under it while selected. */
@Composable
fun PickRow(
    selected: Boolean,
    icon: ImageVector,
    colorIndex: Int,
    title: String,
    sub: String?,
    enabled: Boolean = true,
    divider: Boolean = false,
    onClick: () -> Unit,
    more: (@Composable ColumnScope.() -> Unit)? = null,
) {
    if (divider) RowDivider()
    Row(
        Modifier.fillMaxWidth().then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IconTile(icon, if (enabled) paletteColor(colorIndex) else g.faint)
        Column(Modifier.weight(1f)) {
            Text(title, fontWeight = FontWeight.W500, fontSize = 15.sp, color = if (enabled) Color.Unspecified else g.muted)
            if (!sub.isNullOrBlank()) Text(sub, fontSize = 14.sp, color = g.muted)
        }
        RadioButton(selected && enabled, if (enabled) onClick else null, enabled = enabled)
    }
    if (selected && enabled && more != null) {
        Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp), content = more)
    }
}
