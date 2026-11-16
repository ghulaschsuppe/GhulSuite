// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ReplyAll
import androidx.compose.material.icons.filled.CloudUpload
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Inventory2
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.MailMessageUi
import de.danerd.ghulsuite.model.MailSummaryUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource
import androidx.compose.material3.OutlinedButton

// The mail archive in GhulDrive: copies of mails as .eml files (Mail-Archiv/<account>/<year>/).
// Archiving changes nothing at the provider; the user deletes there with the normal delete.

/** A question before archiving: [what] says which mails, [start] starts the job. */
class ArchiveAsk(val what: String, val start: () -> Unit)

/** Opens where GhulDrive is set up: its area (it shows the setup), else its options page. */
fun openDriveSetup(state: AppState) {
    state.stack.clear()
    if (Tab.DRIVE in state.tabs) state.selectTab(Tab.DRIVE) else state.push(Screen.SettingsPage(SetPage.DRIVE))
}

/**
 * Asks before archiving: copies go to GhulDrive, the mails stay at the provider. Without GhulDrive
 * it explains that the archive needs it and offers to set it up instead.
 */
@Composable
fun ArchiveAskDialog(state: AppState, ask: ArchiveAsk, onDismiss: () -> Unit) {
    val net = state.backend.net
    // Null while GhulDrive is asked; an error (offline) lets the archiving try and say why.
    val ready by produceState<Boolean?>(null) { value = catching { net.driveStatus().serverId != null }.getOrDefault(true) }
    when (ready) {
        null -> {}
        false -> AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(stringResource(Res.string.mail_archive_needs_drive_title)) },
            text = { Text(stringResource(Res.string.mail_archive_needs_drive)) },
            confirmButton = { TextButton(onClick = { onDismiss(); openDriveSetup(state) }) { Text(stringResource(Res.string.mail_archive_setup_drive)) } },
            dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
        )
        true -> AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(stringResource(Res.string.mail_archive_ask_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(ask.what)
                    Text(stringResource(Res.string.mail_archive_ask_text))
                }
            },
            confirmButton = { TextButton(onClick = { onDismiss(); ask.start() }) { Text(stringResource(Res.string.mail_archive_go)) } },
            dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** "Older than" choices: label and months. */
private val OLDER_THAN: List<Pair<StringResource, Int>> = listOf(
    Res.string.mail_archive_1m to 1,
    Res.string.mail_archive_6m to 6,
    Res.string.mail_archive_1y to 12,
    Res.string.mail_archive_2y to 24,
)

/** The first day that is not archived by "older than [months] months" (days since 1970-01-01). */
fun archiveBeforeDays(months: Int): Long = today().plusMonths(-months).days

/**
 * The folder menu of GhulMail (header "⋮"): archive the whole folder, or what is older than a
 * month or more, into GhulDrive. Both ask first.
 */
@Composable
fun MailFolderMenu(state: AppState, accountId: String, folder: String, folderName: String, onDismiss: () -> Unit) {
    val net = state.backend.net
    var older by remember { mutableStateOf(false) }
    var ask by remember { mutableStateOf<ArchiveAsk?>(null) }
    val allText = stringResource(Res.string.mail_archive_ask_folder, folderName)
    val olderTexts = OLDER_THAN.map { (label, months) -> Triple(stringResource(label), months, stringResource(Res.string.mail_archive_ask_older, folderName, stringResource(label))) }
    val hasArchive by produceState<String?>(null, accountId, state.archiveTick) { value = catching { net.archiveFolder(accountId) }.getOrNull() }
    if (ask == null && !older) {
        GhulSheet(onDismiss, folderName) {
            SheetList {
                SheetRow(stringResource(Res.string.mail_archive_folder_all), stringResource(Res.string.mail_archive_folder_sub),
                    leading = { IconTile(Icons.Filled.CloudUpload, areaColor(Tab.DRIVE)) }) {
                    ask = ArchiveAsk(allText) { state.archiveMails(folder) { net.archiveUids(accountId, folder, null).map { accountId to it } } }
                }
                SheetRow(stringResource(Res.string.mail_archive_older), leading = { IconTile(Icons.Filled.History, areaColor(Tab.DRIVE)) }, divider = true) {
                    older = true
                }
                if (hasArchive != null) {
                    SheetRow(stringResource(Res.string.mail_archive_open), leading = { IconTile(Icons.Filled.Inventory2, areaColor(Tab.MAIL)) }, divider = true) {
                        onDismiss()
                        state.archivePath = emptyList()
                        state.push(Screen.MailArchive(accountId))
                    }
                }
            }
            HintText(stringResource(Res.string.mail_archive_menu_hint))
        }
    }
    if (older) {
        GhulSheet({ older = false }, stringResource(Res.string.mail_archive_older)) {
            SheetList {
                olderTexts.forEachIndexed { i, (label, months, what) ->
                    SheetRow(label, divider = i > 0) {
                        older = false
                        val before = archiveBeforeDays(months)
                        ask = ArchiveAsk(what) { state.archiveMails(folder) { net.archiveUids(accountId, folder, before).map { accountId to it } } }
                    }
                }
            }
        }
    }
    ask?.let { a -> ArchiveAskDialog(state, a) { ask = null; onDismiss() } }
}

/** What the name of an archived mail says: `2024-05-03 Erika Musterfrau - Treffen (1a2b3c4d).eml`. */
data class ArchiveName(val day: Day?, val sender: String, val subject: String)

/** Reads the name of an archived mail; null for a file that is not a mail (.eml). */
fun parseArchiveName(name: String): ArchiveName? {
    if (!name.endsWith(".eml", ignoreCase = true)) return null
    var base = name.dropLast(4).trim()
    // The short code the core puts at the end ("(1a2b3c4d)").
    Regex("""\s*\([0-9a-fA-F]{6,16}\)$""").find(base)?.let { base = base.substring(0, it.range.first) }
    val day = if (base.length >= 10) Day.parse(base.substring(0, 10)) else null
    val rest = if (day != null) base.substring(10).trim() else base
    return when {
        day == null -> ArchiveName(null, "", rest)
        rest.startsWith("- ") -> ArchiveName(day, "", rest.drop(2).trim())
        rest == "-" -> ArchiveName(day, "", "")
        rest.contains(" - ") -> ArchiveName(day, rest.substringBefore(" - ").trim(), rest.substringAfter(" - ").trim())
        else -> ArchiveName(day, rest, "")
    }
}

/**
 * The archive of an account in GhulDrive: its year folders, inside them the archived mails (date,
 * sender and subject from the file name). Other files are left out; folders the user made show as
 * folders. A mail opens in the reader.
 */
@Composable
fun MailArchiveScreen(state: AppState, accountId: String, padding: PaddingValues) {
    val net = state.backend.net
    var root by remember { mutableStateOf<String?>(null) }
    var path by state::archivePath
    var items by remember { mutableStateOf<List<DriveItemUi>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    var fetched by remember { mutableStateOf(false) }
    val here = path.lastOrNull()?.first
    val accountName by produceState("", accountId) { value = catching { net.mailAccounts().firstOrNull { it.id == accountId }?.name }.getOrNull().orEmpty() }
    /** The archive lies on another storage location than the one GhulDrive shows now. */
    var elsewhere by remember { mutableStateOf<de.danerd.ghulsuite.model.ServerUi?>(null) }
    var archiveAt by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(here, retry, state.archiveTick, state.driveTick, state.liveRevision()) {
        error = null
        val locs = catching { net.driveLocations() }.getOrNull().orEmpty()
        elsewhere = locs.firstOrNull { it.archive && !it.active }?.server
        // The archive's location changed: its folders are other ones.
        val at = locs.firstOrNull { it.archive }?.server?.id
        if (at != archiveAt) {
            if (archiveAt != null) { root = null; path = emptyList() }
            archiveAt = at
        }
        if (elsewhere != null) { items = emptyList(); return@LaunchedEffect }
        catching {
            val top = root ?: net.archiveFolder(accountId).also { root = it }
            if (top == null) emptyList() else net.driveList(here ?: top, !fetched).also { fetched = true }
        }.onSuccess { items = it }.onFailure { error = describe(it) }
    }
    PlatformBackHandler(enabled = path.isNotEmpty()) { path = path.dropLast(1); items = null }
    val noSender = stringResource(Res.string.mail_archive_no_sender)
    Column(Modifier.padding(padding)) {
        GhulTopBar(
            stringResource(Res.string.mail_archive_entry), onLock = { state.lockNow() },
            onBack = { if (path.isNotEmpty()) { path = path.dropLast(1); items = null } else state.pop() }, backIcon = backIcon(),
            context = {
                val where = (listOf(accountName) + path.map { it.second }).filter { it.isNotEmpty() }.joinToString(" › ")
                if (where.isNotEmpty()) Text(where, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = areaColor(Tab.MAIL), maxLines = 1, overflow = TextOverflow.Ellipsis)
            },
        )
        ScreenColumn {
            LoadError(error) { retry++ }
            elsewhere?.let { srv ->
                // One location is shown at a time (GhulDrive's active one); the archive is read where it is.
                HintText(stringResource(Res.string.mail_archive_elsewhere, srv.name))
                val switched = stringResource(Res.string.drive_switched, srv.name)
                OutlinedButton(onClick = {
                    state.run(onError = { error = it }, block = { net.driveSwitchLocation(srv.id) }) {
                        root = null
                        path = emptyList()
                        state.message(switched)
                        state.changed()
                    }
                }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.drive_location_use_named, srv.name)) }
                return@ScreenColumn
            }
            val list = items
            if (list == null && error == null) LoadingHint()
            if (list != null) {
                val folders = list.filter { it.isDir }.sortedByDescending { it.name }
                val mails = list.filter { !it.isDir }.mapNotNull { f -> parseArchiveName(f.name)?.let { f to it } }
                    .sortedWith(compareByDescending<Pair<DriveItemUi, ArchiveName>> { it.second.day?.days ?: Long.MIN_VALUE }.thenBy { it.first.name })
                if (path.isEmpty()) HintText(stringResource(Res.string.mail_archive_view_hint))
                if (folders.isEmpty() && mails.isEmpty() && error == null) {
                    EmptyState(Icons.Filled.Inventory2, stringResource(Res.string.mail_archive_empty), areaColor(Tab.MAIL))
                }
                if (folders.isNotEmpty()) {
                    ListCard {
                        folders.forEachIndexed { i, f ->
                            ListRow(f.name, leading = { IconTile(Icons.Filled.Folder, paletteColor(1)) }, trailing = { ChevronEnd() }, divider = i > 0) {
                                path = path + (f.id to f.name)
                                items = null
                            }
                        }
                    }
                }
                if (mails.isNotEmpty()) {
                    val unreadable = stringResource(Res.string.mail_archive_unreadable)
                    val today = formatDate(nowSeconds() * 1000)
                    SectionLabel(pluralStringResourceSafe(mails.size))
                    ListCard {
                        mails.forEachIndexed { i, (f, n) ->
                            // Noon of the day: the date stays the same in every time zone.
                            val date = n.day?.let { it.days * 86_400 + 43_200 } ?: 0L
                            val summary = MailSummaryUi(
                                0, n.sender.ifEmpty { noSender }, "", n.subject, date, true, false, false, f.size,
                                preview = if (f.id in state.archiveUnreadable) unreadable else formatSize(f.size),
                            )
                            MailRow(summary, today, "", divider = i > 0) {
                                state.push(Screen.MailArchiveRead(accountId, f.id, f.name))
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun pluralStringResourceSafe(n: Int): String =
    org.jetbrains.compose.resources.pluralStringResource(Res.plurals.mail_archive_count, n, n.toString())

/**
 * An archived mail, read from GhulDrive and shown like any mail: reply and forward work, the
 * attachments can be saved. Delete moves the file to GhulDrive's trash (after the usual question).
 * A file that cannot be read as a mail says so and can still be saved or deleted.
 */
@Composable
fun MailArchiveReadScreen(state: AppState, accountId: String, fileId: String, name: String, padding: PaddingValues) {
    val net = state.backend.net
    var data by remember { mutableStateOf<ByteArray?>(null) }
    var message by remember { mutableStateOf<MailMessageUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var unreadable by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(fileId) {
        val bytes = catching { MemorySink().also { net.driveDownload(fileId, it) {} }.bytes() }
            .onFailure { error = describe(it) }.getOrNull() ?: return@LaunchedEffect
        data = bytes
        catching { net.mailParseEml(accountId, bytes) }
            .onSuccess { message = it }
            .onFailure {
                unreadable = true
                state.archiveUnreadable = state.archiveUnreadable + fileId
            }
    }
    val savedMsg = stringResource(Res.string.msg_saved_file)
    val save = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
    val replies = rememberMailReplies(accountId)
    val accountName by produceState("", accountId) { value = catching { net.mailAccounts().firstOrNull { it.id == accountId }?.name }.getOrNull().orEmpty() }
    val trashMsg = stringResource(Res.string.msg_moved_to_trash)
    val deleteExtra = stringResource(Res.string.mail_archive_delete_extra)
    val m0 = message
    fun delete() {
        state.askDelete(listOf(m0?.subject?.ifBlank { null } ?: name), trash = true, extra = deleteExtra) {
            scope.launch {
                catching { net.driveDelete(fileId) }
                    .onSuccess {
                        state.pop()
                        state.archiveTick++
                        state.doneWithUndo(trashMsg, undo = { net.driveRestore(fileId) }) { state.archiveTick++ }
                    }
                    .onFailure { error = describe(it) }
            }
        }
    }
    Column(Modifier.padding(padding)) {
        GhulTopBar("", onLock = { state.lockNow() }, onBack = { state.pop() }, backIcon = backIcon(), actions = {
            BarIcon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = g.danger) { delete() }
            BarIcon(Icons.Filled.MoreVert, stringResource(Res.string.entry_more)) { more = true }
        })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 20.dp)) {
            BusyBanner(state)
            ErrorText(error)
            if (data == null && error == null) LoadingHint()
            if (unreadable) {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Banner(Icons.Filled.ErrorOutline, stringResource(Res.string.mail_archive_unreadable))
                    Text(name, fontWeight = FontWeight.SemiBold)
                    if (filePickersAvailable) {
                        OutlinedButton(onClick = { data?.let { save(name, it) } }, modifier = Modifier.fillMaxWidth()) {
                            Icon(Icons.Filled.Download, null, Modifier.size(18.dp)); Text(stringResource(Res.string.mail_save_eml))
                        }
                    }
                }
            }
            val m = m0 ?: return@Column
            MailMessageBody(state, m, accountName, emptyList(), archived = true) { a ->
                val bytes = data ?: return@MailMessageBody
                state.run(onError = { error = it }, block = { net.mailEmlAttachment(accountId, bytes, a.index) }) { save(a.name, it) }
            }
        }
        val m = m0 ?: return@Column
        MailReplyBar(state, m, replies)
    }
    if (more) {
        GhulSheet({ more = false }, m0?.subject?.ifBlank { null } ?: name) {
            SheetList {
                val rm = m0
                var first = true
                if (rm != null && (rm.to + rm.cc).size > 1) {
                    SheetRow(stringResource(Res.string.a_reply_all), leading = { IconTile(Icons.AutoMirrored.Filled.ReplyAll, paletteColor(1)) }) {
                        more = false
                        state.push(Screen.MailCompose(replies.replyAll(rm)))
                    }
                    first = false
                }
                if (filePickersAvailable) {
                    SheetRow(stringResource(Res.string.mail_save_eml), leading = { IconTile(Icons.Filled.Download, paletteColor(9)) }, divider = !first) {
                        more = false
                        data?.let { save(name, it) }
                    }
                    first = false
                    if (rm != null) {
                        SheetRow(stringResource(Res.string.mail_save_pdf), leading = { IconTile(Icons.Filled.PictureAsPdf, paletteColor(7)) }, divider = true) {
                            more = false
                            save(fileBase(rm.subject) + ".pdf", textPdf(rm.subject, mailAsText(rm)))
                        }
                    }
                }
                SheetRow(stringResource(Res.string.mail_archive_delete), color = g.danger, divider = !first,
                    leading = { IconTile(Icons.Filled.Delete, g.danger) }) {
                    more = false
                    delete()
                }
            }
        }
    }
}

/**
 * Settings → GhulMail: the GhulDrive storage location the mail archive goes to. Fixed until it
 * is changed here, also while GhulDrive shows another location.
 */
@Composable
fun MailArchiveLocationSection(state: AppState) {
    val net = state.backend.net
    var locations by remember { mutableStateOf<List<de.danerd.ghulsuite.model.DriveLocationUi>>(emptyList()) }
    var choosing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision()) { locations = catching { net.driveLocations() }.getOrNull().orEmpty() }
    if (locations.isEmpty()) return
    val current = locations.firstOrNull { it.archive } ?: locations.firstOrNull { it.active }
    SectionCard(stringResource(Res.string.mail_archive_location)) {
        ListRow(
            current?.server?.name.orEmpty(), current?.server?.let { it.location.ifBlank { "~/" + it.base } + "/drive" },
            leading = { IconTile(Icons.Filled.Inventory2, areaColor(Tab.DRIVE)) },
            trailing = if (locations.size > 1) ({ ChevronEnd() }) else null,
            flush = true,
        ) { if (locations.size > 1) choosing = true }
        Hint(stringResource(Res.string.mail_archive_location_hint))
        ErrorText(error)
    }
    if (choosing) {
        GhulSheet({ choosing = false }, stringResource(Res.string.mail_archive_location)) {
            SheetList {
                locations.forEachIndexed { i, l ->
                    val srv = l.server
                    PickRow(l.server.id == current?.server?.id, Icons.Filled.Inventory2, 1, srv.name, srv.location.ifBlank { "~/" + srv.base } + "/drive", divider = i > 0, onClick = {
                        choosing = false
                        state.run(onError = { error = it }, block = { net.driveSetArchiveLocation(srv.id) }) { state.changed() }
                    })
                }
            }
        }
    }
}
