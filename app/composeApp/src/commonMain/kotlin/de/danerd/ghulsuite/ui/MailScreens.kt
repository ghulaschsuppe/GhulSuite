// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.automirrored.filled.ReplyAll
import androidx.compose.material.icons.filled.MarkEmailUnread
import androidx.compose.material.icons.filled.Flag
import androidx.compose.material.icons.outlined.Flag
import androidx.compose.material.icons.filled.NewLabel
import androidx.compose.material.icons.filled.LabelOff
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MarkEmailRead
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.material.icons.automirrored.filled.Label
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Attachment
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.LockOpen
import androidx.compose.material.icons.filled.GppBad
import androidx.compose.material.icons.filled.HelpOutline
import androidx.compose.material.icons.filled.Verified
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.automirrored.filled.Forward
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Archive
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.Report
import androidx.compose.material.icons.filled.Folder
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.runtime.produceState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.unit.sp
import androidx.compose.material.icons.filled.Image
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.focus.focusRequester
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.MailAccountInputUi
import de.danerd.ghulsuite.model.MailAccountUi
import de.danerd.ghulsuite.model.MailCheckUi
import de.danerd.ghulsuite.model.MailFolderUi
import de.danerd.ghulsuite.model.MailMessageUi
import de.danerd.ghulsuite.model.MailSecurityUi
import de.danerd.ghulsuite.model.MailServersUi
import de.danerd.ghulsuite.model.MailSummaryUi
import de.danerd.ghulsuite.model.OutgoingAttachmentUi
import de.danerd.ghulsuite.model.OutgoingMailUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource
import de.danerd.ghulsuite.model.VaultKindUi

private const val PAGE = 50

/** How often an open mail list checks for new mail. */
private const val MAIL_REFRESH_MS = 45_000L

/** Inbox and folders of the chosen account. */
@Composable
fun MailScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    var accounts by remember { mutableStateOf<List<MailAccountUi>?>(null) }
    LaunchedEffect(state.liveRevision()) {
        accounts = runCatching { net.mailAccounts() }.getOrDefault(emptyList())
        val list = accounts.orEmpty()
        if (state.mailAccount == null || (state.mailAccount != ALL_INBOXES && list.none { it.id == state.mailAccount })) {
            state.mailAccount = list.firstOrNull()?.id
        }
        state.mailReady = list.isNotEmpty()
    }
    val list = accounts ?: return
    Column(Modifier.padding(padding)) {
        if (list.isEmpty()) {
            MailSetup(state)
        } else {
            val shown = if (state.mailAccount == ALL_INBOXES) list else listOf(list.firstOrNull { it.id == state.mailAccount } ?: list.first())
            Mailbox(state, list, shown)
        }
    }
}

/** GhulMail without an account yet: set up right here, in the same sheet as everywhere (#24). */
@Composable
private fun MailSetup(state: AppState) {
    ScreenColumn {
        BusyBanner(state)
        AreaSetupCard(Tab.MAIL, stringResource(Res.string.mail_setup_title), stringResource(Res.string.mail_setup_inline))
        ListCard {
            ListRow(stringResource(Res.string.mail_account_new), stringResource(Res.string.mail_add_sub),
                leading = { IconTile(Icons.Filled.Add, areaColor(Tab.MAIL)) }) { state.mailAccountSheet = MailSheet(null) }
        }
    }
}

/** Value of [AppState.mailAccount] for "all inboxes". */
const val ALL_INBOXES = "*"

/** A listed message with the account it belongs to. */
class MailItem(val accountId: String, val m: MailSummaryUi)

@Composable
private fun Mailbox(state: AppState, accounts: List<MailAccountUi>, shown: List<MailAccountUi>) {
    val net = state.backend.net
    val all = shown.size > 1 || state.mailAccount == ALL_INBOXES
    val folder = if (all) "INBOX" else state.mailFolder
    val key = shown.joinToString { it.id } + folder
    var messages by remember(key) { mutableStateOf<List<MailItem>>(emptyList()) }
    var total by remember(key) { mutableStateOf(0) }
    var query by remember(key) { mutableStateOf("") }
    var searching by remember { mutableStateOf(false) }
    var filter by remember(key) { mutableStateOf("") }
    var reload by remember { mutableStateOf(0) }
    var loading by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    suspend fun load(): Pair<Int, List<MailItem>> {
        var t = 0
        val items = mutableListOf<MailItem>()
        for (a in shown) {
            val p = if (filter == "flagged") {
                val f = net.mailFlagged(a.id, folder)
                de.danerd.ghulsuite.model.MailPageUi(f.size, f)
            } else {
                net.mailList(a.id, folder, 0, PAGE)
            }
            t += p.total
            items += p.messages.map { MailItem(a.id, it) }
        }
        return t to items.sortedByDescending { it.m.date }
    }

    LaunchedEffect(messages, folder) {
        // Every reload of the inbox (also after reading or marking) updates the badge for all accounts.
        if (folder.equals("INBOX", true) && !searching) state.refreshUnread()
        state.mailLoaded = messages
    }
    LaunchedEffect(key, reload, filter == "flagged", state.liveRevision(), state.mailRefreshTick) {
        loading = true
        runCatching { load() }
            .onSuccess { (t, items) -> messages = items; total = t; error = null; searching = false }
            .onFailure { error = describe(it) }
        loading = false
    }
    // While the list is visible, new mail shows up by itself (quietly, without the loading bar).
    val visible = LocalTabActive.current
    // Seen the inbox: its new-mail notification has done its job.
    LaunchedEffect(key, visible, messages) {
        if (visible && folder.equals("INBOX", true)) shown.forEach { net.clearMailNotification(it.id) }
    }
    LaunchedEffect(key, visible, query.isBlank(), filter) {
        if (!visible || query.isNotBlank()) return@LaunchedEffect
        while (true) {
            kotlinx.coroutines.delay(MAIL_REFRESH_MS)
            runCatching { load() }.onSuccess { (t, items) ->
                if (!searching && (t != total || items.firstOrNull()?.m?.uid != messages.firstOrNull()?.m?.uid)) {
                    messages = items
                    total = t
                }
            }
        }
    }
    val swipeLeft = state.backend.settings.mailSwipeLeft
    val keyOf = { item: MailItem -> item.accountId + "|" + item.m.uid }
    fun hideKey(item: MailItem) = hiddenKey("mail", item.accountId + "/" + folder, item.m.uid.toString())
    val trashMsg = stringResource(Res.string.msg_moved_to_trash)
    val archivedMsg = stringResource(Res.string.mail_archived_n)
    val movedMsg = stringResource(Res.string.mail_moved_n)
    // Removing mails from this list (trash, archive, another folder) goes through "Undo".
    fun remove(list: List<MailItem>, message: String, action: suspend (MailItem) -> Unit) {
        state.undoable(list.map(::hideKey), message.replace("%1\$s", list.size.toString()), after = { reload++ }) { list.forEach { action(it) } }
    }
    fun delete(list: List<MailItem>) = remove(list, trashMsg) { net.mailDelete(it.accountId, folder, it.m.uid) }
    fun archive(list: List<MailItem>) = scope.launch {
        // Without an archive folder there is nothing to archive into: it says so, nothing happens.
        val targets = list.associateWith { net.runCatching { mailFolders(it.accountId) }.getOrDefault(emptyList()).firstOrNull { f -> f.role == "archive" } }
        val todo = list.filter { targets[it]?.name != folder }
        if (todo.any { targets[it] == null }) state.messageRes(Res.string.mail_no_archive)
        else if (todo.isNotEmpty()) remove(todo, archivedMsg) { net.mailMove(it.accountId, folder, it.m.uid, targets.getValue(it)!!.name) }
    }
    fun swipe(item: MailItem, left: Boolean) {
        if (left && swipeLeft == 0) return archive(listOf(item)).let {}
        if (left && swipeLeft == 1) return delete(listOf(item))
        scope.launch {
            runCatching { net.mailSetSeen(item.accountId, folder, item.m.uid, if (left) true else !item.m.seen) }.onFailure { state.message(describe(it)) }
            reload++
        }
    }
    // Long press selects mails (as in every list); the bar on top acts on them.
    val selected = rememberListSelection()
    var moving by remember { mutableStateOf<List<MailItem>?>(null) }
    val listed = messages.filter { hideKey(it) !in state.hidden }
    val chosen = listed.filter { keyOf(it) in selected }
    // "All N mails in this folder" (also the ones not loaded yet): one account, plain list only.
    val single = if (all) null else shown.singleOrNull()
    var wholeFolder by remember(key) { mutableStateOf(false) }
    LaunchedEffect(selected.active) { if (!selected.active) wholeFolder = false }
    var movingAll by remember { mutableStateOf(false) }
    var deleteAll by remember { mutableStateOf(false) }
    var emptying by remember { mutableStateOf(false) }
    val folderRole by produceState("", key) {
        value = single?.let { a -> runCatching { net.mailFolders(a.id) }.getOrNull()?.firstOrNull { it.name == folder }?.role } ?: ""
    }
    val doneMsg = stringResource(Res.string.mail_bulk_done)
    // Runs an action on every mail of the folder, then leaves the selection.
    fun bulk(block: suspend (String, List<Long>) -> Unit) {
        val a = single ?: return
        state.run(onError = { error = it }, block = { val uids = net.mailFolderUids(a.id, folder); block(a.id, uids); uids.size }) { n ->
            selected.clear()
            wholeFolder = false
            reload++
            state.message(doneMsg.replace("%1\$s", n.toString()))
        }
    }

    Column(Modifier.fillMaxSize()) {
    if (selected.active) {
        val anyUnread = chosen.any { !it.m.seen }
        val anyUnflagged = chosen.any { !it.m.flagged }
        fun flags(block: suspend (MailItem) -> Unit) {
            val list = chosen
            selected.clear()
            state.run(onError = { error = it }, block = { list.forEach { block(it) } }) { reload++ }
        }
        SelectionHeader(
            if (wholeFolder) total else selected.keys.size,
            onClose = { selected.clear() },
            onAll = { selected.all(listed.map(keyOf)) },
            actions = buildList {
                add(SelAction(if (anyUnread) Icons.Filled.MarkEmailRead else Icons.Filled.MarkEmailUnread,
                    stringResource(if (anyUnread) Res.string.a_mark_read else Res.string.a_mark_unread)) {
                    if (wholeFolder) bulk { a, u -> net.mailSetSeenMany(a, folder, u, anyUnread) }
                    else flags { net.mailSetSeen(it.accountId, folder, it.m.uid, anyUnread) }
                })
                add(SelAction(if (anyUnflagged) Icons.Outlined.Flag else Icons.Filled.Flag, stringResource(if (anyUnflagged) Res.string.a_flag else Res.string.a_unflag)) {
                    if (wholeFolder) bulk { a, u -> net.mailSetFlaggedMany(a, folder, u, anyUnflagged) }
                    else flags { net.mailSetFlagged(it.accountId, folder, it.m.uid, anyUnflagged) }
                })
                add(SelAction(Icons.Filled.Archive, stringResource(Res.string.mail_archive)) {
                    if (wholeFolder) bulk { a, u ->
                        val target = net.mailFolders(a).firstOrNull { it.role == "archive" }?.name
                            ?: throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.INVALID_INPUT, "no archive folder")
                        if (target != folder) net.mailMoveMany(a, folder, u, target)
                    } else { val list = chosen; selected.clear(); archive(list) }
                })
                // Moving needs the folders of one account.
                if (chosen.map { it.accountId }.distinct().size == 1) add(SelAction(Icons.Filled.Folder, stringResource(Res.string.a_move)) {
                    if (wholeFolder) movingAll = true else moving = chosen
                })
                add(SelAction(Icons.Filled.Delete, stringResource(Res.string.a_delete), danger = true) {
                    if (wholeFolder) deleteAll = true else { val list = chosen; selected.clear(); delete(list) }
                })
            },
        )
    }
    if (selected.active && single != null && filter.isEmpty() && !searching && total > listed.size &&
        (wholeFolder || selected.keys.size >= listed.size)
    ) {
        Box(Modifier.fillMaxWidth().clickable(enabled = !wholeFolder) { wholeFolder = true }.padding(horizontal = 16.dp, vertical = 10.dp)) {
            Text(
                stringResource(if (wholeFolder) Res.string.mail_whole_folder_on else Res.string.mail_whole_folder, total.toString()),
                color = areaColor(Tab.MAIL), fontWeight = FontWeight.SemiBold,
            )
        }
    }
    Box(Modifier.weight(1f)) {
    ScreenColumn {
        BusyBanner(state)
        // "Empty folder" only in the trash and spam, with the number and a question first.
        if (!selected.active && single != null && total > 0 && (folderRole == "trash" || folderRole == "junk")) {
            OutlinedButton(onClick = { emptying = true }, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(Res.string.mail_empty_folder))
            }
        }
        SearchField(query, {
            query = it
            if (it.isBlank() && searching) reload++
        }, placeholder = stringResource(Res.string.mail_search), onSearch = {
            if (query.isNotBlank()) {
                state.run(onError = { error = it }, block = {
                    shown.flatMap { a -> net.mailSearch(a.id, folder, query).map { MailItem(a.id, it) } }.sortedByDescending { it.m.date }
                }) {
                    messages = it
                    searching = true
                }
            }
        })
        LoadError(error) { error = null; reload++ }
        state.archiveUnreachable?.let { why ->
            // KONZEPT 9.3.1: the provider's mails and what is kept offline stay usable; nothing is deleted.
            Banner(Icons.Filled.CloudOff, stringResource(Res.string.archive_unreachable, why)) {
                TextButton(onClick = { scope.launch { state.runArchiveOnce(manual = true) } }) { Text(stringResource(Res.string.a_retry)) }
            }
            TextButton(onClick = { state.push(Screen.SettingsPage(SetPage.MAIL)) }) { Text(stringResource(Res.string.archive_place_change)) }
        }
        FilterChipRow(
            listOf(
                "" to stringResource(Res.string.filter_all),
                "unread" to stringResource(Res.string.mail_filter_unread),
                "flagged" to stringResource(Res.string.mail_filter_flagged),
                "attachments" to stringResource(Res.string.mail_filter_attachments),
            ),
            filter, { filter = it },
            icons = mapOf("flagged" to Icons.Outlined.Flag, "attachments" to Icons.Filled.Attachment),
        )
        if (loading && messages.isEmpty()) LoadingHint()
        val shownItems = listed.filter {
            when (filter) {
                "unread" -> !it.m.seen
                "flagged" -> it.m.flagged
                "attachments" -> it.m.hasAttachments
                else -> true
            }
        }
        if (!loading && shownItems.isEmpty() && error == null) EmptyState(tabIcon(Tab.MAIL), stringResource(Res.string.mail_empty), areaColor(Tab.MAIL))
        // Conversations: the newest message of each subject stands for the others (count in brackets).
        val threads = shownItems.groupBy { it.accountId + "|" + it.m.threadKey.ifEmpty { it.m.uid.toString() } }
        val heads = shownItems.filter { item -> threads[item.accountId + "|" + item.m.threadKey.ifEmpty { item.m.uid.toString() }]?.first() === item }
        // Grouped by day: Today, Yesterday, then the date.
        val now = nowSeconds() * 1000
        val today = formatDate(now)
        val yesterday = formatDate(now - 86_400_000L)
        val todayLabel = stringResource(Res.string.day_today)
        val yesterdayLabel = stringResource(Res.string.day_yesterday)
        val groups = heads.groupBy { item ->
            when (val d = if (item.m.date > 0) formatDate(item.m.date * 1000) else "") {
                today -> todayLabel
                yesterday -> yesterdayLabel
                else -> d
            }
        }
        val names = accounts.associate { it.id to it.name }
        for ((day, list) in groups) {
            SectionLabel(day)
            ListCard {
                list.forEachIndexed { i, item ->
                    val count = threads[item.accountId + "|" + item.m.threadKey.ifEmpty { item.m.uid.toString() }]?.size ?: 1
                    val open = state.wide && state.stack.firstOrNull().let { it is Screen.MailRead && it.uid == item.m.uid && it.accountId == item.accountId }
                    val chosenRow = keyOf(item) in selected
                    SwipeRow(enabled = !selected.active, onLeft = { swipe(item, true) }, onRight = { swipe(item, false) }, leftAction = swipeLeft) {
                        Box(if (chosenRow) Modifier.background(g.accentSoft) else if (open) Modifier.background(g.surface2) else Modifier) {
                        MailRow(item.m, today, yesterdayLabel, divider = i > 0, count = count, account = if (all) names[item.accountId] else null,
                            selected = chosenRow, onLongClick = { selected.toggle(keyOf(item)) }) {
                            if (selected.active) selected.toggle(keyOf(item)) else state.push(Screen.MailRead(item.accountId, folder, item.m.uid))
                        }
                        }
                    }
                }
            }
        }
        if (!searching && !all && filter != "flagged" && messages.size < total) {
            OutlinedButton(onClick = {
                val a = shown.first()
                state.run(onError = { error = it }, block = { net.mailList(a.id, folder, messages.size, PAGE) }) { p ->
                    messages = messages + p.messages.map { MailItem(a.id, it) }
                    total = p.total
                }
            }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.mail_more, (total - messages.size).toString())) }
        }
        // The archived mails of this folder, below the provider's (KONZEPT 9.3.1).
        if (!searching && !all && filter.isEmpty() && shown.isNotEmpty()) ArchivedMails(state, shown.first().id, folder, offline = error != null)
        if (messages.isNotEmpty()) HintText(stringResource(Res.string.mail_swipe_hint))
    }
    }
    }
    if (deleteAll) {
        AlertDialog(
            danger = true,
            onDismissRequest = { deleteAll = false },
            title = { Text(stringResource(Res.string.mail_delete_all_title, total.toString())) },
            text = { Text(stringResource(if (folderRole == "trash") Res.string.mail_delete_all_final else Res.string.mail_delete_all_text)) },
            confirmButton = {
                TextButton(onClick = { deleteAll = false; bulk { a, u -> net.mailDeleteMany(a, folder, u) } }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { deleteAll = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (emptying) {
        val emptied = stringResource(Res.string.mail_emptied)
        AlertDialog(
            danger = true,
            onDismissRequest = { emptying = false },
            title = { Text(stringResource(Res.string.mail_empty_title, total.toString())) },
            text = { Text(stringResource(Res.string.mail_empty_text)) },
            confirmButton = {
                TextButton(onClick = {
                    emptying = false
                    val a = single ?: return@TextButton
                    state.run(onError = { error = it }, block = { net.mailEmptyFolder(a.id, folder) }) { n ->
                        reload++
                        state.message(emptied.replace("%1\$s", n.toString()))
                    }
                }) { Text(stringResource(Res.string.mail_empty_yes)) }
            },
            dismissButton = { TextButton(onClick = { emptying = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (movingAll && single != null) {
        val folders by produceState(emptyList<MailFolderUi>(), single.id) { value = runCatching { net.mailFolders(single.id) }.getOrDefault(emptyList()) }
        GhulSheet({ movingAll = false }, stringResource(Res.string.a_move)) {
            SheetList {
                folders.filter { it.name != folder }.forEachIndexed { i, f ->
                    SheetRow(folderLabel(f), leading = { IconTile(folderIcon(f.role), areaColor(Tab.MAIL)) }, divider = i > 0) {
                        movingAll = false
                        bulk { a, u -> net.mailMoveMany(a, folder, u, f.name) }
                    }
                }
            }
        }
    }
    moving?.let { list ->
        val account = list.first().accountId
        val folders by produceState(emptyList<MailFolderUi>(), account) { value = runCatching { net.mailFolders(account) }.getOrDefault(emptyList()) }
        GhulSheet({ moving = null }, stringResource(Res.string.a_move)) {
            SheetList {
                folders.filter { it.name != folder }.forEachIndexed { i, f ->
                    SheetRow(folderLabel(f), leading = { IconTile(folderIcon(f.role), areaColor(Tab.MAIL)) }, divider = i > 0) {
                        moving = null
                        selected.clear()
                        remove(list, movedMsg) { net.mailMove(it.accountId, folder, it.m.uid, f.name) }
                    }
                }
            }
        }
    }
}

/** Swipe a mail row: to the right toggles read, to the left does the chosen action. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SwipeRow(enabled: Boolean = true, onLeft: () -> Unit, onRight: () -> Unit, leftAction: Int, content: @Composable () -> Unit) {
    if (!enabled) return content()
    val st = androidx.compose.material3.rememberSwipeToDismissBoxState(confirmValueChange = { v ->
        when (v) {
            androidx.compose.material3.SwipeToDismissBoxValue.EndToStart -> onLeft()
            androidx.compose.material3.SwipeToDismissBoxValue.StartToEnd -> onRight()
            else -> {}
        }
        false
    })
    val t = g
    androidx.compose.material3.SwipeToDismissBox(
        st,
        backgroundContent = {
            val left = st.dismissDirection == androidx.compose.material3.SwipeToDismissBoxValue.EndToStart
            Row(
                Modifier.fillMaxSize().background(if (left && leftAction == 1) t.danger else t.accent).padding(horizontal = 24.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = if (left) Arrangement.End else Arrangement.Start,
            ) {
                Icon(
                    if (!left) Icons.Filled.MarkEmailRead else when (leftAction) { 0 -> Icons.Filled.Archive; 1 -> Icons.Filled.Delete; else -> Icons.Filled.MarkEmailRead },
                    null, tint = androidx.compose.ui.graphics.Color.White,
                )
            }
        },
    ) { Box(Modifier.background(t.surface)) { content() } }
}

@Composable
private fun folderLabel(f: MailFolderUi): String = when (f.role) {
    "inbox" -> stringResource(Res.string.mail_inbox)
    "sent" -> stringResource(Res.string.mail_sent)
    "drafts" -> stringResource(Res.string.mail_drafts)
    "trash" -> stringResource(Res.string.mail_trash)
    "junk" -> stringResource(Res.string.mail_junk)
    "archive" -> stringResource(Res.string.mail_archive)
    else -> f.name
}

@Composable
@OptIn(ExperimentalFoundationApi::class)
private fun MailRow(
    m: MailSummaryUi, today: String, yesterday: String, divider: Boolean, count: Int = 1, account: String? = null,
    selected: Boolean = false, onLongClick: (() -> Unit)? = null, onClick: () -> Unit,
) {
    val name = m.from.ifBlank { m.fromAddress }
    val unread = !m.seen
    val color = areaColor(Tab.MAIL)
    // An encrypted mail shows "..." as its subject on the server (the real one is inside).
    val hiddenSubject = m.encrypted && (m.subject.isBlank() || m.subject.trim() == "...")
    if (divider) RowDivider()
    Row(
        Modifier.fillMaxWidth()
            .then(if (onLongClick != null) Modifier.combinedClickable(onClick = onClick, onLongClick = onLongClick) else Modifier.clickable(onClick = onClick))
            .padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.Top,
    ) {
        SelectableTile(selected) { Avatar(name) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(name, Modifier.weight(1f, fill = false), fontWeight = if (unread) FontWeight.Bold else FontWeight.W500,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (count > 1) Text("($count)", color = g.muted, fontWeight = FontWeight.W500)
                }
                val millis = m.date * 1000
                val day = if (m.date > 0) formatDate(millis) else ""
                Text(
                    when {
                        m.date <= 0 -> ""
                        day == today -> formatTime(millis)
                        else -> day
                    },
                    fontSize = 12.sp,
                    fontWeight = if (unread) FontWeight.Bold else FontWeight.Normal,
                    color = if (unread) color else g.faint,
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (hiddenSubject) {
                    // The subject travels encrypted too: say so once, quietly, instead of "...".
                    Row(Modifier.weight(1f, fill = false), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Icon(Icons.Filled.Lock, null, Modifier.size(14.dp), tint = g.ok)
                        Text(stringResource(Res.string.mail_preview_encrypted), color = g.muted, maxLines = 1, overflow = TextOverflow.Ellipsis,
                            fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal)
                    }
                } else {
                    Text(m.subject.ifBlank { stringResource(Res.string.mail_no_subject) }, Modifier.weight(1f, fill = false), maxLines = 1, overflow = TextOverflow.Ellipsis,
                        color = g.fg, fontWeight = if (unread) FontWeight.Bold else FontWeight.Normal)
                }
                if (m.flagged) Icon(Icons.Filled.Flag, null, Modifier.size(18.dp), tint = color)
            }
            if (hiddenSubject) {
                // Nothing more to show: the line above says it.
            } else if (m.encrypted) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(Icons.Filled.Lock, null, Modifier.size(14.dp), tint = g.ok)
                    Text(stringResource(Res.string.mail_preview_encrypted), fontSize = 14.sp, color = g.muted, maxLines = 1)
                }
            } else if (m.preview.isNotBlank()) {
                Text(m.preview, fontSize = 14.sp, color = g.muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            if (m.hasAttachments || m.labels.isNotEmpty() || account != null) {
                androidx.compose.foundation.layout.FlowRow(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (account != null) Tag(account)
                    if (m.hasAttachments) Tag(stringResource(Res.string.mail_attachment_tag), icon = Icons.Filled.Attachment)
                    for (l in m.labels) Tag(l, g.warn, Icons.AutoMirrored.Filled.Label)
                }
            }
        }
        if (unread) Box(Modifier.padding(top = 6.dp).size(8.dp).clip(CircleShape).background(color))
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MailReadScreen(state: AppState, accountId: String, folder: String, uid: Long, padding: PaddingValues) {
    val net = state.backend.net
    var message by remember { mutableStateOf<MailMessageUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var flagged by remember { mutableStateOf(false) }
    var folders by remember { mutableStateOf<List<MailFolderUi>>(emptyList()) }
    var moveOpen by remember { mutableStateOf(false) }
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    LaunchedEffect(uid) {
        runCatching { net.mailGet(accountId, folder, uid) }.onSuccess { message = it; state.refreshUnread() }.onFailure { error = describe(it) }
        folders = runCatching { net.mailFolders(accountId) }.getOrDefault(emptyList())
    }
    val replyPrefix = stringResource(Res.string.mail_re)
    val forwardPrefix = stringResource(Res.string.mail_fwd)
    val quoteHead = stringResource(Res.string.mail_quote_head)

    val m0 = message
    fun quote(m: MailMessageUi) = "\n\n" + quoteHead.replace("%1\$s", m.from) + "\n" + m.text.lines().joinToString("\n") { "> $it" }
    fun re(s: String) = if (s.startsWith(replyPrefix, ignoreCase = true) || s.startsWith("Re:", true)) s else "$replyPrefix $s"
    val t = g
    val accountName by produceState("", accountId) { value = runCatching { net.mailAccounts().firstOrNull { it.id == accountId }?.name }.getOrNull().orEmpty() }
    val archive = folders.firstOrNull { it.role == "archive" }
    val inTrash = folders.any { it.role == "trash" && it.name == folder }
    val readKey = listOf(hiddenKey("mail", "$accountId/$folder", uid.toString()))
    // Leaving the mail is the action: the list no longer shows it, "Undo" brings it back.
    fun leave(message: String, action: suspend () -> Unit) {
        state.pop()
        state.undoable(readKey, message, after = { state.refreshUnread() }, action = action)
    }
    val trashMsg = stringResource(Res.string.msg_moved_to_trash)
    val deletedMsg = stringResource(Res.string.msg_deleted)
    val archivedMsg = stringResource(Res.string.mail_archived_n).replace("%1\$s", "1")
    val movedMsg = stringResource(Res.string.mail_moved_n).replace("%1\$s", "1")
    var more by remember { mutableStateOf(false) }
    var labelAdd by remember { mutableStateOf(false) }
    var original by remember { mutableStateOf<String?>(null) }
    val listed = state.mailLoaded.firstOrNull { it.accountId == accountId && it.m.uid == uid }
    var labels by remember(uid) { mutableStateOf(listed?.m?.labels.orEmpty()) }
    val thread = listed?.m?.threadKey?.takeIf { it.isNotEmpty() }?.let { k ->
        state.mailLoaded.filter { it.accountId == accountId && it.m.threadKey == k && it.m.uid != uid }.sortedBy { it.m.date }
    }.orEmpty()
    val savedMsg2 = stringResource(Res.string.msg_saved_file)
    val saveFile = rememberSaveFile { ok -> if (ok) state.message(savedMsg2) }
    val driveDone = stringResource(Res.string.mail_to_drive_done)
    Column(Modifier.padding(padding)) {
        GhulTopBar("", onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = backIcon(state), actions = {
            if (archive != null && archive.name != folder) {
                BarIcon(Icons.Filled.Archive, stringResource(Res.string.mail_archive)) {
                    leave(archivedMsg) { net.mailMove(accountId, folder, uid, archive.name) }
                }
            }
            BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = g.danger) {
                // Into the trash of the mailbox (inside the trash: for good). No question, "Undo".
                leave(if (inTrash) deletedMsg else trashMsg) { net.mailDelete(accountId, folder, uid) }
            }
            BarIcon(Icons.Filled.MoreVert, stringResource(Res.string.entry_more)) { more = true }
        })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 20.dp)) {
            BusyBanner(state)
            ErrorText(error)
            val m = m0 ?: return@Column
            SelectionContainer {
                Text(m.subject.ifBlank { stringResource(Res.string.mail_no_subject) }, fontSize = 22.sp, lineHeight = 27.sp, fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(start = 4.dp, end = 4.dp, bottom = 6.dp))
            }
            FlowRow(Modifier.padding(start = 4.dp, end = 4.dp, bottom = 14.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (accountName.isNotEmpty()) Tag(accountName)
                if (m.encrypted) Tag(stringResource(Res.string.mail_tag_encrypted), t.ok, Icons.Filled.Lock)
                when (m.signature) {
                    de.danerd.ghulsuite.model.MailSignatureUi.VALID -> Tag(stringResource(Res.string.mail_tag_signed), t.ok, Icons.Filled.Verified)
                    de.danerd.ghulsuite.model.MailSignatureUi.UNKNOWN_KEY -> Tag(stringResource(Res.string.mail_tag_unknown), null, Icons.Filled.HelpOutline)
                    de.danerd.ghulsuite.model.MailSignatureUi.INVALID -> Tag(stringResource(Res.string.mail_tag_invalid), t.danger, Icons.Filled.GppBad)
                    de.danerd.ghulsuite.model.MailSignatureUi.NONE -> {}
                }
                if (!m.encrypted) Tag("TLS", t.ok, Icons.Filled.Lock)
                for (l in labels) Tag(l, t.warn, Icons.AutoMirrored.Filled.Label)
            }
            // Earlier messages of the same conversation (collapsed; tap to open).
            for (o in thread) {
                Row(
                    Modifier.fillMaxWidth().padding(bottom = 10.dp).clip(RoundedCornerShape(20.dp)).background(t.surface)
                        .clickable { state.pop(); state.push(Screen.MailRead(o.accountId, folder, o.m.uid)) }.padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Avatar(o.m.from.ifBlank { o.m.fromAddress }, size = 34.dp)
                    Column(Modifier.weight(1f)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(o.m.from.ifBlank { o.m.fromAddress }, fontWeight = FontWeight.Bold, maxLines = 1)
                            if (o.m.date > 0) Text("· " + mailTime(o.m.date), color = t.muted)
                        }
                        if (o.m.encrypted) {
                            // As in the list: one quiet line with the lock, never the "..." subject.
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                Icon(Icons.Filled.Lock, null, Modifier.size(14.dp), tint = t.ok)
                                Text(stringResource(Res.string.mail_preview_encrypted), fontSize = 14.sp, color = t.muted, maxLines = 1)
                            }
                        } else {
                            Text(o.m.preview.ifBlank { o.m.subject }, fontSize = 14.sp, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            // The message (.msg): sender, blocked images, text, attachments.
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Avatar(m.from.ifBlank { m.fromAddress })
                    Column(Modifier.weight(1f)) {
                        Text(m.from.ifBlank { m.fromAddress }, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val to = (m.to + m.cc).joinToString(", ")
                        Text(
                            listOfNotNull(m.fromAddress.takeIf { it != m.from }, if (to.isNotEmpty()) stringResource(Res.string.mail_to_lower, to) else null).joinToString(" · "),
                            fontSize = 13.sp, color = t.muted,
                        )
                    }
                    if (m.date > 0) Text(mailTime(m.date), fontSize = 12.sp, color = t.faint)
                }
                var asText by remember(m.uid) { mutableStateOf(false) }
                val formatted = m.blocks.isNotEmpty() && !asText
                if (m.wasHtml && m.blocks.isEmpty()) Box(Modifier.padding(top = 12.dp)) { Banner(Icons.Filled.Image, stringResource(Res.string.mail_html_hint)) }
                if (m.signature == de.danerd.ghulsuite.model.MailSignatureUi.INVALID) {
                    Box(Modifier.padding(top = 12.dp)) { Banner(Icons.Filled.GppBad, stringResource(Res.string.mail_badge_invalid)) }
                }
                if (formatted) {
                    MailHtmlView(state, m)
                } else {
                    SelectionContainer { Text(m.text, Modifier.padding(top = 14.dp), fontSize = 15.sp, lineHeight = 22.sp, color = t.fg) }
                }
                if (m.blocks.isNotEmpty()) {
                    Text(
                        stringResource(if (asText) Res.string.mail_show_formatted else Res.string.mail_show_text),
                        Modifier.padding(top = 10.dp).clickable { asText = !asText },
                        color = t.accent, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                    )
                }
                for (a in m.attachments) {
                    Row(
                        Modifier.fillMaxWidth().padding(top = 12.dp).clip(RoundedCornerShape(14.dp))
                            .border(1.dp, t.line, RoundedCornerShape(14.dp)).padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        IconTile(Icons.Filled.Description, avatarColor("filetext7"), size = 34.dp)
                        Column(Modifier.weight(1f)) {
                            Text(a.name, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(a.name.substringAfterLast('.', "").uppercase().let { if (it.isEmpty()) "" else "$it · " } + formatSize(a.size), fontSize = 12.sp, color = t.muted)
                        }
                        if (filePickersAvailable) {
                            BarIcon(Icons.Filled.Download, stringResource(Res.string.a_save)) {
                                state.run(onError = { error = it }, block = { net.mailAttachment(accountId, folder, uid, a.index) }) { data -> save(a.name, data) }
                            }
                        }
                    }
                }
            }
            if (m.attachments.isNotEmpty()) HintText(stringResource(Res.string.mail_attachment_warning), Modifier.padding(top = 10.dp))
        }
        val m = m0 ?: return@Column
        // Action bar (.actionbar).
        Column(Modifier.fillMaxWidth().background(t.surface)) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(t.line))
            Row(
                Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 14.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                FilledTonalButton(onClick = {
                    state.push(Screen.MailCompose(MailDraft(accountId, to = m.replyTo.ifBlank { m.fromAddress }, subject = re(m.subject), text = quote(m), inReplyTo = m.messageId)))
                }, modifier = Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 10.dp)) {
                    Icon(Icons.AutoMirrored.Filled.Reply, null, Modifier.size(18.dp))
                    Text(stringResource(Res.string.a_reply), maxLines = 1)
                }
                // Two clear actions; "reply to all" is in the ⋮ menu when there are other recipients.
                OutlinedButton(onClick = {
                    state.push(Screen.MailCompose(MailDraft(accountId, subject = "$forwardPrefix ${m.subject}", text = quote(m))))
                }, modifier = Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 10.dp)) {
                    Icon(Icons.AutoMirrored.Filled.Forward, null, Modifier.size(18.dp))
                    Text(stringResource(Res.string.a_forward), maxLines = 1)
                }
            }
        }
    }
    if (more) {
        GhulSheet({ more = false }, message?.subject?.ifBlank { null } ?: stringResource(Res.string.mail_no_subject)) {
            SheetList {
                val rm = message
                if (rm != null && (rm.to + rm.cc).size > 1) {
                    SheetRow(stringResource(Res.string.a_reply_all), leading = { IconTile(Icons.AutoMirrored.Filled.ReplyAll, paletteColor(1)) }) {
                        more = false
                        state.push(Screen.MailCompose(MailDraft(accountId, to = rm.replyTo.ifBlank { rm.fromAddress },
                            cc = (rm.to + rm.cc).joinToString(", "), subject = re(rm.subject), text = quote(rm), inReplyTo = rm.messageId)))
                    }
                }
                SheetRow(stringResource(Res.string.a_mark_unread), leading = { IconTile(Icons.Filled.MarkEmailUnread, paletteColor(1)) }, divider = rm != null && (rm.to + rm.cc).size > 1) {
                    more = false
                    state.run(onError = { error = it }, block = { net.mailSetSeen(accountId, folder, uid, false) }) { state.changed(); state.pop() }
                }
                SheetRow(stringResource(if (flagged) Res.string.a_unflag else Res.string.a_flag),
                    leading = { IconTile(if (flagged) Icons.Filled.Flag else Icons.Outlined.Flag, paletteColor(3)) }, divider = true) {
                    more = false
                    state.run(onError = { error = it }, block = { net.mailSetFlagged(accountId, folder, uid, !flagged) }) { flagged = !flagged }
                }
                SheetRow(stringResource(Res.string.a_move), leading = { IconTile(Icons.Filled.Folder, paletteColor(5)) }, divider = true) { more = false; moveOpen = true }
                SheetRow(stringResource(Res.string.mail_label_add), leading = { IconTile(Icons.Filled.NewLabel, paletteColor(2)) }, divider = true) { more = false; labelAdd = true }
                for (l in labels) {
                    SheetRow(stringResource(Res.string.mail_label_remove, l), leading = { IconTile(Icons.Filled.LabelOff, paletteColor(2)) }, divider = true) {
                        more = false
                        state.run(onError = { error = it }, block = { net.mailSetLabel(accountId, folder, uid, l, false) }) { labels = labels - l; state.changed() }
                    }
                }
                val m = message
                if (m != null && filePickersAvailable) {
                    SheetRow(stringResource(Res.string.mail_save_eml), if (m.encrypted) stringResource(Res.string.mail_eml_encrypted_hint) else null,
                        leading = { IconTile(Icons.Filled.Download, paletteColor(9)) }, divider = true) {
                        more = false
                        state.run(onError = { error = it }, block = { net.mailRaw(accountId, folder, uid, false) }) { saveFile(fileBase(m.subject) + ".eml", it) }
                    }
                    SheetRow(stringResource(Res.string.mail_save_pdf), leading = { IconTile(Icons.Filled.PictureAsPdf, paletteColor(7)) }, divider = true) {
                        more = false
                        saveFile(fileBase(m.subject) + ".pdf", textPdf(m.subject, mailAsText(m)))
                    }
                }
                if (m != null && state.backend.settings.moduleGhulDrive) {
                    SheetRow(stringResource(Res.string.mail_to_drive), leading = { IconTile(tabIcon(Tab.DRIVE), areaColor(Tab.DRIVE)) }, divider = true) {
                        more = false
                        state.run(onError = { error = it }, block = {
                            net.driveSaveBytes("Mail", fileBase(m.subject) + ".pdf", textPdf(m.subject, mailAsText(m)))
                            for (a in m.attachments) net.driveSaveBytes("Mail", a.name, net.mailAttachment(accountId, folder, uid, a.index))
                        }) { state.message(driveDone); state.changed() }
                    }
                }
                SheetRow(stringResource(Res.string.mail_show_original), leading = { IconTile(Icons.Filled.Code, paletteColor(8)) }, divider = true) {
                    more = false
                    state.run(onError = { error = it }, block = { net.mailHeaders(accountId, folder, uid) }) { original = it }
                }
            }
        }
    }
    if (labelAdd) {
        NameDialog(stringResource(Res.string.mail_label_add), "", onDismiss = { labelAdd = false }) { name ->
            labelAdd = false
            state.run(onError = { error = it }, block = { net.mailSetLabel(accountId, folder, uid, name, true) }) { labels = labels + name; state.changed() }
        }
    }
    original?.let { text ->
        GhulSheet({ original = null }, stringResource(Res.string.mail_show_original)) {
            SelectionContainer {
                Text(text, fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 16.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(g.surface2).padding(12.dp))
            }
        }
    }
    if (moveOpen) {
        GhulSheet({ moveOpen = false }, stringResource(Res.string.a_move)) {
            SheetList {
                folders.filter { it.name != folder }.forEachIndexed { i, f ->
                    SheetRow(folderLabel(f), leading = { IconTile(folderIcon(f.role), areaColor(Tab.MAIL), size = 34.dp) }, divider = i > 0) {
                        moveOpen = false
                        leave(movedMsg) { net.mailMove(accountId, folder, uid, f.name) }
                    }
                }
            }
        }
    }
}

/** File name from a subject. */
fun fileBase(subject: String): String =
    subject.filter { it.isLetterOrDigit() || it in " -_.()" }.trim().take(60).ifEmpty { "Mail" }

/** A mail as plain text with its header lines (for PDF and GhulDrive). */
fun mailAsText(m: MailMessageUi): String = buildString {
    appendLine("Von: " + (if (m.from != m.fromAddress && m.from.isNotEmpty()) "${m.from} <${m.fromAddress}>" else m.fromAddress))
    if (m.to.isNotEmpty()) appendLine("An: " + m.to.joinToString(", "))
    if (m.cc.isNotEmpty()) appendLine("Cc: " + m.cc.joinToString(", "))
    if (m.date > 0) appendLine("Datum: " + formatDateTime(m.date))
    if (m.attachments.isNotEmpty()) appendLine("Anhänge: " + m.attachments.joinToString(", ") { it.name })
    appendLine()
    append(m.text)
}

/** Time of a mail: today the time, otherwise the date. */
@Composable
fun mailTime(seconds: Long): String {
    val ms = seconds * 1000
    return if (formatDate(ms) == formatDate(nowSeconds() * 1000)) formatTime(ms) else formatDate(ms)
}

fun folderIcon(role: String): androidx.compose.ui.graphics.vector.ImageVector = when (role) {
    "inbox" -> Icons.Filled.Inbox
    "sent" -> Icons.AutoMirrored.Filled.Send
    "drafts" -> Icons.Filled.Description
    "trash" -> Icons.Filled.Delete
    "junk" -> Icons.Filled.Report
    "archive" -> Icons.Filled.Archive
    else -> Icons.Filled.Folder
}

@Composable
fun MailComposeScreen(state: AppState, draft: MailDraft, padding: PaddingValues) {
    val net = state.backend.net
    var accounts by remember { mutableStateOf<List<MailAccountUi>>(emptyList()) }
    var accountId by remember { mutableStateOf(draft.accountId) }
    var to by remember { mutableStateOf(draft.to) }
    var cc by remember { mutableStateOf(draft.cc) }
    var bcc by remember { mutableStateOf(draft.bcc) }
    var showCc by remember { mutableStateOf(draft.cc.isNotBlank() || draft.bcc.isNotBlank()) }
    var draftId by remember { mutableStateOf(draft.draftId) }
    var scheduling by remember { mutableStateOf(false) }
    var suggestions by remember { mutableStateOf<List<de.danerd.ghulsuite.model.MailContactUi>>(emptyList()) }
    var subject by remember { mutableStateOf(draft.subject) }
    var text by remember { mutableStateOf(draft.text) }
    // OpenPGP for this one mail (signature, own key, encryption); off sends a plain mail.
    var pgp by remember { mutableStateOf(true) }
    val attachments = remember { mutableStateListOf<OutgoingAttachmentUi>().apply { addAll(draft.attachments) } }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        accounts = runCatching { net.mailAccounts() }.getOrDefault(emptyList())
        if (accountId == null) accountId = accounts.firstOrNull()?.id
    }
    val attach = rememberOpenFile { f -> attachments.add(OutgoingAttachmentUi(f.name, mimeOf(f.name), f.bytes)) }
    // Drafts are saved encrypted in the suite a moment after typing stops, attachments included.
    LaunchedEffect(to, cc, bcc, subject, text, accountId, attachments.size) {
        val acc = accountId ?: return@LaunchedEffect
        if (to.isBlank() && subject.isBlank() && text.isBlank()) return@LaunchedEffect
        if (to == draft.to && subject == draft.subject && text == draft.text && attachments.size == draft.attachments.size && draftId.isEmpty()) return@LaunchedEffect
        kotlinx.coroutines.delay(1500)
        draftId = runCatching {
            net.mailSaveDraft(de.danerd.ghulsuite.model.MailDraftUi(draftId, acc, to, cc, bcc, subject, text, draft.inReplyTo, attachments = attachments.toList()))
        }.getOrDefault(draftId)
    }
    // Address suggestions for the last address being typed.
    LaunchedEffect(to) {
        val last = to.substringAfterLast(',').trim()
        suggestions = if (last.length < 2 || last.contains('@') && last.substringAfter('@').contains('.')) emptyList()
        else runCatching { net.mailContacts(last) }.getOrDefault(emptyList())
    }
    val sentMsg = stringResource(Res.string.mail_sent_msg)
    // Encryption preview: looked up a moment after the recipients stop changing.
    var keys by remember { mutableStateOf<List<de.danerd.ghulsuite.model.MailRecipientKeyUi>?>(null) }
    LaunchedEffect(to, cc, bcc) {
        val all = splitAddresses(to) + splitAddresses(cc) + splitAddresses(bcc)
        if (all.isEmpty()) {
            keys = null
            return@LaunchedEffect
        }
        kotlinx.coroutines.delay(700)
        keys = runCatching { net.mailRecipientKeys(all) }.getOrNull()
    }

    val t = g
    val forwardPrefix = stringResource(Res.string.mail_fwd)
    val title = stringResource(
        when {
            draft.inReplyTo.isNotBlank() -> Res.string.mail_reply_title
            draft.subject.startsWith(forwardPrefix) -> Res.string.mail_forward_title
            else -> Res.string.mail_new_title
        },
    )
    val canSend = !state.busy && accountId != null && splitAddresses(to + "," + cc + "," + bcc).isNotEmpty()
    fun send() {
        val acc = accountId ?: return
        val mail = OutgoingMailUi(acc, splitAddresses(to), splitAddresses(cc), splitAddresses(bcc), subject, text, draft.inReplyTo, attachments.toList(), pgp = pgp)
        state.run(onError = { error = it }, block = { net.mailSend(mail); if (draftId.isNotEmpty()) runCatching { net.mailDeleteDraft(draftId) } }) {
            state.message(sentMsg)
            state.pop()
        }
    }
    val scheduledMsg = stringResource(Res.string.mail_scheduled_done)
    fun sendLater(atMillis: Long) {
        val acc = accountId ?: return
        val mail = OutgoingMailUi(acc, splitAddresses(to), splitAddresses(cc), splitAddresses(bcc), subject, text, draft.inReplyTo, attachments.toList(), pgp = pgp)
        state.run(onError = { error = it }, block = { net.mailSchedule(mail, atMillis); if (draftId.isNotEmpty()) runCatching { net.mailDeleteDraft(draftId) } }) {
            state.message(scheduledMsg.replace("%1\$s", formatDateTime(atMillis)))
            state.pop()
        }
    }
    Column(Modifier.padding(padding)) {
        // Header like every full-screen editor: close, title, own actions, the main action.
        GhulTopBar(
            title, onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = Icons.Filled.Close, showLock = false,
            actions = {
                if (filePickersAvailable) BarIcon(Icons.Filled.Attachment, stringResource(Res.string.a_attach)) { attach() }
                HeaderButton(stringResource(Res.string.a_send), enabled = canSend, icon = Icons.AutoMirrored.Filled.Send) { send() }
            },
        )
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 20.dp)) {
            BusyBanner(state)
            val current = accounts.firstOrNull { it.id == accountId }
            val subjectFocus = remember { androidx.compose.ui.focus.FocusRequester() }
            val ccFocus = remember { androidx.compose.ui.focus.FocusRequester() }
            val bccFocus = remember { androidx.compose.ui.focus.FocusRequester() }
            ComposeLine(stringResource(Res.string.mail_from_short)) {
                var open by remember { mutableStateOf(false) }
                Box {
                    Row(
                        Modifier.clip(RoundedCornerShape(16.dp)).background(t.surface2).clickable(enabled = accounts.size > 1) { open = true }
                            .padding(start = 3.dp, end = 10.dp, top = 3.dp, bottom = 3.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Avatar(current?.name ?: "?", size = 24.dp)
                        Text(current?.email ?: "", fontSize = 14.sp)
                    }
                    if (open) {
                        GhulSheet({ open = false }, stringResource(Res.string.mail_from_short)) {
                            SheetList {
                                accounts.forEachIndexed { i, a ->
                                    SheetRow(a.name.ifBlank { a.email }, a.email, leading = { Avatar(a.name.ifBlank { a.email }) }, divider = i > 0,
                                        end = { if (a.id == accountId) SheetCheck() }) { open = false; accountId = a.id }
                                }
                            }
                        }
                    }
                }
                if (accounts.size > 1) Icon(Icons.Filled.KeyboardArrowDown, null, Modifier.size(20.dp), tint = t.muted)
            }
            ComposeLine(stringResource(Res.string.mail_to_short)) {
                LineInput(to, { to = it }, stringResource(Res.string.mail_to_placeholder), Modifier.weight(1f), KeyboardType.Email)
                if (!showCc) Text(stringResource(Res.string.mail_cc_bcc), fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = t.accent,
                    modifier = Modifier.clickable { showCc = true })
            }
            if (suggestions.isNotEmpty()) {
                Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (c in suggestions) {
                        Row(
                            Modifier.clip(RoundedCornerShape(16.dp)).background(t.surface2).clickable {
                                val parts = splitAddresses(to).dropLast(1) + c.email
                                to = parts.joinToString(", ") + ", "
                                suggestions = emptyList()
                            }.padding(start = 3.dp, end = 10.dp, top = 3.dp, bottom = 3.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Avatar(c.name.ifBlank { c.email }, size = 24.dp)
                            Text(if (c.name.isNotBlank()) "${c.name} · ${c.email}" else c.email, fontSize = 14.sp, maxLines = 1)
                        }
                    }
                }
            }
            if (showCc) {
                ComposeLine(stringResource(Res.string.mail_cc_short), ccFocus) { LineInput(cc, { cc = it }, "", Modifier.weight(1f).focusRequester(ccFocus), KeyboardType.Email) }
                ComposeLine(stringResource(Res.string.mail_bcc_short), bccFocus) { LineInput(bcc, { bcc = it }, "", Modifier.weight(1f).focusRequester(bccFocus), KeyboardType.Email) }
            }
            ComposeLine(stringResource(Res.string.mail_subject_short), subjectFocus) { LineInput(subject, { subject = it }, "", Modifier.weight(1f).focusRequester(subjectFocus)) }
            if (!pgp) {
                Box(Modifier.padding(top = 12.dp)) {
                    SecurityLine(Icons.Filled.LockOpen, g.faint, stringResource(Res.string.mail_pgp_off_hint))
                }
            } else {
                keys?.let { Box(Modifier.padding(top = 12.dp)) { EncryptionPreview(it) } }
            }
            val style = MaterialTheme.typography.bodyLarge.copy(color = t.fg)
            androidx.compose.foundation.text.BasicTextField(
                text, { text = it }, Modifier.fillMaxWidth().heightIn(min = 220.dp).padding(vertical = 14.dp), textStyle = style,
                cursorBrush = androidx.compose.ui.graphics.SolidColor(t.accent),
                decorationBox = { inner -> Box { if (text.isEmpty()) Text(stringResource(Res.string.mail_text), style = style.copy(color = t.faint)); inner() } },
            )
            for (a in attachments.toList()) {
                Row(
                    Modifier.fillMaxWidth().padding(top = 12.dp).clip(RoundedCornerShape(14.dp))
                        .border(1.dp, t.line, RoundedCornerShape(14.dp)).padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    IconTile(Icons.Filled.Description, avatarColor("pdf"), size = 34.dp)
                    Column(Modifier.weight(1f)) {
                        Text(a.name, fontWeight = FontWeight.SemiBold, maxLines = 1)
                        Text(formatSize(a.data.size.toLong()), fontSize = 12.sp, color = t.muted)
                    }
                    BarIcon(Icons.Filled.Close, stringResource(Res.string.a_remove)) { attachments.remove(a) }
                }
            }
            Box(Modifier.fillMaxWidth().padding(top = 12.dp).height(1.dp).background(t.line))
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (filePickersAvailable) BarIcon(Icons.Filled.Attachment, stringResource(Res.string.a_attach)) { attach() }
                Box(Modifier.weight(1f))
                val encrypted = pgp && keys?.isNotEmpty() == true && keys!!.all { it.hasKey }
                BarIcon(Icons.Filled.Schedule, stringResource(Res.string.mail_send_later)) { scheduling = true }
                // PGP on/off for this mail, as a clear switch instead of a bare lock.
                val pgpLabel = stringResource(if (pgp) Res.string.mail_pgp_on else Res.string.mail_pgp_off)
                Row(
                    Modifier.padding(start = 6.dp).clip(RoundedCornerShape(16.dp))
                        .border(1.dp, if (pgp) t.ok.copy(alpha = 0.5f) else t.line, RoundedCornerShape(16.dp))
                        .clickable { pgp = !pgp }.padding(horizontal = 10.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(if (encrypted || pgp) Icons.Filled.Lock else Icons.Filled.LockOpen, null, Modifier.size(16.dp), tint = if (pgp) t.ok else t.muted)
                    Text(pgpLabel, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = if (pgp) t.ok else t.muted)
                }
            }
            ErrorText(error)
            if (draftId.isNotEmpty()) HintText(stringResource(Res.string.mail_draft_saved))
            HintText(stringResource(Res.string.mail_compose_hint))
        }
    }
    if (scheduling) {
        val now = nowSeconds() * 1000
        GhulSheet({ scheduling = false }, stringResource(Res.string.mail_send_later)) {
            SheetList {
                listOf(
                    Res.string.mail_in_1h to now + 3_600_000L,
                    Res.string.mail_in_3h to now + 3 * 3_600_000L,
                    Res.string.mail_tomorrow to now + 24 * 3_600_000L,
                    Res.string.mail_in_week to now + 7 * 24 * 3_600_000L,
                ).forEachIndexed { i, (label, at) ->
                    SheetRow(stringResource(label), formatDateTime(at), divider = i > 0) { scheduling = false; sendLater(at) }
                }
            }
            HintText(stringResource(Res.string.mail_send_later_hint))
        }
    }
}

/** One line of the compose form (.compose .line): label on the left, line below. */
@Composable
private fun ComposeLine(label: String, focus: androidx.compose.ui.focus.FocusRequester? = null, content: @Composable RowScope.() -> Unit) {
    val t = g
    Column {
        // Tapping the label (or anywhere in the line) puts the cursor into the field.
        val tap = if (focus == null) Modifier else Modifier.clickable(
            interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }, indication = null,
        ) { runCatching { focus.requestFocus() } }
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).then(tap).padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(label, Modifier.width(60.dp), color = t.muted, maxLines = 1)
            content()
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(t.line))
    }
}

@Composable
private fun LineInput(value: String, onValue: (String) -> Unit, placeholder: String, modifier: Modifier, type: KeyboardType = KeyboardType.Text) {
    val t = g
    val style = MaterialTheme.typography.bodyLarge.copy(color = t.fg)
    androidx.compose.foundation.text.BasicTextField(
        value, onValue, modifier, singleLine = true, textStyle = style,
        keyboardOptions = KeyboardOptions(keyboardType = type),
        cursorBrush = androidx.compose.ui.graphics.SolidColor(t.accent),
        decorationBox = { inner -> Box { if (value.isEmpty()) Text(placeholder, style = style.copy(color = t.faint)); inner() } },
    )
}

private fun mimeOf(name: String): String = when (name.substringAfterLast('.', "").lowercase()) {
    "pdf" -> "application/pdf"
    "jpg", "jpeg" -> "image/jpeg"
    "png" -> "image/png"
    "gif" -> "image/gif"
    "txt" -> "text/plain"
    "zip" -> "application/zip"
    "doc" -> "application/msword"
    "docx" -> "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    "xlsx" -> "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    "odt" -> "application/vnd.oasis.opendocument.text"
    else -> "application/octet-stream"
}

/** Add or edit an e-mail account. */
/**
 * Adds or edits an e-mail account in a sheet: the same form in the GhulMail area, its settings
 * page, the account switcher and the setup wizard (like GhulDrive's storage).
 */
@Composable
fun MailAccountSheet(state: AppState, id: String?, onSaved: (String) -> Unit = {}, onDismiss: () -> Unit) {
    GhulSheet(onDismiss, stringResource(if (id == null) Res.string.mail_account_new else Res.string.mail_account_edit)) {
        MailAccountForm(state, id) { newId -> onSaved(newId); onDismiss() }
    }
}

/** The account form: address, password, name, signature and (optionally) the servers. */
@Composable
private fun MailAccountForm(state: AppState, id: String?, onSaved: (String) -> Unit) {
    val net = state.backend.net
    var input by remember {
        mutableStateOf(MailAccountInputUi("", "", "", "", "", MailServersUi("", 993, MailSecurityUi.TLS, "", 465, MailSecurityUi.TLS)))
    }
    var serversTouched by remember { mutableStateOf(id != null) }
    var advanced by remember { mutableStateOf(id != null) }
    // Existing accounts keep their signature; new ones get "Greetings, <name>" until edited.
    var signatureTouched by remember { mutableStateOf(id != null) }
    val defaultSignature = stringResource(Res.string.mail_signature_default)
    var certToConfirm by remember { mutableStateOf<MailCheckUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(id) {
        if (id != null) {
            net.mailAccounts().firstOrNull { it.id == id }?.let { a ->
                input = MailAccountInputUi(a.name, a.displayName, a.email, a.username, "", a.servers, signature = a.signature)
            }
        }
    }

    fun checkAndSave(i: MailAccountInputUi) {
        // Check and save in one step: a second state.run inside onSuccess would be dropped
        // because the first one still counts as running.
        state.run(onError = { error = it }, block = {
            // Editing without a new password keeps the stored one; the check then is skipped.
            val check = if (id != null && i.password.isEmpty()) MailCheckUi.Ok else net.mailCheck(i)
            if (check is MailCheckUi.Ok) check to net.mailSaveAccount(id, i) else check to null
        }) { (check, newId) ->
            if (newId != null) {
                state.mailAccount = newId
                state.changed()
                if (id == null) state.askArchiveWhere()
                onSaved(newId)
            } else {
                certToConfirm = check
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        BusyBanner(state)
        run {
                Hint(stringResource(Res.string.mail_account_text))
                OutlinedTextField(input.email, { v ->
                    input = input.copy(email = v.trim())
                    if (!serversTouched && v.contains('@')) input = input.copy(servers = net.mailSuggest(v.trim()))
                    error = null
                }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(Res.string.mail_email)) },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email))
                SecretTextField(input.password, { input = input.copy(password = it); error = null },
                    stringResource(if (id == null) Res.string.mail_password else Res.string.mail_password_keep))
                Hint(stringResource(Res.string.mail_app_password_hint))
                OutlinedTextField(input.displayName, { v ->
                    // The classic signature with the name, as long as the user has not written one.
                    val sig = if (!signatureTouched && v.isNotBlank()) defaultSignature.replace("%1\$s", v.trim()) else input.signature
                    input = input.copy(displayName = v, signature = sig)
                }, Modifier.fillMaxWidth(), singleLine = true,
                    label = { Text(stringResource(Res.string.mail_display_name)) })
                OutlinedTextField(input.signature, { signatureTouched = true; input = input.copy(signature = it) }, Modifier.fillMaxWidth(), minLines = 2,
                    label = { Text(stringResource(Res.string.mail_signature)) })
                Hint(stringResource(Res.string.mail_signature_hint))
                OutlinedTextField(input.name, { input = input.copy(name = it) }, Modifier.fillMaxWidth(), singleLine = true,
                    label = { Text(stringResource(Res.string.mail_account_name)) })
                TextButton(onClick = { advanced = !advanced }) {
                    Text(stringResource(if (advanced) Res.string.mail_hide_servers else Res.string.mail_show_servers))
                }
                if (advanced) {
                    val s = input.servers
                    fun upd(n: MailServersUi) { serversTouched = true; input = input.copy(servers = n) }
                    OutlinedTextField(input.username, { input = input.copy(username = it.trim()) }, Modifier.fillMaxWidth(), singleLine = true,
                        label = { Text(stringResource(Res.string.mail_username)) })
                    Hint(stringResource(Res.string.mail_username_hint))
                    ServerRow(stringResource(Res.string.mail_imap), s.imapHost, s.imapPort, MailSecurityUi.TLS,
                        { h, p, _ -> upd(s.copy(imapHost = h, imapPort = p, imapSecurity = MailSecurityUi.TLS)) }, 993, null)
                    ServerRow(stringResource(Res.string.mail_smtp), s.smtpHost, s.smtpPort, s.smtpSecurity,
                        { h, p, sec -> upd(s.copy(smtpHost = h, smtpPort = p, smtpSecurity = sec)) }, 465, 587)
                    Hint(stringResource(Res.string.mail_secure_only))
                } else {
                    Hint(stringResource(Res.string.mail_servers_auto, input.servers.imapHost.ifBlank { "-" }, input.servers.smtpHost.ifBlank { "-" }))
                }
                ErrorText(error)
                Button(
                    onClick = { checkAndSave(input) },
                    enabled = !state.busy && input.email.contains('@') && (id != null || input.password.isNotEmpty()) &&
                        input.servers.imapHost.isNotBlank() && input.servers.smtpHost.isNotBlank(),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(Res.string.a_check_and_save)) }
        }
    }
    certToConfirm?.let { c ->
        val fp = when (c) {
            is MailCheckUi.ImapCertificate -> c.fingerprint
            is MailCheckUi.SmtpCertificate -> c.fingerprint
            MailCheckUi.Ok -> ""
        }
        FingerprintDialog(
            stringResource(Res.string.mail_cert_title),
            stringResource(if (c is MailCheckUi.ImapCertificate) Res.string.mail_cert_imap else Res.string.mail_cert_smtp),
            fp,
            onTrust = {
                certToConfirm = null
                input = if (c is MailCheckUi.ImapCertificate) input.copy(imapCert = fp) else input.copy(smtpCert = fp)
                checkAndSave(input)
            },
            onDismiss = { certToConfirm = null },
        )
    }
}

@Composable
private fun ServerRow(
    label: String,
    host: String,
    port: Int,
    security: MailSecurityUi,
    onChange: (String, Int, MailSecurityUi) -> Unit,
    tlsPort: Int,
    /** null: only TLS from the start is allowed (IMAP). */
    startTlsPort: Int?,
) {
    SectionLabel(label)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(host, { onChange(it.trim(), port, security) }, Modifier.weight(3f), singleLine = true,
            label = { Text(stringResource(Res.string.server_host)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
        OutlinedTextField(port.toString(), { v -> onChange(host, v.filter(Char::isDigit).take(5).toIntOrNull() ?: 0, security) },
            Modifier.weight(1f), singleLine = true, label = { Text(stringResource(Res.string.server_port)) },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number))
    }
    if (startTlsPort != null) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(security == MailSecurityUi.TLS, { onChange(host, tlsPort, MailSecurityUi.TLS) }, label = { Text("SSL/TLS") })
            FilterChip(security == MailSecurityUi.STARTTLS, { onChange(host, startTlsPort, MailSecurityUi.STARTTLS) }, label = { Text("STARTTLS") })
        }
    }
}

/** Settings: e-mail accounts and notifications. */
@Composable
fun MailAccountsSection(state: AppState) {
    val net = state.backend.net
    var accounts by remember { mutableStateOf<List<MailAccountUi>>(emptyList()) }
    var removing by remember { mutableStateOf<MailAccountUi?>(null) }
    var notifyTick by remember { mutableStateOf(0) }
    LaunchedEffect(state.liveRevision()) { accounts = runCatching { net.mailAccounts() }.getOrDefault(emptyList()) }
    // Accounts with logins, signatures and keys live in the GhulMail file (all devices).
    // Like GhulDrive's storage: rows that open the form in a sheet, the page stays where it is.
    SectionCard(stringResource(Res.string.mail_accounts_all)) {
        if (accounts.isEmpty()) Hint(stringResource(Res.string.ctx_not_set_up))
        for (a in accounts) {
            ListRow(a.name, a.email, flush = true, divider = a != accounts.first(), leading = { Avatar(a.name.ifBlank { a.email }) },
                trailing = { BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_remove), tint = g.danger) { removing = a } }) {
                state.mailAccountSheet = MailSheet(a.id)
            }
        }
        ListRow(stringResource(Res.string.mail_account_new), stringResource(Res.string.mail_add_sub), flush = true,
            leading = { IconTile(Icons.Filled.Add, areaColor(Tab.MAIL)) }, divider = accounts.isNotEmpty()) { state.mailAccountSheet = MailSheet(null) }
    }
    if (accounts.isNotEmpty()) {
        val settings = state.backend.settings
        SectionCard(areaGroupTitle(VaultKindUi.MAIL)) {
            val senders = settings.mailImageSenders.lines().filter { it.isNotBlank() }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(Res.string.mail_images_always))
                    Hint(stringResource(Res.string.mail_images_always_sub, senders.size.toString()))
                }
                if (senders.isNotEmpty()) TextButton(onClick = { settings.mailImageSenders = "" }) { Text(stringResource(Res.string.a_reset)) }
            }
        }
        MailKeysSection(state, accounts)
        SectionCard(deviceGroupTitle()) {
            if (!state.backend.isDesktop) {
                // Reliable reception: is Android allowed to let mail in while the phone sleeps?
                // Re-read while visible: the user comes back from the system settings without an event.
                var bgTick by remember { mutableStateOf(0) }
                LaunchedEffect(Unit) { while (true) { kotlinx.coroutines.delay(2000); bgTick++ } }
                val bg = remember(notifyTick, bgTick) { net.mailBackgroundAllowed() }
                if (bg == false) {
                    Text(stringResource(Res.string.mail_bg_limited), color = warnColor(), style = MaterialTheme.typography.bodyMedium)
                    OutlinedButton(onClick = { net.openMailBackgroundSettings(); notifyTick++ }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(Res.string.mail_bg_allow))
                    }
                } else if (bg == true) {
                    Text(stringResource(Res.string.mail_bg_ok), color = okColor(), style = MaterialTheme.typography.bodyMedium)
                }
                for (a in accounts) key(notifyTick, a.id) {
                    SwitchRow(stringResource(Res.string.mail_notify_for, a.name), net.mailNotificationsOn(a.id)) { on ->
                        state.run(block = { net.setMailNotifications(a.id, on) }) { notifyTick++ }
                    }
                }
                var detail by remember { mutableStateOf(net.mailNotifyDetail()) }
                ChoiceRow(
                    stringResource(Res.string.mail_notify_detail),
                    listOf(0 to Res.string.mail_notify_detail_0, 1 to Res.string.mail_notify_detail_1, 2 to Res.string.mail_notify_detail_2),
                    detail,
                ) { detail = it; net.setMailNotifyDetail(it) }
                Hint(stringResource(Res.string.mail_notify_detail_hint))
                Hint(stringResource(Res.string.mail_notify_hint))
            }
            var offline by remember { mutableStateOf(net.mailOffline()) }
            SwitchRow(stringResource(Res.string.mail_offline), offline) { on ->
                state.run(block = { net.setMailOffline(on) }) { offline = on }
            }
            Hint(stringResource(Res.string.mail_offline_sub))
            ChoiceRow(
                stringResource(Res.string.mail_swipe_left),
                listOf(0 to Res.string.mail_swipe_archive, 1 to Res.string.mail_swipe_delete, 2 to Res.string.mail_swipe_read),
                settings.mailSwipeLeft,
            ) { settings.mailSwipeLeft = it }
        }
    }
    removing?.let { a ->
        AlertDialog(
            danger = true,
            onDismissRequest = { removing = null },
            title = { Text(stringResource(Res.string.mail_remove_title, a.name)) },
            text = { Text(stringResource(Res.string.mail_remove_text)) },
            confirmButton = {
                TextButton(onClick = {
                    removing = null
                    state.run(block = { net.mailRemoveAccount(a.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_remove)) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}



@Composable
private fun SecurityLine(icon: androidx.compose.ui.graphics.vector.ImageVector, color: androidx.compose.ui.graphics.Color, text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(icon, null, tint = color, modifier = Modifier.size(18.dp))
        Text(text, color = color, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
    }
}


/** Under the recipients: will this mail be encrypted? If not, why not, without jargon. */
@Composable
private fun EncryptionPreview(keys: List<de.danerd.ghulsuite.model.MailRecipientKeyUi>) {
    val missing = keys.filter { !it.hasKey }
    if (keys.isEmpty()) return
    if (missing.isEmpty()) {
        SecurityLine(Icons.Filled.Lock, g.ok, stringResource(Res.string.mail_will_encrypt))
    } else {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            SecurityLine(
                Icons.Filled.LockOpen, g.faint,
                stringResource(Res.string.mail_will_sign_only),
            )
            Hint(stringResource(Res.string.mail_no_key_for, missing.joinToString(", ") { it.address }))
        }
    }
}


/** OpenPGP in plain words: the own key per account, and the keys of contacts. */
@Composable
private fun MailKeysSection(state: AppState, accounts: List<MailAccountUi>) {
    val net = state.backend.net
    var known by remember { mutableStateOf<List<de.danerd.ghulsuite.model.MailKeyInfoUi>>(emptyList()) }
    var forgetting by remember { mutableStateOf<de.danerd.ghulsuite.model.MailKeyInfoUi?>(null) }
    var comparing by remember { mutableStateOf<de.danerd.ghulsuite.model.MailKeyInfoUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision()) { known = runCatching { net.mailKnownKeys() }.getOrDefault(emptyList()) }
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val importedMsg = stringResource(Res.string.mail_key_imported)
    val import = rememberOpenFile { f ->
        state.run(onError = { error = it }, block = { net.mailImportKey(f.bytes) }) { addrs ->
            state.message(importedMsg.replace("%1\$s", addrs.joinToString(", ")))
            state.changed()
        }
    }
    SectionCard(stringResource(Res.string.mail_keys_title)) {
        Hint(stringResource(Res.string.mail_keys_text))
        for (a in accounts) {
            Text(a.email, style = MaterialTheme.typography.titleSmall)
            if (a.pgpFingerprint.isNotBlank()) {
                SelectionContainer {
                    Text(a.pgpFingerprint, fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
            }
            if (filePickersAvailable) {
                TextButton(onClick = {
                    state.run(onError = { error = it }, block = { net.mailPublicKey(a.id) }) { armored ->
                        save(a.email.substringBefore('@') + "-public.asc", armored.encodeToByteArray())
                    }
                }) { Text(stringResource(Res.string.mail_key_save)) }
            }
        }
        SectionLabel(stringResource(Res.string.mail_contact_keys))
        if (known.isEmpty()) Hint(stringResource(Res.string.mail_contact_keys_none))
        for (k in known) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(k.email)
                    Hint(
                        stringResource(keySourceLabel(k.source)) + " · " + k.fingerprint.takeLast(19) +
                            if (k.verified) " · " + stringResource(Res.string.mail_key_verified) else "",
                    )
                    // L7: a changed or a refused key is a warning until the user compared it.
                    if (k.previous.isNotEmpty()) Text(stringResource(Res.string.mail_key_changed), color = warnColor(), style = MaterialTheme.typography.bodySmall)
                    if (k.offered.isNotEmpty()) Text(stringResource(Res.string.mail_key_offered), color = warnColor(), style = MaterialTheme.typography.bodySmall)
                }
                TextButton(onClick = { comparing = k }) { Text(stringResource(Res.string.mail_key_compare)) }
                TextButton(onClick = { forgetting = k }) { Text(stringResource(Res.string.mail_key_forget)) }
            }
        }
        if (filePickersAvailable) {
            OutlinedButton(onClick = import, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.mail_key_import)) }
        }
        ErrorText(error)
    }
    comparing?.let { k ->
        AlertDialog(
            onDismissRequest = { comparing = null },
            title = { Text(stringResource(Res.string.mail_key_compare_title, k.email)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(Res.string.mail_key_compare_text))
                    SelectionContainer {
                        Text(k.fingerprint, fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace)
                    }
                    if (k.offered.isNotEmpty()) {
                        Text(stringResource(Res.string.mail_key_offered_was, k.offered), color = warnColor(), style = MaterialTheme.typography.bodySmall)
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    comparing = null
                    state.run(block = { net.mailVerifyKey(k.email) }) { state.changed() }
                }) { Text(stringResource(Res.string.mail_key_matches)) }
            },
            dismissButton = { TextButton(onClick = { comparing = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    forgetting?.let { k ->
        AlertDialog(
            danger = true,
            onDismissRequest = { forgetting = null },
            title = { Text(stringResource(Res.string.mail_key_forget_title, k.email)) },
            text = { Text(stringResource(Res.string.mail_key_forget_text)) },
            confirmButton = {
                TextButton(onClick = {
                    forgetting = null
                    state.run(block = { net.mailForgetKey(k.email) }) { state.changed() }
                }) { Text(stringResource(Res.string.mail_key_forget)) }
            },
            dismissButton = { TextButton(onClick = { forgetting = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

private fun keySourceLabel(source: String) = when (source) {
    "autocrypt" -> Res.string.mail_key_from_mail
    "wkd" -> Res.string.mail_key_from_provider
    "import" -> Res.string.mail_key_from_import
    else -> Res.string.mail_key_from_mail
}

private fun splitAddresses(s: String) = s.split(',', ';', '\n').map { it.trim() }.filter { it.isNotEmpty() }
