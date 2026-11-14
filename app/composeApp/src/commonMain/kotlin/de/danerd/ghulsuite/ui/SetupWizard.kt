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
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Fingerprint
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Mail
import androidx.compose.material.icons.filled.Pin
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

/** The eight steps of a new setup, in this order (sync comes before the areas so Drive can use it). */
private val WZ_STEPS = listOf(
    Res.string.wz_step_password, Res.string.wz_step_factor, Res.string.wz_step_storage, Res.string.wz_step_sync,
    Res.string.wz_step_areas, Res.string.wz_step_lock, Res.string.wz_step_unlock, Res.string.wz_step_code,
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
    AreaInfo(VaultKindUi.DRIVE, Res.string.tab_drive, Res.string.area_drive_sub, Tab.DRIVE, ".ghldrv"),
    AreaInfo(VaultKindUi.MAIL, Res.string.tab_mail, Res.string.area_mail_sub, Tab.MAIL, ".ghlmail"),
    AreaInfo(VaultKindUi.CAL, Res.string.tab_cal, Res.string.area_cal_sub, Tab.CAL, ".ghlcal"),
)

/** Areas with a name the user picks (Drive and Mail have one file without a name of its own). */
val NAMED_AREAS = setOf(VaultKindUi.PASS, VaultKindUi.TOTP, VaultKindUi.CAL)

private enum class DriveChoice { SYNC, OWN, LATER }

private enum class QuickChoice { NONE, BIO, PIN }


/** Area visibility on this device follows the areas chosen (a file exists = shown). */
fun showModule(state: AppState, kind: VaultKindUi, on: Boolean) {
    val s = state.backend.settings
    when (kind) {
        VaultKindUi.PASS -> s.moduleGhulPass = on
        VaultKindUi.TOTP -> s.moduleGhulAuth = on
        VaultKindUi.DRIVE -> s.moduleGhulDrive = on
        VaultKindUi.MAIL -> s.moduleGhulMail = on
        VaultKindUi.CAL -> s.moduleGhulCal = on
    }
}

/**
 * New setup in eight steps (#24). The suite is created when leaving step 3 (storage), the steps
 * after it need an open suite (sync server check, area files). Should the app stop later, the suite
 * exists with an unconfirmed recovery code, and GhulSuite asks for a new code after unlocking.
 */
@Composable
fun NewSuiteWizard(state: AppState, onCancel: () -> Unit) {
    val backend = state.backend
    val net = backend.net
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
    var code by remember { mutableStateOf("") }
    // 3: storage
    var inFolder by remember { mutableStateOf(false) }
    var storageName by remember { mutableStateOf(backend.storageName()) }
    // 4: sync
    var syncOn by remember { mutableStateOf(false) }
    val syncForm = rememberServerForm()
    var syncId by remember { mutableStateOf<String?>(null) }
    // 5: areas
    val areaOn = remember { mutableStateMapOf(VaultKindUi.PASS to true, VaultKindUi.TOTP to true, VaultKindUi.DRIVE to false, VaultKindUi.MAIL to false, VaultKindUi.CAL to true) }
    val areaName = remember { mutableStateMapOf<VaultKindUi, String>() }
    /** Area files this wizard created (switched off again on a second pass: removed). */
    val made = remember { mutableStateMapOf<VaultKindUi, Boolean>() }
    var drive by remember { mutableStateOf(DriveChoice.LATER) }
    val driveForm = rememberServerForm()
    var mailSheet by remember { mutableStateOf(false) }
    var mailAdded by remember { mutableStateOf(false) }
    // 7: unlock
    var quick by remember { mutableStateOf(if (backend.biometricAvailable()) QuickChoice.BIO else QuickChoice.NONE) }
    var quickPin by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        kdf = runCatching { backend.calibrateKdf() }.getOrElse { KdfChoice(64 * 1024, 3, 0) }
        keyLabel = getString(Res.string.setup_default_key_label)
        areaName[VaultKindUi.PASS] = getString(Res.string.area_pass_default)
        areaName[VaultKindUi.TOTP] = getString(Res.string.area_auth_default)
        areaName[VaultKindUi.CAL] = getString(Res.string.area_cal_default)
    }
    val saveKeyfile = rememberSaveFile { ok -> keyfileSaved = ok; if (ok) error = null }
    val openKeyfile = rememberOpenFile { file -> keyfile = file.bytes; keyfileSaved = true; error = null }
    val pickFolder = rememberPickFolder { folder ->
        backend.useStorage(folder)
        storageName = backend.storageName()
        inFolder = true
    }
    val notSaved = stringResource(Res.string.keyfile_not_saved)
    val suiteThere = stringResource(Res.string.storage_suite_exists)
    val missingFmt = stringResource(Res.string.srvf_missing)
    val driveMissingFmt = stringResource(Res.string.wz_drive_missing)
    val oneArea = stringResource(Res.string.wz_one_area)
    val pinRule = stringResource(Res.string.wz_pin_rule)

    fun create() {
        if (backend.exists()) { error = suiteThere; return }
        val choice = kdf ?: KdfChoice(64 * 1024, 3, 0)
        state.run(onError = { error = it }, block = {
            backend.create(
                SetupUi(
                    password = password,
                    keyPin = if (useKey) pin else null,
                    keyLabel = keyLabel.ifBlank { getString(Res.string.setup_default_key_label) },
                    keyfile = if (useKey) null else keyfile,
                    keyfileLabel = getString(Res.string.keyfile_default_label),
                    recoveryLabel = getString(Res.string.setup_recovery_label),
                    kdf = choice,
                    vaults = emptyList(),
                ),
                state.onTouchNeeded,
            )
        }) {
            code = it
            created = true
            password = ""; repeat = ""; pin = ""; keyfile = null
            step = 4
        }
    }

    /** Creates the chosen area files (and removes those switched off again), then step 6. */
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
            if (areaOn[VaultKindUi.DRIVE] == true && drive == DriveChoice.SYNC) net.driveUseSync()
        }) {
            for (a in AREAS) showModule(state, a.kind, areaOn[a.kind] == true)
            step = 6
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
                if (keyPinOk(state, pin)) step = 3
            } else if (keyfile != null && keyfileSaved) step = 3 else error = notSaved
            3 -> create()
            4 -> if (!syncOn) step = 5 else {
                val id = syncId
                val missing = syncForm.missing()
                if (id == null || missing != null) {
                    scope.launch { error = missingFmt.replace("%1\$s", getString(missing ?: Res.string.srvf_step_test)) }
                } else {
                    state.run(onError = { error = it }, block = { net.setSyncServer(id) }) { step = 5 }
                }
            }
            5 -> {
                val driveMissing = driveForm.missing()
                when {
                    AREAS.none { areaOn[it.kind] == true } -> error = oneArea
                    areaOn[VaultKindUi.DRIVE] == true && drive == DriveChoice.OWN && driveMissing != null ->
                        scope.launch { error = driveMissingFmt.replace("%1\$s", getString(driveMissing)) }
                    else -> applyAreas()
                }
            }
            6 -> step = 7
            7 -> {
                settings.quickUnlockHours = settings.quickUnlockHours.let { if (it in listOf(4, 24, 72, 168, 0)) it else 24 }
                when (quick) {
                    QuickChoice.NONE -> {
                        backend.disableQuickUnlock()
                        step = 8
                    }
                    QuickChoice.BIO -> state.run(onError = { error = it }, block = { backend.enableBiometricUnlock() }) { step = 8 }
                    QuickChoice.PIN -> if (quickPin.length !in 6..12 || !quickPin.all(Char::isDigit)) error = pinRule
                    else state.run(onError = { error = it }, block = { backend.enablePinUnlock(quickPin) }) { quickPin = ""; step = 8 }
                }
            }
        }
    }

    fun back() {
        error = null
        when {
            step == 1 -> onCancel()
            // The suite exists from step 4 on: password, factor and storage stay as they are.
            created && step <= 4 -> {}
            else -> step--
        }
    }
    PlatformBackHandler(enabled = !(created && step <= 4)) { back() }

    // Header: title, "Step n of 8 · name", progress bar.
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (!(created && step <= 4)) BarIcon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(Res.string.a_back)) { back() }
        Column {
            Text(stringResource(Res.string.wz_title), fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Text(stringResource(Res.string.wz_step_of, step.toString(), stringResource(WZ_STEPS[step - 1])), fontSize = 13.sp, color = g.muted)
        }
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        for (i in 1..8) Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (i <= step) g.accent else g.line))
    }
    Text(stringResource(WZ_STEPS[step - 1]), fontSize = 22.sp, fontWeight = FontWeight.Bold)
    BusyBanner(state)

    when (step) {
        1 -> {
            SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
            SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
            StrengthMeter(backend.passwordScore(password))
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
            ListCard {
                PickRow(!inFolder, Icons.Filled.Smartphone, 5, stringResource(Res.string.storage_app), stringResource(Res.string.wz_storage_app_sub), onClick = {
                    backend.useStorage(null); storageName = backend.storageName(); inFolder = false
                })
                if (filePickersAvailable) {
                    PickRow(inFolder, Icons.Filled.Folder, 9, stringResource(Res.string.storage_folder), stringResource(Res.string.wz_storage_folder_sub), divider = true, onClick = pickFolder) {
                        Hint(stringResource(Res.string.storage_current, storageLabel(storageName)))
                    }
                }
            }
            HintText(stringResource(Res.string.wz_storage_hint))
            if (backend.exists()) {
                Text(suiteThere, color = warnColor())
                TextButton(onClick = { state.showStart = false; state.vaultExists = true }) { Text(stringResource(Res.string.storage_open_existing)) }
            }
            if (useKey) HintText(stringResource(Res.string.wz_create_touch))
            if (kdf == null) Hint(stringResource(Res.string.setup_calibrating))
        }

        4 -> {
            ListCard {
                PickRow(!syncOn, Icons.Filled.Smartphone, 5, stringResource(Res.string.wz_later), stringResource(Res.string.wz_sync_later_sub), onClick = { syncOn = false })
                PickRow(syncOn, Icons.Filled.Dns, 1, stringResource(Res.string.wz_own_server), stringResource(Res.string.wz_sync_own_sub), divider = true, onClick = { syncOn = true }) {
                    ServerSetup(state, syncForm, { input, fp -> net.saveServer(input, fp) }) { syncId = it }
                }
            }
            HintText(stringResource(Res.string.wz_sync_hint))
        }

        5 -> {
            val syncReady = syncOn && syncId != null
            for (a in AREAS) {
                val on = areaOn[a.kind] == true
                ListCard {
                    Row(
                        Modifier.fillMaxWidth().clickable { areaOn[a.kind] = !on; if (a.kind == VaultKindUi.DRIVE && !on) drive = if (syncReady) DriveChoice.SYNC else DriveChoice.LATER }
                            .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        IconTile(tabIcon(a.tab), areaColor(a.tab))
                        Column(Modifier.weight(1f)) {
                            Text(stringResource(a.title), fontWeight = FontWeight.W500, fontSize = 15.sp, color = if (on) g.fg else g.muted)
                            Text(stringResource(a.sub), fontSize = 14.sp, color = g.muted)
                        }
                        Switch(on, { areaOn[a.kind] = it; if (a.kind == VaultKindUi.DRIVE && it) drive = if (syncReady) DriveChoice.SYNC else DriveChoice.LATER })
                    }
                    if (on) {
                        Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            when (a.kind) {
                                VaultKindUi.PASS, VaultKindUi.TOTP, VaultKindUi.CAL -> OutlinedTextField(
                                    areaName[a.kind].orEmpty(), { areaName[a.kind] = it }, Modifier.fillMaxWidth(),
                                    label = { Text(stringResource(Res.string.l_vault_name)) }, singleLine = true, enabled = made[a.kind] != true,
                                )
                                VaultKindUi.DRIVE -> {
                                    Hint(stringResource(Res.string.wz_drive_where))
                                    SheetList {
                                        PickRow(drive == DriveChoice.SYNC, Icons.Filled.Sync, 2, stringResource(Res.string.drive_like_sync),
                                            stringResource(if (syncReady) Res.string.drive_like_sync_sub else Res.string.wz_drive_sync_first),
                                            enabled = syncReady, onClick = { drive = DriveChoice.SYNC })
                                        PickRow(drive == DriveChoice.OWN, Icons.Filled.Dns, 1, stringResource(Res.string.wz_own_server), stringResource(Res.string.drive_own_sub),
                                            divider = true, onClick = { drive = DriveChoice.OWN }) {
                                            ServerSetup(state, driveForm, { input, fp -> net.driveSaveServer(input, fp) }) { made[VaultKindUi.DRIVE] = true }
                                        }
                                        PickRow(drive == DriveChoice.LATER, Icons.Filled.Schedule, 5, stringResource(Res.string.wz_later), stringResource(Res.string.wz_drive_later_sub),
                                            divider = true, onClick = { drive = DriveChoice.LATER })
                                    }
                                }
                                VaultKindUi.MAIL -> {
                                    // The first account right here, in the same sheet as later in GhulMail.
                                    SheetList {
                                        ListRow(
                                            stringResource(if (mailAdded) Res.string.wz_mail_added else Res.string.mail_account_new),
                                            stringResource(if (mailAdded) Res.string.wz_mail_more else Res.string.mail_add_sub),
                                            leading = { IconTile(if (mailAdded) Icons.Filled.Check else Icons.Filled.Add, paletteColor(4)) },
                                        ) { mailSheet = true }
                                    }
                                    if (!mailAdded) Hint(stringResource(Res.string.wz_mail_later))
                                    if (mailSheet) {
                                        MailAccountSheet(state, null, onSaved = { made[VaultKindUi.MAIL] = true; mailAdded = true }) { mailSheet = false }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            HintText(stringResource(Res.string.wz_areas_hint))
        }

        6 -> {
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

        7 -> {
            HintText(stringResource(Res.string.wz_quick_explain))
            ListCard {
                PickRow(quick == QuickChoice.NONE, Icons.Filled.Usb, 0, stringResource(Res.string.wz_quick_none), stringResource(Res.string.wz_quick_none_sub), onClick = { quick = QuickChoice.NONE })
                PickRow(quick == QuickChoice.BIO, Icons.Filled.Fingerprint, 2, stringResource(Res.string.wz_quick_bio),
                    stringResource(if (backend.biometricAvailable()) Res.string.wz_quick_bio_sub else Res.string.set_quick_bio_unavailable),
                    enabled = backend.biometricAvailable(), divider = true, onClick = { quick = QuickChoice.BIO })
                PickRow(quick == QuickChoice.PIN, Icons.Filled.Pin, 5, stringResource(Res.string.wz_quick_pin), stringResource(Res.string.wz_quick_pin_sub), divider = true, onClick = { quick = QuickChoice.PIN }) {
                    SecretTextField(quickPin, { quickPin = it.filter(Char::isDigit).take(12); error = null }, stringResource(Res.string.wz_quick_pin_field), numeric = true)
                }
            }
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

        8 -> RecoveryCodeSteps(state, code) { finishWizard(state) }
    }

    ErrorText(error)
    if (step < 8) {
        ButtonRow {
            if (step > 1 && !(created && step <= 4)) OutlinedButton(onClick = { back() }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
            Button(
                onClick = { next() },
                enabled = !state.busy && (step != 3 || kdf != null),
                modifier = Modifier.weight(1f),
            ) {
                Text(stringResource(when {
                    step == 3 && useKey -> Res.string.a_register_key
                    step == 3 -> Res.string.a_setup
                    step == 4 && !syncOn -> Res.string.wz_later
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
