// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ArchiveCheckUi
import de.danerd.ghulsuite.model.ArchiveRelationUi
import androidx.compose.ui.platform.testTag
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

/** Extension of the suite file. */
const val SUITE_FILE_EXT = ".ghlzip"

/** File name suggested for a new suite file. */
const val SUITE_FILE_NAME = "GhulSuite$SUITE_FILE_EXT"

/** Extension of a GhulDocs package: the GhulDocs vault (or a part of it) as a file of its own. */
const val DOCS_PACKAGE_EXT = ".ghldocs"

/** Whether [name] is the name of a GhulDocs package (by its extension). */
fun isDocsPackageName(name: String) = name.endsWith(DOCS_PACKAGE_EXT, ignoreCase = true)

/** File name suggested for a GhulDocs package named [name]. */
fun docsPackageName(name: String) = "${name.trim().ifBlank { "GhulDocs" }}$DOCS_PACKAGE_EXT"

/** A suite file was picked where a GhulDocs package was expected (the core's text, translated). */
fun suiteNotDocsPackage() = VaultException(VaultErrorKind.INVALID_INPUT, "this is a suite file, not a GhulDocs package")

/**
 * The protection of the GhulDocs package [ref] (only its first entry is read): a suite file or
 * anything else is refused with a text that says what it is.
 */
suspend fun docsPackageProtection(backend: de.danerd.ghulsuite.model.VaultBackend, ref: SuiteFileRef): ProtectionUi {
    val info = backend.archiveInspect(ref)
    return when (info?.content) {
        de.danerd.ghulsuite.model.ArchiveContentUi.DOCS_VAULT -> info.protection
        de.danerd.ghulsuite.model.ArchiveContentUi.SUITE -> throw suiteNotDocsPackage()
        null -> throw VaultException(VaultErrorKind.INVALID_INPUT, "not a GhulSuite file")
    }
}

/**
 * A suite file picked to be opened: read from [source] (as a stream, when it is opened), saved
 * into [ref] later (an empty id: the app may not write it, "Save as" chooses the place).
 */
class PickedSuiteFile(val ref: SuiteFileRef, val source: SuiteFileRef = ref)

/** A vault file picked to be opened on its own or added (small: read as a whole). */
class PickedVaultFile(val ref: SuiteFileRef, val data: ByteArray)

/** The questions before a checked file replaces the working copy (empty: none). */
@Composable
private fun questions(check: ArchiveCheckUi): List<String> = buildList {
    when (check.relation) {
        ArchiveRelationUi.OTHER_SUITE -> add(stringResource(Res.string.file_q_other_suite))
        ArchiveRelationUi.OLDER -> add(stringResource(Res.string.file_q_older))
        ArchiveRelationUi.BRANCH -> add(stringResource(Res.string.file_q_branch))
        else -> {}
    }
    if (check.unsavedChanges) add(stringResource(Res.string.file_q_unsaved))
    if (check.unverified) add(stringResource(Res.string.file_q_unverified))
}

/**
 * Opens a suite file: password plus key or key file (or, in an emergency, the recovery code
 * with a new password), then everything in it is checked. Questions come before anything is
 * replaced; [onOpened] runs once the working copy is the file's state.
 */
@Composable
fun OpenSuiteFile(
    state: AppState,
    file: PickedSuiteFile,
    onOpened: () -> Unit,
    onCancel: () -> Unit,
) {
    val backend = state.backend
    var protection by remember { mutableStateOf<ProtectionUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var withCode by remember { mutableStateOf(false) }
    var password by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var code by remember { mutableStateOf("") }
    var newPassword by remember { mutableStateOf("") }
    var repeat by remember { mutableStateOf("") }
    var check by remember { mutableStateOf<ArchiveCheckUi?>(null) }
    // Leaving this screen any other way (locking, back) forgets the checked file and its keys.
    DisposableEffect(file) { onDispose { backend.archiveCancel() } }
    val tooShort = pluralStringResource(Res.plurals.setup_pw_too_short, PasswordPolicy.MIN_LENGTH, PasswordPolicy.MIN_LENGTH.toString())
    val tooWeak = stringResource(Res.string.setup_pw_too_weak)
    val mismatch = stringResource(Res.string.setup_pw_mismatch)
    LaunchedEffect(file) {
        // Only its header is read; a GhulDocs package (.ghldocs) is refused with a text that says so.
        catching { backend.archiveProtection(file.source) }.onSuccess { protection = it }.onFailure { error = describe(it) }
    }
    val p = protection
    var withKey by remember(p) { mutableStateOf(state.backend.settings.factorKey) }
    val needsKey = withKey
    val opened = stringResource(Res.string.file_opened)

    fun accept() {
        state.run(onError = { error = it }, block = { backend.archiveAccept() }) {
            // A file handed over without the right to write it again: "Save as" chooses the place.
            backend.setSuiteFile(file.ref.takeIf { it.id.isNotEmpty() })
            check = null
            password = ""; pin = ""; code = ""; newPassword = ""; repeat = ""
            state.afterSuiteFileOpened()
            state.message(opened.replace("%1\$s", file.ref.name))
            onOpened()
        }
    }

    SectionCard(stringResource(Res.string.file_open_title)) {
        Hint(stringResource(Res.string.file_open_text, file.ref.name))
        if (p == null) {
            ErrorText(error)
            if (error == null) Hint(stringResource(Res.string.loading))
            OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_back)) }
            return@SectionCard
        }
        if (!withCode) {
            SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password))
            // Always both ways: what the file is protected with is not given away here.
        FactorChoice(withKey) { withKey = it; error = null; state.backend.settings.factorKey = it }
            if (needsKey) KeyPinField(state, pin) { pin = it; error = null } else KeyfilePicker(keyfile) { keyfile = it; error = null }
            TextButton(onClick = { withCode = true; error = null }) { Text(stringResource(Res.string.file_with_code)) }
        } else {
            Hint(stringResource(Res.string.file_with_code_hint))
            SecretTextField(code, { code = it; error = null }, stringResource(Res.string.l_recovery_code))
            SecretTextField(newPassword, { newPassword = it; error = null }, stringResource(Res.string.l_new_password))
            if (newPassword.isNotEmpty()) StrengthMeter(backend.passwordScore(newPassword))
            SecretTextField(repeat, { repeat = it; error = null }, stringResource(Res.string.l_repeat_password))
            TextButton(onClick = { withCode = false; error = null }) { Text(stringResource(Res.string.file_with_password)) }
        }
        BusyBanner(state)
        ErrorText(error)
        ButtonRow {
            OutlinedButton(onClick = { backend.archiveCancel(); onCancel() }, modifier = Modifier.weight(1f)) {
                Text(stringResource(Res.string.a_back))
            }
            Button(
                modifier = Modifier.weight(1f),
                enabled = !state.busy && if (withCode) {
                    code.isNotBlank() && newPassword.isNotEmpty() && repeat.isNotEmpty()
                } else {
                    password.isNotEmpty() && (if (needsKey) keyPinOk(state, pin) else keyfile != null)
                },
                onClick = {
                    // The same rules as for every new master password (one typo would lock out).
                    if (withCode) {
                        error = PasswordPolicy.check(backend, newPassword, repeat, tooShort, tooWeak, mismatch)
                        if (error != null) return@Button
                    }
                    state.run(onError = { error = it }, block = {
                        if (withCode) {
                            backend.archiveOpenWithCode(file.source, code, newPassword, state.onProgress)
                        } else {
                            backend.archiveOpen(
                                file.source, password, if (needsKey) pin else null,
                                if (needsKey) null else keyfile?.bytes, state.onTouchNeeded, state.onProgress,
                            )
                        }
                    }) { c -> check = c }
                },
            ) { Text(stringResource(Res.string.a_open)) }
        }
    }

    check?.let { c ->
        val asks = questions(c)
        if (asks.isEmpty()) {
            LaunchedEffect(c) { accept() }
        } else {
            AlertDialog(
                danger = c.relation != ArchiveRelationUi.NEWER && c.relation != ArchiveRelationUi.NO_SUITE,
                onDismissRequest = { check = null; backend.archiveCancel() },
                title = { Text(stringResource(Res.string.file_q_title)) },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        for (q in asks) Text(q)
                        if (c.device.isNotBlank() && c.savedAt > 0) {
                            Hint(stringResource(Res.string.file_saved_on, formatDateTime(c.savedAt), c.device))
                        }
                        // Changes here would be lost: saving them first (then "Open" again; the
                        // password stays in its field).
                        if (c.unsavedChanges && state.single == null) {
                            Button(
                                onClick = { check = null; backend.archiveCancel(); state.askSave() },
                                modifier = Modifier.fillMaxWidth().testTag("open-save-first"),
                            ) { Text(stringResource(Res.string.save_first)) }
                        }
                    }
                },
                confirmButton = { TextButton(onClick = { accept() }) { Text(stringResource(Res.string.file_q_open_anyway)) } },
                dismissButton = {
                    TextButton(onClick = { check = null; backend.archiveCancel() }) { Text(stringResource(Res.string.a_cancel)) }
                },
            )
        }
    }
}

/**
 * Options > Suite file: where the suite was opened from or saved to, whether there are changes
 * that are not in it yet, saving (into the same file or a new one) and opening another file.
 */
@Composable
fun SuiteFileSection(state: AppState) {
    val backend = state.backend
    var error by remember { mutableStateOf<String?>(null) }
    var opening by remember { mutableStateOf<PickedSuiteFile?>(null) }
    var foreignDocs by remember { mutableStateOf<ForeignRequest?>(null) }
    // Saving goes through the save dialog (the same as the button in the header).
    val saveAs = rememberCreateSuiteFile { created ->
        if (created != null) {
            state.askSave()
            state.checkAndSave(created, created = true)
        }
    }
    val openOther = rememberOpenSuiteFile { r -> opening = PickedSuiteFile(r) }
    // A suite file or GhulDocs package handed over by another app while unlocked (see
    // AppState.applyLaunch): a GhulDocs package (by its name) is added to GhulDocs.
    LaunchedEffect(Unit) {
        AppLaunch.suiteFile.collect { f ->
            if (f != null) {
                AppLaunch.suiteFile.value = null
                if (isDocsPackageName(f.source.name)) {
                    catching { docsPackageProtection(backend, f.source) }
                        .onSuccess { p ->
                            foreignDocs = ForeignRequest(ByteArray(0), de.danerd.ghulsuite.model.VaultKindUi.DOCS, p, f.source.name.substringBeforeLast('.'), f.source)
                        }
                        .onFailure { error = describe(it) }
                } else {
                    opening = f
                }
            }
        }
    }

    foreignDocs?.let { r -> ForeignVaultDialog(state, r, onDone = { foreignDocs = null }) }
    opening?.let { f ->
        OpenSuiteFile(state, f, onOpened = { opening = null }, onCancel = { opening = null })
        return
    }

    SectionCard(stringResource(Res.string.file_title)) {
        // Read anew with every change (a save that ended, a file opened).
        val r = remember(state.unsaved, state.saveJob, opening) { backend.suiteFile() }
        Hint(if (r == null) stringResource(Res.string.file_none) else stringResource(Res.string.file_current, r.name))
        if (state.unsaved) {
            Banner(Icons.Filled.Warning, stringResource(Res.string.file_unsaved))
        } else if (r != null) {
            Banner(Icons.Filled.CheckCircle, stringResource(Res.string.file_all_saved))
        }
        val at = remember(state.unsaved, state.saveJob) { backend.archiveSavedAt() }
        if (at > 0) Hint(stringResource(Res.string.save_last, formatDateTime(at)))
        // What is where: the working copy at once, the suite file when saving.
        Hint(stringResource(Res.string.file_save_explain))
        val running = state.saveJob != null
        if (filePickersAvailable) {
            if (r != null || state.unsaved) {
                Button(onClick = { state.askSave() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth().testTag("file-save")) {
                    Text(stringResource(Res.string.file_save))
                }
            }
            OutlinedButton(onClick = { saveAs(r?.name ?: SUITE_FILE_NAME) }, enabled = !state.busy && !running, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.file_save_as))
            }
            OutlinedButton(onClick = openOther, enabled = !state.busy && !running, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.file_open_other))
            }
        }
        Hint(stringResource(Res.string.file_hint))
        BusyBanner(state)
        ErrorText(error)
    }
}

/** After a suite file replaced the working copy: everything is shown anew from the first area. */
fun AppState.afterSuiteFileOpened() {
    // A vault file waiting for the suite before is not added to this one unasked.
    pendingAdd = null
    pendingDocsAdd = null
    showStart = false
    vaultExists = true
    openFirstArea = true
    stack.clear()
    changed()
}

/** Text for the start screen's error when a picked file is no suite file. */
suspend fun notASuiteFile(): String = getString(Res.string.file_not_suite)
