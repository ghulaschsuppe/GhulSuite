// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ArchiveTargetUi
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

/**
 * The place a new file is created at, as the system's save dialog gives it. Tests set [override]
 * (there is no dialog without a display): it gets the suggested name and answers like the
 * dialog (null: cancelled).
 */
object CreateFilePicker {
    var override: ((String) -> SuiteFileRef?)? = null
}

/** [rememberCreateSuiteFile], or the answer of [CreateFilePicker.override] in tests. */
@Composable
fun rememberCreateFile(onCreated: (SuiteFileRef?) -> Unit): (String) -> Unit {
    val system = rememberCreateSuiteFile(onCreated)
    return { name -> CreateFilePicker.override?.let { onCreated(it(name)) } ?: system(name) }
}

/**
 * Asks before something is deleted for good to put something new in its place: the warning as a
 * banner, what stays untouched, a word typed out instead of one tap (U4), a red button.
 */
@Composable
fun DeleteForGoodDialog(
    title: String,
    warning: String,
    note: String,
    confirm: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val word = stringResource(Res.string.delete_word)
    var typed by remember { mutableStateOf("") }
    AlertDialog(
        danger = true,
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Banner(Icons.Filled.Warning, warning)
                Text(note)
                ConfirmByName(word, typed) { typed = it }
            }
        },
        confirmButton = { TextButton(enabled = nameMatches(typed, word), onClick = onConfirm) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/**
 * Step 3 of the setup over an existing suite: deleting it without saving asks first. H5: only
 * with the proof of that suite, its master password plus key or key file, or its recovery code
 * (whoever finds the phone, even locked, must not wipe it). A wrong proof deletes nothing.
 */
@Composable
fun DeleteExistingSuiteDialog(state: AppState, onDeleted: () -> Unit, onDismiss: () -> Unit) {
    val word = stringResource(Res.string.delete_word)
    var typed by remember { mutableStateOf("") }
    val proof = remember { ProofInput(state.backend.settings.factorKey) }
    var withCode by remember { mutableStateOf(false) }
    var code by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        danger = true,
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(stringResource(Res.string.wz_delete_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Banner(Icons.Filled.Warning, stringResource(Res.string.wz_delete_warning))
                Text(stringResource(Res.string.wz_delete_note))
                Hint(stringResource(Res.string.wz_delete_proof))
                if (withCode) {
                    SecretTextField(code, { code = it; error = null }, stringResource(Res.string.l_recovery_code))
                    TextButton(onClick = { withCode = false; error = null }) { Text(stringResource(Res.string.wz_delete_with_factors)) }
                } else {
                    ProofFields(state, proof) { error = null }
                    TextButton(onClick = { withCode = true; error = null }) { Text(stringResource(Res.string.wz_delete_with_code)) }
                }
                ConfirmByName(word, typed) { typed = it }
                BusyBanner(state)
                ErrorText(error)
                // The last way out, where no proof is possible (or the suite here is damaged).
                Hint(deleteWayOut(state))
            }
        },
        confirmButton = {
            TextButton(
                enabled = !state.busy && nameMatches(typed, word) && (if (withCode) code.isNotBlank() else proof.ok(state)),
                onClick = {
                    state.run(onError = { error = it }, block = {
                        state.backend.deleteSuite(if (withCode) null else proof.proof(), if (withCode) code else null, state.onTouchNeeded)
                        state.vaultExists = state.backend.exists()
                    }) {
                        proof.clear()
                        code = ""
                        onDeleted()
                    }
                },
            ) { Text(stringResource(Res.string.a_delete_and_replace)) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/**
 * Step 3 of the setup over an existing suite: "Save the existing suite and replace it". The
 * existing suite is unlocked with its own password plus key or key file (a suite file is sealed
 * with its keys), then saved where the user picks, written as a stream and read back. Only
 * after that the existing suite is deleted on this device and [onReplaced] sets up the new one.
 * Cancelled or failed, nothing is deleted; leaving the dialog locks the existing suite again.
 */
@Composable
fun SaveExistingSuiteDialog(state: AppState, onReplaced: () -> Unit, onDismiss: () -> Unit) {
    val backend = state.backend
    val protection = remember { backend.protection() }
    var withKey by remember { mutableStateOf(state.backend.settings.factorKey) }
    val needsKey = withKey
    var password by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var unlocked by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val notSaved = stringResource(Res.string.wz_save_not_saved)
    val failed = stringResource(Res.string.wz_save_failed)
    val otherFile = stringResource(Res.string.wz_save_other_file)
    val savedAs = stringResource(Res.string.wz_saved_as)
    /** The factors the existing suite was unlocked with: deleting it needs them again (H5). */
    val kept = remember { arrayOfNulls<de.danerd.ghulsuite.model.FactorProofUi>(1) }
    /** A text of this dialog that says what happened (shown as it is, not as an error of the core). */
    val plain = remember { arrayOfNulls<String>(1) }
    fun fail(text: String): Nothing {
        plain[0] = text
        throw VaultException(VaultErrorKind.OTHER, text)
    }

    fun leave() {
        if (state.busy) return
        kept[0] = null
        if (unlocked) backend.lock()
        onDismiss()
    }

    /**
     * The file the existing suite was saved to in this dialog: the deletion can fail after it
     * (the key's second touch broke off); then only the deletion is tried again, the file stays
     * the place of the suite.
     */
    var savedTo by remember { mutableStateOf<SuiteFileRef?>(null) }
    val notDeleted = stringResource(Res.string.wz_saved_not_deleted)

    /** Deletes the existing suite with [kept] (H5), then the new one is set up. */
    fun replaceNow() {
        val name = savedTo?.name ?: return
        state.run(onError = { e ->
            // Locked by the try (see deleteSuite): the factors once more, the file is kept.
            unlocked = backend.unlocked.value
            error = notDeleted.replace("%1\$s", name) + "\n" + e
        }, block = {
            backend.deleteSuite(kept[0], null, state.onTouchNeeded)
            state.vaultExists = backend.exists()
        }) {
            unlocked = false
            kept[0] = null
            state.message(savedAs.replace("%1\$s", name))
            onReplaced()
        }
    }

    val pick = rememberCreateFile { target ->
        if (target == null) {
            error = notSaved
            return@rememberCreateFile
        }
        plain[0] = null
        state.run(onError = { error = plain[0] ?: it }, block = {
            // Never over a file of another suite or another state: the old suite is deleted next.
            val there = backend.archiveTarget(target)
            if (there != ArchiveTargetUi.NEW && there != ArchiveTargetUi.SAME) {
                fail(otherFile)
            }
            if (!backend.archiveSave(target, deviceInfo(), created = true, state.onProgress)) {
                fail(failed)
            }
            target
        }) { saved ->
            // Saved and read back (and remembered as the place of the suite): only now the
            // existing suite goes (H5: with the factors it was unlocked with; a key is touched
            // once more).
            savedTo = saved
            replaceNow()
        }
    }

    AlertDialog(
        onDismissRequest = { leave() },
        title = { Text(stringResource(Res.string.wz_save_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!unlocked) {
                    Hint(if (savedTo != null) stringResource(Res.string.wz_saved_confirm_delete) else stringResource(Res.string.wz_save_text))
                    SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
                    // Always both ways: what the file is protected with is not given away here.
        FactorChoice(withKey) { withKey = it; error = null; state.backend.settings.factorKey = it }
                    if (needsKey) KeyPinField(state, pin) { pin = it; error = null } else KeyfilePicker(keyfile) { keyfile = it; error = null }
                } else {
                    Hint(stringResource(Res.string.wz_save_unlocked))
                }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            if (!unlocked) {
                TextButton(
                    enabled = !state.busy && password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null),
                    onClick = {
                        // The app's own screens must not follow this unlock: it is only for saving.
                        state.showStart = true
                        // Saved already: the factors only confirm the deletion.
                        if (savedTo != null) {
                            kept[0] = de.danerd.ghulsuite.model.FactorProofUi(password, if (needsKey) pin else null, if (needsKey) null else keyfile?.bytes)
                            password = ""; pin = ""; keyfile = null
                            error = null
                            replaceNow()
                            return@TextButton
                        }
                        state.run(onError = { error = it }, block = {
                            if (needsKey) backend.unlock(password, pin, null, state.onTouchNeeded)
                            else backend.unlock(password, null, keyfile?.bytes, state.onTouchNeeded)
                        }) {
                            kept[0] = de.danerd.ghulsuite.model.FactorProofUi(password, if (needsKey) pin else null, if (needsKey) null else keyfile?.bytes)
                            password = ""; pin = ""; keyfile = null
                            unlocked = true
                            error = null
                        }
                    },
                ) { Text(stringResource(if (savedTo != null) Res.string.a_delete_and_replace else Res.string.a_unlock)) }
            } else {
                TextButton(enabled = !state.busy, onClick = {
                    error = null
                    pick(backend.suiteFile()?.name ?: SUITE_FILE_NAME)
                }) { Text(stringResource(Res.string.a_save_and_replace)) }
            }
        },
        dismissButton = { TextButton(enabled = !state.busy, onClick = { leave() }) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Where no proof of the suite here is possible: how to remove it by hand (Android, Windows). */
@Composable
fun deleteWayOut(state: AppState): String =
    if (state.backend.isDesktop) stringResource(Res.string.wz_delete_way_desktop, state.backend.dataFolder)
    else stringResource(Res.string.wz_delete_way_android)
