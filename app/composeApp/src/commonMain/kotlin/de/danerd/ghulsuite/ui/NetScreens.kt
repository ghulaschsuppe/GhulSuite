// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.PhpModuleUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerSavedUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.SyncStatusUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource

/** Input of an SFTP server; shared by "add server" and "fetch from my server". */
@Composable
fun ServerFields(
    input: ServerInputUi,
    usePassword: Boolean,
    onInput: (ServerInputUi) -> Unit,
    onUsePassword: (Boolean) -> Unit,
    showName: Boolean = true,
) {
    if (showName) {
        OutlinedTextField(input.name, { onInput(input.copy(name = it)) }, Modifier.fillMaxWidth(),
            label = { Text(stringResource(Res.string.server_name)) }, singleLine = true)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(input.host, { onInput(input.copy(host = it.trim())) }, Modifier.weight(3f),
            label = { Text(stringResource(Res.string.server_host)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
        OutlinedTextField(if (input.port == 0) "" else input.port.toString(),
            { v -> onInput(input.copy(port = v.filter(Char::isDigit).take(5).toIntOrNull() ?: 0)) }, Modifier.weight(1f),
            label = { Text(stringResource(Res.string.server_port)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    }
    OutlinedTextField(input.user, { onInput(input.copy(user = it.trim())) }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.server_user)) }, singleLine = true)
    OutlinedTextField(input.base, { onInput(input.copy(base = it.trim())) }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.server_base)) }, singleLine = true)
    Hint(stringResource(Res.string.server_base_hint))
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(!usePassword, { onUsePassword(false) }, label = { Text(stringResource(Res.string.server_auth_key)) })
        FilterChip(usePassword, { onUsePassword(true) }, label = { Text(stringResource(Res.string.server_auth_password)) })
    }
    if (usePassword) {
        SecretTextField(input.password.orEmpty(), { onInput(input.copy(password = it)) }, stringResource(Res.string.server_password))
    }
}

/** Shows an SSH public key to copy into `authorized_keys`. */
@Composable
fun PublicKeyBox(state: AppState, key: String) {
    Hint(stringResource(Res.string.server_key_hint))
    SelectionContainer {
        Text(key, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
    }
    val label = stringResource(Res.string.server_public_key)
    OutlinedButton(onClick = { state.copy(label, key) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_copy)) }
}

/** Asks the user to compare the server's key fingerprint before it is trusted. */
@Composable
fun FingerprintDialog(title: String, text: String, fingerprint: String, onTrust: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(text)
                SelectionContainer { Text(fingerprint, fontFamily = FontFamily.Monospace) }
            }
        },
        confirmButton = { TextButton(onClick = onTrust) { Text(stringResource(Res.string.a_trust)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

private fun defaultServer() = ServerInputUi(name = "", host = "", port = 22, user = "", password = null, base = "ghulsuite")

/** Options → add a server: the shared server form (#24); the module file by hand as fallback. */
@Composable
fun AddServerScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    val form = rememberServerForm()
    var manual by remember { mutableStateOf(false) }
    if (manual) {
        ManualServerScreen(state, padding)
        return
    }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.server_add_title)) {
                Hint(stringResource(Res.string.set_servers_hint))
                ServerSetup(state, form, { input, fp -> net.saveServer(input, fp) }) {
                    state.changed()
                    state.pop()
                }
                if (form.kind == ServerKindUi.PHP) {
                    TextButton(onClick = { manual = true }) { Text(stringResource(Res.string.php_manual)) }
                }
            }
        }
    }
}

/** Server input of earlier versions: also creates a PHP module file to upload by hand. */
@Composable
private fun ManualServerScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    var kind by remember { mutableStateOf(ServerKindUi.SFTP) }
    var input by remember { mutableStateOf(defaultServer()) }
    var usePassword by remember { mutableStateOf(false) }
    var publicKey by remember { mutableStateOf<String?>(null) }
    var fingerprint by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var module by remember { mutableStateOf<PhpModuleUi?>(null) }
    // With a password: set up the SSH key automatically and do not store the password (default).
    var installKey by remember { mutableStateOf(true) }
    LaunchedEffect(Unit) { publicKey = runCatching { net.sshPublicKey() }.getOrNull() }
    val effective = when (kind) {
        ServerKindUi.SFTP -> if (usePassword && !installKey) input else input.copy(password = null)
        else -> input
    }.copy(kind = kind)
    val setupKey = kind == ServerKindUi.SFTP && usePassword && installKey

    /** Install the key if wanted, then check and store the server (all in one busy step). */
    fun connect(confirmed: String?) {
        state.run(onError = { error = it }, block = {
            if (setupKey) {
                val fp = net.installSshKey(input, confirmed)
                if (fp.isNotEmpty()) return@run ServerSavedUi.Confirm(fp)
            }
            net.saveServer(effective, confirmed)
        }) { result ->
            when (result) {
                is ServerSavedUi.Confirm -> fingerprint = result.fingerprint
                is ServerSavedUi.Saved -> {
                    state.changed()
                    state.pop()
                }
            }
        }
    }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.server_kind)) {
                ServerKindRow(ServerKindUi.SFTP, kind, Res.string.server_kind_sftp, Res.string.server_kind_sftp_sub) {
                    kind = it; input = input.copy(port = 22, password = null); error = null
                }
                ServerKindRow(ServerKindUi.FTPS, kind, Res.string.server_kind_ftps, Res.string.server_kind_ftps_sub) {
                    kind = it; input = input.copy(port = 21, password = ""); error = null
                }
                ServerKindRow(ServerKindUi.PHP, kind, Res.string.server_kind_php, Res.string.server_kind_php_sub) {
                    kind = it; input = input.copy(password = module?.key ?: ""); error = null
                }
            }
            SectionCard(stringResource(Res.string.server_add_title)) {
                when (kind) {
                    ServerKindUi.SFTP -> {
                        Hint(stringResource(Res.string.server_add_text))
                        ServerFields(input, usePassword, { input = it; error = null }, { usePassword = it })
                        if (usePassword) {
                            SwitchRow(stringResource(Res.string.server_install_key), installKey) { installKey = it }
                            Hint(stringResource(if (installKey) Res.string.server_install_key_hint else Res.string.server_password_stored_hint))
                        } else {
                            publicKey?.let { PublicKeyBox(state, it) }
                        }
                    }
                    ServerKindUi.FTPS -> {
                        Hint(stringResource(Res.string.server_ftps_text))
                        OutlinedTextField(input.name, { input = input.copy(name = it) }, Modifier.fillMaxWidth(),
                            label = { Text(stringResource(Res.string.server_name)) }, singleLine = true)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedTextField(input.host, { input = input.copy(host = it.trim()) }, Modifier.weight(3f),
                                label = { Text(stringResource(Res.string.server_host)) }, singleLine = true,
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                            OutlinedTextField(if (input.port == 0) "" else input.port.toString(),
                                { v -> input = input.copy(port = v.filter(Char::isDigit).take(5).toIntOrNull() ?: 0) }, Modifier.weight(1f),
                                label = { Text(stringResource(Res.string.server_port)) }, singleLine = true,
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
                        }
                        OutlinedTextField(input.user, { input = input.copy(user = it.trim()) }, Modifier.fillMaxWidth(),
                            label = { Text(stringResource(Res.string.server_user)) }, singleLine = true)
                        SecretTextField(input.password.orEmpty(), { input = input.copy(password = it) }, stringResource(Res.string.server_password))
                        OutlinedTextField(input.base, { input = input.copy(base = it.trim()) }, Modifier.fillMaxWidth(),
                            label = { Text(stringResource(Res.string.server_base)) }, singleLine = true)
                        Hint(stringResource(Res.string.server_ftps_hint))
                    }
                    ServerKindUi.PHP -> PhpModuleFields(state, input, module, { input = it; error = null }) { m ->
                        module = m
                        input = input.copy(password = m.key)
                    }
                }
                ErrorText(error)
                val ready = when (kind) {
                    ServerKindUi.SFTP -> input.host.isNotBlank() && input.user.isNotBlank() && (!usePassword || !input.password.isNullOrEmpty())
                    ServerKindUi.FTPS -> input.host.isNotBlank() && input.user.isNotBlank() && !input.password.isNullOrEmpty()
                    ServerKindUi.PHP -> input.url.startsWith("https://") && !input.password.isNullOrEmpty()
                }
                Button(onClick = { connect(null) }, enabled = !state.busy && ready, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(Res.string.a_connect))
                }
            }
        }
    }
    fingerprint?.let { fp ->
        FingerprintDialog(
            stringResource(Res.string.server_fp_title),
            stringResource(if (kind == ServerKindUi.SFTP) Res.string.server_fp_text else Res.string.server_cert_text), fp,
            onTrust = {
                fingerprint = null
                connect(fp)
            },
            onDismiss = { fingerprint = null },
        )
    }
}

@Composable
private fun ServerKindRow(kind: ServerKindUi, current: ServerKindUi, title: StringResource, sub: StringResource, onPick: (ServerKindUi) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { onPick(kind) }.padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        RadioButton(kind == current, { onPick(kind) })
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), style = MaterialTheme.typography.titleSmall)
            Hint(stringResource(sub))
        }
    }
}

/**
 * PHP server module: create a module with its own key, save it or upload it through an already
 * stored server, then connect with its address.
 */
@Composable
private fun PhpModuleFields(
    state: AppState,
    input: ServerInputUi,
    module: PhpModuleUi?,
    onInput: (ServerInputUi) -> Unit,
    onModule: (PhpModuleUi) -> Unit,
) {
    val net = state.backend.net
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var uploadVia by remember { mutableStateOf<ServerUi?>(null) }
    var uploadDir by remember { mutableStateOf("") }
    var uploaded by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { servers = runCatching { net.servers() }.getOrDefault(emptyList()).filter { it.kind != ServerKindUi.PHP } }
    val savedMsg = stringResource(Res.string.php_saved)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }

    Hint(stringResource(Res.string.php_text))
    Text(stringResource(Res.string.php_step1), style = MaterialTheme.typography.titleSmall)
    if (module == null) {
        OutlinedButton(onClick = { onModule(net.phpModuleCreate()) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.php_create))
        }
        Hint(stringResource(Res.string.php_have_key))
    } else {
        Hint(stringResource(Res.string.php_created, module.fileName))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { save(module.fileName, module.content.encodeToByteArray()) }, enabled = filePickersAvailable) {
                Text(stringResource(Res.string.php_save))
            }
            if (servers.isNotEmpty()) {
                OutlinedButton(onClick = { uploadVia = servers.first() }) { Text(stringResource(Res.string.php_upload)) }
            }
        }
        if (uploaded) Hint(stringResource(Res.string.php_uploaded))
    }
    Text(stringResource(Res.string.php_step2), style = MaterialTheme.typography.titleSmall)
    OutlinedTextField(input.name, { onInput(input.copy(name = it)) }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.server_name)) }, singleLine = true)
    // Placeholder instead of a pre-filled "https://": emptying the field must not put it back (ST10).
    OutlinedTextField(input.url, { onInput(input.copy(url = it.trim())) }, Modifier.fillMaxWidth(),
        label = { Text(stringResource(Res.string.php_url)) }, singleLine = true,
        placeholder = { Text("https://example.org/ghulsuite.php") },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
    Hint(stringResource(Res.string.php_url_hint, module?.fileName ?: "ghulsuite.php"))
    if (module == null) {
        SecretTextField(input.password.orEmpty(), { onInput(input.copy(password = it.trim())) }, stringResource(Res.string.php_key))
    }
    ErrorText(error)

    uploadVia?.let { via ->
        AlertDialog(
            onDismissRequest = { uploadVia = null },
            title = { Text(stringResource(Res.string.php_upload)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Hint(stringResource(Res.string.php_upload_hint))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        for (s in servers) FilterChip(s.id == via.id, { uploadVia = s }, label = { Text(s.name) })
                    }
                    OutlinedTextField(uploadDir, { uploadDir = it.trim() }, Modifier.fillMaxWidth(), singleLine = true,
                        label = { Text(stringResource(Res.string.php_upload_dir)) })
                    Hint(stringResource(Res.string.php_upload_dir_hint))
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    val m = module ?: return@TextButton
                    uploadVia = null
                    state.run(onError = { error = it }, block = { net.phpModuleUpload(via.id, uploadDir, m) }) { uploaded = true }
                }, enabled = uploadDir.isNotBlank()) { Text(stringResource(Res.string.php_upload)) }
            },
            dismissButton = { TextButton(onClick = { uploadVia = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** Settings: own servers and synchronisation. */
@Composable
fun ServersSection(state: AppState) {
    val net = state.backend.net
    val settings = state.backend.settings
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var sync by remember { mutableStateOf(SyncStatusUi(null, 0)) }
    var removing by remember { mutableStateOf<ServerUi?>(null) }
    /** Removing or switching the sync server while GhulDrive uses it: ask first (#24). */
    var driveAsk by remember { mutableStateOf<(() -> Unit)?>(null) }
    fun guarded(action: () -> Unit) {
        state.run(block = { net.driveStorage() }) { d ->
            if (d.mode == de.danerd.ghulsuite.model.DriveModeUi.SYNC && d.server != null) driveAsk = action else action()
        }
    }
    LaunchedEffect(state.liveRevision(), state.syncing) {
        servers = runCatching { net.servers() }.getOrDefault(emptyList())
        sync = runCatching { net.syncStatus() }.getOrDefault(SyncStatusUi(null, 0))
    }

    SectionCard(stringResource(Res.string.set_servers)) {
        Hint(stringResource(Res.string.set_servers_hint))
        for (s in servers) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(s.name + " · " + kindLabel(s.kind))
                    if (s.kind == ServerKindUi.PHP) {
                        // One address for storage and links; the module also counts downloads.
                        Hint(stringResource(Res.string.server_address, s.location.ifBlank { s.shareUrl }))
                        Hint(stringResource(Res.string.server_share_php))
                    } else {
                        Hint("${s.user}@${s.host}:${s.port}")
                        Hint(stringResource(Res.string.server_location, s.location.ifBlank { "~/" + s.base }))
                        Hint(
                            if (s.canShare) stringResource(Res.string.server_share_on, s.shareUrl)
                            else stringResource(Res.string.server_share_off),
                        )
                    }
                }
                TextButton(onClick = { removing = s }) { Text(stringResource(Res.string.a_remove)) }
            }
            if (s.kind != ServerKindUi.PHP) {
                TextButton(onClick = { state.push(Screen.ShareSetup(s.id)) }) {
                    Text(stringResource(if (s.canShare) Res.string.server_share_change else Res.string.share_setup_title))
                }
            }
        }
        OutlinedButton(onClick = { state.push(Screen.AddServer) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.server_add_title))
        }
    }

    SectionCard(stringResource(Res.string.set_sync)) {
        Hint(stringResource(Res.string.set_sync_hint))
        val noSync = stringResource(Res.string.sync_off)
        var open by remember { mutableStateOf(false) }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(Res.string.sync_server), Modifier.weight(1f))
            androidx.compose.foundation.layout.Box {
                TextButton(onClick = { open = true }, enabled = servers.isNotEmpty()) {
                    Text(servers.firstOrNull { it.id == sync.serverId }?.name ?: noSync)
                }
                androidx.compose.material3.DropdownMenu(open, { open = false }) {
                    androidx.compose.material3.DropdownMenuItem(text = { Text(noSync) }, onClick = {
                        open = false
                        guarded { state.run(block = { net.setSyncServer(null) }) { state.changed() } }
                    })
                    for (s in servers) {
                        androidx.compose.material3.DropdownMenuItem(text = { Text(s.name) }, onClick = {
                            open = false
                            if (s.id != sync.serverId) guarded {
                                state.run(block = { net.setSyncServer(s.id) }) {
                                    state.changed()
                                    state.sync()
                                }
                            }
                        })
                    }
                }
            }
        }
        if (sync.serverId != null) {
            SwitchRow(stringResource(Res.string.sync_auto), settings.autoSync) { settings.autoSync = it }
            Hint(
                if (sync.lastSync > 0) {
                    stringResource(Res.string.sync_last, formatDateTime(sync.lastSync))
                } else {
                    stringResource(Res.string.sync_never)
                },
            )
            val doneText = stringResource(Res.string.sync_done)
            Button(onClick = {
                state.sync { r ->
                    state.message(doneText.replace("%1\$s", r.uploaded.toString()).replace("%2\$s", (r.downloaded + r.merged).toString()))
                }
            }, enabled = !state.syncing, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(if (state.syncing) Res.string.sync_running else Res.string.a_sync_now))
            }
        }
    }

    driveAsk?.let { action -> DriveFollowsSyncDialog(state, onProceed = action, onDismiss = { driveAsk = null }) }
    removing?.let { s ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text(stringResource(Res.string.server_remove_title, s.name)) },
            text = { Text(stringResource(Res.string.server_remove_text)) },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    val remove = { state.run(block = { net.removeServer(s.id) }) { state.changed() } }
                    if (s.id == sync.serverId) guarded(remove) else remove()
                }) { Text(stringResource(Res.string.a_remove)) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** The master password was changed on another device: take over the new one. */
@Composable
fun PasswordChangedDialog(state: AppState) {
    var password by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = {},
        title = { Text(stringResource(Res.string.sync_pw_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.sync_pw_text))
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_new_password))
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = { state.backend.net.adoptRemotePassword(password) }) {
                    state.passwordChangedElsewhere = false
                    state.changed()
                }
            }, enabled = password.isNotEmpty() && !state.busy) { Text(stringResource(Res.string.a_take_over)) }
        },
        dismissButton = {
            TextButton(onClick = { state.passwordChangedElsewhere = false }) { Text(stringResource(Res.string.a_later)) }
        },
    )
}

/** Start option on a new device: download the suite from an own server, then unlock as usual. */
@Composable
fun FetchFromServer(state: AppState, onCancel: () -> Unit) {
    val backend = state.backend
    var input by remember { mutableStateOf(defaultServer()) }
    var usePassword by remember { mutableStateOf(false) }
    var fingerprint by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val deviceKey = remember { runCatching { backend.deviceSshKey() }.getOrNull() }
    val effective = if (usePassword) input else input.copy(password = null)
    val notFound = stringResource(Res.string.fetch_nothing)
    val doneMsg = stringResource(Res.string.fetch_done)

    fun fetch(hostKey: String?) {
        state.run(onError = { error = it }, block = {
            try {
                backend.fetchFromServer(effective, hostKey)
            } catch (e: VaultException) {
                if (e.kind == VaultErrorKind.NOT_FOUND) throw VaultException(VaultErrorKind.OTHER, notFound)
                throw e
            }
        }) { fp ->
            if (fp.isNotEmpty()) {
                fingerprint = fp
            } else {
                state.showStart = false
                state.vaultExists = true
                state.message(doneMsg)
            }
        }
    }

    SectionCard(stringResource(Res.string.start_fetch)) {
        Hint(stringResource(Res.string.fetch_text))
        StorageChooser(state)
        ServerFields(input, usePassword, { input = it; error = null }, { usePassword = it }, showName = false)
        if (!usePassword && deviceKey != null) {
            Hint(stringResource(Res.string.fetch_device_key))
            PublicKeyBox(state, deviceKey)
        }
        ErrorText(error)
        Button(
            onClick = { fetch(null) },
            enabled = !state.busy && input.host.isNotBlank() && input.user.isNotBlank() &&
                (!usePassword || !input.password.isNullOrEmpty()),
            modifier = Modifier.fillMaxWidth(),
        ) { Text(stringResource(Res.string.a_fetch)) }
        TextButton(onClick = onCancel) { Text(stringResource(Res.string.a_back)) }
    }
    fingerprint?.let { fp ->
        FingerprintDialog(
            stringResource(Res.string.server_fp_title), stringResource(Res.string.server_fp_text), fp,
            onTrust = {
                fingerprint = null
                fetch(fp)
            },
            onDismiss = { fingerprint = null },
        )
    }
}

@Composable
fun kindLabel(kind: ServerKindUi): String = when (kind) {
    ServerKindUi.SFTP -> "SFTP"
    ServerKindUi.FTPS -> "FTPS"
    ServerKindUi.PHP -> stringResource(Res.string.server_kind_php_short)
}

/** Unix seconds or milliseconds → local date and time. */
fun formatDateTime(time: Long): String {
    val millis = if (time < 100_000_000_000L) time * 1000 else time
    return formatDate(millis) + " " + formatTime(millis)
}

/** Human-readable size: 1.2 MB. */
fun formatSize(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> "${(bytes * 10 / 1024) / 10.0} KB"
    bytes < 1024L * 1024 * 1024 -> "${(bytes * 10 / (1024 * 1024)) / 10.0} MB"
    else -> "${(bytes * 10 / (1024L * 1024 * 1024)) / 10.0} GB"
}
