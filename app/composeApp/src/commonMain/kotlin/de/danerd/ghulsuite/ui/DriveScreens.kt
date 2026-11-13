// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.DeleteForever
import androidx.compose.material.icons.filled.History
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CreateNewFolder
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.FolderZip
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.material3.Surface
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.filled.Storage
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.runtime.DisposableEffect
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.DriveStatusUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.stringResource
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Sync
import de.danerd.ghulsuite.model.DriveModeUi
import de.danerd.ghulsuite.model.DriveStorageUi

/** A running upload or download. */
private class Transfer(val name: String, val total: Long, val upload: Boolean) {
    var done by mutableStateOf(0L)
    var job: Job? = null
}

/** Folder path from the root: (id, name). */
private data class Crumb(val id: String?, val name: String)

@Composable
fun DriveScreen(state: AppState, padding: PaddingValues) {
    val net = state.backend.net
    var status by remember { mutableStateOf<DriveStatusUi?>(null) }
    var loadError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision()) {
        runCatching { net.driveStatus() }
            .onSuccess { status = it; loadError = null }
            .onFailure { loadError = describe(it) }
    }
    Column(Modifier.padding(padding)) {
        val s = status
        when {
            s == null -> ScreenColumn {
                BusyBanner(state)
                ErrorText(loadError)
            }
            s.serverId == null -> DriveSetup(state)
            else -> DriveBrowser(state, s)
        }
    }
}

/** GhulDrive without storage yet: where the files go, set up right here (#24). */
@Composable
private fun DriveSetup(state: AppState) {
    val net = state.backend.net
    var storage by remember { mutableStateOf<DriveStorageUi?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var own by remember { mutableStateOf(false) }
    LaunchedEffect(state.liveRevision()) { storage = runCatching { net.driveStorage() }.getOrNull() }
    val syncOn = storage?.syncAvailable == true
    ScreenColumn {
        BusyBanner(state)
        AreaSetupCard(Icons.Filled.Cloud, 1, stringResource(Res.string.drive_setup_title), stringResource(Res.string.drive_setup_text))
        if (storage?.mode == DriveModeUi.SYNC) Banner(Icons.Filled.Info, stringResource(Res.string.drive_sync_gone))
        SectionLabel(stringResource(Res.string.wz_drive_where))
        ListCard {
            if (syncOn) {
                ListRow(stringResource(Res.string.drive_like_sync), stringResource(Res.string.drive_like_sync_sub),
                    leading = { IconTile(Icons.Filled.Sync, paletteColor(2)) }) {
                    state.run(onError = { error = it }, block = { net.driveUseSync() }) { state.changed() }
                }
            }
            ListRow(stringResource(Res.string.wz_own_server), stringResource(Res.string.drive_own_sub),
                leading = { IconTile(Icons.Filled.Dns, paletteColor(1)) }, divider = syncOn) { own = true }
        }
        if (!syncOn) HintText(stringResource(Res.string.drive_sync_tip))
        ErrorText(error)
    }
    if (own) DriveServerSheet(state) { own = false }
}

/** Sheet with the server form for GhulDrive's own server (stored in the Drive file). */
@Composable
fun DriveServerSheet(state: AppState, onDone: () -> Unit) {
    val net = state.backend.net
    val form = rememberServerForm()
    val done = stringResource(Res.string.drive_set_up)
    GhulSheet(onDone, stringResource(Res.string.drive_own_title)) {
        ServerSetup(state, form, { input, fp -> net.driveSaveServer(input, fp) }) {
            state.message(done)
            state.changed()
            onDone()
        }
    }
}

@Composable
private fun DriveBrowser(state: AppState, status: DriveStatusUi) {
    val net = state.backend.net
    val scope = rememberCoroutineScope()
    val root = stringResource(Res.string.drive_root)
    var path by remember { mutableStateOf(listOf(Crumb(null, root))) }
    val folder = path.last().id
    // Back goes up one folder first (only while Drive is the visible tab).
    PlatformBackHandler(enabled = LocalTabActive.current && path.size > 1) { path = path.dropLast(1) }
    var items by remember { mutableStateOf<List<DriveItemUi>>(emptyList()) }
    var query by remember { mutableStateOf("") }
    var reload by remember { mutableStateOf(0) }
    var refresh by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var transfer by remember { mutableStateOf<Transfer?>(null) }
    var newFolder by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<DriveItemUi?>(null) }
    var moving by remember { mutableStateOf<DriveItemUi?>(null) }
    var deleting by remember { mutableStateOf<DriveItemUi?>(null) }
    var downloadItem by remember { mutableStateOf<DriveItemUi?>(null) }
    var selecting by remember { mutableStateOf(false) }
    val selected = remember { androidx.compose.runtime.mutableStateListOf<DriveItemUi>() }
    var sharing by remember { mutableStateOf<List<DriveItemUi>?>(null) }
    var deletingMany by remember { mutableStateOf(false) }
    var driveServer by remember { mutableStateOf<ServerUi?>(null) }
    /** "" = folders, or one of the quick views: recent, fav, offline, trash. */
    var view by remember { mutableStateOf("") }
    var usage by remember { mutableStateOf<de.danerd.ghulsuite.model.DriveUsageUi?>(null) }
    var versionsOf by remember { mutableStateOf<DriveItemUi?>(null) }
    var zipping by remember { mutableStateOf<List<DriveItemUi>?>(null) }
    var trashTick by remember { mutableStateOf(0) }
    var emptyTrash by remember { mutableStateOf(false) }
    PlatformBackHandler(enabled = LocalTabActive.current && view.isNotEmpty() && path.size == 1) { view = "" }
    LaunchedEffect(status.serverId) {
        driveServer = runCatching { net.servers() }.getOrDefault(emptyList()).firstOrNull { it.id == status.serverId }
    }

    LaunchedEffect(folder, query, reload, view, trashTick, state.liveRevision()) {
        runCatching {
            when {
                query.isNotBlank() -> net.driveSearch(query)
                view == "recent" -> net.driveRecent(30)
                view == "fav" -> net.driveFavorites()
                view == "offline" -> net.driveOfflineFiles()
                view == "trash" -> net.driveTrash()
                else -> net.driveList(folder, refresh)
            }
        }.onSuccess { items = it; error = null }.onFailure { error = describe(it) }
        refresh = false
        usage = runCatching { net.driveUsage() }.getOrNull()
    }
    // Empties the trash of old items once per session (setting: after 30 days).
    LaunchedEffect(Unit) {
        val days = state.backend.settings.driveTrashDays
        if (days > 0) runCatching { net.driveEmptyTrash(days) }.onSuccess { if (it > 0) reload++ }
    }

    val uploadedMsg = stringResource(Res.string.drive_uploaded)
    val downloadedMsg = stringResource(Res.string.drive_downloaded)
    fun runTransfer(t: Transfer, block: suspend () -> Unit, done: String) {
        transfer = t
        t.job = scope.launch {
            try {
                block()
                state.message(done.replace("%1\$s", t.name))
                reload++
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = describe(e)
            } finally {
                transfer = null
            }
        }
    }
    val upload = rememberOpenStream { source: ByteSource ->
        val t = Transfer(source.name, source.size, upload = true)
        runTransfer(t, { net.driveUpload(folder, source) { t.done = it } }, uploadedMsg)
    }
    val download = rememberCreateStream { sink: ByteSink? ->
        val item = downloadItem
        downloadItem = null
        if (sink == null || item == null) return@rememberCreateStream
        val t = Transfer(item.name, item.size, upload = false)
        runTransfer(t, { net.driveDownload(item.id, sink) { t.done = it } }, downloadedMsg)
    }

    // The "+" button of the main screen opens the "add" sheet.
    DisposableEffect(Unit) {
        state.driveReady = true
        onDispose { state.driveReady = false; state.driveSelection = null }
    }
    var addSheet by remember { mutableStateOf(false) }
    var fileSheet by remember { mutableStateOf<DriveItemUi?>(null) }
    val uploadTick = state.driveUploadTick
    var seenTick by remember { mutableStateOf(uploadTick) }
    LaunchedEffect(uploadTick) {
        if (uploadTick != seenTick) {
            seenTick = uploadTick
            addSheet = true
        }
    }
    val scanFailed = stringResource(Res.string.drive_scan_failed)
    val scan = rememberTakePhoto { jpeg ->
        if (jpeg == null) return@rememberTakePhoto
        val pdf = jpegPdf(jpeg)
        if (pdf == null) { state.message(scanFailed); return@rememberTakePhoto }
        val name = "Scan " + formatDateTime(nowSeconds()).replace(':', '-') + ".pdf"
        var pos = 0
        val src = object : ByteSource {
            override val name = name
            override val size = pdf.size.toLong()
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                if (pos >= pdf.size) return -1
                val n = minOf(length, pdf.size - pos); pdf.copyInto(buffer, offset, pos, pos + n); pos += n; return n
            }
            override fun close() {}
        }
        val t = Transfer(name, pdf.size.toLong(), upload = true)
        runTransfer(t, { net.driveUpload(folder, src) { t.done = it } }, uploadedMsg)
    }
    var sharesActive by remember { mutableStateOf(0) }
    LaunchedEffect(state.liveRevision()) {
        sharesActive = runCatching { net.shareList().size }.getOrDefault(0)
    }
    // Selection: the bottom navigation turns into the selection bar.
    LaunchedEffect(selecting, selected.size) {
        state.driveSelection = if (!selecting) null else DriveSelection(
            count = selected.size,
            onShare = { sharing = selected.toList() },
            onDelete = { deletingMany = true },
            onAll = { selected.clear(); selected.addAll(items) },
            onDone = { selecting = false; selected.clear() },
            onZip = { zipping = selected.toList() },
        )
    }
    val color = areaColor(Tab.DRIVE)
    val t = g
    fun open(item: DriveItemUi) {
        if (item.isDir) {
            query = ""
            path = if (item.parent == folder) path + Crumb(item.id, item.name) else listOf(path.first(), Crumb(item.id, item.name))
        } else {
            fileSheet = item
        }
    }
    fun toggle(item: DriveItemUi) {
        if (!selecting) selecting = true
        if (item in selected) selected.remove(item) else selected.add(item)
    }

    ScreenColumn {
        BusyBanner(state)
        SearchField(query, { query = it }, placeholder = stringResource(Res.string.drive_search))
        if (path.size == 1 && query.isBlank() && !selecting) {
            // Where the files are (.card.storage).
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    IconTile(Icons.Filled.Dns, paletteColor(1))
                    Column(Modifier.weight(1f)) {
                        Text(driveServer?.name ?: "…", fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            listOfNotNull(driveServer?.let { it.location.ifBlank { "~/" + it.base } + "/drive" }, stringResource(Res.string.drive_encrypted)).joinToString(" · "),
                            fontSize = 13.sp, color = t.muted,
                        )
                    }
                    if (error == null) Box(Modifier.clickable { refresh = true; reload++ }) { Tag(stringResource(Res.string.drive_current), t.ok, Icons.Filled.Check) }
                }
                usage?.let { u ->
                    val total = (u.images + u.videos + u.audio + u.documents + u.other + u.trash + u.versions).coerceAtLeast(1)
                    Row(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).background(t.surface3)) {
                        for ((v, c) in listOf(u.images to color, u.videos to areaColor(Tab.PASS), u.documents to areaColor(Tab.MAIL), u.audio to paletteColor(6),
                            u.other + u.trash + u.versions to t.faint)) {
                            if (v > 0) Box(Modifier.weight(v.toFloat() / total).fillMaxHeight().background(c))
                        }
                    }
                    Text(
                        listOfNotNull(
                            stringResource(Res.string.drive_status, status.files.toString(), formatSize(status.bytes)),
                            if (u.images > 0) stringResource(Res.string.drive_use_images, formatSize(u.images)) else null,
                            if (u.videos > 0) stringResource(Res.string.drive_use_videos, formatSize(u.videos)) else null,
                            if (u.documents > 0) stringResource(Res.string.drive_use_docs, formatSize(u.documents)) else null,
                            if (u.trash > 0) stringResource(Res.string.drive_use_trash, formatSize(u.trash)) else null,
                        ).joinToString(" · "),
                        fontSize = 13.sp, color = t.muted,
                    )
                } ?: Text(stringResource(Res.string.drive_status, status.files.toString(), formatSize(status.bytes)), fontSize = 13.sp, color = t.muted)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for ((v, ic, label) in listOf(
                    Triple("recent", Icons.Filled.Schedule, Res.string.drive_view_recent),
                    Triple("fav", Icons.Filled.StarBorder, Res.string.chip_favorites),
                    Triple("offline", Icons.Filled.Download, Res.string.drive_view_offline),
                    Triple("trash", Icons.Filled.Delete, Res.string.drive_view_trash),
                )) {
                    QuickTile(ic, stringResource(label), Modifier.weight(1f), on = view == v) { view = if (view == v) "" else v; selecting = false; selected.clear() }
                }
            }
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).clickable { state.push(Screen.Shares) }
                    .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                IconTile(Icons.Filled.Link, paletteColor(5))
                Column(Modifier.weight(1f)) {
                    Text(stringResource(Res.string.shares_title), fontWeight = FontWeight.W500)
                    Text(stringResource(Res.string.drive_shares_active, sharesActive.toString()), fontSize = 14.sp, color = t.muted)
                }
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp), tint = t.faint)
            }
        } else if (query.isBlank()) {
            HintText(path.joinToString(" › ") { it.name })
        }
        transfer?.let { tr ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                IconTile(if (tr.upload) Icons.Filled.Upload else Icons.Filled.Download, paletteColor(4))
                Column(Modifier.weight(1f)) {
                    Text(stringResource(if (tr.upload) Res.string.drive_uploading else Res.string.drive_downloading, tr.name),
                        fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Box(Modifier.padding(top = 6.dp).fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(t.surface3)) {
                        val f = if (tr.total > 0) (tr.done.toFloat() / tr.total).coerceIn(0f, 1f) else 0f
                        Box(Modifier.fillMaxWidth(f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(color))
                    }
                    Text(formatSize(tr.done) + if (tr.total > 0) " / " + formatSize(tr.total) else "", fontSize = 12.sp, color = t.muted)
                }
                BarIcon(Icons.Filled.Close, stringResource(Res.string.a_cancel)) { tr.job?.cancel(); transfer = null }
            }
        }
        ErrorText(error)
        SectionLabel(
            when {
                query.isNotBlank() -> stringResource(Res.string.drive_search_results)
                view == "recent" -> stringResource(Res.string.drive_view_recent)
                view == "fav" -> stringResource(Res.string.chip_favorites)
                view == "offline" -> stringResource(Res.string.drive_view_offline)
                view == "trash" -> stringResource(Res.string.drive_view_trash)
                path.size == 1 -> stringResource(Res.string.drive_my_files)
                else -> stringResource(Res.string.drive_n_files, items.size.toString())
            },
            trailing = {
                if (view == "trash") {
                    if (items.isNotEmpty()) Text(stringResource(Res.string.drive_empty_trash), Modifier.clickable { emptyTrash = true })
                } else {
                    Text(stringResource(if (selecting) Res.string.a_done else Res.string.a_select),
                        Modifier.clickable { selecting = !selecting; selected.clear() })
                }
            },
        )
        if (view == "trash") HintText(stringResource(Res.string.drive_trash_hint, state.backend.settings.driveTrashDays.toString()))
        if (view == "offline") HintText(stringResource(Res.string.drive_offline_hint))
        if (items.isEmpty() && error == null) {
            EmptyState(Icons.Filled.Cloud, stringResource(if (query.isBlank()) Res.string.drive_empty else Res.string.drive_nothing_found), color)
        }
        if (items.isNotEmpty() && state.driveGrid && !selecting) {
            val cols = LocalColumns.current
            for (pair in items.chunked(cols)) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    for (item in pair) DriveTile(item, Modifier.weight(1f)) { open(item) }
                    repeat(cols - pair.size) { Box(Modifier.weight(1f)) }
                }
            }
        } else if (items.isNotEmpty()) {
            ListCard {
                items.forEachIndexed { i, item ->
                    DriveRow(item, selecting, item in selected, divider = i > 0, onSelect = { toggle(item) }, onOpen = { if (selecting) toggle(item) else open(item) })
                }
            }
        }
        HintText(stringResource(Res.string.drive_privacy))
    }

    if (addSheet) {
        GhulSheet({ addSheet = false }, stringResource(Res.string.drive_add_title)) {
            SheetList {
                SheetRow(stringResource(Res.string.drive_upload_files), leading = { IconTile(Icons.Filled.Upload, paletteColor(1)) }, enabled = filePickersAvailable && transfer == null) {
                    addSheet = false; upload()
                }
                if (cameraPhotoAvailable) {
                    SheetRow(stringResource(Res.string.drive_scan), stringResource(Res.string.drive_scan_sub), leading = { IconTile(Icons.Filled.PhotoCamera, paletteColor(2)) }, divider = true) {
                        addSheet = false; scan()
                    }
                }
                SheetRow(stringResource(Res.string.drive_new_folder_short), leading = { IconTile(Icons.Filled.Folder, paletteColor(9)) }, divider = true) {
                    addSheet = false; newFolder = true
                }
            }
        }
    }
    fileSheet?.let { item ->
        GhulSheet({ fileSheet = null }) {
            val (ic, c) = if (item.isDir) Icons.Filled.Folder to paletteColor(1) else fileKind(item.name)
            Box(Modifier.fillMaxWidth().height(220.dp).clip(RoundedCornerShape(18.dp)).background(c.copy(alpha = 0.14f)), contentAlignment = Alignment.Center) {
                Icon(ic, null, Modifier.size(64.dp), tint = c)
            }
            SheetTitle(item.name)
            HintText(formatSize(item.size) + " · " + stringResource(Res.string.drive_changed, formatDate(item.modified)) + " · " + stringResource(Res.string.drive_encrypted_blocks), Modifier.padding(start = 0.dp))
            ButtonRow {
                Button(onClick = { fileSheet = null; state.run(onError = { error = it }, block = { net.shareDriveToApps(listOf(item)) }) {} }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Filled.Visibility, null, Modifier.size(18.dp)); Text(stringResource(Res.string.a_open))
                }
                OutlinedButton(onClick = { fileSheet = null; downloadItem = item; download(item.name, "application/octet-stream") }, modifier = Modifier.weight(1f),
                    enabled = filePickersAvailable) {
                    Icon(Icons.Filled.Download, null, Modifier.size(18.dp)); Text(stringResource(Res.string.a_save))
                }
            }
            @Composable
            fun End(icon: androidx.compose.ui.graphics.vector.ImageVector) = Icon(icon, null, Modifier.size(20.dp), tint = t.muted)
            if (item.trashed != 0L) SheetList {
                SheetRow(stringResource(Res.string.drive_restore), end = { End(Icons.Filled.Restore) }) {
                    fileSheet = null
                    state.run(onError = { error = it }, block = { net.driveRestore(item.id) }) { trashTick++; state.changed() }
                }
                SheetRow(stringResource(Res.string.drive_delete_forever), color = t.danger, divider = true, end = { End(Icons.Filled.DeleteForever) }) {
                    fileSheet = null
                    state.run(onError = { error = it }, block = { net.driveDeleteForever(item.id) }) { trashTick++; state.changed() }
                }
            } else SheetList {
                SheetRow(stringResource(if (item.offline) Res.string.drive_offline_off else Res.string.drive_offline_on), end = { End(Icons.Filled.Download) }) {
                    fileSheet = null
                    state.run(onError = { error = it }, block = { net.driveSetOffline(item.id, !item.offline) }) { reload++ }
                }
                SheetRow(stringResource(if (item.favorite) Res.string.auth_unfavorite else Res.string.auth_favorite), divider = true,
                    end = { End(if (item.favorite) Icons.Filled.Star else Icons.Filled.StarBorder) }) {
                    fileSheet = null
                    state.run(onError = { error = it }, block = { net.driveSetFavorite(item.id, !item.favorite) }) { reload++ }
                }
                SheetRow(stringResource(Res.string.a_share_dots), divider = true, end = { End(Icons.AutoMirrored.Filled.Send) }) { fileSheet = null; sharing = listOf(item) }
                if (item.versions > 0) {
                    SheetRow(stringResource(Res.string.drive_versions), item.versions.toString(), divider = true, end = { End(Icons.Filled.History) }) { fileSheet = null; versionsOf = item }
                }
                SheetRow(stringResource(Res.string.a_rename), divider = true, end = { End(Icons.Filled.Edit) }) { fileSheet = null; renaming = item }
                SheetRow(stringResource(Res.string.a_move), divider = true, end = { End(Icons.Filled.Folder) }) { fileSheet = null; moving = item }
                SheetRow(stringResource(Res.string.drive_to_trash), color = t.danger, divider = true, end = { End(Icons.Filled.Delete) }) {
                    fileSheet = null
                    state.run(onError = { error = it }, block = { net.driveDelete(item.id) }) { state.changed() }
                }
            }
        }
    }

    sharing?.let { files ->
        ShareSheet(state, files, onDismiss = { sharing = null }, onDone = { sharing = null; selecting = false; selected.clear(); reload++ },
            onZip = { sharing = null; zipping = files })
    }
    versionsOf?.let { item ->
        var list by remember(item.id) { mutableStateOf<List<de.danerd.ghulsuite.model.DriveVersionUi>>(emptyList()) }
        LaunchedEffect(item.id) { list = runCatching { net.driveVersions(item.id) }.getOrDefault(emptyList()) }
        GhulSheet({ versionsOf = null }, stringResource(Res.string.drive_versions)) {
            HintText(item.name)
            SheetList {
                list.forEachIndexed { i, v ->
                    SheetRow(formatDateTime(v.modified), formatSize(v.size), divider = i > 0, end = {
                        TextButton(onClick = {
                            versionsOf = null
                            state.run(onError = { error = it }, block = { net.driveRestoreVersion(item.id, v.index) }) { reload++ }
                        }) { Text(stringResource(Res.string.drive_restore)) }
                    })
                }
            }
            HintText(stringResource(Res.string.drive_versions_hint))
        }
    }
    zipping?.let { files ->
        var pw by remember { mutableStateOf("") }
        var zipError by remember { mutableStateOf<String?>(null) }
        val savedMsg = stringResource(Res.string.msg_saved_file)
        val saveZip = rememberSaveFile { ok -> if (ok) state.message(savedMsg) }
        GhulSheet({ zipping = null }, stringResource(Res.string.drive_zip_title)) {
            HintText(stringResource(Res.string.drive_zip_hint))
            SecretTextField(pw, { pw = it; zipError = null }, stringResource(Res.string.f_password))
            ErrorText(zipError)
            Button(onClick = {
                val name = (if (files.size == 1) files[0].name.substringBeforeLast('.') else "GhulDrive") + ".zip"
                state.run(onError = { zipError = it }, block = { net.driveZip(files.map { it.id }, pw) }) { data -> zipping = null; saveZip(name, data) }
            }, enabled = pw.length >= 8 && filePickersAvailable, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Lock, null, Modifier.size(18.dp)); Text(stringResource(Res.string.drive_zip_create))
            }
        }
    }
    if (emptyTrash) {
        AlertDialog(
            onDismissRequest = { emptyTrash = false },
            title = { Text(stringResource(Res.string.drive_empty_trash)) },
            text = { Text(stringResource(Res.string.drive_empty_trash_text)) },
            confirmButton = {
                TextButton(onClick = {
                    emptyTrash = false
                    state.run(onError = { error = it }, block = { net.driveEmptyTrash(0) }) { trashTick++; state.changed() }
                }) { Text(stringResource(Res.string.drive_delete_forever)) }
            },
            dismissButton = { TextButton(onClick = { emptyTrash = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (deletingMany) {
        AlertDialog(
            onDismissRequest = { deletingMany = false },
            title = { Text(stringResource(Res.string.drive_delete_many_title, selected.size.toString())) },
            text = { Text(stringResource(Res.string.drive_delete_folder_text)) },
            confirmButton = {
                TextButton(onClick = {
                    deletingMany = false
                    val ids = selected.map { it.id }
                    state.run(onError = { error = it }, block = { ids.forEach { net.driveDelete(it) } }) {
                        selected.clear(); selecting = false; state.changed()
                    }
                }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { deletingMany = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    if (newFolder) {
        NameDialog(stringResource(Res.string.drive_new_folder), "", onDismiss = { newFolder = false }) { name ->
            newFolder = false
            state.run(onError = { error = it }, block = { net.driveCreateFolder(folder, name) }) { reload++ }
        }
    }
    renaming?.let { item ->
        NameDialog(stringResource(Res.string.a_rename), item.name, onDismiss = { renaming = null }) { name ->
            renaming = null
            state.run(onError = { error = it }, block = { net.driveRename(item.id, name) }) { reload++ }
        }
    }
    moving?.let { item ->
        MoveDialog(state, item, onDismiss = { moving = null }) { target ->
            moving = null
            state.run(onError = { error = it }, block = { net.driveMove(item.id, target) }) { reload++ }
        }
    }
    deleting?.let { item ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text(stringResource(Res.string.drive_delete_title, item.name)) },
            text = { Text(stringResource(if (item.isDir) Res.string.drive_delete_folder_text else Res.string.drive_delete_text)) },
            confirmButton = {
                TextButton(onClick = {
                    deleting = null
                    state.run(onError = { error = it }, block = { net.driveDelete(item.id) }) { state.changed() }
                }) { Text(stringResource(Res.string.a_delete)) }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

@Composable
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
private fun DriveRow(item: DriveItemUi, selecting: Boolean, selected: Boolean, divider: Boolean, onSelect: () -> Unit, onOpen: () -> Unit) {
    val t = g
    val color = areaColor(Tab.DRIVE)
    if (divider) RowDivider()
    Row(
        Modifier.fillMaxWidth().background(if (selected) color.copy(alpha = 0.12f) else androidx.compose.ui.graphics.Color.Transparent)
            .combinedClickable(onClick = onOpen, onLongClick = onSelect)
            .heightIn(min = 60.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (selected) {
            Box(Modifier.size(42.dp).clip(RoundedCornerShape(13.dp)).background(color), contentAlignment = Alignment.Center) {
                Icon(Icons.Filled.Check, null, Modifier.size(22.dp), tint = androidx.compose.ui.graphics.Color.White)
            }
        } else {
            val (ic, c) = if (item.isDir) Icons.Filled.Folder to paletteColor(1) else fileKind(item.name)
            IconTile(ic, c)
        }
        Column(Modifier.weight(1f)) {
            Text(item.name, maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.W500)
            Text(if (item.isDir) formatDate(item.modified) else formatSize(item.size) + " · " + formatDate(item.modified), fontSize = 14.sp, color = t.muted, maxLines = 1)
        }
        if (item.offline) Icon(Icons.Filled.Download, null, Modifier.size(20.dp), tint = t.muted)
        if (item.favorite) Icon(Icons.Filled.Star, null, Modifier.size(20.dp), tint = areaColor(Tab.MAIL))
    }
}

/** Tile of the grid view (.tile). */
@Composable
private fun DriveTile(item: DriveItemUi, modifier: Modifier, onClick: () -> Unit) {
    val t = g
    val (ic, c) = if (item.isDir) Icons.Filled.Folder to paletteColor(1) else fileKind(item.name)
    Column(modifier.clip(RoundedCornerShape(18.dp)).background(t.surface).clickable(onClick = onClick).padding(10.dp)) {
        Box(Modifier.fillMaxWidth().height(92.dp).clip(RoundedCornerShape(12.dp)).background(c.copy(alpha = 0.14f)), contentAlignment = Alignment.Center) {
            Icon(ic, null, Modifier.size(34.dp), tint = c)
        }
        Text(item.name, Modifier.padding(top = 8.dp), fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(if (item.isDir) formatDate(item.modified) else formatSize(item.size), fontSize = 12.sp, color = t.muted)
    }
}

/** Icon and colour of a file (as in the design preview: PDF, image, text, video, audio). */
fun fileKind(name: String): Pair<androidx.compose.ui.graphics.vector.ImageVector, androidx.compose.ui.graphics.Color> = when (ext(name)) {
    "jpg", "jpeg", "png", "gif", "webp", "heic" -> Icons.Filled.Image to paletteColor(1)
    "mp4", "mov", "mkv", "webm" -> Icons.Filled.Movie to paletteColor(4)
    "mp3", "m4a", "ogg", "flac", "wav" -> Icons.Filled.MusicNote to paletteColor(5)
    "pdf" -> Icons.Filled.Description to paletteColor(7)
    "zip", "7z", "rar", "tar", "gz" -> Icons.Filled.FolderZip to paletteColor(9)
    else -> Icons.Filled.Description to paletteColor(9)
}

/** Quick action under the storage card (.quick). */
@Composable
private fun QuickTile(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, modifier: Modifier = Modifier, enabled: Boolean = true, on: Boolean = false, onClick: () -> Unit) {
    val t = g
    Column(
        modifier.clip(RoundedCornerShape(18.dp)).background(if (on) areaColor(Tab.DRIVE).copy(alpha = 0.14f) else t.surface).clickable(enabled = enabled, onClick = onClick)
            .alpha(if (enabled) 1f else 0.5f).padding(vertical = 12.dp, horizontal = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(icon, null, Modifier.size(22.dp), tint = t.muted)
        Text(label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun ActionTile(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Surface(onClick = onClick, shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface, modifier = modifier) {
        Column(Modifier.padding(vertical = 14.dp, horizontal = 6.dp), horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(icon, null, tint = MaterialTheme.colorScheme.onSurface)
            Text(label, style = MaterialTheme.typography.labelMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
fun NameDialog(title: String, initial: String, onDismiss: () -> Unit, onDone: (String) -> Unit) {
    var name by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(name, { name = it }, singleLine = true, modifier = Modifier.fillMaxWidth()) },
        confirmButton = { TextButton(onClick = { onDone(name.trim()) }, enabled = name.isNotBlank()) { Text(stringResource(Res.string.a_save)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/** Lets the user browse folders and choose where to move [item]. */
@Composable
private fun MoveDialog(state: AppState, item: DriveItemUi, onDismiss: () -> Unit, onMove: (String?) -> Unit) {
    val root = stringResource(Res.string.drive_root)
    var path by remember { mutableStateOf(listOf(Crumb(null, root))) }
    var folders by remember { mutableStateOf<List<DriveItemUi>>(emptyList()) }
    val here = path.last().id
    LaunchedEffect(here) {
        folders = runCatching { state.backend.net.driveList(here, false) }.getOrDefault(emptyList())
            .filter { it.isDir && it.id != item.id }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(Res.string.drive_move_title, item.name)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(path.joinToString(" / ") { it.name }, style = MaterialTheme.typography.titleSmall)
                if (path.size > 1) TextButton(onClick = { path = path.dropLast(1) }) { Text("← " + stringResource(Res.string.a_back)) }
                for (f in folders) {
                    TextButton(onClick = { path = path + Crumb(f.id, f.name) }) {
                        Icon(Icons.Filled.Folder, null)
                        Text(" " + f.name)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onMove(here) }, enabled = here != item.parent) { Text(stringResource(Res.string.drive_move_here)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

private fun ext(name: String) = name.substringAfterLast('.', "").lowercase()

private fun fileIcon(name: String) = when (ext(name)) {
    "jpg", "jpeg", "png", "gif", "webp", "heic" -> Icons.Filled.Image
    "mp4", "mov", "mkv", "webm" -> Icons.Filled.Movie
    "mp3", "m4a", "ogg", "flac", "wav" -> Icons.Filled.MusicNote
    "pdf" -> Icons.Filled.PictureAsPdf
    "zip", "7z", "rar", "tar", "gz" -> Icons.Filled.FolderZip
    else -> Icons.Filled.Description
}

@Composable
private fun fileColor(name: String) = when (ext(name)) {
    "jpg", "jpeg", "png", "gif", "webp", "heic" -> avatarColor("c")
    "mp4", "mov", "mkv", "webm" -> avatarColor("d")
    "pdf" -> MaterialTheme.colorScheme.error
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}
