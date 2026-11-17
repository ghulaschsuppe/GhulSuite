// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ContainerInfoUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.ShareVaultUi
import de.danerd.ghulsuite.model.SharedVaultUi
import de.danerd.ghulsuite.model.SingleSaveUi
import de.danerd.ghulsuite.model.SingleWrite
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

/** GhulDrive and GhulMail need the servers and accounts of a suite: they can only be added to one. */
fun opensOnItsOwn(kind: VaultKindUi) = kind != VaultKindUi.DRIVE && kind != VaultKindUi.MAIL

/**
 * A vault file was picked: open only this vault, add it to the own suite (unlocked first if
 * needed) or, without a suite on this device, build a new suite around it.
 */
@Composable
fun VaultFileChoice(
    state: AppState,
    file: PickedSuiteFile,
    info: ContainerInfoUi,
    onOnlyThis: () -> Unit,
    onNewSuite: () -> Unit,
    onCancel: () -> Unit,
) {
    val kind = vaultKindOf(info.kind)
    val hasSuite = state.backend.exists()
    // Opened on its own, the vault goes back into this file: only if the app may write it.
    var writable by remember(file) { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(file) { writable = catching { state.backend.canWriteSuiteFile(file.ref) }.getOrDefault(false) }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(Res.string.single_choice_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.single_choice_text, file.ref.name, stringResource(kindLabel(kind))))
                if (opensOnItsOwn(kind)) {
                    Button(onClick = onOnlyThis, enabled = writable == true, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.single_only_this))
                    }
                    Hint(stringResource(if (writable == false) Res.string.single_not_writable else Res.string.single_only_this_hint))
                } else {
                    Hint(stringResource(Res.string.single_not_alone))
                }
                if (hasSuite) {
                    OutlinedButton(onClick = {
                        state.pendingAdd = PickedFile(file.ref.name, file.data)
                        onCancel()
                        // Back to the own suite: added once it is unlocked (at once if it is).
                        state.showStart = false
                        state.vaultExists = true
                    }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.single_add_to_suite)) }
                    Hint(stringResource(Res.string.single_add_hint))
                } else {
                    OutlinedButton(onClick = onNewSuite, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.single_new_suite)) }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onCancel) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Password plus key or key file of the vault; then only this vault is open. */
@Composable
fun OpenSingleVault(state: AppState, file: PickedSuiteFile, info: ContainerInfoUi, onCancel: () -> Unit) {
    val backend = state.backend
    val kind = vaultKindOf(info.kind)
    val hasKey = info.protection.securityKeys > 0
    val hasKeyfile = info.protection.keyfiles > 0
    var withKey by remember { mutableStateOf(hasKey) }
    val needsKey = withKey && hasKey
    var password by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    SectionCard(stringResource(Res.string.single_open_title)) {
        Hint(stringResource(Res.string.single_open_text, file.ref.name, stringResource(kindLabel(kind))))
        SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
        if (hasKey && hasKeyfile) FactorChoice(withKey) { withKey = it; error = null }
        if (needsKey) KeyPinField(state, pin) { pin = it; error = null } else KeyfilePicker(keyfile) { keyfile = it; error = null }
        BusyBanner(state)
        ErrorText(error)
        ButtonRow {
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
            Button(
                modifier = Modifier.weight(1f),
                enabled = !state.busy && password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null),
                onClick = {
                    state.run(onError = { error = it }, block = {
                        backend.openSingle(
                            file.ref, file.data, password, if (needsKey) pin else null,
                            if (needsKey) null else keyfile?.bytes, state.onTouchNeeded,
                        )
                    }) { k ->
                        password = ""; pin = ""
                        state.pendingAdd = null
                        state.single = SingleUi(k, file.ref)
                        state.singleSave = de.danerd.ghulsuite.model.SingleSaveUi.SAVED
                        state.unsaved = false
                        state.stack.clear()
                        state.tab = tabOf(k)
                        state.showStart = false
                        state.vaultExists = true
                    }
                },
            ) { Text(stringResource(Res.string.a_open)) }
        }
    }
}

/** Options while a vault is open on its own: its file, whether all is in it, closing. */
@Composable
fun SingleVaultSection(state: AppState) {
    val s = state.single ?: return
    val backend = state.backend
    val name = state.vaults.firstOrNull()?.name ?: s.file.name
    val savedAs = stringResource(Res.string.file_saved)
    val notWritten = stringResource(Res.string.file_not_written)
    // Into another file (also when the old one changed elsewhere or is gone): it is used from now on.
    val saveAs = rememberCreateSuiteFile { ref ->
        if (ref != null) {
            state.run(block = { backend.saveSingleAs(ref) }) { ok ->
                if (ok) {
                    // Closed meanwhile: nothing to show any more.
                    if (state.single == s) state.single = s.copy(file = ref)
                    state.singleSave = SingleSaveUi.SAVED
                    state.unsaved = backend.archiveUnsaved()
                    state.message(savedAs.replace("%1\$s", ref.name))
                } else {
                    state.message(notWritten)
                }
            }
        }
    }
    SectionCard(stringResource(Res.string.single_title)) {
        Hint(stringResource(Res.string.single_text, name, s.file.name))
        when {
            state.singleSave == SingleSaveUi.CHANGED_ELSEWHERE -> Banner(Icons.Filled.Warning, stringResource(Res.string.single_changed_elsewhere))
            state.singleSave == SingleSaveUi.MISSING -> Banner(
                Icons.Filled.Warning,
                stringResource(if (backend.canWriteFilesAnew) Res.string.single_missing else Res.string.single_missing_save_as),
            )
            state.singleNotSaved -> Banner(Icons.Filled.Warning, stringResource(Res.string.single_not_saved))
            state.unsaved -> Banner(Icons.Filled.Warning, stringResource(Res.string.single_saving))
            else -> Banner(Icons.Filled.CheckCircle, stringResource(Res.string.single_saved))
        }
        if (state.singleSave == SingleSaveUi.CHANGED_ELSEWHERE) {
            Button(onClick = { state.saveSingle(SingleWrite.OVERWRITE) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.file_overwrite))
            }
        } else if (state.singleSave == SingleSaveUi.MISSING) {
            // Written anew only where the platform can (and never over a file changed meanwhile).
            if (backend.canWriteFilesAnew) {
                Button(onClick = { state.saveSingle(SingleWrite.ANEW) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.single_write_anew))
                }
            }
        } else if (state.singleNotSaved) {
            Button(onClick = { state.saveSingle() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.single_try_again))
            }
        }
        if (filePickersAvailable) {
            OutlinedButton(onClick = { saveAs(s.file.name) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.file_save_as))
            }
        }
        Button(onClick = { backend.lock() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.single_close))
        }
        // Only with an own suite on this device to add it to.
        if (backend.singleFile() != null && backend.ownSuiteExists()) {
            OutlinedButton(onClick = {
                // The vault as it is here (also when writing its file failed), then the own suite.
                state.run(block = { backend.singleVaultBytes() }) { data ->
                    state.pendingAdd = PickedFile(s.file.name, data)
                    backend.lock()
                }
            }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.single_add_to_suite)) }
        }
        Hint(stringResource(Res.string.single_close_hint))
        BusyBanner(state)
    }
}

/**
 * A vault file waits to be added to the own suite: once it is unlocked, it is added (a vault
 * with another protection asks for its password and key or key file once).
 */
@Composable
fun PendingAddHandler(state: AppState) {
    val file = state.pendingAdd ?: return
    val backend = state.backend
    val unlocked by backend.unlocked.collectAsState()
    var foreign by remember { mutableStateOf<ForeignRequest?>(null) }
    val added = stringResource(Res.string.msg_vault_added)
    LaunchedEffect(file, unlocked, state.single) {
        if (!unlocked || state.single != null || foreign != null) return@LaunchedEffect
        val name = file.name.substringBeforeLast('.')
        state.run(onError = { state.pendingAdd = null; state.message(it) }, block = {
            try {
                val id = backend.addVaultFile(file.bytes, name)
                backend.listVaults().firstOrNull { it.id == id }?.kind to null
            } catch (e: VaultException) {
                if (e.kind != VaultErrorKind.DIFFERENT_PROTECTION) throw e
                val info = backend.inspectFile(file.bytes)
                null to ForeignRequest(file.bytes, vaultKindOf(info.kind), info.protection, name)
            }
        }) { (kind, request) ->
            if (request != null) {
                foreign = request
            } else {
                state.pendingAdd = null
                kind?.let { state.enableModule(it) }
                state.changed()
                state.message(added)
            }
        }
    }
    foreign?.let { r -> ForeignVaultDialog(state, r, onDone = { foreign = null; state.pendingAdd = null }) }
}

/**
 * "Hand on as a vault": the picked entries go into a new vault file with a password of its own
 * and a new key file or a security key. The file is saved, then the key file, then the new
 * recovery code is shown.
 */
@Composable
fun ShareAsVaultDialog(state: AppState, vaultId: String, ids: List<String>, onDone: () -> Unit) {
    val backend = state.backend
    var name by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var master by remember { mutableStateOf("") }
    var withKey by remember { mutableStateOf(false) }
    var pin by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var shared by remember { mutableStateOf<SharedVaultUi?>(null) }
    var keyfile by remember { mutableStateOf<ByteArray?>(null) }
    /**
     * After the form: 0 the code (noted first, so a vault file never exists without it), 1 the
     * key file saved, 2 the vault file saved.
     */
    var step by remember { mutableStateOf(0) }
    val notSaved = stringResource(Res.string.share_vault_not_saved)
    val keyLabel = stringResource(Res.string.setup_default_key_label)
    val keyfileLabel = stringResource(Res.string.keyfile_default_label)
    val recoveryLabel = stringResource(Res.string.setup_recovery_label)
    val tooShort = pluralStringResource(Res.plurals.setup_pw_too_short, PasswordPolicy.MIN_LENGTH, PasswordPolicy.MIN_LENGTH.toString())
    val tooWeak = stringResource(Res.string.setup_pw_too_weak)
    val mismatch = stringResource(Res.string.setup_pw_mismatch)
    val saveKeyfile = rememberSaveFile { ok -> if (ok) step = 2 else error = notSaved }
    val saveVault = rememberSaveFile { ok -> if (ok) step = 3 else error = notSaved }

    val result = shared
    AlertDialog(
        onDismissRequest = { if (!state.busy && result == null) onDone() },
        title = { Text(stringResource(Res.string.share_vault_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                when {
                    result == null -> {
                        Hint(pluralStringResource(Res.plurals.share_vault_text, ids.size, ids.size.toString()))
                        OutlinedTextField(
                            name, { name = it; error = null }, label = { Text(stringResource(Res.string.l_vault_name)) },
                            singleLine = true, modifier = Modifier.fillMaxWidth(),
                        )
                        SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_vault_password))
                        if (password.isNotEmpty()) StrengthMeter(backend.passwordScore(password))
                        SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
                        Hint(stringResource(Res.string.share_vault_factor))
                        ChipRow {
                            FilterChip(!withKey, { withKey = false; error = null }, label = { Text(stringResource(Res.string.share_vault_new_keyfile)) })
                            FilterChip(withKey, { withKey = true; error = null }, label = { Text(stringResource(Res.string.factor_key)) })
                        }
                        if (withKey) KeyPinField(state, pin) { pin = it; error = null }
                        Hint(stringResource(Res.string.share_vault_master))
                        SecretTextField(master, { master = it; error = null }, stringResource(Res.string.l_master_password))
                    }
                    step == 0 -> {
                        Hint(stringResource(Res.string.share_vault_code))
                        RecoveryCodeView(result.recoveryCode)
                        PrintSheetButton(state, result.recoveryCode)
                    }
                    step == 1 -> Hint(stringResource(Res.string.share_vault_save_keyfile))
                    step == 2 -> Hint(stringResource(Res.string.share_vault_save_file))
                    else -> Hint(stringResource(Res.string.share_vault_done))
                }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            when {
                result == null -> TextButton(
                    enabled = !state.busy && name.isNotBlank() && password.isNotEmpty() && master.isNotEmpty() &&
                        (!withKey || keyPinOk(state, pin)),
                    onClick = {
                        run {
                            error = PasswordPolicy.check(backend, password, repeat, tooShort, tooWeak, mismatch)
                            if (error != null) return@run
                            val kf = if (withKey) null else backend.generateKeyfile()
                            state.run(onError = { error = it }, block = {
                                backend.shareAsVault(
                                    ShareVaultUi(
                                        vaultId = vaultId, ids = ids, name = name.trim(), masterPassword = master,
                                        password = password, keyfile = kf,
                                        keyLabel = keyLabel, keyfileLabel = keyfileLabel, recoveryLabel = recoveryLabel,
                                    ),
                                    if (withKey) pin else null, state.onTouchNeeded,
                                )
                            }) { r ->
                                master = ""; password = ""; repeat = ""; pin = ""
                                keyfile = kf
                                shared = r
                            }
                        }
                    },
                ) { Text(stringResource(Res.string.a_create)) }
                step == 0 -> TextButton(onClick = { step = if (keyfile != null) 1 else 2 }) { Text(stringResource(Res.string.a_next)) }
                step == 1 -> TextButton(onClick = {
                    error = null
                    keyfile?.let { saveKeyfile("${name.trim().ifBlank { "vault" }}.ghlkey", it) }
                }) { Text(stringResource(Res.string.share_vault_save_keyfile_button)) }
                step == 2 -> TextButton(onClick = {
                    error = null
                    saveVault("${name.trim().ifBlank { "vault" }}.${extensionOf(result.kind)}", result.data)
                }) { Text(stringResource(Res.string.share_vault_save_file_button)) }
                else -> TextButton(onClick = onDone) { Text(stringResource(Res.string.a_finish)) }
            }
        },
        dismissButton = {
            if (result == null) TextButton(onClick = onDone, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) }
        },
    )
}

/**
 * Changes of a vault opened on its own that could not be written into its file when it was
 * closed (storage gone, app ended): offered again until written, saved elsewhere or given up.
 * A file changed elsewhere meanwhile is not overwritten without asking.
 */
@Composable
fun PendingSingleWrites(state: AppState) {
    val backend = state.backend
    val unlocked by backend.unlocked.collectAsState()
    val changes by backend.singleWritesChanged.collectAsState()
    var pending by remember { mutableStateOf<List<de.danerd.ghulsuite.model.PendingSingleUi>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }
    var tick by remember { mutableStateOf(0) }
    var confirmGiveUp by remember { mutableStateOf(false) }
    LaunchedEffect(unlocked, state.single, tick, changes) {
        pending = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) {
            catching { backend.pendingSingleWrites() }.getOrDefault(emptyList())
        }
    }
    val stillNot = stringResource(Res.string.single_pending_failed)
    val p = pending.firstOrNull()
    val saveAs = rememberCreateSuiteFile { ref ->
        val key = p?.key
        if (ref != null && key != null) {
            state.run(onError = { error = it }, block = { backend.saveSingleWriteAs(key, ref) }) { ok ->
                if (ok) { error = null; tick++ } else error = stillNot
            }
        }
    }
    if (p == null || state.single != null) return
    fun write(mode: SingleWrite) {
        state.run(onError = { error = it }, block = { backend.retrySingleWrite(p.key, mode) }) { r ->
            when (r) {
                SingleSaveUi.SAVED -> { error = null; tick++ }
                SingleSaveUi.CHANGED_ELSEWHERE, SingleSaveUi.MISSING -> tick++
                SingleSaveUi.FAILED -> error = stillNot
            }
        }
    }
    if (confirmGiveUp) {
        AlertDialog(
            danger = true,
            onDismissRequest = { confirmGiveUp = false },
            title = { Text(stringResource(Res.string.single_pending_discard_title)) },
            text = {
                Text(
                    stringResource(
                        when {
                            !p.targetKnown -> Res.string.single_pending_discard_unknown
                            p.missing -> Res.string.single_pending_discard_missing
                            // Changed or cut short (a write cut off may still look like a vault): be careful.
                            p.fileIntact && !p.changedElsewhere -> Res.string.single_pending_discard_text
                            else -> Res.string.single_pending_discard_damaged
                        },
                        p.file.name,
                    ),
                )
            },
            confirmButton = {
                TextButton(onClick = { confirmGiveUp = false; backend.discardSingleWrite(p.key); tick++ }) {
                    Text(stringResource(Res.string.single_pending_discard))
                }
            },
            dismissButton = { TextButton(onClick = { confirmGiveUp = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
        return
    }
    AlertDialog(
        danger = true,
        onDismissRequest = {},
        title = { Text(stringResource(Res.string.single_pending_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(if (p.targetKnown) Res.string.single_pending_text else Res.string.single_pending_unknown, p.file.name))
                if (p.changedElsewhere) Banner(Icons.Filled.Warning, stringResource(Res.string.single_pending_changed))
                if (p.missing && p.targetKnown) {
                    Banner(
                        Icons.Filled.Warning,
                        stringResource(if (backend.canWriteFilesAnew) Res.string.single_pending_missing else Res.string.single_missing_save_as),
                    )
                }
                if (filePickersAvailable) {
                    OutlinedButton(onClick = { error = null; saveAs(p.file.name) }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.file_save_as))
                    }
                }
                TextButton(onClick = { confirmGiveUp = true }, enabled = !state.busy) {
                    Text(stringResource(Res.string.single_pending_discard))
                }
                ErrorText(error)
            }
        },
        confirmButton = {
            if (p.targetKnown && (!p.missing || backend.canWriteFilesAnew)) {
                TextButton(enabled = !state.busy, onClick = {
                    write(
                        when {
                            p.changedElsewhere -> SingleWrite.OVERWRITE
                            // Checked again: back meanwhile and changed elsewhere is asked anew.
                            p.missing -> SingleWrite.ANEW
                            else -> SingleWrite.CHECKED
                        },
                    )
                }) {
                    Text(
                        stringResource(
                            when {
                                p.changedElsewhere -> Res.string.file_overwrite
                                p.missing -> Res.string.single_write_anew
                                else -> Res.string.single_pending_write
                            },
                        ),
                    )
                }
            }
        },
        dismissButton = {},
    )
}
