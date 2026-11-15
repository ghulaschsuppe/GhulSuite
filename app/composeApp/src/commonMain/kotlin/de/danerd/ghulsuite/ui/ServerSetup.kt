// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.ServerFoldersUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerSavedUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource

/** Folder name suggested for a new suite folder on a server. */
private const val SUGGESTED_FOLDER = "ghulsuite"

/** Hostings deliver these folders as the website; vaults do not belong there. */
private val WEB_ROOTS = setOf("public_html", "htdocs", "httpdocs", "html", "www", "web", "public")

/**
 * State of one server form, kept by the screen that shows it (the wizard asks it what is still
 * missing before it moves on).
 */
@Stable
class ServerForm {
    var kind by mutableStateOf(ServerKindUi.SFTP)
    var input by mutableStateOf(ServerInputUi(name = "", host = "", port = 22, user = "", password = null, base = ""))
    var usePassword by mutableStateOf(false)
    /** SFTP with password: put the suite's SSH key on the server and do not store the password. */
    var installKey by mutableStateOf(true)
    /** Fingerprint of the SSH host key or TLS certificate the user confirmed. */
    var confirmed by mutableStateOf<String?>(null)
    /** The login was tried and works (only then the folder can be chosen). */
    var connected by mutableStateOf(false)
    /** The last "Test connection" failed: the checklist says so instead of "still to do". */
    var connectFailed by mutableStateOf(false)
    /** Chosen folder, relative to the login folder ("" = none chosen yet). */
    var folder by mutableStateOf("")
    /** Set once the server is checked and stored. */
    var savedId by mutableStateOf<String?>(null)
    var savedWhere by mutableStateOf("")

    val loginReady: Boolean
        get() = input.host.isNotBlank() && input.user.isNotBlank() &&
            (kind == ServerKindUi.SFTP && !usePassword || !input.password.isNullOrEmpty())

    /** The step still missing, or null when the server is stored. */
    fun missing(): StringResource? = when {
        savedId != null -> null
        !loginReady -> Res.string.srvf_step_login
        !connected -> Res.string.srvf_step_connect
        folder.isBlank() -> Res.string.srvf_step_folder
        else -> Res.string.srvf_step_test
    }

    fun switchKind(k: ServerKindUi) {
        kind = k
        input = input.copy(port = if (k == ServerKindUi.SFTP) 22 else 21, password = if (k == ServerKindUi.SFTP) null else "")
        usePassword = k != ServerKindUi.SFTP
        confirmed = null
        connected = false
        folder = ""
        savedId = null
    }

    /** The login changed: test it again, then choose the folder again. */
    fun relogin() {
        connected = false
        connectFailed = false
        folder = ""
    }

    /** The login as typed. */
    fun login(): ServerInputUi = when (kind) {
        ServerKindUi.SFTP -> input.copy(kind = ServerKindUi.SFTP, password = if (usePassword) input.password else null)
        ServerKindUi.FTPS -> input.copy(kind = ServerKindUi.FTPS)
    }
}

@Composable
fun rememberServerForm(): ServerForm = remember { ServerForm() }

/**
 * One server form for the setup wizard, GhulDrive and the options. First the login is tested
 * ("Test connection"); only a working login gets to pick a folder from the server's own list,
 * then the server is saved. A numbered checklist below names what is still missing. The server
 * module for share links is installed later from the list of servers, over the same login.
 *
 * [save] stores the checked server (suite list for synchronisation, Drive file for GhulDrive).
 */
@Composable
fun ServerSetup(
    state: AppState,
    form: ServerForm,
    save: suspend (ServerInputUi, String?) -> ServerSavedUi,
    onSaved: (String) -> Unit,
) {
    val net = state.backend.net
    var error by remember { mutableStateOf<String?>(null) }
    var fingerprint by remember { mutableStateOf<String?>(null) }
    /** What to do once the fingerprint is confirmed. */
    var afterConfirm by remember { mutableStateOf<(() -> Unit)?>(null) }
    var picking by remember { mutableStateOf(false) }
    var publicKey by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { publicKey = catching { net.sshPublicKey() }.getOrNull() }

    SingleChoiceSegmentedButtonRow {
        for (k in ServerKindUi.entries) {
            SegmentedButton(form.kind == k, { if (form.kind != k) { form.switchKind(k); error = null } }) { SegLabel(kindLabel(k)) }
        }
    }

    val saved = form.savedId
    if (saved != null) {
        Banner(Icons.Filled.CheckCircle, stringResource(Res.string.srvf_connected, form.savedWhere))
        return
    }

    /** Stores the server: SFTP with password puts the SSH key on it first. */
    fun store(input: ServerInputUi, confirmed: String?) {
        state.run(onError = { error = it }, block = {
            if (form.kind == ServerKindUi.SFTP && form.usePassword && form.installKey) {
                val fp = net.installSshKey(form.input.copy(kind = ServerKindUi.SFTP), confirmed)
                if (fp.isNotEmpty()) return@run ServerSavedUi.Confirm(fp)
            }
            save(input, confirmed)
        }) { r ->
            when (r) {
                is ServerSavedUi.Confirm -> {
                    fingerprint = r.fingerprint
                    afterConfirm = { store(input, r.fingerprint) }
                }
                is ServerSavedUi.Saved -> {
                    form.confirmed = confirmed
                    form.savedWhere = "${input.host}:/${form.folder}"
                    form.savedId = r.id
                    onSaved(r.id)
                }
            }
        }
    }

    fun test() {
        val login = form.login()
        val input = when (form.kind) {
            ServerKindUi.SFTP -> (if (form.usePassword && form.installKey) login.copy(password = null) else login).copy(base = form.folder)
            ServerKindUi.FTPS -> login.copy(base = form.folder)
        }
        store(input.copy(name = input.name.ifBlank { input.host }), form.confirmed)
    }

    val loginFirst = stringResource(Res.string.srvf_login_first)
    // Login.
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(form.input.host, { form.input = form.input.copy(host = it.trim()); form.relogin(); error = null }, Modifier.weight(3f),
            label = { Text(stringResource(Res.string.server_host)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
        OutlinedTextField(if (form.input.port == 0) "" else form.input.port.toString(),
            { v -> form.input = form.input.copy(port = v.filter(Char::isDigit).take(5).toIntOrNull() ?: 0); form.relogin() }, Modifier.weight(1f),
            label = { Text(stringResource(Res.string.server_port)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    }
    OutlinedTextField(form.input.user, { form.input = form.input.copy(user = it.trim()); form.relogin(); error = null }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.server_user)) }, singleLine = true)
    if (form.kind == ServerKindUi.SFTP) {
        ChipRow {
            FilterChip(!form.usePassword, { form.usePassword = false; form.relogin() }, label = { Text(stringResource(Res.string.server_auth_key)) })
            FilterChip(form.usePassword, { form.usePassword = true; form.relogin(); if (form.input.password == null) form.input = form.input.copy(password = "") },
                label = { Text(stringResource(Res.string.server_auth_password)) })
        }
    }
    if (form.kind != ServerKindUi.SFTP || form.usePassword) {
        SecretTextField(form.input.password.orEmpty(), { form.input = form.input.copy(password = it); form.relogin(); error = null }, stringResource(Res.string.server_password))
    }
    if (form.kind == ServerKindUi.SFTP && form.usePassword) {
        SwitchRow(stringResource(Res.string.server_install_key), form.installKey) { form.installKey = it }
        Hint(stringResource(if (form.installKey) Res.string.server_install_key_hint else Res.string.server_password_stored_hint))
    } else if (form.kind == ServerKindUi.SFTP) {
        publicKey?.let { PublicKeyBox(state, it) }
    }

    // Step 1: test the login. Only a working login gets to choose a folder.
    fun connect() {
        state.run(onError = { error = it; form.connectFailed = true }, block = { net.serverBrowseInput(form.login(), form.confirmed, "") }) { r ->
            when (r) {
                is ServerFoldersUi.Confirm -> {
                    fingerprint = r.fingerprint
                    afterConfirm = { connect() }
                }
                is ServerFoldersUi.Folders -> {
                    error = null
                    form.connectFailed = false
                    form.connected = true
                    // Straight on to step 2.
                    picking = true
                }
            }
        }
    }
    if (!form.connected) {
        Button(onClick = { if (form.loginReady) connect() else error = loginFirst }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(if (state.busy) Res.string.srvf_connecting else Res.string.srvf_test))
        }
    } else {
        Banner(Icons.Filled.CheckCircle, stringResource(if (form.folder.isBlank()) Res.string.srvf_connect_ok else Res.string.srvf_connect_folder))
        // Step 2: folder from the server's own list.
        ListCard {
            ListRow(
                if (form.folder.isBlank()) stringResource(Res.string.srvf_folder_choose) else form.folder.trimEnd('/') + "/",
                stringResource(if (form.folder.isBlank()) Res.string.srvf_folder_choose_sub else Res.string.srvf_folder_change),
                leading = { IconTile(Icons.Filled.Folder, paletteColor(9)) },
                onClick = { picking = true },
            )
        }
    }

    if (form.connected && form.folder.isNotBlank()) {
        Button(onClick = { test() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(if (state.busy) Res.string.srvf_saving else Res.string.srvf_save))
        }
    }
    ErrorText(error)
    ServerChecklist(form)

    if (picking) {
        FolderPicker(state, form, onFingerprint = { fp, retry -> fingerprint = fp; afterConfirm = retry }, onDismiss = { picking = false }) {
            form.folder = it
            picking = false
        }
    }
    fingerprint?.let { fp ->
        FingerprintDialog(
            stringResource(Res.string.server_fp_title),
            stringResource(if (form.kind == ServerKindUi.SFTP) Res.string.server_fp_text else Res.string.server_cert_text), fp,
            onTrust = {
                fingerprint = null
                form.confirmed = fp
                afterConfirm?.invoke()
                afterConfirm = null
            },
            onDismiss = { fingerprint = null; afterConfirm = null },
        )
    }
}

/** Checklist under a server form: login, connection, folder, save. */
@Composable
fun ServerChecklist(form: ServerForm) {
    val steps = buildList {
        add(Res.string.srvf_step_login to form.loginReady)
        add(Res.string.srvf_step_connect to form.connected)
        add(Res.string.srvf_step_folder to form.folder.isNotBlank())
        add(Res.string.srvf_step_test to (form.savedId != null))
    }
    // One step per line: side by side, long names do not fit on a phone.
    Column(Modifier.fillMaxWidth().padding(start = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        steps.forEachIndexed { i, (label, done) ->
            val failed = !done && label == Res.string.srvf_step_connect && form.connectFailed
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(
                    when { done -> Icons.Filled.CheckCircle; failed -> Icons.Filled.Error; else -> Icons.Filled.Schedule }, null, Modifier.size(16.dp),
                    tint = when { done -> okColor(); failed -> g.danger; else -> g.faint },
                )
                Text("${i + 1}. " + stringResource(label), fontSize = 13.sp, color = when { done -> g.fg; failed -> g.danger; else -> g.muted })
            }
        }
    }
}

/**
 * Whether [path] (relative to the login folder) lies in a website: a usual web root folder or a
 * folder named like a domain (many hostings deliver `example.org/` as that domain's site).
 */
internal fun inWebsite(path: String): Boolean = path.split('/').any { p ->
    p.lowercase() in WEB_ROOTS || Regex("""^[a-z0-9-]+(\.[a-z0-9-]+)*\.[a-z]{2,}$""").matches(p.lowercase())
}

/** Sheet listing the server's folders: open a folder, go up, create one, choose. */
@Composable
private fun FolderPicker(
    state: AppState,
    form: ServerForm,
    onFingerprint: (String, () -> Unit) -> Unit,
    onDismiss: () -> Unit,
    onPick: (String) -> Unit,
) {
    val net = state.backend.net
    var path by remember { mutableStateOf("") }
    var folders by remember { mutableStateOf<List<String>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var newName by remember { mutableStateOf<String?>(null) }
    var reload by remember { mutableStateOf(0) }

    // Read directly, not through state.run: that runs one action at a time and would drop this
    // listing while another one is still busy (the list would then stay at "Reading folders").
    // Tapping on to the next folder cancels the listing of the last one.
    LaunchedEffect(path, reload, form.confirmed) {
        folders = null
        error = null
        catching { net.serverBrowseInput(form.login(), form.confirmed, path) }
            .onSuccess { r ->
                when (r) {
                    is ServerFoldersUi.Confirm -> onFingerprint(r.fingerprint) { reload++ }
                    is ServerFoldersUi.Folders -> folders = r.folders
                }
            }
            .onFailure { error = describe(it) }
    }
    val join = { name: String -> if (path.isEmpty()) name else "$path/$name" }

    GhulSheet(onDismiss, stringResource(Res.string.srvf_folders_title)) {
        HintText(stringResource(Res.string.srvf_folders_hint))
        if (inWebsite(path)) {
            Text(stringResource(Res.string.srvf_folder_in_website), color = g.danger, fontSize = 13.sp)
        }
        SheetList {
            SheetRow(if (path.isEmpty()) "/" else "/$path/")
            if (path.isNotEmpty()) {
                SheetRow(stringResource(Res.string.srvf_folder_up), leading = { IconTile(Icons.AutoMirrored.Filled.ArrowBack, paletteColor(5)) }, divider = true) {
                    path = path.substringBeforeLast('/', "")
                }
            }
            val list = folders.orEmpty()
            val suggest = path.isEmpty() && SUGGESTED_FOLDER !in list
            if (suggest) {
                SheetRow(SUGGESTED_FOLDER, stringResource(Res.string.srvf_folder_recommended), leading = { IconTile(Icons.Filled.Add, paletteColor(1)) }, divider = true) {
                    onPick(SUGGESTED_FOLDER)
                }
            }
            for (f in list) {
                val hint = if (f in WEB_ROOTS) stringResource(Res.string.srvf_folder_public) else null
                SheetRow(f, hint, leading = { IconTile(Icons.Filled.Folder, paletteColor(1)) }, divider = true) { path = join(f) }
            }
            if (folders != null && list.isEmpty()) SheetRow(stringResource(Res.string.srvf_folder_empty), divider = true)
            SheetRow(stringResource(Res.string.srvf_folder_new), leading = { IconTile(Icons.Filled.Add, paletteColor(5)) }, divider = true) { newName = "" }
        }
        if (folders == null && error == null) HintText(stringResource(Res.string.srvf_folder_loading))
        ErrorText(error)
        newName?.let { name ->
            OutlinedTextField(name, { newName = it.replace("/", "").trim() }, Modifier.fillMaxWidth(),
                label = { Text(stringResource(Res.string.srvf_folder_new_name)) }, singleLine = true)
            Button(onClick = { onPick(join(name)) }, enabled = name.isNotBlank(), modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.srvf_folder_use_new, join(name)))
            }
        }
        if (path.isNotEmpty() && newName == null) {
            Button(onClick = { onPick(path) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.srvf_folder_use, "/$path/")) }
        }
    }
}
