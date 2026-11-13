// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CheckCircle
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
import de.danerd.ghulsuite.model.PhpInstalledUi
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
    /** Chosen folder, relative to the login folder ("" = none chosen yet). */
    var folder by mutableStateOf("")
    var installed by mutableStateOf<PhpInstalledUi?>(null)
    var address by mutableStateOf("")
    var keepFtps by mutableStateOf(true)
    /** Set once the server is checked and stored. */
    var savedId by mutableStateOf<String?>(null)
    var savedWhere by mutableStateOf("")
    var limit by mutableStateOf(0L)

    val loginReady: Boolean
        get() = input.host.isNotBlank() && input.user.isNotBlank() &&
            (kind == ServerKindUi.SFTP && !usePassword || !input.password.isNullOrEmpty())

    /** The step still missing, or null when the server is stored. */
    fun missing(): StringResource? = when {
        savedId != null -> null
        !loginReady -> Res.string.srvf_step_login
        folder.isBlank() -> Res.string.srvf_step_folder
        kind == ServerKindUi.PHP && installed?.key.isNullOrEmpty() -> Res.string.srvf_step_upload
        kind == ServerKindUi.PHP -> Res.string.srvf_step_check
        else -> Res.string.srvf_step_test
    }

    fun switchKind(k: ServerKindUi) {
        kind = k
        input = input.copy(port = if (k == ServerKindUi.SFTP) 22 else 21, password = if (k == ServerKindUi.SFTP) null else "")
        usePassword = k != ServerKindUi.SFTP
        confirmed = null
        folder = ""
        installed = null
        address = ""
        savedId = null
    }

    /** The login as typed: SFTP/FTPS directly, for the PHP module its FTPS upload login. */
    fun login(): ServerInputUi = when (kind) {
        ServerKindUi.SFTP -> input.copy(kind = ServerKindUi.SFTP, password = if (usePassword) input.password else null)
        else -> input.copy(kind = ServerKindUi.FTPS)
    }
}

@Composable
fun rememberServerForm(): ServerForm = remember { ServerForm() }

/**
 * One server form for the setup wizard, GhulDrive and the options: SFTP and FTPS log in and pick
 * a folder from the server's own list, then test the connection (which stores the server). The
 * PHP module logs in to the web hosting once with FTPS, picks a folder, uploads the module and
 * checks the address the app suggests. A checklist below names what is still missing.
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
    LaunchedEffect(Unit) { publicKey = runCatching { net.sshPublicKey() }.getOrNull() }

    SingleChoiceSegmentedButtonRow {
        for ((k, label) in listOf(ServerKindUi.SFTP to "SFTP", ServerKindUi.FTPS to "FTPS", ServerKindUi.PHP to stringResource(Res.string.srvf_kind_php))) {
            SegmentedButton(form.kind == k, { if (form.kind != k) { form.switchKind(k); error = null } }) { Text(label) }
        }
    }

    val saved = form.savedId
    if (saved != null) {
        Banner(Icons.Filled.CheckCircle, stringResource(if (form.kind == ServerKindUi.PHP) Res.string.srvf_module_runs else Res.string.srvf_connected, form.savedWhere))
        if (form.kind == ServerKindUi.PHP && form.limit in 1 until 4L * 1024 * 1024) {
            HintText(stringResource(Res.string.srvf_limit_small, formatSize(form.limit)))
        }
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
                    form.savedWhere = if (form.kind == ServerKindUi.PHP) input.url else "${input.host}:/${form.folder}"
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
            else -> login.copy(base = form.folder)
        }
        store(input.copy(name = input.name.ifBlank { input.host }), form.confirmed)
    }

    val loginFirst = stringResource(Res.string.srvf_login_first)
    if (form.kind == ServerKindUi.PHP) HintText(stringResource(Res.string.srvf_php_intro))
    // Login (for the PHP module: the FTPS login of the web hosting).
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(form.input.host, { form.input = form.input.copy(host = it.trim()); form.folder = ""; error = null }, Modifier.weight(3f),
            label = { Text(stringResource(if (form.kind == ServerKindUi.PHP) Res.string.srvf_host_php else Res.string.server_host)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
        OutlinedTextField(if (form.input.port == 0) "" else form.input.port.toString(),
            { v -> form.input = form.input.copy(port = v.filter(Char::isDigit).take(5).toIntOrNull() ?: 0) }, Modifier.weight(1f),
            label = { Text(stringResource(Res.string.server_port)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    }
    OutlinedTextField(form.input.user, { form.input = form.input.copy(user = it.trim()); error = null }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.server_user)) }, singleLine = true)
    if (form.kind == ServerKindUi.SFTP) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(!form.usePassword, { form.usePassword = false }, label = { Text(stringResource(Res.string.server_auth_key)) })
            FilterChip(form.usePassword, { form.usePassword = true; if (form.input.password == null) form.input = form.input.copy(password = "") },
                label = { Text(stringResource(Res.string.server_auth_password)) })
        }
    }
    if (form.kind != ServerKindUi.SFTP || form.usePassword) {
        SecretTextField(form.input.password.orEmpty(), { form.input = form.input.copy(password = it); error = null }, stringResource(Res.string.server_password))
    }
    if (form.kind == ServerKindUi.SFTP && form.usePassword) {
        SwitchRow(stringResource(Res.string.server_install_key), form.installKey) { form.installKey = it }
        Hint(stringResource(if (form.installKey) Res.string.server_install_key_hint else Res.string.server_password_stored_hint))
    } else if (form.kind == ServerKindUi.SFTP) {
        publicKey?.let { PublicKeyBox(state, it) }
    }

    // Folder from the server's own list.
    ListCard {
        ListRow(
            if (form.folder.isBlank()) stringResource(Res.string.srvf_folder_choose) else form.folder.trimEnd('/') + "/",
            stringResource(if (form.folder.isBlank()) Res.string.srvf_folder_choose_sub else Res.string.srvf_folder_change),
            leading = { IconTile(Icons.Filled.Folder, paletteColor(9)) },
            onClick = { if (form.loginReady) picking = true else error = loginFirst },
        )
    }

    val installed = form.installed
    if (form.kind == ServerKindUi.PHP && installed != null && installed.key.isNotEmpty()) {
        Banner(Icons.Filled.CheckCircle, stringResource(Res.string.srvf_uploaded, form.folder.trimEnd('/')))
        OutlinedTextField(form.address, { form.address = it.trim(); error = null }, Modifier.fillMaxWidth(),
            label = { Text(stringResource(Res.string.srvf_address_label)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
        Hint(stringResource(if (installed.url == null) Res.string.srvf_address_not_found else Res.string.srvf_address_hint))
        SwitchRow(stringResource(Res.string.srvf_keep_ftps), form.keepFtps) { form.keepFtps = it }
        Hint(stringResource(Res.string.srvf_keep_ftps_sub))
        FilledTonalButton(onClick = {
            state.run(onError = { error = it }, block = { net.phpModuleProbe(form.address, installed.key) }) { info ->
                form.limit = info.limit
                store(
                    ServerInputUi(
                        kind = ServerKindUi.PHP, name = "", host = "", port = 0, user = "", password = installed.key, base = "",
                        url = form.address, upload = if (form.keepFtps) installed.upload else null,
                    ),
                    null,
                )
            }
        }, enabled = !state.busy && form.address.startsWith("https://"), modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.srvf_check_address))
        }
    } else if (form.folder.isNotBlank()) {
        if (form.kind == ServerKindUi.PHP) {
            FilledTonalButton(onClick = {
                fun install(confirmed: String?) {
                    state.run(onError = { error = it }, block = { net.phpModuleInstall(form.login(), confirmed, form.folder, "") }) { r ->
                        if (r.confirm != null) {
                            fingerprint = r.confirm
                            afterConfirm = { form.confirmed = r.confirm; install(r.confirm) }
                        } else {
                            form.installed = r
                            form.limit = r.limit
                            form.address = r.url ?: r.tried.firstOrNull().orEmpty()
                        }
                    }
                }
                install(form.confirmed)
            }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.srvf_upload)) }
        } else {
            FilledTonalButton(onClick = { test() }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.srvf_test))
            }
        }
    }
    ErrorText(error)
    ServerChecklist(form)

    if (picking) {
        FolderPicker(state, form, onFingerprint = { fp, retry -> fingerprint = fp; afterConfirm = retry }, onDismiss = { picking = false }) {
            form.folder = it
            form.installed = null
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

/** Checklist under a server form: login, folder, (upload), test. */
@Composable
fun ServerChecklist(form: ServerForm) {
    val steps = buildList {
        add(Res.string.srvf_step_login to form.loginReady)
        add(Res.string.srvf_step_folder to form.folder.isNotBlank())
        if (form.kind == ServerKindUi.PHP) add(Res.string.srvf_step_upload to !form.installed?.key.isNullOrEmpty())
        add((if (form.kind == ServerKindUi.PHP) Res.string.srvf_step_check else Res.string.srvf_step_test) to (form.savedId != null))
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        for ((label, done) in steps) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Icon(if (done) Icons.Filled.CheckCircle else Icons.Filled.Schedule, null, Modifier.size(14.dp), tint = if (done) okColor() else g.faint)
                Text(stringResource(label), fontSize = 12.sp, color = if (done) g.fg else g.muted)
            }
        }
    }
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

    LaunchedEffect(path, reload, form.confirmed) {
        folders = null
        state.run(onError = { error = it }, block = { net.serverBrowseInput(form.login(), form.confirmed, path) }) { r ->
            when (r) {
                is ServerFoldersUi.Confirm -> onFingerprint(r.fingerprint) { reload++ }
                is ServerFoldersUi.Folders -> { folders = r.folders; error = null }
            }
        }
    }
    val join = { name: String -> if (path.isEmpty()) name else "$path/$name" }

    GhulSheet(onDismiss, stringResource(Res.string.srvf_folders_title)) {
        HintText(stringResource(if (form.kind == ServerKindUi.PHP) Res.string.srvf_folders_hint_php else Res.string.srvf_folders_hint))
        SheetList {
            SheetRow(if (path.isEmpty()) "/" else "/$path/")
            if (path.isNotEmpty()) {
                SheetRow(stringResource(Res.string.srvf_folder_up), leading = { IconTile(Icons.AutoMirrored.Filled.ArrowBack, paletteColor(5)) }, divider = true) {
                    path = path.substringBeforeLast('/', "")
                }
            }
            val list = folders.orEmpty()
            val suggest = form.kind != ServerKindUi.PHP && path.isEmpty() && SUGGESTED_FOLDER !in list
            if (suggest) {
                SheetRow(SUGGESTED_FOLDER, stringResource(Res.string.srvf_folder_recommended), leading = { IconTile(Icons.Filled.Add, paletteColor(1)) }, divider = true) {
                    onPick(SUGGESTED_FOLDER)
                }
            }
            for (f in list) {
                val hint = when {
                    form.kind != ServerKindUi.PHP && f in WEB_ROOTS -> stringResource(Res.string.srvf_folder_public)
                    else -> null
                }
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
