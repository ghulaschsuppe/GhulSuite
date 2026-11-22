// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.InsertDriveFile
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.ThemeMode
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource
import de.danerd.ghulsuite.model.VaultKindUi

/** Times of the automatic lock: the same choice in the setup wizard and in Options. */
internal val AUTO_LOCK_OPTIONS: List<Pair<Int, StringResource>> = listOf(
    0 to Res.string.lock_immediately,
    30 to Res.string.lock_30s,
    60 to Res.string.lock_1m,
    120 to Res.string.lock_2m,
    300 to Res.string.lock_5m,
    900 to Res.string.lock_15m,
)

private val CLIPBOARD_OPTIONS: List<Pair<Int, StringResource>> = listOf(
    0 to Res.string.clip_never,
    15 to Res.string.clip_15s,
    30 to Res.string.lock_30s,
    45 to Res.string.clip_45s,
    60 to Res.string.lock_1m,
    120 to Res.string.lock_2m,
)

internal val QUICK_HOURS_OPTIONS: List<Pair<Int, StringResource>> = listOf(
    1 to Res.string.quick_1h,
    4 to Res.string.quick_4h,
    8 to Res.string.quick_8h,
    24 to Res.string.quick_24h,
    72 to Res.string.quick_3d,
    168 to Res.string.quick_7d,
    0 to Res.string.quick_always,
)

private val SHAKE_OPTIONS: List<Pair<Int, StringResource>> = listOf(
    0 to Res.string.shake_low,
    1 to Res.string.shake_medium,
    2 to Res.string.shake_high,
)

private val THEME_OPTIONS: List<Pair<ThemeMode, StringResource>> = listOf(
    ThemeMode.SYSTEM to Res.string.theme_system,
    ThemeMode.LIGHT to Res.string.theme_light,
    ThemeMode.DARK to Res.string.theme_dark,
)

/** A choice row (.row with value and chevron); the options open in a sheet. */
@Composable
fun <T> ChoiceRow(label: String, options: List<Pair<T, StringResource>>, selected: T, onSelect: (T) -> Unit) {
    var open by remember { mutableStateOf(false) }
    LabelValueRow(label, stringResource(options.firstOrNull { it.first == selected }?.second ?: options.first().second)) { open = true }
    if (open) {
        GhulSheet({ open = false }, label) {
            SheetList {
                options.forEachIndexed { i, (value, text) ->
                    SheetRow(stringResource(text), divider = i > 0,
                        end = { if (value == selected) SheetCheck() }) {
                        onSelect(value); open = false
                    }
                }
            }
        }
    }
}

/** Pages of the settings, opened from the overview. */
enum class SetPage { UNLOCK, LOCK, PRIVACY, FACTORS, VAULTS, FILE, IMPORT, MODULES, PASS, AUTH, DOCS, CAL, LOOK, ABOUT }

@Composable
fun SettingsPageScreen(state: AppState, page: SetPage, padding: PaddingValues) {
    val backend = state.backend
    val settings = backend.settings
    var slots by remember { mutableStateOf<List<SlotUi>>(emptyList()) }
    var addKey by remember { mutableStateOf(false) }
    var newCode by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<SlotUi?>(null) }
    // H6: quick unlock is set up with both factors and a validity that is fixed in its file:
    // the validity to set it up with (null: no dialog open).
    var setPin by remember { mutableStateOf<Int?>(null) }
    var askBioPassword by remember { mutableStateOf<Int?>(null) }
    var screenshotWarning by remember { mutableStateOf(false) }
    // M14: a change that weakens the protection waits here for the master password.
    var weaken by remember { mutableStateOf<Pair<String, () -> Unit>?>(null) }
    var ready by remember { mutableStateOf(backend.quickUnlockReady()) }
    var keyfileDialog by remember { mutableStateOf(false) }

    LaunchedEffect(state.liveRevision()) {
        slots = catching { backend.slots() }.getOrDefault(emptyList())
        ready = backend.quickUnlockReady()
    }
    val keyCount = slots.count { it.type == SlotType.SECURITY_KEY }
    val factorCount = slots.count { it.type != SlotType.RECOVERY_CODE }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            when (page) {
                SetPage.UNLOCK -> {
            SectionCard(stringResource(Res.string.set_unlock)) {
                // From an older version: switched on, but its file must be made anew (H6).
                if (ready.needsSetup) Banner(Icons.Filled.Warning, stringResource(Res.string.set_quick_needs_setup))
                Hint(stringResource(Res.string.set_quick_hint))
                val bioAvailable = backend.biometricAvailable()
                SwitchRow(stringResource(Res.string.set_quick_bio), settings.quickUnlockBiometric && ready.biometric) { on ->
                    if (on) {
                        askBioPassword = settings.quickUnlockHours
                    } else {
                        backend.disableQuickUnlockMethod(bio = true)
                        state.changed()
                    }
                }
                if (!bioAvailable) Hint(stringResource(Res.string.set_quick_bio_unavailable))
                SwitchRow(stringResource(Res.string.set_quick_pin), settings.quickUnlockPin && ready.pin) { on ->
                    if (on) setPin = settings.quickUnlockHours else {
                        backend.disableQuickUnlockMethod(bio = false)
                        state.changed()
                    }
                }
                // The validity is part of each method's quick unlock file (H6): shown per method,
                // another one sets that method up again, with both factors.
                val bioOn = settings.quickUnlockBiometric && ready.biometric
                val pinOn = settings.quickUnlockPin && ready.pin
                if (bioOn) {
                    val now = settings.quickHoursBio.takeIf { it >= 0 } ?: settings.quickUnlockHours
                    ChoiceRow(stringResource(Res.string.set_quick_validity_bio), QUICK_HOURS_OPTIONS, now) { hours ->
                        if (hours != now) askBioPassword = hours
                    }
                }
                if (pinOn) {
                    val now = settings.quickHoursPin.takeIf { it >= 0 } ?: settings.quickUnlockHours
                    ChoiceRow(stringResource(Res.string.set_quick_validity_pin), QUICK_HOURS_OPTIONS, now) { hours ->
                        if (hours != now) setPin = hours
                    }
                }
                if (!bioOn && !pinOn) {
                    ChoiceRow(stringResource(Res.string.set_quick_validity), QUICK_HOURS_OPTIONS, settings.quickUnlockHours) { settings.quickUnlockHours = it }
                }
                if (ready.any) Hint(stringResource(Res.string.set_quick_validity_fixed))
                Hint(stringResource(Res.string.set_quick_always_full))
            }

                }
                SetPage.LOCK -> {
            SectionCard(stringResource(Res.string.set_security)) {
                Button(onClick = { state.lockNow() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_lock_now)) }
                val autoLockText = stringResource(Res.string.gate_auto_lock)
                val screenOffText = stringResource(Res.string.gate_screen_off)
                val shakeText = stringResource(Res.string.gate_shake)
                ChoiceRow(stringResource(Res.string.set_auto_lock), AUTO_LOCK_OPTIONS, settings.autoLockSeconds) { v ->
                    if (autoLockWeakens(settings.autoLockSeconds, v)) weaken = autoLockText to { settings.autoLockSeconds = v }
                    else settings.autoLockSeconds = v
                }
                SwitchRow(
                    stringResource(if (backend.isDesktop) Res.string.set_lock_session else Res.string.set_lock_screen_off),
                    settings.lockOnScreenOff,
                ) { on -> if (on) settings.lockOnScreenOff = true else weaken = screenOffText to { settings.lockOnScreenOff = false } }
                if (!backend.isDesktop) {
                    SwitchRow(stringResource(Res.string.set_shake), settings.shakeToLock) { on ->
                        if (on) settings.shakeToLock = true else weaken = shakeText to { settings.shakeToLock = false }
                    }
                    if (settings.shakeToLock) {
                        val sensText = stringResource(Res.string.gate_shake_sensitivity)
                        ChoiceRow(stringResource(Res.string.set_shake_sensitivity), SHAKE_OPTIONS, settings.shakeSensitivity) { v ->
                            // Less sensitive is nearly "shaking off".
                            if (v < settings.shakeSensitivity) weaken = sensText to { settings.shakeSensitivity = v } else settings.shakeSensitivity = v
                        }
                    }
                    if (settings.moduleGhulCal) {
                        val remText = stringResource(Res.string.gate_panic_reminders)
                        SwitchRow(stringResource(Res.string.set_panic_reminders), settings.panicClearReminders) { on ->
                            if (on) settings.panicClearReminders = true else weaken = remText to { settings.panicClearReminders = false }
                        }
                    }
                    Hint(stringResource(Res.string.set_panic_hint))
                } else {
                    // On wide windows the lock is in the sidebar, else in the top bar.
                    Hint(stringResource(if (LocalWide.current) Res.string.set_panic_hint_sidebar else Res.string.set_panic_hint_desktop))
                }
            }
                }
                SetPage.PRIVACY -> {
            SectionCard(stringResource(Res.string.set_privacy)) {
                val clipText = stringResource(Res.string.gate_clipboard)
                ChoiceRow(stringResource(Res.string.set_clipboard), CLIPBOARD_OPTIONS, settings.clipboardClearSeconds) { v ->
                    if (clipboardWeakens(settings.clipboardClearSeconds, v)) weaken = clipText to { settings.clipboardClearSeconds = v }
                    else settings.clipboardClearSeconds = v
                }
                SwitchRow(stringResource(Res.string.set_screenshots), settings.allowScreenshots) { on ->
                    if (on) screenshotWarning = true else settings.allowScreenshots = false
                }
            }

                }
                SetPage.FACTORS -> {
            SectionCard(stringResource(Res.string.set_keys)) {
                Hint(stringResource(Res.string.set_keys_hint))
                // A key without a backup (a suite from before the backup was required): said plainly.
                if (keyCount > 0 && factorCount < 2) Banner(Icons.Filled.Warning, stringResource(Res.string.set_key_no_backup))
                for (s in slots) {
                    ListRow(
                        s.label.ifBlank { stringResource(typeLabel(s.type)) },
                        stringResource(typeLabel(s.type)) + (if (s.created > 0) " · " + formatDate(s.created) else ""),
                        leading = {
                            IconTile(
                                when (s.type) {
                                    SlotType.SECURITY_KEY -> Icons.Filled.Key
                                    SlotType.RECOVERY_CODE -> Icons.Filled.Description
                                    SlotType.KEYFILE -> Icons.Filled.InsertDriveFile
                                },
                                paletteColor(0),
                            )
                        },
                        trailing = {
                            if (s.type != SlotType.RECOVERY_CODE && factorCount > 1) {
                                BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = g.danger) { removing = s }
                            }
                        },
                        divider = s != slots.first(), flush = true,
                    )
                }
                if (keyCount == 0) {
                    Text(stringResource(Res.string.set_no_key_hint), color = warnColor(), style = MaterialTheme.typography.bodyMedium)
                } else if (factorCount < 2) {
                    Text(stringResource(Res.string.set_backup_key_hint), color = warnColor(), style = MaterialTheme.typography.bodyMedium)
                }
                OutlinedButton(onClick = { addKey = true }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_add_key)) }
                if (filePickersAvailable) {
                    OutlinedButton(onClick = { keyfileDialog = true }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.a_add_keyfile))
                    }
                }
                OutlinedButton(onClick = { newCode = true }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_new_code)) }
                Hint(stringResource(Res.string.set_swap_hint))
                // L1: old backups still open with what was valid back then.
                Hint(stringResource(Res.string.set_lost_hint))
            }


                }
                SetPage.VAULTS -> if (state.single == null) VaultsSection(state) else SingleVaultSection(state)
                SetPage.FILE -> if (state.single != null) SingleVaultSection(state) else SuiteFileSection(state)
                SetPage.IMPORT -> ImportExportSection(state)
                SetPage.MODULES -> AreasPage(state)
                SetPage.PASS -> {
            SectionCard(areaGroupTitle(VaultKindUi.PASS)) {
                ChoiceRow(stringResource(Res.string.set_gen_default), listOf(0 to Res.string.gen_chars, 1 to Res.string.gen_passphrase), settings.generatorMode) { settings.generatorMode = it }
            }
            SectionCard(deviceGroupTitle()) {
                SwitchRow(stringResource(Res.string.set_show_subtitles), settings.showEntrySubtitles) { settings.showEntrySubtitles = it }
                SwitchRow(stringResource(Res.string.sec_pwned), settings.pwnedCheck) { settings.pwnedCheck = it }
                Hint(stringResource(Res.string.sec_pwned_sub))
            }
            AreaFileNote(VaultKindUi.PASS)
            AreaFooter(state, VaultKindUi.PASS)
                }
                SetPage.AUTH -> {
            SectionCard(areaGroupTitle(VaultKindUi.TOTP)) {
                Hint(stringResource(Res.string.area_auth_nothing))
            }
            SectionCard(deviceGroupTitle()) {
                SwitchRow(stringResource(Res.string.set_totp_hide), settings.totpHideCodes) { settings.totpHideCodes = it }
                SwitchRow(stringResource(Res.string.set_totp_next), settings.totpShowNext) { settings.totpShowNext = it }
            }
            AreaFileNote(VaultKindUi.TOTP)
            AreaFooter(state, VaultKindUi.TOTP)
                }
                SetPage.DOCS -> {
                    DocsSettingsSection(state)
                    AreaFooter(state, VaultKindUi.DOCS)
                }
                SetPage.CAL -> {
                    // B22: Windows has no background service: reminders need the app running (also locked).
                    if (backend.isDesktop) Hint(stringResource(Res.string.cal_desktop_running))
                    CalSettingsSection(state)
                    AreaFileNote(VaultKindUi.CAL)
                    AreaFooter(state, VaultKindUi.CAL)
                }
                SetPage.LOOK -> {
            SectionCard(stringResource(Res.string.set_view)) {
                ChoiceRow(stringResource(Res.string.set_theme), THEME_OPTIONS, settings.theme) { settings.theme = it }
                ChoiceRow(
                    stringResource(Res.string.set_start_tab),
                    listOf(
                        -1 to Res.string.set_start_last, 0 to Res.string.set_start_first, 1 to Res.string.set_start_second,
                        2 to Res.string.set_start_third, 3 to Res.string.set_start_fourth,
                    )
                        .filter { it.first < state.tabs.size - 1 },
                    settings.startTab,
                ) { settings.startTab = it }
                // F90: name the areas, not the internal tab codes.
                val order = state.tabs.dropLast(1).map { stringResource(tabTitle(it)) }
                Hint(stringResource(Res.string.set_start_hint, order.joinToString(", ")))
                OutlinedButton(onClick = { state.push(Screen.SettingsPage(SetPage.MODULES)) }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.set_areas_in_bar))
                }
                ChoiceRow(
                    stringResource(Res.string.set_language),
                    listOf("" to Res.string.set_language_system, "de" to Res.string.set_language_de, "en" to Res.string.set_language_en),
                    settings.appLanguage,
                ) { tag ->
                    settings.appLanguage = tag
                    applyAppLanguage(tag)
                }
                if (backend.isDesktop) Hint(stringResource(Res.string.set_language_note))
            }
                }
                SetPage.ABOUT -> {
            AboutSection(state)
                }
            }
        }
    }

    if (addKey) AddKeyDialog(state) { addKey = false }
    if (keyfileDialog) KeyfileDialog(state) { keyfileDialog = false }
    setPin?.let { hours -> SetAppPinDialog(state, hours) { setPin = null; ready = backend.quickUnlockReady() } }
    askBioPassword?.let { hours -> BioPasswordDialog(state, hours) { askBioPassword = null; ready = backend.quickUnlockReady() } }
    if (screenshotWarning) {
        // The warning and the master password in one step (M14).
        PasswordGateDialog(
            state, stringResource(Res.string.set_screenshots), stringResource(Res.string.set_screenshots_warning),
            confirmLabel = stringResource(Res.string.a_allow),
            onDismiss = { screenshotWarning = false },
        ) { settings.allowScreenshots = true; screenshotWarning = false }
    }
    weaken?.let { (text, apply) ->
        PasswordGateDialog(
            state, stringResource(Res.string.gate_weaken_title), text, confirmLabel = stringResource(Res.string.a_apply),
            onDismiss = { weaken = null },
        ) { apply(); weaken = null }
    }
    if (newCode) NewCodeDialog(state) { newCode = false }
    removing?.let { s -> RemoveSlotDialog(state, s) { removing = null } }
}

/** A new recovery code: both factors of now first (who finds the app unlocked cannot do it). */
@Composable
fun NewCodeDialog(state: AppState, onDismiss: () -> Unit) {
    val proof = rememberProofInput(state)
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.a_new_code)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.set_new_code_text))
                ProofFields(state, proof) { error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = {
                    state.backend.newRecoveryCode(proof.proof(), getString(Res.string.setup_recovery_label), state.onTouchNeeded)
                }) {
                    proof.clear()
                    onDismiss()
                    state.codeUnconfirmed = false
                    state.protectionChanged()
                    state.push(Screen.RecoveryCode(it))
                }
            }, enabled = !state.busy && proof.ok(state), modifier = Modifier.testTag("new-code-confirm")) { Text(stringResource(Res.string.a_create)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
private fun RemoveSlotDialog(state: AppState, s: SlotUi, onDismiss: () -> Unit) {
    val proof = rememberProofInput(state)
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        danger = true,
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.set_remove_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.set_remove_text, s.label.ifBlank { stringResource(typeLabel(s.type)) }))
                ProofFields(state, proof) { error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = { state.backend.removeSlot(proof.proof(), s.id, state.onTouchNeeded) }) {
                    proof.clear()
                    onDismiss()
                    state.protectionChanged()
                }
            }, enabled = !state.busy && proof.ok(state)) { Text(stringResource(Res.string.a_delete)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
private fun SetAppPinDialog(state: AppState, hours: Int, onDismiss: () -> Unit) {
    val proof = rememberProofInput(state)
    var pin by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var validity by remember { mutableStateOf(hours) }
    var error by remember { mutableStateOf<String?>(null) }
    val mismatch = stringResource(Res.string.set_pin_mismatch)
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.set_quick_pin)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.set_app_pin_text))
                SecretTextField(pin, { pin = it.filter(Char::isDigit).take(12); error = null }, stringResource(Res.string.l_app_pin), numeric = true)
                SecretTextField(repeat, { repeat = it.filter(Char::isDigit).take(12); error = null }, stringResource(Res.string.l_app_pin_repeat), numeric = true)
                ChoiceRow(stringResource(Res.string.set_quick_validity), QUICK_HOURS_OPTIONS, validity) { validity = it }
                // H6: who finds the app unlocked cannot set a PIN of his own: both factors.
                ProofFields(state, proof) { error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                if (pin != repeat) {
                    error = mismatch
                } else {
                    state.run(onError = { error = it }, block = { state.backend.enablePinUnlock(proof.proof(), pin, validity, state.onTouchNeeded) }) {
                        proof.clear()
                        state.changed()
                        onDismiss()
                    }
                }
            }, enabled = !state.busy && pin.length >= MIN_APP_PIN && proof.ok(state), modifier = Modifier.testTag("quick-pin-save")) { Text(stringResource(Res.string.a_save)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

private fun typeLabel(t: SlotType) = when (t) {
    SlotType.SECURITY_KEY -> Res.string.slot_key
    SlotType.RECOVERY_CODE -> Res.string.slot_code
    SlotType.KEYFILE -> Res.string.slot_keyfile
}

/** Creates (and saves) or picks a key file and adds it as second factor. */
@Composable
private fun KeyfileDialog(state: AppState, onDismiss: () -> Unit) {
    var keyfile by remember { mutableStateOf<ByteArray?>(null) }
    var saved by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val failText = rememberSaveFileFailedText()
    val save = rememberSaveFile { r ->
        saved = r == SaveFileResult.SAVED
        // r7 N-3: saved clears an earlier error; cancelled keeps it.
        if (saved) error = null else failText(r)?.let { error = it }
    }
    val open = rememberOpenFile { file -> keyfile = file.bytes; saved = true }
    var label by remember { mutableStateOf("") }
    val proof = rememberProofInput(state)
    val done = stringResource(Res.string.msg_keyfile_added)
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.keyfile_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.keyfile_text))
                OutlinedButton(onClick = {
                    val data = keyfile ?: state.backend.generateKeyfile().also { keyfile = it }
                    save(KEYFILE_NAME, data)
                }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_create_save)) }
                OutlinedButton(onClick = open, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_use_existing)) }
                if (saved) Text(stringResource(Res.string.keyfile_ready), color = okColor())
                OutlinedTextField(
                    label, { label = it }, label = { Text(stringResource(Res.string.l_keyfile_label)) }, singleLine = true,
                )
                ProofFields(state, proof) { error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val data = keyfile ?: return@TextButton
                state.run(onError = { error = it }, block = {
                    state.backend.addKeyfile(proof.proof(), data, label.ifBlank { getString(Res.string.keyfile_default_label) }, state.onTouchNeeded)
                }) {
                    proof.clear()
                    state.changed()
                    state.message(done)
                    onDismiss()
                }
            }, enabled = !state.busy && saved && keyfile != null && proof.ok(state)) { Text(stringResource(Res.string.a_add)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
private fun AddKeyDialog(state: AppState, onDismiss: () -> Unit) {
    var pin by remember { mutableStateOf("") }
    var label by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val proof = rememberProofInput(state)
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.a_add_key)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.set_add_key_text))
                ProofFields(state, proof) { error = null }
                // The new key (after the one of now, if that is how it is confirmed).
                Hint(stringResource(Res.string.set_add_key_new))
                OutlinedTextField(label, { label = it }, label = { Text(stringResource(Res.string.l_key_label)) }, singleLine = true)
                KeyPinField(state, pin) { pin = it; error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            val added = stringResource(Res.string.msg_key_added)
            TextButton(onClick = {
                state.run(onError = { error = it }, block = {
                    state.backend.addSecurityKey(proof.proof(), pin, label.ifBlank { getString(Res.string.slot_backup_key) }, state.onTouchNeeded)
                }) {
                    proof.clear()
                    state.changed()
                    state.message(added)
                    onDismiss()
                }
            }, enabled = !state.busy && keyPinOk(state, pin) && proof.ok(state)) { Text(stringResource(Res.string.a_register_key)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
fun RecoveryCodeScreen(state: AppState, code: String, padding: PaddingValues) {
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            Hint(stringResource(Res.string.set_code_shown_once))
            // Like during setup: the code counts only once a group of it was typed back.
            RecoveryCodeSteps(state, code) {
                state.changed()
                state.pop()
            }
        }
    }
}

@Composable
fun ChangePasswordScreen(state: AppState, padding: PaddingValues) {
    // The current password plus a key or key file (who knows the password cannot lock the owner out).
    val proof = rememberProofInput(state)
    var new by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.a_change_password)) {
                Hint(stringResource(Res.string.set_change_pw_text))
                ProofFields(state, proof) { error = null }
                SecretTextField(new, { new = it; error = null }, stringResource(Res.string.l_new_password))
                // Only once something is typed: an empty field is not "very weak".
                if (new.isNotEmpty()) StrengthMeter(state.backend.passwordScore(new))
                SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
                ErrorText(error)
                val done = stringResource(Res.string.msg_password_changed)
                Button(onClick = {
                    scope.launch {
                        error = PasswordPolicy.check(state.backend, new, repeat)
                        if (error == null) {
                            state.run(onError = { error = it }, block = { state.backend.changePassword(proof.proof(), new, state.onTouchNeeded) }) {
                                proof.clear()
                                new = ""; repeat = ""
                                state.protectionChanged()
                                state.message(done)
                                state.pop()
                            }
                        }
                    }
                }, enabled = !state.busy && proof.ok(state), modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_save))
                }
            }
        }
    }
}

/** About and help: error report, licence, libraries used. */
@Composable
private fun AboutSection(state: AppState) {
    val backend = state.backend
    var sheet by remember { mutableStateOf("") }
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { state.reportSaveFile(it, savedMsg) }
    val report = buildString {
        appendLine("GhulSuite ${backend.appVersion}")
        appendLine(backend.coreVersion)
        appendLine(deviceInfo())
        appendLine()
        append(backend.crashReport() ?: "-")
    }
    UpdateSection(state)
    // Author (AGPL-3.0 section 7(b): this notice must stay visible in modified versions too).
    SectionCard {
        Text(stringResource(Res.string.about_made_by), fontWeight = androidx.compose.ui.text.font.FontWeight.Bold, fontSize = 16.sp)
        Text(stringResource(Res.string.about_made_by_sub), color = g.muted, fontSize = 14.sp)
    }
    // Website, legal notice and privacy policy (open in the browser).
    val uri = androidx.compose.ui.platform.LocalUriHandler.current
    val siteUrl = stringResource(Res.string.about_site_url)
    val imprintUrl = stringResource(Res.string.about_imprint_url)
    val privacyUrl = stringResource(Res.string.about_privacy_url)
    ListCard {
        ListRow(stringResource(Res.string.about_site), "www.ghulsuite.de", trailing = { ChevronEnd() }) { catching { uri.openUri(siteUrl) } }
        ListRow(stringResource(Res.string.about_imprint), divider = true, trailing = { ChevronEnd() }) { catching { uri.openUri(imprintUrl) } }
        ListRow(stringResource(Res.string.about_privacy), stringResource(Res.string.about_privacy_sub), divider = true, trailing = { ChevronEnd() }) { catching { uri.openUri(privacyUrl) } }
    }
    ListCard {
        ListRow(stringResource(Res.string.about_report), stringResource(Res.string.about_report_sub), trailing = { ChevronEnd() }) { sheet = "report" }
        ListRow(stringResource(Res.string.about_license), "AGPL-3.0-or-later", divider = true, trailing = { ChevronEnd() }) { sheet = "license" }
        ListRow(stringResource(Res.string.about_libraries), divider = true, trailing = { ChevronEnd() }) { sheet = "libs" }
    }
    HintText("GhulSuite ${backend.appVersion} · ${backend.coreVersion}")
    when (sheet) {
        "report" -> GhulSheet({ sheet = "" }, stringResource(Res.string.about_report)) {
            val reportLabel = stringResource_report()
            val reportFile = stringResource(Res.string.about_report_file)
            HintText(stringResource(Res.string.about_report_hint))
            SelectionContainer {
                Text(report, fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, fontSize = 12.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(g.surface2).padding(12.dp))
            }
            ButtonRow {
                OutlinedButton(onClick = { state.copy(reportLabel, report); backend.clearCrashReport() }, modifier = Modifier.weight(1f)) {
                    Text(stringResource(Res.string.a_copy))
                }
                // Android: hand it to another app (mail, messenger); elsewhere save it as a file.
                if (backend.canShareToApps) {
                    OutlinedButton(onClick = {
                        state.run(block = { backend.shareToApps(reportFile, "text/plain", report.encodeToByteArray()) }) { backend.clearCrashReport() }
                    }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_share)) }
                } else if (filePickersAvailable) {
                    OutlinedButton(onClick = { save(reportFile, report.encodeToByteArray()); backend.clearCrashReport() }, modifier = Modifier.weight(1f)) {
                        Text(stringResource(Res.string.a_save))
                    }
                }
            }
        }
        "license" -> GhulSheet({ sheet = "" }, stringResource(Res.string.about_license)) {
            Text(stringResource(Res.string.about_license_text))
            HintText(stringResource(Res.string.about_trademark))
        }
        "libs" -> GhulSheet({ sheet = "" }, stringResource(Res.string.about_libraries)) {
            SheetList {
                LIBRARIES.forEachIndexed { i, (name, lic) -> SheetRow(name, lic, divider = i > 0) }
            }
            HintText(stringResource(Res.string.about_eff))
        }
    }
}

@Composable
private fun stringResource_report(): String = stringResource(Res.string.about_report)


/** Updates: automatically or by hand, stable or development versions. */
@Composable
private fun UpdateSection(state: AppState) {
    val up = state.backend.updater
    val s = state.backend.settings
    SectionCard(stringResource(Res.string.upd_section)) {
        if (!up.available) {
            HintText(stringResource(Res.string.upd_unavailable))
            return@SectionCard
        }
        SwitchRow(stringResource(Res.string.upd_auto), s.updateAuto) { s.updateAuto = it }
        ChoiceRow(
            stringResource(Res.string.upd_channel),
            listOf(false to Res.string.upd_stable, true to Res.string.upd_dev),
            s.updateDev,
        ) { dev ->
            s.updateDev = dev
            state.update = null
        }
        HintText(stringResource(if (s.updateDev) Res.string.upd_dev_hint else Res.string.upd_stable_hint))
        Button(onClick = { state.checkUpdate(manual = true) }, enabled = !state.updateBusy, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(if (state.updateBusy) Res.string.upd_checking else Res.string.upd_check))
        }
    }
}

/** "Version X is available" after a check: install, or later. */
@Composable
fun UpdateDialog(state: AppState) {
    val u = state.update ?: return
    val up = state.backend.updater
    AlertDialog(
        onDismissRequest = { state.update = null },
        title = { Text(stringResource(if (u.version.contains('-')) Res.string.upd_title_dev else Res.string.upd_title, u.version)) },
        text = {
            // The sheet of the dialog scrolls long release notes itself.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(if (up.installs) Res.string.upd_text else Res.string.upd_text_portable, formatSize(u.size)))
                if (u.notes.isNotBlank()) {
                    Text(u.notes.take(3000), fontSize = 13.sp, color = g.muted)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { state.installUpdate(u) }) {
                Text(stringResource(if (up.installs) Res.string.upd_install else Res.string.upd_open_page))
            }
        },
        dismissButton = { TextButton(onClick = { state.update = null }) { Text(stringResource(Res.string.a_later)) } },
    )
}

private val LIBRARIES = listOf(
    "Kotlin, Compose Multiplatform" to "Apache-2.0",
    "UniFFI" to "MPL-2.0",
    "RustCrypto (ChaCha20-Poly1305, AES, SHA-2, HKDF, Argon2, P-256)" to "MIT / Apache-2.0",
    "rustls, ring, webpki-roots" to "Apache-2.0 / ISC / MPL-2.0",
    "zxcvbn" to "MIT",
    "Apache PDFBox (Windows)" to "Apache-2.0",
    "zip" to "MIT",
    "tokio, serde, ciborium" to "MIT / Apache-2.0",
    "ZXing" to "Apache-2.0",
    "JNA (Windows)" to "Apache-2.0 / LGPL-2.1",
    "EFF Large Wordlist" to "CC BY 3.0 US",
)

/** Fingerprint unlock: both factors of now and the validity first (who finds the app unlocked cannot). */
@Composable
private fun BioPasswordDialog(state: AppState, hours: Int, onDismiss: () -> Unit) {
    val proof = rememberProofInput(state)
    var validity by remember { mutableStateOf(hours) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.set_quick_bio)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ChoiceRow(stringResource(Res.string.set_quick_validity), QUICK_HOURS_OPTIONS, validity) { validity = it }
                // Set up again (another validity): the old one is replaced as soon as it starts.
                if (state.backend.quickUnlockReady().biometric) Hint(stringResource(Res.string.set_quick_bio_cancel_note))
                ProofFields(state, proof) { error = null }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = { state.backend.enableBiometricUnlock(proof.proof(), validity, state.onTouchNeeded) }) {
                    proof.clear()
                    state.changed()
                    onDismiss()
                }
            }, enabled = !state.busy && proof.ok(state), modifier = Modifier.testTag("quick-bio-save")) { Text(stringResource(Res.string.a_save)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}
