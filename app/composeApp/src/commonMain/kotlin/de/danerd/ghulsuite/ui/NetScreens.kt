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

/**
 * Settings → Synchronisation: the servers the suite synchronises with, which one, and how. Only
 * the synchronisation: GhulDrive has storage locations of its own (Settings → GhulDrive).
 */
@Composable
fun ServersSection(state: AppState) {
    val net = state.backend.net
    var servers by remember { mutableStateOf<List<ServerUi>>(emptyList()) }
    var removing by remember { mutableStateOf<ServerUi?>(null) }
    /** Loaded once (until then: "Loading …", not "no servers" and "Off"), and why it failed. */
    var loaded by remember { mutableStateOf(false) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var reload by remember { mutableStateOf(0) }
    LaunchedEffect(state.liveRevision(), reload) {
        val list = catching { net.servers() }.getOrElse { loadError = describe(it); return@LaunchedEffect }
        servers = list
        loaded = true
        loadError = null
    }
    val retry: () -> Unit = { loadError = null; reload++ }

    if (!loaded) {
        SectionCard(stringResource(Res.string.sync_servers_title)) {
            if (loadError != null) LoadError(loadError, retry) else Hint(stringResource(Res.string.loading))
        }
        return
    }
    SectionCard(stringResource(Res.string.sync_servers_title)) {
        Hint(stringResource(Res.string.set_servers_hint))
        LoadError(loadError, retry)
        servers.forEachIndexed { i, s ->
            // Like the list of mail accounts: a row with a tile, and removing as a red icon.
            ListRow(
                s.name, kindLabel(s.kind), leading = { IconTile(Icons.Filled.Dns, paletteColor(1)) },
                trailing = { BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_remove), tint = g.danger) { removing = s } },
                divider = i > 0, flush = true,
            )
            Column(Modifier.fillMaxWidth().padding(start = 56.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Hint("${s.user}@${s.host}:${s.port}")
                Hint(stringResource(Res.string.server_location, s.location.ifBlank { "~/" + s.base }))
            }
        }
        OutlinedButton(onClick = { state.push(Screen.AddServer) }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(Res.string.server_add_title))
        }
    }

    removing?.let { s ->
        // Like every deletion: ask first (F103).
        AlertDialog(
            danger = true,
            onDismissRequest = { removing = null },
            title = { Text(stringResource(Res.string.server_remove_title, s.name)) },
            text = { Text(stringResource(Res.string.server_remove_text)) },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    state.run(onError = { loadError = it }, block = { net.removeServer(s.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_remove)) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(Res.string.a_cancel)) } },
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
