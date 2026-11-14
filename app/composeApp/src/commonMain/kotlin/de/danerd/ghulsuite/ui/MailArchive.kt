// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Forward
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.ArchiveModeUi
import de.danerd.ghulsuite.model.ArchivePlaceUi
import de.danerd.ghulsuite.model.ArchiveRuleUi
import de.danerd.ghulsuite.model.ArchiveRefUi
import de.danerd.ghulsuite.model.ArchivedMailUi
import de.danerd.ghulsuite.model.MailAccountUi
import de.danerd.ghulsuite.model.MailArchiveCountUi
import de.danerd.ghulsuite.model.MailArchiveOptionsUi
import de.danerd.ghulsuite.model.MailArchiveStatusUi
import de.danerd.ghulsuite.model.MailMessageUi
import de.danerd.ghulsuite.resources.*
import de.danerd.ghulsuite.resources.Res
import org.jetbrains.compose.resources.stringResource

/** Choices for the deadline (days); anything else is "own value". */
private val ARCHIVE_DAYS = listOf(1, 7, 30, 90, 365)

/**
 * The mail archive in the GhulMail options (KONZEPT 9.3.1): where it lives, which accounts, after
 * how many days, what happens at the provider, the copy on this device.
 */
@Composable
fun MailArchiveSection(state: AppState) {
    val net = state.backend.net
    var status by remember { mutableStateOf<MailArchiveStatusUi?>(null) }
    var accounts by remember { mutableStateOf<List<MailAccountUi>>(emptyList()) }
    var counts by remember { mutableStateOf<Map<String, MailArchiveCountUi>>(emptyMap()) }
    var error by remember { mutableStateOf<String?>(null) }
    var reload by remember { mutableStateOf(0) }
    var placeSheet by remember { mutableStateOf(false) }
    var ownServer by remember { mutableStateOf(false) }
    var moveTo by remember { mutableStateOf<String?>(null) }
    var moveOffered by remember { mutableStateOf(false) }
    var askDelete by remember { mutableStateOf(false) }
    var askRule by remember { mutableStateOf<Pair<Boolean, ArchiveRuleUi>?>(null) }
    var customDays by remember { mutableStateOf(false) }
    var clearCopy by remember { mutableStateOf(false) }
    var trashFor by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(reload, state.liveRevision()) {
        status = runCatching { net.archiveStatus() }.onFailure { error = describe(it) }.getOrNull()
        accounts = runCatching { net.mailAccounts() }.getOrDefault(emptyList())
        val s = status
        counts = if (s != null && s.mode != ArchiveModeUi.NONE) {
            s.options.accounts.associateWith { a -> runCatching { net.archiveCount(a) }.getOrNull() ?: MailArchiveCountUi(0, 0, 0) }
        } else {
            emptyMap()
        }
    }
    val doneMsg = stringResource(Res.string.archive_done)
    val foreignMsg = stringResource(Res.string.archive_foreign)
    val connectedMsg = stringResource(Res.string.archive_connected)
    val createdMsg = stringResource(Res.string.archive_created)
    val restoredMsg = stringResource(Res.string.archive_restored)
    val busyMsg = stringResource(Res.string.archive_busy)

    /** Handles the answer of "put the archive there". */
    fun placed(target: String?, place: ArchivePlaceUi) {
        when (place) {
            ArchivePlaceUi.CREATED -> state.message(createdMsg)
            ArchivePlaceUi.CONNECTED -> state.message(connectedMsg)
            ArchivePlaceUi.MOVE_OFFERED -> { moveTo = target; moveOffered = true }
            ArchivePlaceUi.FOREIGN -> error = foreignMsg
        }
        reload++
        state.changed()
    }

    fun setOptions(change: (MailArchiveOptionsUi) -> MailArchiveOptionsUi) {
        val s = status ?: return
        val next = change(s.options)
        state.run(onError = { error = it }, block = { net.archiveSetOptions(next) }) { reload++ }
    }

    SectionCard(stringResource(Res.string.archive_title)) {
        Hint(stringResource(Res.string.archive_intro))
        val s = status
        if (s == null) {
            if (error == null) LoadingHint()
            ErrorText(error)
            return@SectionCard
        }
        if (s.mode == ArchiveModeUi.NONE) {
            SectionLabel(stringResource(Res.string.archive_where))
            ListCard {
                if (s.syncAvailable) {
                    ListRow(stringResource(Res.string.drive_like_sync), stringResource(Res.string.archive_like_sync_sub),
                        leading = { IconTile(Icons.Filled.Sync, areaColor(Tab.MAIL)) }) {
                        state.run(onError = { error = it }, block = { net.archiveSetup(null) }) { placed(null, it) }
                    }
                }
                ListRow(stringResource(Res.string.wz_own_server), stringResource(Res.string.archive_own_sub),
                    leading = { IconTile(Icons.Filled.Dns, areaColor(Tab.MAIL)) }, divider = s.syncAvailable) { ownServer = true }
            }
            Hint(stringResource(Res.string.archive_needs_server))
            ErrorText(error)
        } else {
            // Where it lives.
            ListRow(
                stringResource(Res.string.archive_place),
                when (s.mode) {
                    ArchiveModeUi.SYNC -> stringResource(Res.string.archive_place_sync, s.server?.name ?: "-")
                    else -> s.server?.let { "${it.name} · ${it.host}" } ?: "-"
                },
                leading = { IconTile(Icons.Filled.Dns, areaColor(Tab.MAIL)) },
                flush = true,
            ) { placeSheet = true }
            if (state.archiveUnreachable != null) {
                Banner(Icons.Filled.CloudOff, stringResource(Res.string.archive_unreachable, state.archiveUnreachable ?: ""))
            }
            // Which accounts.
            SectionLabel(stringResource(Res.string.archive_accounts))
            for (a in accounts) {
                val on = a.id in s.options.accounts
                SwitchRow(a.name.ifBlank { a.email }, on) { v ->
                    setOptions { o -> o.copy(accounts = if (v) o.accounts + a.id else o.accounts - a.id) }
                }
                counts[a.id]?.takeIf { on && it.mails > 0 }?.let { c ->
                    Hint(stringResource(Res.string.archive_count, c.mails.toString(), formatSize(c.bytes)))
                }
            }
            // After how many days.
            val days = s.options.days
            ChoiceRow(
                stringResource(Res.string.archive_days),
                ARCHIVE_DAYS.map { it to dayLabel(it) } + listOf(-1 to Res.string.archive_days_own),
                if (days in ARCHIVE_DAYS) days else -1,
            ) { d -> if (d < 0) customDays = true else setOptions { it.copy(days = d) } }
            if (days !in ARCHIVE_DAYS) Hint(stringResource(Res.string.archive_days_custom, days.toString()))
            SwitchRow(stringResource(Res.string.archive_sent), s.options.sent) { v -> setOptions { it.copy(sent = v) } }
            val rules = listOf(
                ArchiveRuleUi.LEAVE to Res.string.archive_rule_leave,
                ArchiveRuleUi.ARCHIVE to Res.string.archive_rule_archive,
                ArchiveRuleUi.DELETE to Res.string.archive_rule_delete,
            )
            ChoiceRow(stringResource(Res.string.archive_spam), rules, s.options.spam) { r ->
                if (r == ArchiveRuleUi.DELETE) askRule = true to r else setOptions { it.copy(spam = r) }
            }
            ChoiceRow(stringResource(Res.string.archive_trash), rules, s.options.trash) { r ->
                if (r == ArchiveRuleUi.DELETE) askRule = false to r else setOptions { it.copy(trash = r) }
            }
            // Deleting at the provider: off until switched on and confirmed.
            SwitchRow(stringResource(Res.string.archive_delete), s.options.deleteAtProvider) { v ->
                if (v) askDelete = true else setOptions { it.copy(deleteAtProvider = false) }
            }
            Hint(stringResource(if (s.options.deleteAtProvider) Res.string.archive_delete_on else Res.string.archive_delete_off))
            if (!state.backend.isDesktop) {
                SwitchRow(stringResource(Res.string.archive_wifi), state.backend.settings.archiveWifiOnly) { state.backend.settings.archiveWifiOnly = it }
            }
            Button(onClick = {
                state.run(onError = { error = it }, block = { state.runArchiveOnce(manual = true) }) { r ->
                    reload++
                    when {
                        r == null -> {}
                        r.busy.isNotEmpty() -> state.message(busyMsg.replace("%1\$s", r.busy))
                        else -> state.message(doneMsg.replace("%1\$s", r.archived.toString()).replace("%2\$s", r.removed.toString()))
                    }
                }
            }, enabled = !state.archiveRunning && s.options.accounts.isNotEmpty(), modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Archive, null, Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(stringResource(if (state.archiveRunning) Res.string.archive_running else Res.string.archive_now))
            }
            for (a in s.options.accounts) {
                val name = accounts.firstOrNull { it.id == a }?.name ?: continue
                TextButton(onClick = { trashFor = a }) { Text(stringResource(Res.string.archive_trash_of, name)) }
            }

            // This device's copy.
            SectionLabel(stringResource(Res.string.archive_offline))
            SwitchRow(stringResource(Res.string.archive_full_copy), s.fullCopy) { v ->
                state.run(onError = { error = it }, block = {
                    net.archiveSetFullCopy(v, state.archiveDeviceName())
                    if (v) net.archiveDownload(200, state.archiveDeviceName())
                }) { reload++ }
            }
            Hint(stringResource(Res.string.archive_full_copy_hint))
            if (s.offlineBytes > 0) {
                Hint(stringResource(Res.string.archive_offline_size, formatSize(s.offlineBytes)))
                OutlinedButton(onClick = { clearCopy = true }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.archive_clear_copy)) }
            }
            val others = s.copies.filter { !it.thisDevice && it.mails > 0 }
            for (c in others) {
                Hint(stringResource(Res.string.archive_copy_on, c.name.ifBlank { "?" }, c.mails.toString(), formatDate(c.at)))
            }
            if (s.fullCopy) {
                OutlinedButton(onClick = {
                    state.run(onError = { error = it }, block = { net.archiveRestoreFromCopy() }) { n ->
                        reload++
                        state.message(restoredMsg.replace("%1\$s", n.toString()))
                    }
                }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.archive_restore_copy)) }
                Hint(stringResource(Res.string.archive_restore_copy_hint))
            }
            ErrorText(error)
        }
    }

    if (ownServer) {
        val form = rememberServerForm()
        GhulSheet({ ownServer = false }, stringResource(Res.string.archive_own_title)) {
            ServerSetup(state, form, { input, fp -> net.archiveSaveServer(input, fp) }) { id ->
                ownServer = false
                state.run(onError = { error = it }, block = { net.archiveSetup(id) }) { placed(id, it) }
            }
        }
    }
    if (placeSheet) {
        val s = status
        GhulSheet({ placeSheet = false }, stringResource(Res.string.archive_place_change)) {
            Hint(stringResource(Res.string.archive_place_change_hint))
            SheetList {
                if (s?.syncAvailable == true) {
                    SheetRow(stringResource(Res.string.drive_like_sync), stringResource(Res.string.archive_like_sync_sub),
                        leading = { IconTile(Icons.Filled.Sync, areaColor(Tab.MAIL)) }) {
                        placeSheet = false
                        state.run(onError = { error = it }, block = { net.archiveSetup(null) }) { placed(null, it) }
                    }
                }
                SheetRow(stringResource(Res.string.wz_own_server), stringResource(Res.string.archive_own_sub),
                    leading = { IconTile(Icons.Filled.Dns, areaColor(Tab.MAIL)) }, divider = s?.syncAvailable == true) {
                    placeSheet = false
                    ownServer = true
                }
            }
        }
    }
    if (moveOffered) {
        AlertDialog(
            onDismissRequest = { moveOffered = false },
            title = { Text(stringResource(Res.string.archive_move_title)) },
            text = { Text(stringResource(Res.string.archive_move_text)) },
            confirmButton = {
                TextButton(onClick = {
                    moveOffered = false
                    val target = moveTo
                    state.run(onError = { error = it }, block = { net.archiveMove(target) }) { placed(target, it) }
                }) { Text(stringResource(Res.string.archive_move_yes)) }
            },
            dismissButton = { TextButton(onClick = { moveOffered = false }) { Text(stringResource(Res.string.a_later)) } },
        )
    }
    if (askDelete) {
        AlertDialog(
            danger = true,
            onDismissRequest = { askDelete = false },
            title = { Text(stringResource(Res.string.archive_delete_title)) },
            text = { Text(stringResource(Res.string.archive_delete_text)) },
            confirmButton = {
                TextButton(onClick = { askDelete = false; setOptions { it.copy(deleteAtProvider = true) } }) {
                    Text(stringResource(Res.string.archive_delete_yes))
                }
            },
            dismissButton = { TextButton(onClick = { askDelete = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    askRule?.let { (spam, rule) ->
        AlertDialog(
            danger = true,
            onDismissRequest = { askRule = null },
            title = { Text(stringResource(if (spam) Res.string.archive_spam else Res.string.archive_trash)) },
            text = { Text(stringResource(Res.string.archive_rule_delete_text, s0(status?.options?.days ?: 30))) },
            confirmButton = {
                TextButton(onClick = {
                    askRule = null
                    setOptions { if (spam) it.copy(spam = rule) else it.copy(trash = rule) }
                }) { Text(stringResource(Res.string.archive_rule_delete)) }
            },
            dismissButton = { TextButton(onClick = { askRule = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (customDays) {
        var text by remember { mutableStateOf((status?.options?.days ?: 30).toString()) }
        val n = text.trim().toIntOrNull()
        AlertDialog(
            onDismissRequest = { customDays = false },
            title = { Text(stringResource(Res.string.archive_days_own)) },
            text = {
                OutlinedTextField(text, { text = it.filter(Char::isDigit).take(5) }, singleLine = true,
                    label = { Text(stringResource(Res.string.archive_days)) },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
            },
            confirmButton = {
                TextButton(enabled = n != null && n in 1..36_500, onClick = {
                    customDays = false
                    setOptions { it.copy(days = n ?: it.days) }
                }) { Text(stringResource(Res.string.a_save)) }
            },
            dismissButton = { TextButton(onClick = { customDays = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (clearCopy) ClearCopyDialog(state, { clearCopy = false }) { reload++ }
    trashFor?.let { a -> ArchiveTrashSheet(state, a) { trashFor = null; reload++ } }
}

/** "Delete this device's copy": a typed word when it may be the only full copy. */
@Composable
private fun ClearCopyDialog(state: AppState, onDismiss: () -> Unit, onDone: () -> Unit) {
    val net = state.backend.net
    val only by produceState<Boolean?>(null) { value = runCatching { net.archiveOnlyCopy() }.getOrDefault(true) }
    var typed by remember { mutableStateOf("") }
    val word = stringResource(Res.string.archive_clear_word)
    AlertDialog(
        danger = true,
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.archive_clear_copy)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(stringResource(if (only == true) Res.string.archive_clear_only else Res.string.archive_clear_text))
                if (only == true) ConfirmByName(word, typed) { typed = it }
            }
        },
        confirmButton = {
            TextButton(enabled = only == false || (only == true && nameMatches(typed, word)), onClick = {
                onDismiss()
                state.run(block = { net.archiveClearCopy() }) { onDone() }
            }) { Text(stringResource(Res.string.a_delete)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** The archive's trash of an account: restore, or delete for good. */
@Composable
private fun ArchiveTrashSheet(state: AppState, accountId: String, onDismiss: () -> Unit) {
    val net = state.backend.net
    var list by remember { mutableStateOf<List<ArchivedMailUi>?>(null) }
    var reload by remember { mutableStateOf(0) }
    LaunchedEffect(reload) { list = runCatching { net.archiveTrash(accountId) }.getOrDefault(emptyList()) }
    GhulSheet(onDismiss, stringResource(Res.string.archive_trash_title)) {
        Hint(stringResource(Res.string.archive_trash_hint))
        val l = list
        if (l == null) {
            LoadingHint()
        } else if (l.isEmpty()) {
            Hint(stringResource(Res.string.archive_trash_empty))
        } else {
            SheetList {
                l.forEachIndexed { i, m ->
                    SheetRow(m.summary.subject.ifBlank { m.summary.from }, m.summary.from, divider = i > 0,
                        leading = { IconTile(Icons.Filled.Restore, areaColor(Tab.MAIL), size = 34.dp) }) {
                        state.run(block = { net.archiveRestore(accountId, listOf(m.ref)) }) { reload++; state.changed() }
                    }
                }
            }
            OutlinedButton(onClick = {
                state.run(block = { net.archiveEmptyTrash(accountId, 0) }) { reload++ }
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.archive_trash_empty_now)) }
        }
    }
}

/**
 * Below the provider's mails of a folder: the archived ones (no own view, KONZEPT 9.3.1). Shows
 * only mails the provider no longer has; with [offline] (provider unreachable) all of them.
 */
@Composable
fun ArchivedMails(state: AppState, accountId: String, folder: String, offline: Boolean) {
    val net = state.backend.net
    var page by remember(accountId, folder) { mutableStateOf<List<ArchivedMailUi>>(emptyList()) }
    var total by remember(accountId, folder) { mutableStateOf(0) }
    LaunchedEffect(accountId, folder, offline, state.liveRevision()) {
        val st = runCatching { net.archiveStatus() }.getOrNull()
        if (st == null || st.mode == ArchiveModeUi.NONE || accountId !in st.options.accounts) {
            page = emptyList(); total = 0; return@LaunchedEffect
        }
        runCatching { net.archiveList(accountId, folder, 0, 50, offline) }.onSuccess { page = it.mails; total = it.total }
    }
    if (page.isEmpty()) return
    SectionLabel(stringResource(Res.string.archive_section, total.toString()))
    ListCard {
        page.forEachIndexed { i, m ->
            ArchivedRow(m, divider = i > 0) { state.push(Screen.MailArchiveRead(accountId, m.folder, m.key)) }
        }
    }
    if (total > page.size) {
        OutlinedButton(onClick = {
            state.run(block = { net.archiveList(accountId, folder, page.size, 50, offline) }) { p -> page = page + p.mails; total = p.total }
        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.mail_more, (total - page.size).toString())) }
    }
}

@Composable
private fun ArchivedRow(m: ArchivedMailUi, divider: Boolean, onClick: () -> Unit) {
    val t = g
    val s = m.summary
    ListRow(
        s.subject.ifBlank { stringResource(Res.string.mail_no_subject) },
        listOf(s.from.ifBlank { s.fromAddress }, if (s.date > 0) mailTime(s.date) else "").filter { it.isNotEmpty() }.joinToString(" · "),
        leading = { Avatar(s.from.ifBlank { s.fromAddress }) },
        trailing = { Icon(Icons.Filled.Inventory2, stringResource(Res.string.archive_tag), Modifier.size(18.dp), tint = t.muted) },
        divider = divider,
        bold = !s.seen,
        onClick = onClick,
    )
}

/** An archived mail: read, reply, forward, save, into the archive's trash. */
@Composable
fun MailArchiveReadScreen(state: AppState, accountId: String, folder: String, key: String, padding: PaddingValues) {
    val net = state.backend.net
    var message by remember { mutableStateOf<MailMessageUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val ref = ArchiveRefUi(key, folder)
    LaunchedEffect(key) {
        runCatching { net.archiveGet(accountId, folder, key) }.onSuccess { message = it }.onFailure { error = describe(it) }
        runCatching { net.archiveSetSeen(accountId, listOf(ref), true) }
    }
    val replyPrefix = stringResource(Res.string.mail_re)
    val forwardPrefix = stringResource(Res.string.mail_fwd)
    val quoteHead = stringResource(Res.string.mail_quote_head)
    fun quote(m: MailMessageUi) = "\n\n" + quoteHead.replace("%1\$s", m.from) + "\n" + m.text.lines().joinToString("\n") { "> $it" }
    val deleted = stringResource(Res.string.archive_deleted)
    val t = g
    Column(Modifier.padding(padding)) {
        GhulTopBar("", onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = backIcon(state), actions = {
            if (filePickersAvailable) {
                BarIcon(Icons.Filled.Download, stringResource(Res.string.mail_save_eml)) {
                    state.run(onError = { error = it }, block = { net.archiveRaw(accountId, folder, key) }) {
                        save(fileBase(message?.subject ?: "mail") + ".eml", it)
                    }
                }
            }
            BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = g.danger) {
                state.pop()
                state.undoable(listOf(hiddenKey("archive", "$accountId/$folder", key)), deleted, action = {
                    net.archiveDelete(accountId, listOf(ref))
                })
            }
        })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 20.dp)) {
            ErrorText(error)
            val m = message ?: run { if (error == null) LoadingHint(); return@Column }
            SelectionContainer {
                Text(m.subject.ifBlank { stringResource(Res.string.mail_no_subject) }, fontSize = 22.sp, fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(start = 4.dp, end = 4.dp, bottom = 6.dp))
            }
            FlowRow(Modifier.padding(start = 4.dp, bottom = 14.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Tag(stringResource(Res.string.archive_tag), t.ok, Icons.Filled.Inventory2)
                if (m.encrypted) Tag(stringResource(Res.string.mail_tag_encrypted), t.ok, Icons.Filled.Lock)
            }
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Avatar(m.from.ifBlank { m.fromAddress })
                    Column(Modifier.weight(1f)) {
                        Text(m.from.ifBlank { m.fromAddress }, fontWeight = FontWeight.Bold, maxLines = 1)
                        Text(m.fromAddress, fontSize = 13.sp, color = t.muted, maxLines = 1)
                    }
                    if (m.date > 0) Text(mailTime(m.date), fontSize = 12.sp, color = t.faint)
                }
                if (m.blocks.isNotEmpty()) {
                    MailHtmlView(state, m)
                } else {
                    SelectionContainer { Text(m.text, Modifier.padding(top = 14.dp), fontSize = 15.sp, lineHeight = 22.sp, color = t.fg) }
                }
                for (a in m.attachments) {
                    Row(Modifier.fillMaxWidth().padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        IconTile(Icons.Filled.Description, avatarColor("filetext7"), size = 34.dp)
                        Column(Modifier.weight(1f)) {
                            Text(a.name, fontWeight = FontWeight.Bold, maxLines = 1)
                            Text(formatSize(a.size), fontSize = 12.sp, color = t.muted)
                        }
                        if (filePickersAvailable) {
                            BarIcon(Icons.Filled.Download, stringResource(Res.string.a_save)) {
                                state.run(onError = { error = it }, block = { net.archiveAttachment(accountId, folder, key, a.index) }) { save(a.name, it) }
                            }
                        }
                    }
                }
            }
        }
        val m = message ?: return@Column
        Row(Modifier.fillMaxWidth().background(t.surface).navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilledTonalButton(onClick = {
                val subject = if (m.subject.startsWith(replyPrefix, true) || m.subject.startsWith("Re:", true)) m.subject else "$replyPrefix ${m.subject}"
                state.push(Screen.MailCompose(MailDraft(accountId, to = m.replyTo.ifBlank { m.fromAddress }, subject = subject, text = quote(m), inReplyTo = m.messageId)))
            }, modifier = Modifier.weight(1f)) {
                Icon(Icons.AutoMirrored.Filled.Reply, null, Modifier.size(18.dp))
                Text(stringResource(Res.string.a_reply))
            }
            OutlinedButton(onClick = {
                state.push(Screen.MailCompose(MailDraft(accountId, subject = "$forwardPrefix ${m.subject}", text = quote(m))))
            }, modifier = Modifier.weight(1f)) {
                Icon(Icons.AutoMirrored.Filled.Forward, null, Modifier.size(18.dp))
                Text(stringResource(Res.string.a_forward))
            }
        }
    }
}

/** Questions of the archive that can come up anywhere: the first removal at the provider. */
@Composable
fun MailArchiveDialogs(state: AppState) {
    val net = state.backend.net
    val ask = state.archiveAsk ?: return
    val removedMsg = stringResource(Res.string.archive_removed)
    AlertDialog(
        // Tapping beside it is "later": nothing is deleted.
        onDismissRequest = { state.archiveAsk = null },
        title = { Text(stringResource(Res.string.archive_first_title)) },
        text = { Text(stringResource(Res.string.archive_first_text, ask.second.toString())) },
        confirmButton = {
            TextButton(onClick = {
                state.archiveAsk = null
                state.run(block = { net.archiveConfirmRemoval(ask.first) }) { r ->
                    state.message(removedMsg.replace("%1\$s", r.removed.toString()))
                    state.changed()
                }
            }) { Text(stringResource(Res.string.archive_first_yes, ask.second.toString())) }
        },
        dismissButton = { TextButton(onClick = { state.archiveAsk = null }) { Text(stringResource(Res.string.a_later)) } },
    )
}

/** "Where should your mails live?" after the first account was added, while no archive is set up. */
@Composable
fun ArchiveWhereDialog(state: AppState, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.archive_where_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.archive_where_text))
                ListCard {
                    ListRow(stringResource(Res.string.archive_where_keep), stringResource(Res.string.archive_where_keep_sub)) { onDismiss() }
                    ListRow(stringResource(Res.string.archive_where_archive), stringResource(Res.string.archive_where_archive_sub), divider = true) {
                        onDismiss()
                        state.push(Screen.SettingsPage(SetPage.MAIL))
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_later)) } },
    )
}

private fun dayLabel(d: Int) = when (d) {
    1 -> Res.string.archive_days_1
    7 -> Res.string.archive_days_7
    30 -> Res.string.archive_days_30
    90 -> Res.string.archive_days_90
    else -> Res.string.archive_days_365
}

private fun s0(n: Int) = n.toString()
