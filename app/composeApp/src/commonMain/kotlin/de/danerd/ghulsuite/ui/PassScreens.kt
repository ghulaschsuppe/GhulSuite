// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Attachment
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Warning
import de.danerd.ghulsuite.model.SecurityIssueKindUi
import kotlinx.coroutines.launch
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.ui.graphics.Color
import de.danerd.ghulsuite.model.EntryExtrasUi
import de.danerd.ghulsuite.model.SecurityReportUi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.CreditCard
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material.icons.filled.Badge
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material.icons.filled.AutoFixHigh
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Casino
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.sp
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Link
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.filled.Password
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.EntrySummaryUi
import de.danerd.ghulsuite.model.EntryUi
import de.danerd.ghulsuite.model.FieldType
import de.danerd.ghulsuite.model.FieldUi
import de.danerd.ghulsuite.model.PasswordOptionsUi
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Key

@Composable
fun PassListScreen(state: AppState, padding: PaddingValues) {
    var entries by remember { mutableStateOf<List<EntrySummaryUi>?>(null) }
    var report by remember { mutableStateOf<SecurityReportUi?>(null) }
    var query by remember { mutableStateOf("") }
    var filter by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    val vaults = state.vaultsOf(VaultKindUi.PASS)
    LaunchedEffect(state.liveRevision(), state.passVault, retry) {
        runCatching { state.backend.listEntries(state.passVault) }
            .onSuccess { entries = it; error = null }.onFailure { error = describe(it); if (entries == null) entries = emptyList() }
        report = runCatching { state.backend.securityReport() }.getOrNull()
    }
    val showVault = vaults.size > 1 && state.passVault == null
    val q = query.trim().lowercase()
    // Entries whose deletion waits for "Undo" are gone from the list already.
    val all = entries.orEmpty().filter { hiddenKey("pass", it.vaultId, it.id) !in state.hidden }
    // Type filters only for types that occur (no empty "Cards" chip).
    val types = ENTRY_TYPES.filter { t -> all.any { it.type == t.type } }
    if (filter.isNotEmpty() && filter != "fav" && types.none { it.type == filter }) filter = ""
    val shown = all.filter {
        (filter.isEmpty() || (filter == "fav" && it.favorite) || it.type == filter) &&
            (q.isEmpty() || it.title.lowercase().contains(q) || it.subtitle.lowercase().contains(q))
    }
    val recent = state.backend.settings.recentEntries.split(',').mapNotNull { key ->
        all.firstOrNull { "${it.vaultId}/${it.id}" == key }
    }
    val color = areaColor(Tab.PASS)
    val t = g
    // Long press selects entries (then tap adds or removes); the bar on top acts on them, as in
    // every list. With one entry chosen it also copies and edits.
    val selected = rememberListSelection()
    val keyOf = { e: EntrySummaryUi -> e.vaultId + "/" + e.id }
    val selecting = selected.active
    val toggle = { e: EntrySummaryUi -> selected.toggle(keyOf(e)) }
    val noValue = stringResource(Res.string.pass_no_value)
    val userLabel = stringResource(Res.string.f_username)
    val pwLabel = stringResource(Res.string.f_password)
    fun copyField(e: EntrySummaryUi, field: String, label: String) {
        state.run(block = { state.backend.getEntry(e.vaultId, e.id).value(field) }) { v ->
            if (v.isEmpty()) state.message(noValue.replace("%1\$s", label)) else state.copy(label, v)
        }
    }
    val deletedMsg = stringResource(Res.string.msg_deleted_n)
    fun delete(list: List<EntrySummaryUi>) {
        selected.clear()
        state.undoable(list.map { hiddenKey("pass", it.vaultId, it.id) }, deletedMsg.replace("%1\$s", list.size.toString())) {
            for (x in list) state.backend.deleteEntry(x.vaultId, x.id)
        }
    }
    Column(Modifier.fillMaxSize().padding(padding)) {
    if (selecting) {
        val chosen = all.filter { keyOf(it) in selected }
        val one = chosen.singleOrNull()
        SelectionHeader(
            selected.keys.size,
            onClose = { selected.clear() },
            onAll = { selected.all(shown.map(keyOf)) },
            actions = buildList {
                if (one != null) {
                    add(SelAction(Icons.Filled.Person, stringResource(Res.string.pass_copy_username)) { selected.clear(); copyField(one, "username", userLabel) })
                    add(SelAction(Icons.Filled.Key, stringResource(Res.string.pass_copy_password)) { selected.clear(); copyField(one, "password", pwLabel) })
                    add(SelAction(Icons.Filled.Edit, stringResource(Res.string.a_edit)) { selected.clear(); state.push(Screen.EntryEdit(one.vaultId, one.id, one.type)) })
                }
                add(SelAction(Icons.Filled.Delete, stringResource(Res.string.a_delete), danger = true) { delete(chosen) })
            },
        )
    }
    // The same insets as every tab (ScreenColumn): the search field sits at the same height everywhere.
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(top = 4.dp, bottom = 100.dp)) {
        item(key = "search") {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SearchField(query, { query = it }, placeholder = stringResource(Res.string.pass_search))
                LoadError(error) { retry++ }
                if (entries == null) LoadingHint()
                if (all.isNotEmpty()) {
                    FilterChipRow(
                        listOf("" to stringResource(Res.string.filter_all), "fav" to stringResource(Res.string.chip_favorites)) +
                            types.map { it.type to stringResource(chipLabel(it.type) ?: it.label) },
                        filter, { filter = it },
                        icons = types.associate { it.type to typeIcon(it.type) } + ("fav" to Icons.Filled.StarBorder),
                    )
                }
            }
        }
        if (q.isEmpty() && filter.isEmpty() && all.isNotEmpty()) {
            val r = report
            if (r != null && r.checked > 0) {
                item(key = "security") {
                    val count = r.issues.map { it.id }.distinct().size
                    ListCard(Modifier.padding(horizontal = 16.dp)) {
                        ListRow(
                            stringResource(Res.string.sec_title),
                            if (count == 0) stringResource(Res.string.sec_all_good) else stringResource(Res.string.sec_n_need_attention, count.toString()),
                            leading = { IconTile(Icons.Filled.VerifiedUser, if (count == 0) t.ok else paletteColor(3)) },
                            trailing = { ChevronEnd() },
                        ) { state.push(Screen.Security) }
                    }
                }
            }
            if (recent.isNotEmpty()) {
                item(key = "recent") {
                    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SectionLabel(stringResource(Res.string.pass_recent))
                        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            for (e in recent) {
                                Row(
                                    Modifier.height(36.dp).clip(CircleShape).background(t.surface).border(1.dp, t.line, CircleShape)
                                        .clickable { state.push(Screen.EntryDetail(e.vaultId, e.id)) }.padding(start = 3.dp, end = 14.dp),
                                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    TypeBadge(e.type, e.title, 30.dp)
                                    Text(e.title.ifBlank { stringResource(Res.string.untitled) }, fontSize = 14.sp, fontWeight = FontWeight.W500)
                                }
                            }
                        }
                    }
                }
            }
        }
        if (entries != null && shown.isEmpty()) {
            item(key = "empty") {
                EmptyState(Icons.Filled.Password, stringResource(if (q.isEmpty() && filter.isEmpty()) Res.string.pass_empty else Res.string.pass_no_match), color)
            }
        }
        if (shown.isNotEmpty()) {
            item(key = "label") {
                SectionLabel(stringResource(Res.string.pass_count, shown.size.toString()), Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp))
            }
        }
        itemsIndexed(shown, key = { _, e -> e.vaultId + e.id }) { i, e ->
            val sub = listOfNotNull(
                e.subtitle.takeIf { state.backend.settings.showEntrySubtitles && it.isNotBlank() },
                e.vaultName.takeIf { showVault },
            ).joinToString(" · ")
            Column(Modifier.cardItem(i, shown.size)) {
                ListRow(
                    title = e.title.ifBlank { stringResource(Res.string.untitled) },
                    subtitle = sub,
                    leading = { SelectableTile(keyOf(e) in selected) { TypeBadge(e.type, e.title) } },
                    trailing = if (e.hasTotp || e.favorite) ({
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            if (e.hasTotp) Icon(Icons.Filled.Schedule, null, Modifier.size(20.dp), tint = t.muted)
                            if (e.favorite) Icon(Icons.Filled.Star, null, Modifier.size(20.dp), tint = areaColor(Tab.MAIL))
                        }
                    }) else null,
                    divider = i > 0,
                    selected = keyOf(e) in selected,
                    highlighted = state.wide && state.stack.firstOrNull().let { it is Screen.EntryDetail && it.id == e.id && it.vaultId == e.vaultId },
                    onLongClick = { toggle(e) },
                    onClick = { if (selecting) toggle(e) else state.push(Screen.EntryDetail(e.vaultId, e.id)) },
                )
            }
        }
    }
    }
}

@Composable
private fun TypeBadge(type: String, title: String, size: androidx.compose.ui.unit.Dp = 42.dp) {
    if (type == "login") Avatar(title.ifBlank { type }, size = size) else IconTile(typeIcon(type), avatarColor(type), size)
}

/** Plural label of a type for the filter chips. */
private fun chipLabel(type: String) = when (type) {
    "login" -> Res.string.chip_logins
    "card" -> Res.string.chip_cards
    "note" -> Res.string.chip_notes
    "wifi" -> Res.string.chip_wifi
    "identity" -> Res.string.chip_identities
    "ssh" -> Res.string.chip_ssh
    else -> null
}

/** Icon of an entry type (as in the design preview). */
fun typeIcon(type: String): androidx.compose.ui.graphics.vector.ImageVector = when (type) {
    "login" -> Icons.Filled.Public
    "card" -> Icons.Filled.CreditCard
    "note" -> Icons.Filled.Description
    "wifi" -> Icons.Filled.Wifi
    "identity" -> Icons.Filled.Badge
    "ssh" -> Icons.Filled.Terminal
    else -> Icons.Filled.Password
}

/** "New entry" sheet: entry types as a list, plus the password generator on its own. */
@Composable
fun TypePickerDialog(onPick: (String) -> Unit, onDismiss: () -> Unit, onGenerate: (() -> Unit)? = null, onImport: (() -> Unit)? = null) {
    GhulSheet(onDismiss, stringResource(Res.string.pass_new_entry)) {
        SheetList {
            ENTRY_TYPES.forEachIndexed { i, t ->
                SheetRow(
                    stringResource(t.label),
                    leading = { IconTile(typeIcon(t.type), avatarColor(t.type)) },
                    end = { ChevronEnd() },
                    divider = i > 0,
                ) { onPick(t.type) }
            }
            // Bring in passwords from another app (Proton Pass, Bitwarden, KeePass …) right here.
            if (onImport != null) {
                SheetRow(
                    stringResource(Res.string.pass_add_import), stringResource(Res.string.pass_add_import_sub),
                    leading = { IconTile(Icons.Filled.Download, paletteColor(3)) },
                    end = { ChevronEnd() },
                    divider = true,
                ) { onImport() }
            }
        }
        if (onGenerate != null) {
            FilledTonalButton(onClick = onGenerate, modifier = Modifier.fillMaxWidth().padding(top = 2.dp)) {
                Icon(Icons.Filled.AutoFixHigh, null, Modifier.size(18.dp))
                Text(stringResource(Res.string.gen_only))
            }
        }
    }
}

@Composable
fun EntryDetailScreen(state: AppState, vaultId: String, id: String, padding: PaddingValues) {
    var entry by remember { mutableStateOf<EntryUi?>(null) }
    val deletedMsg = stringResource(Res.string.msg_deleted_n)
    var sharing by remember { mutableStateOf(false) }
    var shareServers by remember { mutableStateOf<List<de.danerd.ghulsuite.model.ServerUi>>(emptyList()) }
    LaunchedEffect(id, state.liveRevision()) {
        entry = runCatching { state.backend.getEntry(vaultId, id) }.getOrElse { state.message(describe(it)); state.pop(); null }
        shareServers = runCatching { state.backend.net.servers() }.getOrDefault(emptyList()).filter { it.canShare }
    }
    val e = entry ?: return
    val def = entryType(e.type)
    val known = def.fields.map { it.name }.toSet()
    // Plain text of the entry for a one-time link (built here because the labels are resources).
    val shareText = buildString {
        appendLine(e.value("title"))
        appendLine()
        for (f in def.fields.filter { it.name != "title" }) {
            val v = e.value(f.name)
            if (v.isNotBlank()) appendLine(stringResource(f.label) + ": " + v)
        }
        for (f in e.fields.filter { it.name !in known && it.value.isNotBlank() }) appendLine(f.name + ": " + f.value)
    }
    var menu by remember { mutableStateOf(false) }
    var extras by remember { mutableStateOf<EntryExtrasUi?>(null) }
    var sheet by remember { mutableStateOf("") }
    val extrasTick = state.liveRevision()
    var reloadExtras by remember { mutableStateOf(0) }
    LaunchedEffect(id, extrasTick, reloadExtras) { extras = runCatching { state.backend.entryExtras(vaultId, id) }.getOrNull() }
    LaunchedEffect(id) { state.backend.settings.markRecent(vaultId, id) }
    val fav = extras?.favorite == true
    val t = g
    Column(Modifier.padding(padding)) {
        // Detail screens: no title in the bar (the name stands below), edit as a pencil, then the menu.
        GhulTopBar(
            "", onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = backIcon(state),
            actions = {
                BarIcon(if (fav) Icons.Filled.Star else Icons.Filled.StarBorder, stringResource(Res.string.chip_favorites),
                    tint = if (fav) areaColor(Tab.MAIL) else Color.Unspecified) {
                    state.run(block = { state.backend.setFavorite(vaultId, id, !fav) }) { reloadExtras++; state.changed() }
                }
                BarIcon(Icons.Filled.Edit, stringResource(Res.string.a_edit)) { state.push(Screen.EntryEdit(vaultId, id, e.type)) }
                BarIcon(Icons.Filled.MoreVert, stringResource(Res.string.entry_more)) { menu = true }
            },
        )
        ScreenColumn {
            val vaultName = state.vaults.firstOrNull { it.id == vaultId }?.name
            Row(
                Modifier.padding(top = 6.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                TypeBadge(e.type, e.value("title"), 64.dp)
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(e.value("title").ifBlank { stringResource(Res.string.untitled) }, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (vaultName != null) Tag(vaultName, icon = Icons.Filled.Layers)
                        val pw = e.value("password")
                        if (pw.isNotEmpty()) {
                            // Same wording as the strength bar ("medium" is not called "weak").
                            val sc = state.backend.passwordScore(pw)
                            if (sc < 3) Tag(stringResource(strengthLabel(sc)).replaceFirstChar { it.uppercase() }, if (sc < 2) t.danger else t.warn)
                        }
                    }
                }
            }
            // Every field its own card (.field); the 2FA key shows as the current code.
            for (f in def.fields.filter { it.name != "title" }) {
                val v = e.value(f.name)
                if (v.isBlank()) continue
                if (f.name == "totp") EntryCodeCard(state, v) else
                    FieldCard(state, stringResource(f.label), v, f.edit == EditKind.SECRET, password = f.name == "password", url = f.edit == EditKind.LIST && f.name == "urls")
            }
            for (f in e.fields.filter { it.name !in known && it.value.isNotBlank() }) {
                FieldCard(state, f.name, f.value, f.type == FieldType.SECRET)
            }
            if (e.type == "wifi" && e.value("ssid").isNotBlank()) {
                FilledTonalButton(onClick = { sheet = "wifi" }, modifier = Modifier.fillMaxWidth()) {
                    Icon(Icons.Filled.QrCode2, null, Modifier.size(18.dp)); Text(stringResource(Res.string.pass_wifi_qr))
                }
            }
            SectionLabel(stringResource(Res.string.entry_more))
            ListCard {
                val x = extras
                ListRow(stringResource(Res.string.pass_history), leading = { IconTile(Icons.Filled.History, paletteColor(5)) },
                    trailing = { MoreEnd((x?.history?.size ?: 0).toString()) }, onClick = { sheet = "history" })
                ListRow(stringResource(Res.string.pass_attachments), leading = { IconTile(Icons.Filled.Attachment, paletteColor(1)) },
                    trailing = { MoreEnd((x?.attachments?.size ?: 0).toString()) }, divider = true, onClick = { sheet = "attachments" })
                ListRow(stringResource(Res.string.pass_add_field), leading = { IconTile(Icons.Filled.Add, paletteColor(2)) },
                    trailing = { MoreEnd("") }, divider = true,
                    onClick = { state.push(Screen.EntryEdit(vaultId, id, e.type)) })
            }
            val created = extras?.created ?: 0L
            HintText(
                listOfNotNull(
                    if (e.modified > 0) stringResource(Res.string.entry_changed, formatDate(e.modified)) else null,
                    if (created > 0) stringResource(Res.string.entry_created, formatDate(created)) else null,
                ).joinToString(" · "),
            )
        }
    }
    if (sheet == "export") {
        var pw by remember { mutableStateOf("") }
        var error by remember { mutableStateOf<String?>(null) }
        val savedMsg = stringResource(Res.string.msg_saved_file)
        val saveExport = rememberSaveFile { ok -> if (ok) { state.message(savedMsg); sheet = "" } }
        val fileName = (e.value("title").ifBlank { "eintrag" }).replace(Regex("[^\\p{L}\\p{N} ._-]"), "_") + ".csv"
        GhulSheet({ sheet = "" }, stringResource(Res.string.pass_export_one)) {
            HintText(stringResource(Res.string.pass_export_one_warning))
            SecretTextField(pw, { pw = it; error = null }, stringResource(Res.string.l_master_password))
            ErrorText(error)
            ButtonRow {
                OutlinedButton(onClick = { sheet = "" }, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_cancel)) }
                Button(onClick = {
                    state.run(onError = { error = it }, block = { state.backend.exportEntries(vaultId, listOf(id), pw) }) { data ->
                        pw = ""
                        saveExport(fileName, data)
                    }
                }, enabled = pw.isNotEmpty() && !state.busy, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_save)) }
            }
        }
    }
    if (menu) {
        val others = state.vaultsOf(VaultKindUi.PASS).filter { it.id != vaultId && it.available }
        val copySuffix = stringResource(Res.string.pass_copy_suffix)
        GhulSheet({ menu = false }, e.value("title").ifBlank { stringResource(Res.string.untitled) }) {
            SheetList {
                if (shareServers.isNotEmpty()) {
                    SheetRow(stringResource(Res.string.pass_share_short), stringResource(Res.string.pass_share_hint),
                        leading = { IconTile(Icons.Filled.Link, paletteColor(2)) }) { menu = false; sharing = true }
                }
                SheetRow(stringResource(Res.string.pass_duplicate), leading = { IconTile(Icons.Filled.ContentCopy, paletteColor(1)) }, divider = shareServers.isNotEmpty()) {
                    menu = false
                    state.run(block = { state.backend.duplicateEntry(vaultId, id, copySuffix) }) { nid ->
                        state.changed(); state.pop(); state.push(Screen.EntryDetail(vaultId, nid))
                    }
                }
                if (others.isNotEmpty()) SheetRow(stringResource(Res.string.pass_move), leading = { IconTile(Icons.Filled.Folder, paletteColor(5)) }, divider = true) { menu = false; sheet = "move" }
                if (filePickersAvailable) {
                    SheetRow(stringResource(Res.string.pass_export_one), stringResource(Res.string.pass_export_one_sub),
                        leading = { IconTile(Icons.Filled.Upload, paletteColor(9)) }, divider = true) { menu = false; sheet = "export" }
                }
                SheetRow(stringResource(Res.string.a_delete), leading = { IconTile(Icons.Filled.Delete, g.danger) }, color = g.danger, divider = true) {
                    menu = false
                    state.pop()
                    state.undoable(listOf(hiddenKey("pass", vaultId, id)), deletedMsg.replace("%1\$s", "1")) { state.backend.deleteEntry(vaultId, id) }
                }
            }
        }
    }
    when (sheet) {
        "history" -> GhulSheet({ sheet = "" }, stringResource(Res.string.pass_history)) {
            val items = extras?.history.orEmpty()
            if (items.isEmpty()) HintText(stringResource(Res.string.pass_history_empty))
            if (items.isNotEmpty()) SheetList {
                items.forEachIndexed { i, h ->
                    var shown by remember { mutableStateOf(false) }
                    SheetRow(
                        if (shown) h.password else "••••••••••••",
                        stringResource(Res.string.pass_history_until, formatDateTime(h.changed)),
                        divider = i > 0,
                        end = {
                            val oldLabel = stringResource_old()
                            Row {
                                BarIcon(if (shown) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, null) { shown = !shown }
                                BarIcon(Icons.Filled.ContentCopy, stringResource(Res.string.a_copy)) { state.copy(oldLabel, h.password) }
                            }
                        },
                    )
                }
            }
            HintText(stringResource(Res.string.pass_history_hint))
        }
        "attachments" -> AttachmentsSheet(state, vaultId, id, extras?.attachments.orEmpty(), { reloadExtras++ }) { sheet = "" }
        "move" -> GhulSheet({ sheet = "" }, stringResource(Res.string.pass_move)) {
            SheetList {
                state.vaultsOf(VaultKindUi.PASS).filter { it.id != vaultId && it.available }.forEachIndexed { i, v ->
                    SheetRow(v.name, leading = { Avatar(v.name) }, divider = i > 0) {
                        sheet = ""
                        state.run(block = { state.backend.moveEntry(vaultId, id, v.id) }) { nid ->
                            state.changed(); state.pop(); state.push(Screen.EntryDetail(v.id, nid))
                        }
                    }
                }
            }
        }
        "wifi" -> GhulSheet({ sheet = "" }, stringResource(Res.string.pass_wifi_qr)) {
            val ssid = e.value("ssid").replace("\\", "\\\\").replace(";", "\\;").replace(",", "\\,").replace(":", "\\:")
            val pw = e.value("password").replace("\\", "\\\\").replace(";", "\\;").replace(",", "\\,").replace(":", "\\:")
            QrImage("WIFI:T:${if (pw.isEmpty()) "nopass" else "WPA"};S:$ssid;P:$pw;;", Modifier.fillMaxWidth())
            HintText(stringResource(Res.string.pass_wifi_qr_hint))
        }
    }
    if (sharing) {
        PassShareDialog(state, shareServers, e.value("title").ifBlank { "GhulPass" }, shareText) { sharing = false }
    }
}

@Composable
private fun stringResource_old(): String = stringResource(Res.string.pass_history_old)

/** Number and chevron at the end of a "More" row. */
@Composable
private fun MoreEnd(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (text.isNotEmpty()) Text(text, fontSize = 13.sp, color = g.faint)
        ChevronEnd()
    }
}

/** The 2FA key of a login shows as the current code with its countdown ring. */
@Composable
private fun EntryCodeCard(state: AppState, secret: String) {
    var now by remember { mutableStateOf(nowSeconds()) }
    LaunchedEffect(Unit) { while (true) { kotlinx.coroutines.delay(1000); now = nowSeconds() } }
    val code = state.backend.entryTotp(secret, now) ?: return
    val t = g
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface).padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(Res.string.pass_code), fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = t.accent)
            Text(code.code.chunked((code.code.length + 1) / 2).joinToString(" "), fontFamily = FontFamily.Monospace, fontSize = 22.sp,
                fontWeight = FontWeight.Bold, color = if (code.remaining <= 5) t.danger else areaColor(Tab.AUTH))
        }
        CodeRing(code.remaining, code.period, areaColor(Tab.AUTH), t.danger)
        val label = stringResource(Res.string.pass_code)
        BarIcon(Icons.Filled.ContentCopy, stringResource(Res.string.a_copy)) { state.copy(label, code.code) }
    }
}

/** Files of an entry: add, save, remove. */
@Composable
private fun AttachmentsSheet(state: AppState, vaultId: String, id: String, list: List<de.danerd.ghulsuite.model.AttachmentUi>, onChanged: () -> Unit, onDismiss: () -> Unit) {
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val add = rememberOpenFile { f -> state.run(block = { state.backend.addAttachment(vaultId, id, f.name, f.bytes) }) { onChanged(); state.changed() } }
    val removedMsg = stringResource(Res.string.msg_deleted)
    // Removing a file asks nothing: it disappears, and "Undo" in the snackbar brings it back.
    fun remove(name: String) = state.undoable(listOf(hiddenKey("att", "$vaultId/$id", name)), "$removedMsg: $name", after = onChanged) {
        state.backend.removeAttachment(vaultId, id, name)
    }
    val files = list.filter { hiddenKey("att", "$vaultId/$id", it.name) !in state.hidden }
    GhulSheet(onDismiss, stringResource(Res.string.pass_attachments)) {
        if (files.isEmpty()) HintText(stringResource(Res.string.pass_attachments_empty))
        if (files.isNotEmpty()) SheetList {
            files.forEachIndexed { i, a ->
                val (ic, c) = fileKind(a.name)
                SheetRow(a.name, formatSize(a.size), leading = { IconTile(ic, c) }, divider = i > 0, end = {
                    Row {
                        if (filePickersAvailable) BarIcon(Icons.Filled.Download, stringResource(Res.string.a_save)) {
                            state.run(block = { state.backend.attachment(vaultId, id, a.name) }) { data -> save(a.name, data) }
                        }
                        BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = g.danger) { remove(a.name) }
                    }
                })
            }
        }
        if (filePickersAvailable) {
            FilledTonalButton(onClick = add, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Add, null, Modifier.size(18.dp)); Text(stringResource(Res.string.pass_attach_add))
            }
        }
        HintText(stringResource(Res.string.pass_attachments_hint))
    }
}

/** Security check: reused and weak passwords, sites without 2FA, optional breach check. */
@Composable
fun SecurityScreen(state: AppState, padding: PaddingValues) {
    var report by remember { mutableStateOf<SecurityReportUi?>(null) }
    var entries by remember { mutableStateOf<List<EntrySummaryUi>>(emptyList()) }
    var breached by remember { mutableStateOf<Map<String, Int>?>(null) }
    var checking by remember { mutableStateOf(false) }
    val settings = state.backend.settings
    LaunchedEffect(state.liveRevision()) {
        report = runCatching { state.backend.securityReport() }.getOrNull()
        entries = runCatching { state.backend.listEntries(null) }.getOrDefault(emptyList())
    }
    val t = g
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    fun check() {
        checking = true
        scope.launch {
            val found = mutableMapOf<String, Int>()
            for (e in entries.filter { it.type == "login" }) {
                val pw = runCatching { state.backend.getEntry(e.vaultId, e.id).value("password") }.getOrDefault("")
                if (pw.isEmpty()) continue
                val n = runCatching { state.backend.pwnedCount(pw) }.getOrElse { state.message(describe(it)); checking = false; return@launch }
                if (n > 0) found[e.vaultId + "/" + e.id] = n
            }
            breached = found
            checking = false
        }
    }
    LaunchedEffect(settings.pwnedCheck, entries) { if (settings.pwnedCheck && breached == null && entries.isNotEmpty()) check() }
    Column(Modifier.padding(padding)) {
        ScreenColumn {
            val r = report ?: return@ScreenColumn
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(r.score.toString(), fontSize = 40.sp, fontWeight = FontWeight.ExtraBold, color = if (r.score >= 80) t.ok else if (r.score >= 50) t.warn else t.danger)
                Column(Modifier.weight(1f)) {
                    Text(stringResource(when {
                        r.score >= 95 -> Res.string.sec_score_great
                        r.score >= 70 -> Res.string.sec_score_good
                        else -> Res.string.sec_score_bad
                    }), fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    Text(stringResource(Res.string.sec_local), color = t.muted)
                }
            }
            @Composable
            fun Group(kind: SecurityIssueKindUi, title: String, why: String, tag: Color, icon: androidx.compose.ui.graphics.vector.ImageVector) {
                val list = r.issues.filter { it.kind == kind }
                if (list.isEmpty()) return
                SectionLabel(title)
                ListCard {
                    list.forEachIndexed { i, x ->
                        ListRow(x.title.ifBlank { stringResource(Res.string.untitled) }, why, leading = { Avatar(x.title) }, divider = i > 0,
                            trailing = { Tag("", tag, icon) }, onClick = { state.push(Screen.EntryDetail(x.vaultId, x.id)) })
                    }
                }
            }
            Group(SecurityIssueKindUi.REUSED, stringResource(Res.string.sec_reused), stringResource(Res.string.sec_reused_why), t.warn, Icons.Filled.ContentCopy)
            Group(SecurityIssueKindUi.WEAK, stringResource(Res.string.sec_weak), stringResource(Res.string.sec_weak_why), t.danger, Icons.Filled.Warning)
            Group(SecurityIssueKindUi.NO_SECOND_FACTOR, stringResource(Res.string.sec_no2fa), stringResource(Res.string.sec_no2fa_why), t.warn, Icons.Filled.VerifiedUser)
            breached?.let { b ->
                SectionLabel(stringResource(Res.string.sec_breached))
                if (b.isEmpty()) HintText(stringResource(Res.string.sec_breached_none)) else ListCard {
                    b.entries.forEachIndexed { i, (key, n) ->
                        val e = entries.firstOrNull { it.vaultId + "/" + it.id == key } ?: return@forEachIndexed
                        ListRow(e.title, stringResource(Res.string.sec_breached_n, n.toString()), leading = { Avatar(e.title) }, divider = i > 0,
                            onClick = { state.push(Screen.EntryDetail(e.vaultId, e.id)) })
                    }
                }
            }
            SectionCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(Res.string.sec_pwned), fontWeight = FontWeight.SemiBold)
                        Text(stringResource(Res.string.sec_pwned_sub), fontSize = 14.sp, color = t.muted)
                    }
                    Switch(settings.pwnedCheck, { settings.pwnedCheck = it; if (it) check() else breached = null })
                }
                if (checking) Hint(stringResource(Res.string.sec_checking))
            }
        }
    }
}

/** One-time link for an entry: encrypted on the device, the key stays in the link. */
@Composable
private fun PassShareDialog(state: AppState, servers: List<de.danerd.ghulsuite.model.ServerUi>, title: String, text: String, onDismiss: () -> Unit) {
    var result by remember { mutableStateOf<de.danerd.ghulsuite.model.ShareInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val mail = rememberMailAvailable(state)
    result?.let { ShareResultDialog(state, it, mail, onDismiss); return }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.pass_share)) },
        text = {
            Column(Modifier.verticalScroll(androidx.compose.foundation.rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ShareOptionsForm(state, servers, onError = { error = it }, defaultDays = 1, defaultMax = 1) { options ->
                    val name = title.filter { it.isLetterOrDigit() || it in " -_." }.trim().ifEmpty { "GhulPass" } + ".txt"
                    state.run(onError = { error = it }, block = { state.backend.net.shareBytes(name, text.encodeToByteArray(), options) }) {
                        result = it
                    }
                }
                ErrorText(error)
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

@Composable
private fun FieldCard(state: AppState, label: String, value: String, secret: Boolean, password: Boolean = false, url: Boolean = false) {
    var visible by remember { mutableStateOf(false) }
    val uri = androidx.compose.ui.platform.LocalUriHandler.current
    val t = g
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface).padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Column(Modifier.weight(1f).padding(end = 8.dp)) {
            Text(label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = t.accent, modifier = Modifier.padding(bottom = 2.dp))
            if (secret && visible) {
                PasswordText(value, style = MaterialTheme.typography.bodyLarge)
            } else {
                Text(if (secret) "••••••••••••" else value, fontSize = 15.sp, color = t.fg)
            }
            if (password) {
                val score = state.backend.passwordScore(value).coerceIn(0, 4)
                Box(Modifier.padding(top = 8.dp).fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.surface3)) {
                    Box(
                        Modifier.fillMaxWidth((score + 1) / 5f).height(6.dp).clip(RoundedCornerShape(3.dp))
                            .background(if (score < 2) t.danger else if (score < 3) t.warn else t.ok),
                    )
                }
                Text(stringResource(strengthLabel(score)), fontSize = 13.sp, color = t.muted, modifier = Modifier.padding(top = 4.dp))
            }
        }
        if (secret) BarIcon(if (visible) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, stringResource(Res.string.a_show)) { visible = !visible }
        val link = value.lineSequence().firstOrNull { it.startsWith("https://") || it.startsWith("http://") }
        if (url && link != null) BarIcon(Icons.Filled.Link, link) { runCatching { uri.openUri(link) } }
        BarIcon(Icons.Filled.ContentCopy, stringResource(Res.string.a_copy)) { state.copy(label, value) }
    }
}

fun strengthLabel(score: Int) = listOf(Res.string.pw_s0, Res.string.pw_s1, Res.string.pw_s2, Res.string.pw_s3, Res.string.pw_s4)[score.coerceIn(0, 4)]

@Composable
fun EntryEditScreen(state: AppState, vaultId: String, id: String?, type: String, padding: PaddingValues) {
    val def = entryType(type)
    val values = remember { mutableStateMapOf<String, String>() }
    var original by remember { mutableStateOf<EntryUi?>(null) }
    var generatorFor by remember { mutableStateOf<String?>(null) }
    var loaded by remember { mutableStateOf(id == null) }
    val customOrder = remember { androidx.compose.runtime.mutableStateListOf<String>() }
    var addField by remember { mutableStateOf(false) }
    LaunchedEffect(id) {
        if (id != null) {
            runCatching { state.backend.getEntry(vaultId, id) }
                .onSuccess { e ->
                    original = e
                    e.fields.forEach { values[it.name] = it.value }
                    val known = entryType(type).fields.map { it.name }.toSet()
                    customOrder.addAll(e.fields.map { it.name }.filter { it !in known })
                    loaded = true
                }
                .onFailure { state.message(describe(it)); state.pop() }
        }
    }
    if (!loaded) return
    val save = {
        val names = def.fields.map { it.name }.toSet()
        val kept = customOrder.filter { it !in names }.map { n -> FieldUi(n, original?.fields?.firstOrNull { it.name == n }?.type ?: FieldType.TEXT, values[n].orEmpty()) }
            .filter { it.value.isNotEmpty() }
        val fields = def.fields.map { f ->
            val raw = values[f.name].orEmpty()
            val clean = if (f.edit == EditKind.LIST) raw.lines().map { it.trim() }.filter { it.isNotEmpty() }.joinToString("\n") else raw
            FieldUi(f.name, f.storageType, clean)
        }.filter { it.value.isNotEmpty() } + kept
        state.run(block = { state.backend.saveEntry(EntryUi(vaultId, id, type, fields)) }) { newId ->
            state.changed()
            state.pop()
            if (id == null) state.push(Screen.EntryDetail(vaultId, newId))
        }
    }
    Column(Modifier.padding(padding)) {
        // Full-screen editor: close on the left, the main action on the right, as in every area.
        GhulTopBar(
            stringResource(if (id == null) Res.string.pass_new_entry else Res.string.a_edit),
            onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = Icons.Filled.Close, showLock = false,
            actions = { HeaderButton(stringResource(Res.string.a_save), enabled = !state.busy) { save() } },
        )
        ScreenColumn {
            BusyBanner(state)
            SectionLabel(stringResource(def.label))
            run {
                for (f in def.fields) {
                    val v = values[f.name].orEmpty()
                    when (f.edit) {
                        // The generator only for passwords, not for 2FA keys, card numbers or CVCs.
                        EditKind.SECRET -> SecretTextField(v, { values[f.name] = it }, stringResource(f.label), trailing = if (f.name in GENERATED_FIELDS) ({
                            IconButton(onClick = { generatorFor = f.name }) {
                                Icon(Icons.Filled.Password, stringResource(Res.string.a_generate))
                            }
                        }) else null)
                        EditKind.TEXT -> OutlinedTextField(
                            v, { values[f.name] = it }, label = { Text(stringResource(f.label)) },
                            singleLine = true, modifier = Modifier.fillMaxWidth(),
                        )
                        EditKind.MULTILINE, EditKind.LIST -> OutlinedTextField(
                            v, { values[f.name] = it }, label = { Text(stringResource(f.label)) },
                            supportingText = if (f.edit == EditKind.LIST) {
                                { Text(stringResource(Res.string.hint_one_per_line)) }
                            } else null,
                            minLines = 2, modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
            }
            // Own fields (name chosen by the user): edit, remove, add.
            val knownNames = def.fields.map { it.name }.toSet()
            for (name in customOrder) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(values[name].orEmpty(), { values[name] = it }, Modifier.weight(1f), label = { Text(name) })
                    BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_remove), tint = g.danger) { customOrder.remove(name); values.remove(name) }
                }
            }
            FilledTonalButton(onClick = { addField = true }, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Add, null, Modifier.size(18.dp)); Text(stringResource(Res.string.pass_add_field))
            }
        }
    }
    if (addField) {
        NameDialog(stringResource(Res.string.pass_add_field), "", onDismiss = { addField = false }) { name ->
            addField = false
            val n = name.trim()
            if (n.isNotEmpty() && n !in customOrder && entryType(type).fields.none { it.name == n } && !n.startsWith("attachment:")) customOrder.add(n)
        }
    }
    generatorFor?.let { field ->
        GeneratorDialog(state, onUse = { values[field] = it; generatorFor = null }, onDismiss = { generatorFor = null })
    }
}

@Composable
fun GeneratorDialog(state: AppState, onUse: (String) -> Unit, onDismiss: () -> Unit, useLabel: String? = null) {
    val settings = state.backend.settings
    var options by remember { mutableStateOf(PasswordOptionsUi(length = settings.generatorLength)) }
    var password by remember { mutableStateOf("") }
    var round by remember { mutableStateOf(0) }
    var words by remember { mutableStateOf(5) }
    var separator by remember { mutableStateOf("-") }
    val passphrase = settings.generatorMode == 1
    LaunchedEffect(options, round, passphrase, words, separator) {
        password = runCatching {
            if (passphrase) state.backend.generatePassphrase(words, separator, true, true) else state.backend.generatePassword(options)
        }.getOrDefault("")
    }
    val t = g
    val pwLabel = stringResource(Res.string.f_password)
    GhulSheet(onDismiss, stringResource(Res.string.gen_title)) {
        SingleChoiceSegmentedButtonRow {
            SegmentedButton(!passphrase, { settings.generatorMode = 0 }) { Text(stringResource(Res.string.gen_chars)) }
            SegmentedButton(passphrase, { settings.generatorMode = 1 }) { Text(stringResource(Res.string.gen_passphrase)) }
        }
        val score = state.backend.passwordScore(password).coerceIn(0, 4)
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(t.surface2).padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                PasswordText(password, style = MaterialTheme.typography.bodyLarge.copy(fontSize = 20.sp, lineHeight = 28.sp))
                Box(Modifier.padding(top = 8.dp).fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.surface3)) {
                    Box(Modifier.fillMaxWidth((score + 1) / 5f).height(6.dp).clip(RoundedCornerShape(3.dp))
                        .background(if (score < 2) t.danger else if (score < 3) t.warn else t.ok))
                }
                Text(stringResource(strengthLabel(score)) + " · " + stringResource(Res.string.gen_colors), fontSize = 13.sp, color = t.muted,
                    modifier = Modifier.padding(top = 4.dp))
            }
            BarIcon(Icons.Filled.Casino, stringResource(Res.string.gen_again)) { round++ }
        }
        if (passphrase) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(Res.string.gen_words))
                GhulSlider(words.toFloat(), { words = it.toInt() }, 3f..10f, Modifier.weight(1f))
                Text(words.toString(), fontWeight = FontWeight.Bold)
            }
            // Separator between the words (N7).
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.gen_separator))
                for (sep in listOf("-", ".", "_", " ", "/")) {
                    FilterChip(separator == sep, { separator = sep }, { Text(if (sep == " ") "␣" else sep, fontWeight = FontWeight.Bold) })
                }
            }
            HintText(stringResource(Res.string.gen_passphrase_hint))
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(Res.string.gen_length_label))
                GhulSlider(options.length.toFloat(), { options = options.copy(length = it.toInt()); settings.generatorLength = it.toInt() }, 12f..64f, Modifier.weight(1f))
                Text(options.length.toString(), fontWeight = FontWeight.Bold)
            }
            SwitchRow(stringResource(Res.string.gen_upper), options.uppercase) { options = options.copy(uppercase = it) }
            SwitchRow(stringResource(Res.string.gen_lower), options.lowercase) { options = options.copy(lowercase = it) }
            SwitchRow(stringResource(Res.string.gen_digits), options.digits) { options = options.copy(digits = it) }
            SwitchRow(stringResource(Res.string.gen_symbols), options.symbols) { options = options.copy(symbols = it) }
            SwitchRow(stringResource(Res.string.gen_ambiguous), options.avoidAmbiguous) { options = options.copy(avoidAmbiguous = it) }
        }
        ButtonRow {
            OutlinedButton(onClick = { state.copy(pwLabel, password) }, modifier = Modifier.weight(1f)) {
                Icon(Icons.Filled.ContentCopy, null, Modifier.size(18.dp)); Text(stringResource(Res.string.a_copy))
            }
            Button(onClick = { onUse(password) }, enabled = password.isNotEmpty(), modifier = Modifier.weight(1f)) {
                Text(useLabel ?: stringResource(Res.string.a_use))
            }
        }
    }
}

@Composable
private fun stringResource_password(): String = stringResource(Res.string.f_password)

@Composable
fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    // The whole line switches, not only the small switch.
    Row(Modifier.fillMaxWidth().heightIn(min = 40.dp).clickable { onChange(!checked) }, verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f), fontSize = 15.sp)
        Switch(checked, onChange)
    }
}

/** Secret fields that get the password generator. */
private val GENERATED_FIELDS = setOf("password", "passphrase")
