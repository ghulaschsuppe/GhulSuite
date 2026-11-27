// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.NewFactorUi
import de.danerd.ghulsuite.model.SlotType
import de.danerd.ghulsuite.model.SlotUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource

/**
 * What protects the suite, as Options, Keys and key files shows it: the main factor (a key if
 * there is one, else a key file), its backups and the recovery code.
 */
data class ProtectionView(val main: SlotUi?, val backups: List<SlotUi>, val code: SlotUi?) {
    val keyMain: Boolean get() = main?.type == SlotType.SECURITY_KEY

    /** A key without a backup (a suite from before the backup was required). */
    val needsBackup: Boolean get() = keyMain && backups.isEmpty()

    /** All second factors (what a change of the protection removes). */
    val factors: List<SlotUi> get() = listOfNotNull(main) + backups
}

fun protectionOf(slots: List<SlotUi>): ProtectionView {
    val factors = slots.filter { it.type != SlotType.RECOVERY_CODE }.sortedBy { it.created }
    val main = factors.firstOrNull { it.type == SlotType.SECURITY_KEY } ?: factors.firstOrNull()
    return ProtectionView(main, factors.filter { it != main }, slots.firstOrNull { it.type == SlotType.RECOVERY_CODE })
}

/** A change of the second factors, each a flow of steps in one dialog. */
sealed class FactorTask {
    /** [slot] is replaced by a new factor; the old one goes. */
    data class Replace(val slot: SlotUi, val isMain: Boolean) : FactorTask()

    /** A key gets the backup it lacks. */
    data object AddBackup : FactorTask()

    /** From key to key file or back (with the backup a key needs); all old factors go. */
    data object Switch : FactorTask()
}

/** One screen of a factor flow. */
enum class FactorStep { CONFIRM, NEW_FACTOR, NEW_BACKUP, REMOVED, DONE }

/** The steps of [task], in order: the protection of now is always confirmed first. */
fun factorSteps(task: FactorTask, view: ProtectionView): List<FactorStep> = when (task) {
    is FactorTask.Replace -> listOf(FactorStep.CONFIRM, FactorStep.NEW_FACTOR, FactorStep.REMOVED, FactorStep.DONE)
    FactorTask.AddBackup -> listOf(FactorStep.CONFIRM, FactorStep.NEW_FACTOR, FactorStep.DONE)
    FactorTask.Switch -> if (view.keyMain) {
        listOf(FactorStep.CONFIRM, FactorStep.NEW_FACTOR, FactorStep.REMOVED, FactorStep.DONE)
    } else {
        listOf(FactorStep.CONFIRM, FactorStep.NEW_FACTOR, FactorStep.NEW_BACKUP, FactorStep.REMOVED, FactorStep.DONE)
    }
}

/** What the new factor of [task] is: a key (true), a key file (false), or the user's choice (null). */
private fun newIsKey(task: FactorTask, view: ProtectionView): Boolean? = when (task) {
    is FactorTask.Replace -> if (task.isMain) task.slot.type == SlotType.SECURITY_KEY else null
    FactorTask.AddBackup -> null
    FactorTask.Switch -> !view.keyMain
}

/** The slots [task] removes once its new factors are there. */
private fun removedBy(task: FactorTask, view: ProtectionView): List<String> = when (task) {
    is FactorTask.Replace -> listOf(task.slot.id)
    FactorTask.AddBackup -> emptyList()
    FactorTask.Switch -> view.factors.map { it.id }
}

const val FACTOR_STEP_TAG = "factor-step-"
const val FACTOR_RECONFIRM_TAG = "factor-reconfirm"

/**
 * How long the confirmation with a key of now is used for the change (the core keeps it two
 * minutes, once). Older, or after a failed try, the key of now confirms once more first.
 */
object FactorTiming {
    var confirmMaxMillis = 100_000L
}

/** Errors of the proof itself (password or key file of now): back to the step that asks for it. */
private val PROOF_ERRORS = setOf(
    VaultErrorKind.WRONG_PASSWORD, VaultErrorKind.WRONG_KEYFILE, VaultErrorKind.KEYFILE_REQUIRED, VaultErrorKind.SECURITY_KEY_REQUIRED,
)
const val FACTOR_NEXT_TAG = "factor-next"

/**
 * The flow of [task], one step per screen: (1) both factors of now, (2) the new factor (a key is
 * plugged in and touched, a key file made and saved), (3) the old factor removed, (4) done, with
 * the advice to save the suite file and about older copies of it.
 */
@Composable
fun FactorFlowDialog(state: AppState, task: FactorTask, view: ProtectionView, onDismiss: () -> Unit) {
    val backend = state.backend
    val steps = remember(task) { factorSteps(task, view) }
    var step by remember(task) { mutableStateOf(FactorStep.CONFIRM) }
    val proof = rememberProofInput(state)
    var error by remember { mutableStateOf<String?>(null) }
    // The new factor of the step now (key or key file), with what it needs.
    val fixed = newIsKey(task, view)
    var asKey by remember(task) { mutableStateOf(fixed ?: (task as? FactorTask.Replace)?.slot?.type?.let { it == SlotType.SECURITY_KEY } ?: !filePickersAvailable) }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<ByteArray?>(null) }
    var keyfileSaved by remember { mutableStateOf(false) }
    // The new factor of the step is there (the check mark), so "Next" goes on.
    var added by remember { mutableStateOf(false) }
    val failText = rememberSaveFileFailedText()
    val saveKeyfile = rememberSaveFile { r ->
        keyfileSaved = r == SaveFileResult.SAVED
        if (keyfileSaved) error = null else failText(r)?.let { error = it }
    }
    val sameKey = stringResource(Res.string.fac_same_key)
    val removes = removedBy(task, view)
    // Confirmed with a key of now: the core's confirmation stands in for it, once and for a short
    // time. A key file confirms itself with every change (no confirmation is left behind).
    var keyProof by remember { mutableStateOf(false) }
    var confirmedAt by remember { mutableStateOf<kotlin.time.TimeMark?>(null) }
    // The confirmation is used up or too old: the key of now confirms once more before the next try.
    var reconfirm by remember { mutableStateOf(false) }

    fun freshFactor() {
        pin = ""; keyfile = null; keyfileSaved = false; added = false; error = null
    }

    fun next() {
        val i = steps.indexOf(step)
        step = steps[i + 1]
        if (step == FactorStep.NEW_BACKUP) {
            freshFactor()
            asKey = true
        }
    }

    /** Sets up the new factor of this step; the old ones go with the last new one. */
    fun setUp() {
        val last = steps.indexOf(step) + 1 >= steps.size || steps[steps.indexOf(step) + 1] != FactorStep.NEW_BACKUP
        // Review M-1: a confirmation that is too old is not tried (it would fail and be used up).
        if (keyProof && (reconfirm || (confirmedAt?.elapsedNow()?.inWholeMilliseconds ?: Long.MAX_VALUE) > FactorTiming.confirmMaxMillis)) {
            reconfirm = true
            return
        }
        var proofFailed = false
        state.run(onError = {
            error = it
            // Review M-1: whatever failed, the confirmation of the key of now is gone with it.
            if (keyProof) reconfirm = true
            // A wrong password or key file of now: back to where it is asked for.
            else if (proofFailed) step = FactorStep.CONFIRM
        }, block = {
            val label = when {
                task is FactorTask.Replace && task.isMain -> task.slot.label.ifBlank { getString(if (asKey) Res.string.setup_default_key_label else Res.string.keyfile_default_label) }
                asKey -> getString(if (step == FactorStep.NEW_FACTOR && fixed == true) Res.string.setup_default_key_label else Res.string.slot_backup_key)
                else -> getString(Res.string.keyfile_default_label)
            }
            val f = if (asKey) NewFactorUi.Key(pin, label) else NewFactorUi.Keyfile(keyfile!!, label)
            try {
                backend.replaceFactors(proof.proof(), listOf(f), if (last) removes else emptyList(), state.onTouchNeeded)
                true
            } catch (e: VaultException) {
                // The same key again: said at once, the step stays.
                if (e.kind == VaultErrorKind.ALREADY_EXISTS && asKey) false else {
                    proofFailed = e.kind in PROOF_ERRORS
                    throw e
                }
            }
        }) { ok ->
            if (!ok) {
                error = sameKey
                if (keyProof) reconfirm = true
                return@run
            }
            added = true
            pin = ""
            // Its last step asks to save (no second dialog for it, as after other changes of the protection).
            state.changed()
        }
    }

    /** Review M-1: the key of now once more (in its own connection), then the new factor again. */
    fun confirmAgain() {
        state.run(onError = { error = it }, block = { backend.confirmProtection(proof.proof(), state.onTouchNeeded) }) {
            reconfirm = false
            confirmedAt = kotlin.time.TimeSource.Monotonic.markNow()
            error = null
        }
    }

    val title = when (task) {
        is FactorTask.Replace -> stringResource(Res.string.a_replace) + ": " + task.slot.label.ifBlank { stringResource(slotTypeLabel(task.slot.type)) }
        FactorTask.AddBackup -> stringResource(Res.string.fac_setup_backup)
        FactorTask.Switch -> stringResource(Res.string.fac_switch)
    }
    val closing = step == FactorStep.REMOVED || step == FactorStep.DONE
    AlertDialog(
        onDismissRequest = { if (!state.busy && !closing) onDismiss() },
        title = { Text(title) },
        text = {
            Column(Modifier.testTag(FACTOR_STEP_TAG + step.name.lowercase()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(Res.string.fac_step_of, (steps.indexOf(step) + 1).toString(), steps.size.toString()))
                when (step) {
                    FactorStep.CONFIRM -> {
                        Text(stringResource(Res.string.fac_confirm_text))
                        ProofFields(state, proof) { error = null }
                    }
                    FactorStep.NEW_FACTOR, FactorStep.NEW_BACKUP -> if (reconfirm) {
                        Text(stringResource(Res.string.fac_reconfirm))
                    } else {
                        val choose = (step == FactorStep.NEW_BACKUP || fixed == null) && filePickersAvailable
                        if (choose && !added) {
                            Text(stringResource(Res.string.fac_choose_kind))
                            ChipRow {
                                FilterChip(asKey, { asKey = true; freshFactor() }, label = { Text(stringResource(Res.string.factor_key)) })
                                FilterChip(!asKey, { asKey = false; freshFactor() }, label = { Text(stringResource(Res.string.factor_keyfile)) })
                            }
                        }
                        if (asKey) {
                            if (!added) {
                                Text(stringResource(if (step == FactorStep.NEW_BACKUP) Res.string.fac_new_backup_key else Res.string.fac_new_key))
                                KeyPinField(state, pin) { pin = it; error = null }
                            } else {
                                Done(stringResource(Res.string.fac_key_ready))
                            }
                        } else {
                            if (!added) {
                                Text(stringResource(Res.string.fac_new_keyfile))
                                FilledTonalButton(onClick = {
                                    val data = keyfile ?: backend.generateKeyfile().also { keyfile = it }
                                    saveKeyfile(KEYFILE_NAME, data)
                                }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.keyfile_create_save)) }
                                if (keyfileSaved) Done(stringResource(Res.string.keyfile_ready))
                            } else {
                                Done(stringResource(Res.string.keyfile_ready))
                                Hint(stringResource(Res.string.fac_keyfile_copy))
                            }
                        }
                    }
                    FactorStep.REMOVED -> Done(stringResource(if (removes.size > 1) Res.string.fac_removed_many else Res.string.fac_removed_one))
                    FactorStep.DONE -> {
                        Done(stringResource(Res.string.fac_done))
                        if (removes.isNotEmpty()) Text(stringResource(Res.string.fac_old_copies))
                    }
                }
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            when (step) {
                FactorStep.CONFIRM -> TextButton(onClick = {
                    val p = proof.proof()
                    state.run(onError = { error = it }, block = { backend.confirmProtection(p, state.onTouchNeeded) }) {
                        keyProof = p.pin != null
                        confirmedAt = kotlin.time.TimeSource.Monotonic.markNow()
                        reconfirm = false
                        error = null
                        next()
                    }
                }, enabled = !state.busy && proof.ok(state), modifier = Modifier.testTag(FACTOR_NEXT_TAG)) { Text(stringResource(Res.string.a_next)) }
                FactorStep.NEW_FACTOR, FactorStep.NEW_BACKUP -> if (reconfirm) {
                    TextButton(onClick = { confirmAgain() }, enabled = !state.busy, modifier = Modifier.testTag(FACTOR_RECONFIRM_TAG)) { Text(stringResource(Res.string.a_next)) }
                } else if (added) {
                    TextButton(onClick = { next() }, modifier = Modifier.testTag(FACTOR_NEXT_TAG)) { Text(stringResource(Res.string.a_next)) }
                } else {
                    val ready = if (asKey) keyPinOk(state, pin) else keyfile != null && keyfileSaved
                    TextButton(onClick = { setUp() }, enabled = !state.busy && ready, modifier = Modifier.testTag(FACTOR_NEXT_TAG)) {
                        Text(stringResource(if (asKey) Res.string.a_register_key else Res.string.a_next))
                    }
                }
                FactorStep.REMOVED -> TextButton(onClick = { next() }, modifier = Modifier.testTag(FACTOR_NEXT_TAG)) { Text(stringResource(Res.string.a_next)) }
                FactorStep.DONE -> TextButton(onClick = {
                    proof.clear()
                    onDismiss()
                    state.askSave()
                }, modifier = Modifier.testTag(FACTOR_NEXT_TAG)) { Text(stringResource(Res.string.a_save_now)) }
            }
        },
        dismissButton = {
            if (step == FactorStep.DONE) {
                TextButton(onClick = { proof.clear(); onDismiss() }) { Text(stringResource(Res.string.a_later)) }
            } else if (!closing) {
                TextButton(onClick = { proof.clear(); onDismiss() }, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) }
            }
        },
    )
}

/** A step that is done: a check mark and what is done. */
@Composable
private fun Done(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(Icons.Filled.CheckCircle, null, tint = okColor())
        Text(text, color = okColor())
    }
}

fun slotTypeLabel(t: SlotType) = when (t) {
    SlotType.SECURITY_KEY -> Res.string.slot_key
    SlotType.RECOVERY_CODE -> Res.string.slot_code
    SlotType.KEYFILE -> Res.string.slot_keyfile
}
