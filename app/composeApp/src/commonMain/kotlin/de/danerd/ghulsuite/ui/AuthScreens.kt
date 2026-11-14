// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.material.icons.filled.Key
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.OtpAlgorithmUi
import de.danerd.ghulsuite.model.TotpCodeUi
import de.danerd.ghulsuite.model.TotpInputUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.delay
import org.jetbrains.compose.resources.getString
import org.jetbrains.compose.resources.stringResource
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.foundation.border

private fun grouped(code: String): String = if (code.length >= 6) code.chunked((code.length + 1) / 2).joinToString(" ") else code

@Composable
fun AuthListScreen(state: AppState, padding: PaddingValues) {
    var codes by remember { mutableStateOf<List<TotpCodeUi>?>(null) }
    var renaming by remember { mutableStateOf<TotpCodeUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    var revealed by remember { mutableStateOf<Map<String, Long>>(emptyMap()) }
    var sorting by remember { mutableStateOf(false) }
    var exporting by remember { mutableStateOf<Pair<TotpCodeUi, String>?>(null) }
    var clock by remember { mutableStateOf<Long?>(null) }
    LaunchedEffect(Unit) { clock = runCatching { state.backend.clockOffset(nowSeconds()) }.getOrNull() }
    val settings = state.backend.settings
    val vaults = state.vaultsOf(VaultKindUi.TOTP)
    val filter = state.authVault
    LaunchedEffect(state.liveRevision(), retry) {
        while (true) {
            runCatching { state.backend.totpCodes(nowSeconds()) }
                .onSuccess { codes = it; error = null }
                .onFailure { e -> error = describe(e); if (codes == null) codes = emptyList() }
            // After an error it waits for "Try again" instead of asking every second.
            if (error != null) break
            delay(1000)
        }
    }
    var query by remember { mutableStateOf("") }
    val q = query.trim().lowercase()
    val shown = codes?.filter {
        hiddenKey("auth", it.vaultId, it.id) !in state.hidden &&
            (filter == null || it.vaultId == filter) && (q.isEmpty() || (it.issuer + " " + it.account).lowercase().contains(q))
    }
    val showVault = vaults.size > 1 && filter == null
    val grid = state.authGrid
    val cols = LocalColumns.current
    // Grid: tiles per row; list: one card per row on phones, two side by side in wide panes.
    val perRow = if (grid) cols else (cols / 2).coerceAtLeast(1)
    // Long press selects accounts (as in every list); swipe right edits, swipe left deletes.
    // Deleting asks nothing: "Undo" in the snackbar brings the accounts back.
    val selected = rememberListSelection()
    val keyOf = { c: TotpCodeUi -> c.vaultId + "/" + c.id }
    val selecting = selected.active
    val toggle = { c: TotpCodeUi -> selected.toggle(keyOf(c)) }
    val deletedMsg = stringResource(Res.string.msg_deleted_n)
    fun delete(list: List<TotpCodeUi>) {
        selected.clear()
        state.undoable(list.map { hiddenKey("auth", it.vaultId, it.id) }, deletedMsg.replace("%1\$s", list.size.toString())) {
            for (c in list) state.backend.deleteTotp(c.vaultId, c.id)
        }
    }
    Column(Modifier.fillMaxSize().padding(padding)) {
    if (selecting) {
        val chosen = shown.orEmpty().filter { keyOf(it) in selected }
        val one = chosen.singleOrNull()
        SelectionHeader(
            selected.keys.size,
            onClose = { selected.clear() },
            onAll = { selected.all(shown.orEmpty().map(keyOf)) },
            actions = buildList {
                if (one != null) {
                    add(SelAction(Icons.Filled.Edit, stringResource(Res.string.a_rename)) { selected.clear(); renaming = one })
                    add(SelAction(if (one.favorite) Icons.Filled.Star else Icons.Filled.StarBorder,
                        stringResource(if (one.favorite) Res.string.a_unfavorite else Res.string.a_favorite)) {
                        selected.clear()
                        state.run(block = { state.backend.setFavorite(one.vaultId, one.id, !one.favorite) }) { state.changed() }
                    })
                    add(SelAction(Icons.Filled.QrCode2, stringResource(Res.string.auth_export_qr)) {
                        selected.clear()
                        state.run(block = { state.backend.totpUri(one.vaultId, one.id) }) { exporting = one to it }
                    })
                }
                add(SelAction(Icons.Filled.Delete, stringResource(Res.string.a_delete), danger = true) { delete(chosen) })
            },
        )
    }
    LazyColumn(
        Modifier.align(Alignment.CenterHorizontally).widthIn(max = 672.dp).fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 100.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item(key = "search") {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SearchField(query, { query = it }, placeholder = stringResource(Res.string.auth_search))
                LoadError(error) { retry++ }
                if (codes == null) LoadingHint()
            }
        }
        if (shown?.isEmpty() == true) {
            item(key = "empty") { EmptyState(androidx.compose.material.icons.Icons.Filled.Key, stringResource(Res.string.auth_empty), areaColor(Tab.AUTH)) }
        }
        val sorted = when (settings.authSort) {
            1 -> shown.orEmpty().sortedByDescending { it.modified }
            else -> shown.orEmpty()
        }
        val favs = sorted.filter { it.favorite }
        val rest = sorted.filter { !it.favorite }
        fun section(key: String, list: List<TotpCodeUi>) {
            val rows = list.chunked(perRow)
            items(rows, key = { r -> key + r.joinToString { it.vaultId + it.id } }) { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    for (c in row) {
                        val hidden = settings.totpHideCodes && (revealed[c.id] ?: 0L) < nowSeconds()
                        Box(Modifier.weight(1f)) {
                            EditDeleteSwipe(!grid && !selecting, onEdit = { renaming = c }, onDelete = { delete(listOf(c)) }) {
                                TotpCard(
                                    c,
                                    vaultName = c.vaultName.takeIf { showVault },
                                    hidden = hidden,
                                    showNext = settings.totpShowNext,
                                    grid = grid,
                                    selected = keyOf(c) in selected,
                                    onClick = {
                                        when {
                                            selecting -> toggle(c)
                                            hidden -> revealed = revealed + (c.id to nowSeconds() + 15)
                                            else -> state.copy(c.issuer.ifBlank { c.account }, c.code)
                                        }
                                    },
                                    onLongClick = { toggle(c) },
                                )
                            }
                        }
                    }
                    repeat(perRow - row.size) { Box(Modifier.weight(1f)) }
                }
            }
        }
        if (favs.isNotEmpty()) {
            item(key = "favlabel") { SectionLabel(stringResource(Res.string.chip_favorites)) }
            section("f", favs)
        }
        if (rest.isNotEmpty()) {
            item(key = "label") {
                SectionLabel(stringResource(Res.string.auth_all_accounts), trailing = {
                    Text(stringResource(Res.string.auth_sort), Modifier.clickable { sorting = true })
                })
            }
            section("a", rest)
        }
        if (!shown.isNullOrEmpty()) {
            item(key = "hint") {
                HintText(stringResource(Res.string.auth_tap_hint_short) + " " + stringResource(Res.string.auth_swipe_hint), Modifier.padding(top = 2.dp))
            }
            item(key = "clock") {
                clock?.let { off ->
                    val ok = kotlin.math.abs(off) <= 15
                    Banner(Icons.Filled.Schedule, if (ok) stringResource(Res.string.auth_clock_ok) else stringResource(Res.string.auth_clock_off, off.toString()))
                }
            }
        }
    }
    }
    if (sorting) {
        GhulSheet({ sorting = false }, stringResource(Res.string.auth_sort)) {
            SheetList {
                listOf(0 to Res.string.auth_sort_name, 1 to Res.string.auth_sort_newest).forEachIndexed { i, (v, label) ->
                    SheetRow(stringResource(label), divider = i > 0, end = { if (settings.authSort == v) SheetCheck() }) {
                        settings.authSort = v; sorting = false
                    }
                }
            }
            HintText(stringResource(Res.string.auth_sort_hint))
        }
    }
    exporting?.let { (c, uri) ->
        GhulSheet({ exporting = null }, stringResource(Res.string.auth_export_qr)) {
            Banner(Icons.Filled.Warning, stringResource(Res.string.auth_export_warning))
            QrImage(uri, Modifier.fillMaxWidth())
            HintText(c.issuer.ifBlank { c.account })
        }
    }
    renaming?.let { c ->
        var issuer by remember(c.id) { mutableStateOf(c.issuer) }
        var account by remember(c.id) { mutableStateOf(c.account) }
        AlertDialog(
            onDismissRequest = { renaming = null },
            title = { Text(stringResource(Res.string.a_rename)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(issuer, { issuer = it }, label = { Text(stringResource(Res.string.f_issuer)) }, singleLine = true)
                    OutlinedTextField(account, { account = it }, label = { Text(stringResource(Res.string.f_account)) }, singleLine = true)
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    renaming = null
                    state.run(block = { state.backend.renameTotp(c.vaultId, c.id, issuer, account) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_save)) }
            },
            dismissButton = { TextButton(onClick = { renaming = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** Countdown ring of a code (.ring): track and remaining time, seconds in the middle. */
@Composable
fun CodeRing(remaining: Int, period: Int, color: androidx.compose.ui.graphics.Color, danger: androidx.compose.ui.graphics.Color) {
    val t = g
    Box(Modifier.size(38.dp), contentAlignment = Alignment.Center) {
        androidx.compose.foundation.Canvas(Modifier.size(38.dp)) {
            val stroke = 4.dp.toPx()
            val r = 15.dp.toPx()
            val topLeft = androidx.compose.ui.geometry.Offset(center.x - r, center.y - r)
            val sz = androidx.compose.ui.geometry.Size(2 * r, 2 * r)
            drawArc(t.surface3, 0f, 360f, false, topLeft, sz, style = androidx.compose.ui.graphics.drawscope.Stroke(stroke))
            drawArc(
                if (remaining <= 5) danger else color, -90f, 360f * remaining / period.coerceAtLeast(1), false, topLeft, sz,
                style = androidx.compose.ui.graphics.drawscope.Stroke(stroke, cap = androidx.compose.ui.graphics.StrokeCap.Round),
            )
        }
        Text(remaining.toString(), fontSize = 12.sp, fontWeight = FontWeight.Bold, color = t.muted)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TotpCard(
    c: TotpCodeUi,
    vaultName: String?,
    hidden: Boolean,
    showNext: Boolean,
    grid: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
) {
    val t = g
    val accent = areaColor(Tab.AUTH)
    val soon = c.remaining <= 5
    val name = c.issuer.ifBlank { c.account }
    val code = if (hidden) "••• •••" else grouped(c.code)
    val sub = listOfNotNull(c.account.takeIf { c.issuer.isNotBlank() && it.isNotBlank() }, vaultName).joinToString(" · ")
    Box(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(if (selected) t.accentSoft else t.surface)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick).padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        if (grid) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    SelectableTile(selected) { Avatar(name) }
                    Box(Modifier.weight(1f))
                    CodeRing(c.remaining, c.period, accent, t.danger)
                }
                Text(name, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                Text(code, fontFamily = FontFamily.Monospace, fontSize = 24.sp, fontWeight = FontWeight.Bold, color = if (soon) t.danger else accent)
            }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                SelectableTile(selected) { Avatar(name) }
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        Text(name, fontWeight = FontWeight.SemiBold, maxLines = 1)
                        if (sub.isNotEmpty()) Text(sub, fontSize = 13.sp, color = t.muted, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                    }
                    Text(code, fontFamily = FontFamily.Monospace, fontSize = 30.sp, lineHeight = 33.sp, fontWeight = FontWeight.Bold,
                        letterSpacing = 1.2.sp, color = if (soon) t.danger else accent)
                    if (hidden) {
                        Text(stringResource(Res.string.auth_tap_to_show), fontSize = 12.sp, color = t.muted)
                    } else if (showNext && c.remaining <= 8) {
                        Text(stringResource(Res.string.auth_next, grouped(c.nextCode)), fontSize = 12.sp, color = t.muted)
                    }
                }
                CodeRing(c.remaining, c.period, accent, t.danger)
            }
        }
    }
}

@Composable
fun AddTotpScreen(state: AppState, vaultId: String, padding: PaddingValues, startMode: Int = -1) {
    // 0 = scan QR code, 1 = manual, 2 = otpauth link
    var mode by remember { mutableStateOf(if (startMode >= 0 && (startMode != 0 || qrScannerAvailable)) startMode else if (qrScannerAvailable) 0 else 1) }
    var scanAttempt by remember { mutableStateOf(0) }
    var uri by remember { mutableStateOf("") }
    var issuer by remember { mutableStateOf("") }
    var account by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var algorithm by remember { mutableStateOf(OtpAlgorithmUi.SHA1) }
    var digits by remember { mutableStateOf("6") }
    var period by remember { mutableStateOf("30") }
    var error by remember { mutableStateOf<String?>(null) }

    fun done() {
        state.changed()
        state.pop()
    }

    Column(Modifier.padding(padding)) {
        ScreenColumn {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                val modes = listOfNotNull(
                    if (qrScannerAvailable) 0 to Res.string.auth_add_qr else null,
                    1 to Res.string.auth_add_manual,
                    2 to Res.string.auth_add_uri,
                )
                modes.forEachIndexed { i, (m, label) ->
                    SegmentedButton(selected = mode == m, onClick = { mode = m; error = null }, shape = SegmentedButtonDefaults.itemShape(i, modes.size)) {
                        Text(stringResource(label))
                    }
                }
            }
            if (mode == 0) {
                SectionCard {
                    Hint(stringResource(Res.string.auth_qr_hint))
                    androidx.compose.runtime.key(scanAttempt) {
                        QrScanner { text ->
                        if (text.startsWith("otpauth-migration://")) {
                            // Google Authenticator export: all accounts at once.
                            state.run(onError = { error = it; scanAttempt++ }, block = {
                                val r = state.backend.importData(text.encodeToByteArray(), "google-authenticator", null, vaultId, false)
                                getString(Res.string.msg_migration_done, r.totps.toString(), r.duplicates.toString())
                            }) { msg ->
                                state.message(msg)
                                done()
                            }
                            return@QrScanner
                        }
                        state.run(onError = { error = it; scanAttempt++ }, block = { state.backend.parseOtpauth(text) }) { t ->
                            issuer = t.issuer
                            account = t.account
                            secret = t.secret
                            algorithm = t.algorithm
                            digits = t.digits.toString()
                            period = t.period.toString()
                            mode = 1
                        }
                        }
                    }
                    ErrorText(error)
                }
            } else if (mode == 2) {
                SectionCard {
                    Hint(stringResource(Res.string.auth_uri_hint))
                    OutlinedTextField(uri, { uri = it; error = null }, label = { Text("otpauth://…") }, modifier = Modifier.fillMaxWidth())
                    ErrorText(error)
                    Button(onClick = {
                        state.run(onError = { error = it }, block = {
                            val parsed = state.backend.parseOtpauth(uri)
                            state.backend.addTotp(vaultId, parsed)
                        }) { done() }
                    }, enabled = !state.busy && uri.isNotBlank(), modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.a_add)) }
                }
            } else {
                SectionCard {
                    OutlinedTextField(issuer, { issuer = it }, label = { Text(stringResource(Res.string.f_issuer)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(account, { account = it }, label = { Text(stringResource(Res.string.f_account)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    SecretTextField(secret, { secret = it; error = null }, stringResource(Res.string.f_secret_base32))
                    SectionLabel(stringResource(Res.string.auth_advanced))
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                        OtpAlgorithmUi.entries.forEachIndexed { i, a ->
                            // Steam Guard: five characters, every 30 seconds.
                            SegmentedButton(selected = algorithm == a, onClick = {
                                algorithm = a
                                if (a == OtpAlgorithmUi.STEAM) { digits = "5"; period = "30" } else if (digits == "5") digits = "6"
                            }, shape = SegmentedButtonDefaults.itemShape(i, OtpAlgorithmUi.entries.size)) {
                                Text(if (a == OtpAlgorithmUi.STEAM) "Steam" else a.name)
                            }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedTextField(digits, { digits = it.filter(Char::isDigit) }, label = { Text(stringResource(Res.string.f_digits)) }, singleLine = true, modifier = Modifier.weight(1f))
                        OutlinedTextField(period, { period = it.filter(Char::isDigit) }, label = { Text(stringResource(Res.string.f_period)) }, singleLine = true, modifier = Modifier.weight(1f))
                    }
                    ErrorText(error)
                    ButtonRow {
                        OutlinedButton(onClick = { state.pop() }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_cancel)) }
                        Button(onClick = {
                            val input = TotpInputUi(issuer, account, secret, algorithm, digits.toIntOrNull() ?: 0, period.toIntOrNull() ?: 0)
                            state.run(onError = { error = it }, block = { state.backend.addTotp(vaultId, input) }) { done() }
                        }, enabled = !state.busy && secret.isNotBlank(), modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_add)) }
                    }
                }
            }
        }
    }
}
