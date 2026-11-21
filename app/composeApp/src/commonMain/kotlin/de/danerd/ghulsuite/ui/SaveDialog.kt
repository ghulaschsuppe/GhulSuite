// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ArchiveTargetInfoUi
import de.danerd.ghulsuite.model.ArchiveTargetUi
import de.danerd.ghulsuite.model.SafeSaveUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.stringResource

/** Where saving the suite file stands, as the save dialog shows it. */
sealed interface SaveStep {
    /** "Write the changes into the suite file now?" (or: no suite file yet, choose a place). */
    data object Ask : SaveStep

    /** The header of the file at the place is read (is it this working copy's own state?). */
    data object Checking : SaveStep

    /** The file there was saved elsewhere meanwhile, or is another file: the user decides. */
    class Conflict(val target: SuiteFileRef, val info: ArchiveTargetInfoUi, val created: Boolean) : SaveStep

    /** Being written and read back (a background job, it goes on when the dialog closes or the suite locks). */
    class Saving(val job: BgJob) : SaveStep

    /** Saved into [target]; [newer]: changes came in while it was written (still unsaved). */
    class Done(val target: SuiteFileRef, val newer: Boolean) : SaveStep

    /** Not saved: [text] says why; [target]: where it was tried (null: no place yet). */
    class Failed(val text: String, val target: SuiteFileRef?) : SaveStep
}

/** Test tags of the save dialog and the save button in the header. */
const val SAVE_BUTTON_TAG = "save-needed"
const val SAVE_CONFIRM_TAG = "save-confirm"

/** The save job that runs now, if any. */
val AppState.saveJob: BgJob? get() = jobs.list.firstOrNull { it.kind == JobKind.SAVE && it.active }

/**
 * Saves the working copy into [target]: first its header is read (a file saved elsewhere or
 * another file is not overwritten unasked, unless [force]), then it is written as a background job
 * ([startSaveJob]). [created]: the file was just made for this ("Save as").
 */
fun AppState.checkAndSave(target: SuiteFileRef, force: Boolean = false, created: Boolean = false) {
    // One at a time (a second tap while the file is checked is dropped).
    if (saveJob != null || saveStep == SaveStep.Checking) return
    saveStep = SaveStep.Checking
    scope.launch {
        val info = if (force) {
            null
        } else {
            try {
                backend.archiveTargetInfo(target)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (saveStep == SaveStep.Checking) saveStep = SaveStep.Failed(describe(e), target)
                return@launch
            }
        }
        // Closed meanwhile (cancelled) or locked: the result is not used.
        if (saveStep != SaveStep.Checking || !backend.unlocked.value) {
            if (saveStep == SaveStep.Checking) saveStep = SaveStep.Ask
            return@launch
        }
        // The place of the suite file cannot be read (a stick that is not there).
        if (!force && info == null && target == backend.suiteFile()) {
            saveStep = SaveStep.Failed(str(Res.string.file_unreachable), target)
            return@launch
        }
        if (info != null && info.target in setOf(ArchiveTargetUi.CHANGED_ELSEWHERE, ArchiveTargetUi.OTHER_SUITE, ArchiveTargetUi.OTHER_PROTECTION)) {
            saveStep = SaveStep.Conflict(target, info, created)
            return@launch
        }
        startSaveJob(target, created)
    }
}

/**
 * Writes the suite file as a background job without "Cancel": it goes on when the suite is
 * locked (the core notes the saved state then as well) and the automatic lock waits for it while
 * it makes progress; on Android a notification keeps the app alive meanwhile. Stopping midway
 * would leave an incomplete file on a stick (Android writes into the chosen document directly).
 */
fun AppState.startSaveJob(target: SuiteFileRef, created: Boolean) {
    val job = jobs.start(
        JobKind.SAVE,
        title = { str(Res.string.save_title) },
        lane = "suite-file",
        cancellable = false,
    ) { job ->
        try {
            val ok = backend.archiveSave(target, deviceInfo(), created) { done, total ->
                job.bytes = done
                job.bytesTotal = total
            }
            if (!ok) throw JobFailed(str(Res.string.file_not_written))
            backend.setSuiteFile(target)
            readUnsaved()
            // Windows: "Save" before quitting, or "Quit after saving".
            if (quitAfterSave) {
                quitAfterSave = false
                onQuit?.invoke()
            }
            val text = str(Res.string.file_saved, target.name)
            when {
                // Locked meanwhile: no file name on the lock screen.
                !backend.unlocked.value -> { noteAfterUnlock(text); null }
                saveDialog != null -> { saveStep = SaveStep.Done(target, unsaved); null }
                else -> text
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            readUnsaved()
            quitAfterSave = false
            val text = if (e is JobFailed) e.text else describe(e)
            when {
                !backend.unlocked.value -> { noteAfterUnlock(str(Res.string.save_failed_title) + ": " + text); null }
                saveDialog != null -> { saveStep = SaveStep.Failed(text, target); null }
                else -> throw JobFailed(text)
            }
        }
    }
    saveStep = SaveStep.Saving(job)
}

/** Closes the save dialog; once saved, what was asked to follow (e.g. opening another file) runs. */
fun AppState.closeSaveDialog() {
    val then = (saveStep as? SaveStep.Done)?.let { saveDialog?.then }
    // Closed without saving: quitting does not wait for a save any more.
    if (saveJob == null) quitAfterSave = false
    saveDialog = null
    saveStep = SaveStep.Ask
    then?.invoke()
}

/**
 * The dialog behind the save button: which file, when it was saved last, then saving with its
 * progress; a file saved elsewhere meanwhile is only overwritten when the user says so (saving
 * as a new file is offered first). Without a suite file it asks for a place.
 */
@Composable
fun SaveSuiteDialog(state: AppState) {
    state.saveDialog ?: return
    val backend = state.backend
    val saveAs = rememberCreateSuiteFile { created -> if (created != null) state.checkAndSave(created, created = true) }
    // A save that runs (started before, e.g. the dialog was closed meanwhile) is shown.
    val running = state.saveJob
    androidx.compose.runtime.LaunchedEffect(running) {
        if (running != null && state.saveStep !is SaveStep.Saving) state.saveStep = SaveStep.Saving(running)
    }
    val step = state.saveStep
    val target = backend.suiteFile()
    // Android: a folder shared once makes saving safe (new file next to the old one, then swapped).
    var shared by androidx.compose.runtime.remember { mutableStateOf(0) }
    val grant = rememberGrantFolder { shared++ }
    val safe = androidx.compose.runtime.remember(target, shared) { target?.let { backend.safeSave(it) } }
    val dismiss = { state.closeSaveDialog() }
    GhulSheet(onDismissRequest = dismiss, title = stringResource(
        when (step) {
            is SaveStep.Conflict -> Res.string.save_conflict_title
            is SaveStep.Failed -> Res.string.save_failed_title
            else -> Res.string.save_title
        },
    )) {
        when (step) {
            SaveStep.Ask -> {
                if (target == null) {
                    Text(stringResource(Res.string.save_no_file))
                } else {
                    Text(stringResource(Res.string.save_ask))
                    Hint(stringResource(Res.string.save_file, target.name))
                }
                val at = backend.archiveSavedAt()
                Hint(if (at > 0) stringResource(Res.string.save_last, formatDateTime(at)) else stringResource(Res.string.save_never))
                Hint(stringResource(Res.string.save_hint))
                when (safe) {
                    SafeSaveUi.FOLDER_NEEDED -> {
                        Hint(stringResource(Res.string.save_safe_folder))
                        OutlinedButton(onClick = { target?.let { grant(it.id) } }, modifier = Modifier.fillMaxWidth()) {
                            Text(stringResource(Res.string.save_safe_grant))
                        }
                    }
                    SafeSaveUi.OVERWRITES -> Hint(stringResource(Res.string.save_overwrites))
                    else -> {}
                }
                if (target != null) {
                    Button(onClick = { state.checkAndSave(target) }, modifier = Modifier.fillMaxWidth().testTag(SAVE_CONFIRM_TAG)) {
                        Text(stringResource(Res.string.file_save))
                    }
                }
                if (filePickersAvailable) {
                    OutlinedButton(onClick = { saveAs(target?.name ?: SUITE_FILE_NAME) }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(if (target == null) Res.string.save_place else Res.string.file_save_as))
                    }
                }
                OutlinedButton(onClick = { state.closeSaveDialog() }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_later))
                }
            }
            SaveStep.Checking -> {
                Text(stringResource(Res.string.save_checking))
                LinearProgressIndicator(Modifier.fillMaxWidth(), color = g.accent, trackColor = g.surface3)
                // A slow place (a cloud app fetching the file first): cancelled, the result is dropped.
                OutlinedButton(onClick = { state.closeSaveDialog() }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_cancel))
                }
            }
            is SaveStep.Conflict -> {
                Text(
                    stringResource(
                        when (step.info.target) {
                            ArchiveTargetUi.OTHER_SUITE -> Res.string.save_conflict_other
                            ArchiveTargetUi.OTHER_PROTECTION -> Res.string.save_conflict_protection
                            else -> Res.string.save_conflict_changed
                        },
                    ),
                )
                if (step.info.device.isNotBlank() && step.info.time > 0) {
                    Hint(stringResource(Res.string.file_saved_on, formatDateTime(step.info.time), step.info.device))
                }
                if (filePickersAvailable) {
                    Button(onClick = { state.saveStep = SaveStep.Ask; saveAs(SUITE_FILE_NAME) }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.save_keep_mine))
                    }
                }
                // A newer protection there is not overwritten from here: that would undo it.
                if (step.info.target != ArchiveTargetUi.OTHER_PROTECTION) {
                    androidx.compose.runtime.CompositionLocalProvider(LocalButtonLook provides ButtonLook.DANGER) {
                        TextButton(
                            onClick = { state.checkAndSave(step.target, force = true, created = step.created) },
                            modifier = Modifier.testTag("save-overwrite"),
                        ) { Text(stringResource(Res.string.file_overwrite)) }
                    }
                }
                OutlinedButton(onClick = { state.closeSaveDialog() }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_cancel))
                }
            }
            is SaveStep.Saving -> {
                val p = step.job.progress
                Text(if (p == null) stringResource(Res.string.save_running) else stringResource(Res.string.save_progress, "${(p * 100).toInt()} %"))
                val bar = Modifier.fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp))
                if (p == null) {
                    LinearProgressIndicator(bar, color = g.accent, trackColor = g.surface3)
                } else {
                    Box(bar.background(g.surface3)) { Box(Modifier.fillMaxWidth(p).height(6.dp).clip(RoundedCornerShape(3.dp)).background(g.accent)) }
                }
                Hint(stringResource(Res.string.save_hint))
                // The dialog may close; the job goes on in the strip under the header.
                OutlinedButton(onClick = { state.saveDialog = null }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.save_background))
                }
                // Ended while the dialog was closed and opened again: the job's end decides.
                val failedText = stringResource(Res.string.file_not_written)
                androidx.compose.runtime.LaunchedEffect(step.job.active) {
                    if (!step.job.active && state.saveStep === step) {
                        state.saveStep = if (step.job.state == JobState.DONE && target != null) SaveStep.Done(target, state.unsaved)
                        else SaveStep.Failed(step.job.result.ifEmpty { failedText }, target)
                    }
                }
            }
            is SaveStep.Done -> {
                Text(stringResource(Res.string.file_saved, step.target.name))
                if (step.newer) Hint(stringResource(Res.string.save_newer))
                Button(onClick = { state.closeSaveDialog() }, modifier = Modifier.fillMaxWidth().testTag("save-done")) {
                    Text(stringResource(Res.string.a_ok))
                }
            }
            is SaveStep.Failed -> {
                Text(step.text)
                step.target?.let { t ->
                    Button(onClick = { state.checkAndSave(t) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_retry)) }
                }
                if (filePickersAvailable) {
                    OutlinedButton(onClick = { state.saveStep = SaveStep.Ask; saveAs(step.target?.name ?: SUITE_FILE_NAME) }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.file_save_as))
                    }
                }
                OutlinedButton(onClick = { state.closeSaveDialog() }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_cancel))
                }
            }
        }
    }
}

/** Text of the progress of the save job that runs now ("45 %"), empty while it is not known. */
fun savePercent(p: Float?): String = if (p == null || p < 0f) "" else "${(p * 100).toInt()} %"
