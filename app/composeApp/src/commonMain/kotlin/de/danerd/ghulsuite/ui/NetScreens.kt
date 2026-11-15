// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Delete
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.offset
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
import de.danerd.ghulsuite.model.PickedFile
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
    passwordOnly: Boolean = false,
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
    if (!passwordOnly) {
        ChipRow {
            FilterChip(!usePassword, { onUsePassword(false) }, label = { Text(stringResource(Res.string.server_auth_key)) })
            FilterChip(usePassword, { onUsePassword(true) }, label = { Text(stringResource(Res.string.server_auth_password)) })
        }
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

/** Options → add a server: the shared server form (#24). */
@Composable
fun AddServerScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    val form = rememberServerForm()
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            BusyBanner(state)
            SectionCard(stringResource(Res.string.server_add_title)) {
                Hint(stringResource(Res.string.set_servers_hint))
                ServerSetup(state, form, { input, fp -> net.saveServer(input, fp) }) {
                    state.changed()
                    state.pop()
                }
            }
        }
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
    var removingModule by remember { mutableStateOf<ServerUi?>(null) }
    /** GhulDrive's server (one of the list, or its own one kept in the Drive file). */
    var driveServer by remember { mutableStateOf<ServerUi?>(null) }
    /** Removing or switching the sync server while GhulDrive uses it: ask first (#24). */
    var driveAsk by remember { mutableStateOf<(() -> Unit)?>(null) }
    fun guarded(action: () -> Unit) {
        state.run(block = { net.driveStorage() }) { d ->
            if (d.mode == de.danerd.ghulsuite.model.DriveModeUi.SYNC && d.server != null) driveAsk = action else action()
        }
    }
    /** Loaded once (until then: "Loading …", not "no servers" and "Off"), and why it failed. */
    var loaded by remember { mutableStateOf(false) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var reload by remember { mutableStateOf(0) }
    var syncLoaded by remember { mutableStateOf(false) }
    var syncError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision(), state.syncing, reload) {
        // While a synchronisation runs the core answers only after it (it holds the suite);
        // until then the page says it is loading instead of showing an empty list and "Off".
        // The servers come first: without them the page cannot show anything.
        val list = catching { net.servers() }.getOrElse { loadError = describe(it); return@LaunchedEffect }
        driveServer = catching { net.driveStorage() }.getOrNull()?.server
        servers = list
        loaded = true
        loadError = null
        // The sync status on its own: if it cannot be read, its card says so instead of "Off".
        catching { net.syncStatus() }
            .onSuccess { sync = it; syncLoaded = true; syncError = null }
            .onFailure { syncError = describe(it) }
    }
    val retry: () -> Unit = { loadError = null; syncError = null; reload++ }

    if (!loaded) {
        SectionCard(stringResource(Res.string.set_servers)) {
            if (loadError != null) LoadError(loadError, retry) else Hint(stringResource(Res.string.loading))
        }
        return
    }
    SectionCard(stringResource(Res.string.set_servers)) {
        Hint(stringResource(Res.string.set_servers_hint))
        LoadError(loadError, retry)
        // GhulDrive's own server is not in the suite's list: shown as well (its module, its links).
        val drive = driveServer
        val shown = if (drive != null && servers.none { it.id == drive.id }) servers + drive else servers
        val moduleUpdated = stringResource(Res.string.module_updated)
        shown.forEachIndexed { i, s ->
            val ownDrive = s.id == drive?.id && servers.none { it.id == s.id }
            // Like the list of mail accounts: a row with a tile, and removing as a red icon.
            ListRow(
                s.name, kindLabel(s.kind), leading = { IconTile(Icons.Filled.Dns, paletteColor(1)) },
                trailing = if (ownDrive) null else ({ BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_remove), tint = g.danger) { removing = s } }),
                divider = i > 0, flush = true,
            )
            // Below the name, in line with it: where, what for, and the server module.
            Column(Modifier.fillMaxWidth().padding(start = 56.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Hint("${s.user}@${s.host}:${s.port}")
                Hint(stringResource(Res.string.server_location, s.location.ifBlank { "~/" + s.base }))
                val uses = listOfNotNull(
                    stringResource(Res.string.set_sync).takeIf { s.id == sync.serverId },
                    stringResource(Res.string.tab_drive).takeIf { s.id == drive?.id },
                )
                Hint(if (uses.isEmpty()) stringResource(Res.string.server_used_none) else stringResource(Res.string.server_used_for, uses.joinToString(", ")))
                if (ownDrive) Hint(stringResource(Res.string.server_drive_own))
                Hint(
                    when {
                        !s.hasModule -> stringResource(Res.string.server_module_none)
                        s.moduleOutdated -> stringResource(Res.string.server_module_old, s.moduleVersion.toString(), s.moduleCurrent.toString())
                        else -> stringResource(Res.string.server_module_on, s.moduleVersion.toString())
                    },
                )
                // The text buttons' own padding: their words line up with the lines above.
                Box(Modifier.offset(x = (-12).dp)) {
                    ChipRow {
                        if (!s.hasModule) {
                            TextButton(onClick = { state.push(Screen.ModuleInstall(s.id)) }) { Text(stringResource(Res.string.module_install)) }
                        } else {
                            if (s.moduleOutdated) {
                                TextButton(onClick = {
                                    state.run(onError = { loadError = it }, block = { net.serverModuleUpdate(s.id) }) { v ->
                                        state.message(moduleUpdated.replace("%1\$s", v.toString()))
                                        state.changed()
                                    }
                                }) { Text(stringResource(Res.string.module_update)) }
                            }
                            TextButton(onClick = { removingModule = s }, colors = androidx.compose.material3.ButtonDefaults.textButtonColors(contentColor = g.danger)) {
                                Text(stringResource(Res.string.module_remove))
                            }
                        }
                    }
                }
            }
        }
        OutlinedButton(onClick = { state.push(Screen.AddServer) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.server_add_title))
        }
    }

    SectionCard(stringResource(Res.string.set_sync)) {
        Hint(stringResource(Res.string.set_sync_hint))
        if (!syncLoaded) {
            if (syncError != null) LoadError(syncError, retry) else Hint(stringResource(Res.string.loading))
            return@SectionCard
        }
        LoadError(syncError, retry)
        val noSync = stringResource(Res.string.sync_off)
        var open by remember { mutableStateOf(false) }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(Res.string.sync_server), Modifier.weight(1f))
            TextButton(onClick = { open = true }, enabled = servers.isNotEmpty()) {
                Text(servers.firstOrNull { it.id == sync.serverId }?.name ?: noSync)
            }
        }
        // The choice opens as a sheet like every other choice in the app.
        if (open) {
            GhulSheet({ open = false }, stringResource(Res.string.sync_server)) {
                SheetList {
                    SheetRow(noSync, end = { if (sync.serverId == null) SheetCheck() }) {
                        open = false
                        guarded { state.run(block = { net.setSyncServer(null) }) { state.changed() } }
                    }
                    servers.forEach { s ->
                        SheetRow(s.name, s.location.ifBlank { s.host }, leading = { IconTile(Icons.Filled.Dns, paletteColor(1)) }, divider = true,
                            end = { if (s.id == sync.serverId) SheetCheck() }) {
                            open = false
                            if (s.id != sync.serverId) guarded {
                                state.run(block = { net.setSyncServer(s.id) }) {
                                    // One run for the choice (with its errors shown), not an
                                    // automatic one and then a second full one.
                                    state.changed(sync = false)
                                    state.sync()
                                }
                            }
                        }
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
    removingModule?.let { s ->
        // Like every deletion: ask first (F103).
        AlertDialog(
            danger = true,
            onDismissRequest = { removingModule = null },
            title = { Text(stringResource(Res.string.module_remove_title, s.name)) },
            text = { Text(stringResource(Res.string.module_remove_text)) },
            confirmButton = {
                TextButton(onClick = {
                    removingModule = null
                    state.run(onError = { loadError = it }, block = { net.serverModuleRemove(s.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_remove)) }
            },
            dismissButton = { TextButton(onClick = { removingModule = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    removing?.let { s ->
        AlertDialog(
            danger = true,
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
                state.run(onError = { error = it }, block = {
                    try {
                        state.backend.net.adoptRemotePassword(password)
                        true
                    } catch (e: VaultException) {
                        // A key was removed there as well: the key dialog takes it over (and asks
                        // for the password again).
                        if (e.kind == VaultErrorKind.PROTECTION_CHANGED_ELSEWHERE) false else throw e
                    }
                }) { done ->
                    state.passwordChangedElsewhere = false
                    if (done) state.changed() else state.protectionChangedElsewhere = true
                }
            }, enabled = password.isNotEmpty() && !state.busy) { Text(stringResource(Res.string.a_take_over)) }
        },
        dismissButton = {
            TextButton(onClick = { state.passwordChangedElsewhere = false }) { Text(stringResource(Res.string.a_later)) }
        },
    )
}

/**
 * A key was removed and the recovery code renewed on another device: this device can no longer
 * read the server's files. One touch of a remaining key (or the key file) takes the new
 * protection over; the new master password is asked for only if it changed as well.
 */
@Composable
fun ProtectionChangedDialog(state: AppState) {
    val backend = state.backend
    var withKey by remember { mutableStateOf(true) }
    var pin by remember { mutableStateOf("") }
    var keyfile by remember { mutableStateOf<PickedFile?>(null) }
    var password by remember { mutableStateOf("") }
    var askPassword by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = {},
        title = { Text(stringResource(Res.string.sync_protection_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.sync_protection_text))
                FactorChoice(withKey) { withKey = it; error = null }
                if (withKey) KeyPinField(state, pin) { pin = it; error = null } else KeyfilePicker(keyfile) { keyfile = it; error = null }
                if (askPassword) {
                    Hint(stringResource(Res.string.sync_protection_password))
                    SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_new_password))
                }
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = {
                    try {
                        backend.adoptRemoteProtection(
                            if (askPassword) password else null,
                            if (withKey) pin else null,
                            if (withKey) null else keyfile?.bytes,
                            state.onTouchNeeded,
                        )
                        true
                    } catch (e: VaultException) {
                        // The master password changed there too: ask for it, then try again.
                        if (e.kind == VaultErrorKind.PASSWORD_CHANGED_ELSEWHERE && !askPassword) false else throw e
                    }
                }) { done ->
                    if (done) {
                        state.protectionChangedElsewhere = false
                        state.changed()
                        state.sync(quiet = true)
                    } else {
                        askPassword = true
                    }
                }
            }, enabled = !state.busy && (if (withKey) keyPinOk(state, pin) else keyfile != null) &&
                (!askPassword || password.isNotEmpty())) { Text(stringResource(Res.string.a_take_over)) }
        },
        dismissButton = {
            TextButton(onClick = { state.protectionChangedElsewhere = false }) { Text(stringResource(Res.string.a_later)) }
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
    val deviceKey = remember { catching { backend.deviceSshKey() }.getOrNull() }
    // U7: SFTP (key or password) or FTPS (password).
    val effective = when (input.kind) {
        ServerKindUi.SFTP -> if (usePassword) input else input.copy(password = null)
        ServerKindUi.FTPS -> input
    }
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
        ChipRow {
            for (k in ServerKindUi.entries) {
                FilterChip(input.kind == k, {
                    input = input.copy(kind = k, port = if (k == ServerKindUi.SFTP) 22 else 21, password = null)
                    error = null
                }, label = { Text(kindLabel(k)) })
            }
        }
        when (input.kind) {
            ServerKindUi.SFTP -> {
                ServerFields(input, usePassword, { input = it; error = null }, { usePassword = it }, showName = false)
                if (!usePassword && deviceKey != null) {
                    Hint(stringResource(Res.string.fetch_device_key))
                    PublicKeyBox(state, deviceKey)
                }
            }
            ServerKindUi.FTPS -> {
                ServerFields(input, true, { input = it; error = null }, {}, showName = false, passwordOnly = true)
            }
        }
        ErrorText(error)
        ButtonRow {
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_back)) }
            Button(
                onClick = { fetch(null) },
                enabled = !state.busy && when (input.kind) {
                    ServerKindUi.SFTP -> input.host.isNotBlank() && input.user.isNotBlank() && (!usePassword || !input.password.isNullOrEmpty())
                    ServerKindUi.FTPS -> input.host.isNotBlank() && input.user.isNotBlank() && !input.password.isNullOrEmpty()
                },
                modifier = Modifier.weight(1f),
            ) { Text(stringResource(Res.string.a_fetch)) }
        }
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

fun kindLabel(kind: ServerKindUi): String = when (kind) {
    ServerKindUi.SFTP -> "SFTP"
    ServerKindUi.FTPS -> "FTPS"
}

/** Unix seconds or milliseconds → local date and time. */
fun formatDateTime(time: Long): String {
    val millis = if (time < 100_000_000_000L) time * 1000 else time
    return formatDate(millis) + " " + formatTime(millis)
}

/** A Long count as the Int that picks the plural form of a text (very large counts stay "other"). */
fun Long.pluralCount(): Int = coerceIn(Int.MIN_VALUE.toLong(), Int.MAX_VALUE.toLong()).toInt()

/**
 * The text for a count, looked up after an action is done. It never fails: if the text cannot be
 * read, the numbers alone are shown, so a finished action is never reported as failed.
 */
suspend fun countText(
    env: org.jetbrains.compose.resources.ResourceEnvironment,
    res: org.jetbrains.compose.resources.PluralStringResource,
    count: Int,
    vararg args: Any,
): String = try {
    org.jetbrains.compose.resources.getPluralString(env, res, count, *args)
} catch (c: kotlinx.coroutines.CancellationException) {
    throw c
} catch (_: Exception) {
    args.joinToString(" ")
}

/** Human-readable size: 1.2 MB. */
fun formatSize(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> "${(bytes * 10 / 1024) / 10.0} KB"
    bytes < 1024L * 1024 * 1024 -> "${(bytes * 10 / (1024 * 1024)) / 10.0} MB"
    else -> "${(bytes * 10 / (1024L * 1024 * 1024)) / 10.0} GB"
}
