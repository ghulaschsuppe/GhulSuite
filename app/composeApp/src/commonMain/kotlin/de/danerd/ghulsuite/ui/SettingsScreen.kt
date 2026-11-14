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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
    val t = g
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable { open = true },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), fontSize = 15.sp)
        Text(stringResource(options.firstOrNull { it.first == selected }?.second ?: options.first().second), fontSize = 13.sp, color = t.faint)
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.padding(start = 6.dp).size(18.dp), tint = t.faint)
    }
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
enum class SetPage { UNLOCK, LOCK, PRIVACY, FACTORS, VAULTS, IMPORT, SERVERS, MODULES, PASS, AUTH, DRIVE, MAIL, CAL, LOOK, ABOUT }

@Composable
fun SettingsPageScreen(state: AppState, page: SetPage, padding: PaddingValues) {
    val backend = state.backend
    val settings = backend.settings
    var slots by remember { mutableStateOf<List<SlotUi>>(emptyList()) }
    var addKey by remember { mutableStateOf(false) }
    var newCode by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<SlotUi?>(null) }
    var setPin by remember { mutableStateOf(false) }
    var screenshotWarning by remember { mutableStateOf(false) }
    var ready by remember { mutableStateOf(backend.quickUnlockReady()) }
    var keyfileDialog by remember { mutableStateOf(false) }

    LaunchedEffect(state.liveRevision()) {
        slots = runCatching { backend.slots() }.getOrDefault(emptyList())
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
                Hint(stringResource(Res.string.set_quick_hint))
                val bioAvailable = backend.biometricAvailable()
                SwitchRow(stringResource(Res.string.set_quick_bio), settings.quickUnlockBiometric && ready.biometric) { on ->
                    if (on) {
                        state.run(block = { backend.enableBiometricUnlock() }) { state.changed() }
                    } else {
                        backend.disableQuickUnlockMethod(bio = true)
                        state.changed()
                    }
                }
                if (!bioAvailable) Hint(stringResource(Res.string.set_quick_bio_unavailable))
                SwitchRow(stringResource(Res.string.set_quick_pin), settings.quickUnlockPin && ready.pin) { on ->
                    if (on) setPin = true else {
                        backend.disableQuickUnlockMethod(bio = false)
                        state.changed()
                    }
                }
                ChoiceRow(stringResource(Res.string.set_quick_validity), QUICK_HOURS_OPTIONS, settings.quickUnlockHours) {
                    settings.quickUnlockHours = it
                }
                Hint(stringResource(Res.string.set_quick_always_full))
            }

                }
                SetPage.LOCK -> {
            SectionCard(stringResource(Res.string.set_security)) {
                Button(onClick = { state.lockNow() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_lock_now)) }
                ChoiceRow(stringResource(Res.string.set_auto_lock), AUTO_LOCK_OPTIONS, settings.autoLockSeconds) { settings.autoLockSeconds = it }
                SwitchRow(
                    stringResource(if (backend.isDesktop) Res.string.set_lock_session else Res.string.set_lock_screen_off),
                    settings.lockOnScreenOff,
                ) { settings.lockOnScreenOff = it }
                if (!backend.isDesktop) {
                    SwitchRow(stringResource(Res.string.set_shake), settings.shakeToLock) { settings.shakeToLock = it }
                    if (settings.shakeToLock) {
                        ChoiceRow(stringResource(Res.string.set_shake_sensitivity), SHAKE_OPTIONS, settings.shakeSensitivity) { settings.shakeSensitivity = it }
                    }
                    Hint(stringResource(Res.string.set_panic_hint))
                    SwitchRow(stringResource(Res.string.set_panic_mail), settings.panicStopsMail) { settings.panicStopsMail = it }
                    Hint(stringResource(Res.string.set_panic_mail_hint))
                } else {
                    // On wide windows the lock is in the sidebar, else in the top bar.
                    Hint(stringResource(if (LocalWide.current) Res.string.set_panic_hint_sidebar else Res.string.set_panic_hint_desktop))
                }
            }
                }
                SetPage.PRIVACY -> {
            SectionCard(stringResource(Res.string.set_privacy)) {
                ChoiceRow(stringResource(Res.string.set_clipboard), CLIPBOARD_OPTIONS, settings.clipboardClearSeconds) { settings.clipboardClearSeconds = it }
                SwitchRow(stringResource(Res.string.set_screenshots), settings.allowScreenshots) { on ->
                    if (on) screenshotWarning = true else settings.allowScreenshots = false
                }
            }

                }
                SetPage.FACTORS -> {
            SectionCard(stringResource(Res.string.set_keys)) {
                Hint(stringResource(Res.string.set_keys_hint))
                for (s in slots) {
                    ListRow(
                        s.label.ifBlank { stringResource(typeLabel(s.type)) },
                        stringResource(typeLabel(s.type)) + (if (s.created > 0) " · " + formatDate(s.created) else "") +
                            if (s.needsUpdate) " · " + stringResource(Res.string.slot_needs_update) else "",
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
                Hint(stringResource(Res.string.storage_current, storageLabel(backend.storageName())))
            }


                }
                SetPage.VAULTS -> VaultsSection(state)
                SetPage.IMPORT -> ImportExportSection(state)
                SetPage.SERVERS -> ServersSection(state)
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
            if (!backend.isDesktop) AutofillSection(state)
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
                SetPage.DRIVE -> {
                    DriveSettingsSection(state)
                    AreaFileNote(VaultKindUi.DRIVE)
                    AreaFooter(state, VaultKindUi.DRIVE)
                }
                SetPage.MAIL -> {
                    MailAccountsSection(state)
                    AreaFileNote(VaultKindUi.MAIL)
                    AreaFooter(state, VaultKindUi.MAIL)
                }
                SetPage.CAL -> {
                    CalSettingsSection(state)
                    AreaFileNote(VaultKindUi.CAL)
                    AreaFooter(state, VaultKindUi.CAL)
                }
                SetPage.LOOK -> {
            SectionCard(stringResource(Res.string.set_view)) {
                ChoiceRow(stringResource(Res.string.set_theme), THEME_OPTIONS, settings.theme) { settings.theme = it }
                ChoiceRow(
                    stringResource(Res.string.set_start_tab),
                    listOf(-1 to Res.string.set_start_last, 0 to Res.string.set_start_first, 1 to Res.string.set_start_second, 2 to Res.string.set_start_third, 3 to Res.string.set_start_fourth, 4 to Res.string.set_start_fifth)
                        .filter { it.first < state.tabs.size - 1 },
                    settings.startTab,
                ) { settings.startTab = it }
                Hint(stringResource(Res.string.set_start_hint, state.tabs.dropLast(1).joinToString(", ")))
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
    if (setPin) SetAppPinDialog(state) { setPin = false; ready = backend.quickUnlockReady() }
    if (screenshotWarning) {
        AlertDialog(
            onDismissRequest = { screenshotWarning = false },
            title = { Text(stringResource(Res.string.set_screenshots)) },
            text = { Text(stringResource(Res.string.set_screenshots_warning)) },
            confirmButton = {
                TextButton(onClick = { settings.allowScreenshots = true; screenshotWarning = false }) {
                    Text(stringResource(Res.string.a_allow))
                }
            },
            dismissButton = { TextButton(onClick = { screenshotWarning = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (newCode) {
        AlertDialog(
            onDismissRequest = { newCode = false },
            title = { Text(stringResource(Res.string.a_new_code)) },
            text = { Text(stringResource(Res.string.set_new_code_text)) },
            confirmButton = {
                TextButton(onClick = {
                    newCode = false
                    state.run(block = { backend.newRecoveryCode(getString(Res.string.setup_recovery_label)) }) {
                        state.changed()
                        state.push(Screen.RecoveryCode(it))
                    }
                }) { Text(stringResource(Res.string.a_create)) }
            },
            dismissButton = { TextButton(onClick = { newCode = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    removing?.let { s ->
        AlertDialog(
            danger = true,
            onDismissRequest = { removing = null },
            title = { Text(stringResource(Res.string.set_remove_title)) },
            text = {
                Text(stringResource(Res.string.set_remove_text, s.label.ifBlank { stringResource(typeLabel(s.type)) }))
            },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    state.run(block = { backend.removeSlot(s.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

@Composable
private fun SetAppPinDialog(state: AppState, onDismiss: () -> Unit) {
    var pin by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
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
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                if (pin != repeat) {
                    error = mismatch
                } else {
                    state.run(onError = { error = it }, block = { state.backend.enablePinUnlock(pin) }) {
                        state.changed()
                        onDismiss()
                    }
                }
            }, enabled = !state.busy && pin.length >= 6) { Text(stringResource(Res.string.a_save)) }
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
    val save = rememberSaveFile { ok -> saved = ok }
    val open = rememberOpenFile { file -> keyfile = file.bytes; saved = true }
    var label by remember { mutableStateOf("") }
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
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val data = keyfile ?: return@TextButton
                state.run(onError = { error = it }, block = {
                    state.backend.addKeyfile(data, label.ifBlank { getString(Res.string.keyfile_default_label) })
                }) {
                    state.changed()
                    state.message(done)
                    onDismiss()
                }
            }, enabled = !state.busy && saved && keyfile != null) { Text(stringResource(Res.string.a_add)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
private fun AddKeyDialog(state: AppState, onDismiss: () -> Unit) {
    var pin by remember { mutableStateOf("") }
    var label by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.a_add_key)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.set_add_key_text))
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
                    state.backend.addSecurityKey(pin, label.ifBlank { getString(Res.string.slot_backup_key) }, state.onTouchNeeded)
                }) {
                    state.changed()
                    state.message(added)
                    onDismiss()
                }
            }, enabled = !state.busy && keyPinOk(state, pin)) { Text(stringResource(Res.string.a_register_key)) }
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
    var current by remember { mutableStateOf("") }
    var new by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.a_change_password)) {
                Hint(stringResource(Res.string.set_change_pw_text))
                SecretTextField(current, { current = it; error = null }, stringResource(Res.string.l_current_password))
                SecretTextField(new, { new = it; error = null }, stringResource(Res.string.l_new_password))
                StrengthMeter(state.backend.passwordScore(new))
                SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
                ErrorText(error)
                val done = stringResource(Res.string.msg_password_changed)
                Button(onClick = {
                    scope.launch {
                        error = PasswordPolicy.check(state.backend, new, repeat)
                        if (error == null) {
                            state.run(onError = { error = it }, block = { state.backend.changePassword(current, new) }) {
                                state.message(done)
                                state.pop()
                            }
                        }
                    }
                }, enabled = !state.busy && current.isNotEmpty(), modifier = Modifier.fillMaxWidth()) {
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
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val report = buildString {
        appendLine("GhulSuite ${backend.appVersion}")
        appendLine(backend.coreVersion)
        appendLine(deviceInfo())
        appendLine()
        append(backend.crashReport() ?: "-")
    }
    // Author (AGPL-3.0 section 7(b): this notice must stay visible in modified versions too).
    SectionCard {
        Text(stringResource(Res.string.about_made_by), fontWeight = androidx.compose.ui.text.font.FontWeight.Bold, fontSize = 16.sp)
        Text(stringResource(Res.string.about_made_by_sub), color = g.muted, fontSize = 14.sp)
    }
    ListCard {
        ListRow(stringResource(Res.string.about_report), stringResource(Res.string.about_report_sub), flush = true, trailing = { ChevronEnd() }) { sheet = "report" }
        ListRow(stringResource(Res.string.about_license), "AGPL-3.0-or-later", divider = true, flush = true, trailing = { ChevronEnd() }) { sheet = "license" }
        ListRow(stringResource(Res.string.about_libraries), divider = true, flush = true, trailing = { ChevronEnd() }) { sheet = "libs" }
    }
    HintText("GhulSuite ${backend.appVersion} · ${backend.coreVersion}")
    when (sheet) {
        "report" -> GhulSheet({ sheet = "" }, stringResource(Res.string.about_report)) {
            val reportLabel = stringResource_report()
            HintText(stringResource(Res.string.about_report_hint))
            Text(report, fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, fontSize = 12.sp,
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(g.surface2).padding(12.dp))
            ButtonRow {
                OutlinedButton(onClick = { state.copy(reportLabel, report) }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_copy)) }
                if (filePickersAvailable) Button(onClick = { save("GhulSuite-Fehlerbericht.txt", report.encodeToByteArray()) }, modifier = Modifier.weight(1f)) {
                    Text(stringResource(Res.string.a_save))
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

private val LIBRARIES = listOf(
    "Kotlin, Compose Multiplatform" to "Apache-2.0",
    "UniFFI" to "MPL-2.0",
    "RustCrypto (AES-GCM, ChaCha20-Poly1305, SHA-2, HKDF, Argon2, P-256)" to "MIT / Apache-2.0",
    "rustls, ring, webpki-roots" to "Apache-2.0 / ISC / MPL-2.0",
    "russh, russh-sftp" to "Apache-2.0",
    "async-imap, mail-parser, lettre" to "MIT / Apache-2.0",
    "rPGP (pgp)" to "MIT / Apache-2.0",
    "zxcvbn" to "MIT",
    "zip" to "MIT",
    "tokio, futures, serde, ciborium" to "MIT / Apache-2.0",
    "ZXing" to "Apache-2.0",
    "JNA (Windows)" to "Apache-2.0 / LGPL-2.1",
    "EFF Large Wordlist" to "CC BY 3.0 US",
)
