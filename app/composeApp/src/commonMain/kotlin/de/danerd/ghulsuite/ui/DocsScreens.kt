// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Article
import androidx.compose.material.icons.automirrored.filled.DriveFileMove
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CreateNewFolder
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.DeleteForever
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.DocumentScanner
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.FolderZip
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DocsItemUi
import de.danerd.ghulsuite.model.DocsUsageUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.pluralStringResource
import org.jetbrains.compose.resources.stringResource

/** A folder on the way from the top of a vault: its id and name. */
private data class Crumb(val id: String, val name: String)

/** Key of a document for [AppState.hidden] (a document in the trash is another key than in its folder). */
private fun docKey(place: String, item: DocsItemUi) = hiddenKey("docs", place, item.vaultId + "/" + item.id)

/** Icon and colour of a file or folder of GhulDocs: by its type, else by its name. */
fun docKind(item: DocsItemUi): Pair<ImageVector, Color> {
    if (item.isFolder) return Icons.Filled.Folder to paletteColor(1)
    val m = item.mime
    return when {
        m.startsWith("image/") -> Icons.Filled.Image to paletteColor(1)
        m == "application/pdf" -> Icons.Filled.PictureAsPdf to paletteColor(7)
        m.startsWith("text/") -> Icons.AutoMirrored.Filled.Article to paletteColor(9)
        m.startsWith("video/") -> Icons.Filled.Movie to paletteColor(4)
        m.startsWith("audio/") -> Icons.Filled.MusicNote to paletteColor(5)
        m.contains("zip") -> Icons.Filled.FolderZip to paletteColor(9)
        else -> fileKind(item.name)
    }
}

/** The GhulDocs tab: the folders and files of a vault, its favourites, trash and search. */
@Composable
fun DocsScreen(state: AppState, padding: PaddingValues) {
    val docs = state.backend.docs
    val vaults = state.vaultsOf(VaultKindUi.DOCS).filter { it.available }
    /** A vault entered from "All vaults" (the header's choice stays "All"). */
    var entered by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(vaults.map { it.id }) { if (entered != null && vaults.none { it.id == entered }) entered = null }
    // A choice in the header line ("All vaults" included) starts from there, not from a vault entered before.
    LaunchedEffect(state.docsVault) { entered = null }
    val vault: String? = state.docsVault ?: entered ?: vaults.singleOrNull()?.id
    val vaultName = vaults.firstOrNull { it.id == vault }?.name.orEmpty()
    val severalVaults = vaults.size > 1 && state.docsVault == null
    // The folders inside the vault (another vault starts at its top).
    var pathState by remember { mutableStateOf<Pair<String?, List<Crumb>>>(null to emptyList()) }
    val path = pathState.second.takeIf { pathState.first == vault }.orEmpty()
    fun setPath(p: List<Crumb>, inVault: String? = vault) {
        pathState = inVault to p
    }
    val folder = path.lastOrNull()?.id
    /** "" = folders, or one of the views: "fav", "trash". */
    var view by remember { mutableStateOf("") }
    var query by remember { mutableStateOf("") }
    SideEffect { state.docsTrashShown = view == "trash" }
    DisposableEffect(Unit) { onDispose { state.docsTrashShown = false } }
    val active = LocalTabActive.current
    // Back: out of a view, up one folder, back to "All vaults" (only while GhulDocs is visible).
    PlatformBackHandler(enabled = active && (view.isNotEmpty() || path.isNotEmpty() || (entered != null && state.docsVault == null && vaults.size > 1))) {
        when {
            view.isNotEmpty() -> view = ""
            path.isNotEmpty() -> setPath(path.dropLast(1))
            else -> entered = null
        }
    }
    // Long press selects (as in every list); registered after the back of the folders, so "back"
    // leaves the selection first.
    val selected = rememberListSelection()

    var items by remember { mutableStateOf<List<DocsItemUi>>(emptyList()) }
    var loaded by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var reload by remember { mutableStateOf(0) }
    var usage by remember { mutableStateOf<DocsUsageUi?>(null) }
    /** Which place the loaded list belongs to: another one is not shown under the new path while it loads. */
    var itemsOf by remember { mutableStateOf<String?>(null) }
    val showVaults = vault == null && query.isBlank() && view.isEmpty()
    LaunchedEffect(vault, folder, query, view, state.liveRevision(), reload) {
        val place = "$vault|$folder|$view|${query.isNotBlank()}"
        if (itemsOf != place) {
            items = emptyList()
            loaded = false
            error = null
        }
        if (!showVaults) {
            catching {
                when {
                    query.isNotBlank() -> docs.search(vault, query)
                    view == "fav" -> docs.favorites(vault)
                    view == "trash" -> docs.trash(vault)
                    vault != null -> docs.list(vault, folder)
                    else -> emptyList()
                }
            }.onSuccess { items = it; itemsOf = place; error = null }.onFailure { e ->
                // The folder is gone (deleted elsewhere, in the trash): back to the top of the vault.
                if (e is VaultException && e.kind == VaultErrorKind.NOT_FOUND && path.isNotEmpty()) setPath(emptyList())
                else error = describe(e)
            }
        }
        loaded = true
        usage = catching { docs.usage(vault) }.getOrNull()
    }

    // --- adding: files of the device, files shared from other apps --------------------------------
    var addSheet by remember { mutableStateOf(false) }
    var choosingVault by remember { mutableStateOf(false) }
    var newFolder by remember { mutableStateOf(false) }
    /** Where the files picked next go (the picker answers later on Android). */
    val addTarget = remember { arrayOfNulls<Pair<String, String?>>(1) }
    val pick = rememberOpenStreams { sources ->
        val t = addTarget[0]
        addTarget[0] = null
        if (t == null) sources.forEach { runCatching { it.close() } } else state.addDocs(t.first, t.second, sources)
    }
    val addTick = state.docsAddTick
    var seenTick by remember { mutableStateOf(addTick) }
    LaunchedEffect(addTick) {
        if (addTick != seenTick) {
            seenTick = addTick
            if (vault == null) choosingVault = true else addSheet = true
        }
    }

    val place = if (view == "trash") "trash" else "files"
    val visible = items.filter { docKey(place, it) !in state.hidden }
    val selecting = selected.active
    val keyOf = { i: DocsItemUi -> i.vaultId + "/" + i.id }
    val chosen = visible.filter { keyOf(it) in selected }
    var renaming by remember { mutableStateOf<DocsItemUi?>(null) }
    var moving by remember { mutableStateOf<List<DocsItemUi>?>(null) }
    var sharing by remember { mutableStateOf<Pair<String, List<String>>?>(null) }
    var emptyTrash by remember { mutableStateOf(false) }
    val trashMsg = stringResource(Res.string.msg_moved_to_trash)
    val deletedMsg = stringResource(Res.string.msg_deleted_n)
    val restoredMsg = stringResource(Res.string.docs_restored)

    fun open(item: DocsItemUi) {
        when {
            item.trashed != null || view == "trash" -> selected.toggle(keyOf(item))
            item.isFolder -> {
                val inList = query.isBlank() && view.isEmpty() && item.vaultId == vault && item.parent == folder
                query = ""
                view = ""
                if (state.docsVault == null && vaults.size > 1) entered = item.vaultId
                // From a search or a view the folders above it are not listed here: its breadcrumb
                // starts at the vault.
                setPath(if (inList) path + Crumb(item.id, item.name) else listOf(Crumb(item.id, item.name)), item.vaultId)
            }
            else -> state.push(Screen.DocView(item.vaultId, item.id))
        }
    }
    fun toTrash(list: List<DocsItemUi>) = state.askDelete(list.map { it.name }, trash = true) {
        selected.clear()
        state.undoable(list.map { docKey("files", it) }, trashMsg) {
            for (x in list) docs.toTrash(x.vaultId, x.id)
        }
    }
    fun deleteForever(list: List<DocsItemUi>) = state.askDelete(list.map { it.name }) {
        selected.clear()
        state.undoable(list.map { docKey("trash", it) }, deletedMsg.replace("%1\$s", list.size.toString())) {
            for (x in list) docs.deleteForever(x.vaultId, x.id)
        }
    }
    fun restore(list: List<DocsItemUi>) {
        selected.clear()
        state.run(onError = { error = it }, block = { for (x in list) docs.restore(x.vaultId, x.id) }) {
            state.message(restoredMsg.replace("%1\$s", list.size.toString()))
            state.changed()
        }
    }
    fun favorite(list: List<DocsItemUi>) {
        val on = list.any { !it.favorite }
        selected.clear()
        state.run(onError = { error = it }, block = { for (x in list) docs.setFavorite(x.vaultId, x.id, on) }) { state.changed() }
    }

    val color = areaColor(Tab.DOCS)
    val t = g
    Column(Modifier.fillMaxSize().padding(padding)) {
        if (selecting) {
            val one = chosen.singleOrNull()
            val oneVault = chosen.map { it.vaultId }.distinct().singleOrNull()
            SelectionHeader(
                selected.keys.size,
                onClose = { selected.clear() },
                onAll = { selected.all(visible.map(keyOf)) },
                total = visible.size,
                actions = if (view == "trash") listOf(
                    SelAction(Icons.Filled.Restore, stringResource(Res.string.docs_restore)) { restore(chosen) },
                    SelAction(Icons.Filled.DeleteForever, stringResource(Res.string.docs_delete_forever), danger = true) { deleteForever(chosen) },
                ) else buildList {
                    val allFav = chosen.isNotEmpty() && chosen.all { it.favorite }
                    add(SelAction(if (allFav) Icons.Filled.StarBorder else Icons.Filled.Star, stringResource(if (allFav) Res.string.a_unfavorite else Res.string.a_favorite)) { favorite(chosen) })
                    if (one != null) add(SelAction(Icons.Filled.Edit, stringResource(Res.string.a_rename)) { selected.clear(); renaming = one })
                    if (oneVault != null) {
                        add(SelAction(Icons.AutoMirrored.Filled.DriveFileMove, stringResource(Res.string.a_move)) { moving = chosen })
                        add(SelAction(Icons.Filled.Share, stringResource(Res.string.share_vault_action)) {
                            selected.clear()
                            sharing = oneVault to chosen.map { it.id }
                        })
                    }
                    add(SelAction(Icons.Filled.Delete, stringResource(Res.string.a_to_trash), danger = true) { toTrash(chosen) })
                },
            )
        }
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(top = 4.dp, bottom = 100.dp)) {
            item(key = "head") {
                Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    BusyBanner(state)
                    SearchField(query, { query = it; selected.clear() }, placeholder = stringResource(Res.string.docs_search))
                    LoadError(error) { error = null; reload++ }
                    if (!loaded && error == null && !showVaults) LoadingHint()
                    // Selecting keeps the head as it is (nothing jumps, no second title).
                    val atTop = path.isEmpty() && query.isBlank() && view.isEmpty()
                    if (atTop) {
                        DocsUsageCard(usage, severalVaults = vaults.size > 1, all = vault == null)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            QuickTile(Icons.Filled.StarBorder, stringResource(Res.string.chip_favorites), Modifier.weight(1f).testTag("docs-view-fav")) { view = "fav"; selected.clear() }
                            QuickTile(Icons.Filled.Delete, stringResource(Res.string.docs_trash), Modifier.weight(1f).testTag("docs-view-trash")) { view = "trash"; selected.clear() }
                        }
                    }
                    if (query.isBlank() && view.isNotEmpty()) {
                        // A view (favourites, trash): the way back to the folders.
                        Breadcrumb(
                            listOf(
                                if (vault != null) vaultName else stringResource(Res.string.vault_all_vaults),
                                stringResource(if (view == "fav") Res.string.chip_favorites else Res.string.docs_trash),
                            ),
                        ) { view = ""; selected.clear() }
                    } else if (query.isBlank() && view.isEmpty() && vault != null && (path.isNotEmpty() || severalVaults)) {
                        // Inside a folder, or a vault entered from "All vaults": the way back up.
                        Breadcrumb(
                            buildList {
                                if (severalVaults) add(stringResource(Res.string.vault_all_vaults))
                                add(vaultName)
                                addAll(path.map { it.name })
                            },
                        ) { i ->
                            val offset = if (severalVaults) 1 else 0
                            when {
                                severalVaults && i == 0 -> { entered = null; setPath(emptyList()) }
                                else -> setPath(path.take(i - offset))
                            }
                            selected.clear()
                        }
                    }
                    SectionLabel(
                        when {
                            query.isNotBlank() -> stringResource(Res.string.docs_search_results)
                            view == "fav" -> stringResource(Res.string.chip_favorites)
                            view == "trash" -> stringResource(Res.string.docs_trash)
                            showVaults -> stringResource(Res.string.set_vaults)
                            else -> pluralStringResource(Res.plurals.docs_n_items, visible.size, visible.size.toString())
                        },
                        trailing = when {
                            view == "trash" && visible.isNotEmpty() -> ({
                                Text(stringResource(Res.string.docs_empty_trash), Modifier.clickable { emptyTrash = true }.testTag("docs-empty-trash"))
                            })
                            else -> null
                        },
                    )
                    if (view == "trash") HintText(stringResource(Res.string.docs_trash_hint))
                    if (showVaults && vaults.isEmpty()) EmptyState(tabIcon(Tab.DOCS), stringResource(Res.string.docs_no_vault), color)
                    if (!showVaults && loaded && visible.isEmpty() && error == null) {
                        EmptyState(
                            if (view == "trash") Icons.Filled.Delete else tabIcon(Tab.DOCS),
                            stringResource(
                                when {
                                    query.isNotBlank() -> Res.string.docs_nothing_found
                                    view == "fav" -> Res.string.docs_no_favorites
                                    view == "trash" -> Res.string.docs_trash_empty
                                    else -> Res.string.docs_empty
                                },
                            ),
                            color,
                        )
                    }
                }
            }
            if (showVaults) {
                itemsIndexed(vaults, key = { _, v -> "vault-" + v.id }) { i, v ->
                    Column(Modifier.cardItem(i, vaults.size)) {
                        ListRow(
                            v.name, pluralStringResource(Res.plurals.docs_vault_items, v.entries, v.entries.toString()),
                            leading = { IconTile(tabIcon(Tab.DOCS), color) },
                            trailing = { ChevronEnd() }, divider = i > 0,
                        ) { entered = v.id }
                    }
                }
            } else {
                // A search, the favourites and the trash show where each item is.
                val wherePlace = query.isNotBlank() || view.isNotEmpty()
                itemsIndexed(visible, key = { _, it -> place + it.vaultId + it.id }) { i, item ->
                    Column(Modifier.cardItem(i, visible.size)) {
                        DocsRow(
                            item, keyOf(item) in selected, divider = i > 0,
                            where = if (wherePlace) docWhere(item, vaults, vault == null) else null,
                            onLongClick = { selected.toggle(keyOf(item)) },
                        ) { if (selecting) selected.toggle(keyOf(item)) else open(item) }
                    }
                }
            }
            item(key = "foot") {
                HintText(stringResource(Res.string.docs_privacy), Modifier.padding(horizontal = 16.dp, vertical = 12.dp))
            }
        }
    }

    if (choosingVault) {
        VaultPickerDialog(state, VaultKindUi.DOCS, onPick = { id ->
            choosingVault = false
            entered = id
            addSheet = true
        }, onDismiss = { choosingVault = false })
    }
    if (addSheet) {
        val target = vault
        GhulSheet({ addSheet = false }, stringResource(Res.string.docs_add_title)) {
            SheetList {
                SheetRow(stringResource(Res.string.docs_add_files), stringResource(Res.string.docs_add_files_sub),
                    leading = { IconTile(Icons.Filled.Upload, color) }, enabled = filePickersAvailable && target != null) {
                    addSheet = false
                    addTarget[0] = target!! to folder
                    pick()
                }
                // Scanning (Android): the camera in the app, the pages into one PDF in this folder.
                if (LocalScanEngine.current != null) {
                    SheetRow(stringResource(Res.string.docs_add_scan), stringResource(Res.string.docs_add_scan_sub),
                        leading = { IconTile(Icons.Filled.DocumentScanner, paletteColor(7)) }, enabled = target != null, divider = true) {
                        addSheet = false
                        state.push(Screen.DocScan(target!!, folder))
                    }
                }
                SheetRow(stringResource(Res.string.docs_new_folder), leading = { IconTile(Icons.Filled.CreateNewFolder, paletteColor(9)) }, divider = true) {
                    addSheet = false
                    newFolder = true
                }
            }
        }
    }
    if (newFolder) {
        val target = vault
        NameDialog(stringResource(Res.string.docs_new_folder), "", onDismiss = { newFolder = false }) { name ->
            newFolder = false
            if (target != null) state.run(onError = { error = it }, block = { docs.createFolder(target, folder, name) }) { state.changed() }
        }
    }
    renaming?.let { item ->
        NameDialog(stringResource(Res.string.a_rename), item.name, onDismiss = { renaming = null }) { name ->
            renaming = null
            state.run(onError = { error = it }, block = { docs.rename(item.vaultId, item.id, name) }) { state.changed() }
        }
    }
    moving?.let { list ->
        val v = list.first().vaultId
        FolderPickerDialog(
            state, fixedVault = v, startFolder = if (v == vault) path.map { it.id to it.name } else emptyList(),
            exclude = list.filter { it.isFolder }.map { it.id }.toSet(),
            title = if (list.size == 1) stringResource(Res.string.docs_move_title, list[0].name) else pluralStringResource(Res.plurals.docs_move_n, list.size, list.size.toString()),
            confirm = stringResource(Res.string.docs_move_here),
            onDismiss = { moving = null },
        ) { _, picked ->
            moving = null
            selected.clear()
            val target = picked.lastOrNull()?.first
            state.run(onError = { error = it }, block = { for (x in list) if (x.parent != target) docs.move(x.vaultId, x.id, target) }) { state.changed() }
        }
    }
    sharing?.let { (v, ids) -> ShareAsVaultDialog(state, v, ids, docs = true) { sharing = null } }
    if (emptyTrash) {
        AlertDialog(
            danger = true,
            onDismissRequest = { emptyTrash = false },
            title = { Text(stringResource(Res.string.docs_empty_trash_title)) },
            text = { Text(stringResource(Res.string.docs_empty_trash_text)) },
            confirmButton = {
                TextButton(onClick = {
                    emptyTrash = false
                    state.run(onError = { error = it }, block = { docs.emptyTrash(vault) }) { state.changed() }
                }, modifier = Modifier.testTag("ask-confirm")) { Text(stringResource(Res.string.docs_delete_forever)) }
            },
            dismissButton = { TextButton(onClick = { emptyTrash = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
    // Files shared from another app: where they go is chosen here.
    state.docsIncoming?.let { files ->
        if (active) IncomingDocsSheet(state, files, vaults, vault, path.map { it.id to it.name })
    }
}

/** "In Private › Taxes", or only the folders when one vault is shown. */
@Composable
private fun docWhere(item: DocsItemUi, vaults: List<VaultUi>, withVault: Boolean): String {
    val parts = buildList {
        if (withVault) vaults.firstOrNull { it.id == item.vaultId }?.name?.let { add(it) }
        if (item.path.isNotBlank()) addAll(item.path.split('/'))
    }
    return if (parts.isEmpty()) stringResource(Res.string.docs_at_top) else stringResource(Res.string.docs_in, parts.joinToString(" › "))
}

/** One file or folder: its tile (the check while chosen), name, size or count, date, star. */
@Composable
private fun DocsRow(item: DocsItemUi, selected: Boolean, divider: Boolean, where: String?, onLongClick: () -> Unit, onClick: () -> Unit) {
    val (icon, c) = docKind(item)
    val what = if (item.isFolder) pluralStringResource(Res.plurals.docs_n_items, item.items, item.items.toString()) else formatSizeSi(item.size)
    ListRow(
        title = item.name,
        subtitle = listOfNotNull(what, formatDate(item.trashed ?: item.modified), where).joinToString(" · "),
        leading = { SelectableTile(selected) { IconTile(icon, c) } },
        trailing = if (item.favorite) ({ Icon(Icons.Filled.Star, null, Modifier.size(20.dp), tint = starColor()) }) else null,
        divider = divider,
        selected = selected,
        onLongClick = onLongClick,
        onClick = onClick,
    )
}

/**
 * How much the GhulDocs vault shown holds: "412 MB of 1 GB", a bar, the number of documents; with
 * several GhulDocs vaults also all of them together ("All GhulDocs vaults: 1.4 GB of 3 GB").
 * Without a vault shown ("All vaults"): all of them together.
 */
@Composable
fun DocsUsageCard(usage: DocsUsageUi?, severalVaults: Boolean = false, all: Boolean = false) {
    val t = g
    val color = areaColor(Tab.DOCS)
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(t.surface).padding(16.dp).testTag("docs-usage"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            IconTile(tabIcon(Tab.DOCS), color)
            Column(Modifier.weight(1f)) {
                Text(
                    when {
                        usage == null -> stringResource(Res.string.loading)
                        all -> stringResource(Res.string.docs_usage, formatSizeSi(usage.suiteUsed), formatSizeSi(usage.suiteLimit))
                        else -> stringResource(Res.string.docs_usage, formatSizeSi(usage.used), formatSizeSi(usage.limit))
                    },
                    fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
                Text(
                    listOfNotNull(
                        usage?.let {
                            val n = if (all) it.suiteFiles else it.files
                            pluralStringResource(Res.plurals.docs_n_documents, n, n.toString())
                        },
                        stringResource(Res.string.docs_encrypted_here),
                    ).joinToString(" · "),
                    fontSize = 13.sp, color = t.muted, maxLines = 2,
                )
            }
        }
        // The bar stands there while the usage is still counted: nothing jumps.
        val suitePart = usage?.let { (it.suiteUsed.toFloat() / it.suiteLimit.coerceAtLeast(1)).coerceIn(0f, 1f) } ?: 0f
        val part = if (all) suitePart else usage?.let { (it.used.toFloat() / it.limit.coerceAtLeast(1)).coerceIn(0f, 1f) } ?: 0f
        val full = part >= 0.9f
        // All vaults together almost full while this one is not: said as such.
        val suiteFull = !full && !all && suitePart >= 0.9f
        Box(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).background(t.surface3)) {
            if (part > 0f) Box(Modifier.fillMaxWidth(part.coerceAtLeast(0.01f)).fillMaxHeight().background(if (full) t.warn else color))
        }
        if (severalVaults && !all && usage != null) {
            Text(
                stringResource(Res.string.docs_usage_suite, formatSizeSi(usage.suiteUsed), formatSizeSi(usage.suiteLimit)),
                fontSize = 13.sp, color = if (suitePart >= 0.9f) t.warn else t.muted, maxLines = 2,
            )
        }
        if (full) Text(stringResource(if (all) Res.string.docs_almost_full_suite else Res.string.docs_almost_full), fontSize = 13.sp, color = t.warn)
        if (suiteFull) Text(stringResource(Res.string.docs_almost_full_suite), fontSize = 13.sp, color = t.warn)
    }
}

/** A view of GhulDocs under the usage card (favourites, trash). */
@Composable
private fun QuickTile(icon: ImageVector, label: String, modifier: Modifier = Modifier, on: Boolean = false, onClick: () -> Unit) {
    val t = g
    Column(
        modifier.clip(RoundedCornerShape(20.dp)).background(if (on) t.accentSoft else t.surface).clickable(onClick = onClick)
            .padding(vertical = 12.dp, horizontal = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(icon, null, Modifier.size(22.dp), tint = if (on) t.accent else t.muted)
        Text(label, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = if (on) t.accent else t.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Where a folder is: every part but the last can be tapped to go back there. */
@Composable
private fun Breadcrumb(parts: List<String>, onPick: (Int) -> Unit) {
    val t = g
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).testTag("docs-breadcrumb"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        parts.forEachIndexed { i, p ->
            val last = i == parts.lastIndex
            Text(
                p, maxLines = 1, fontSize = 14.sp,
                fontWeight = if (last) FontWeight.SemiBold else FontWeight.Normal,
                color = if (last) t.fg else areaColor(Tab.DOCS),
                modifier = Modifier.clip(RoundedCornerShape(8.dp)).testTag("docs-crumb-$i").then(if (last) Modifier else Modifier.clickable { onPick(i) })
                    .padding(horizontal = 4.dp, vertical = 6.dp),
            )
            if (!last) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(16.dp), tint = t.muted)
        }
    }
}

/**
 * Chooses a folder of a GhulDocs vault: browse into folders, back up, "here". [fixedVault] keeps
 * the vault; else the vaults are the first level (several of them). [exclude] are folders that
 * cannot be the target (those being moved).
 */
@Composable
fun FolderPickerDialog(
    state: AppState,
    fixedVault: String?,
    startFolder: List<Pair<String, String>> = emptyList(),
    exclude: Set<String> = emptySet(),
    title: String,
    confirm: String,
    startVault: String? = fixedVault,
    onDismiss: () -> Unit,
    onPick: (String, List<Pair<String, String>>) -> Unit,
) {
    val vaults = state.vaultsOf(VaultKindUi.DOCS).filter { it.available }
    var vault by remember { mutableStateOf(startVault ?: vaults.singleOrNull()?.id) }
    var path by remember { mutableStateOf(startFolder) }
    var folders by remember { mutableStateOf<List<DocsItemUi>>(emptyList()) }
    val here = path.lastOrNull()?.first
    LaunchedEffect(vault, here) {
        val v = vault
        folders = if (v == null) emptyList()
        else catching { state.backend.docs.list(v, here) }.getOrDefault(emptyList()).filter { it.isFolder && it.id !in exclude }
    }
    val canLeaveVault = fixedVault == null && vaults.size > 1
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                val v = vault
                if (v == null) {
                    for (x in vaults) {
                        ListRow(x.name, leading = { IconTile(tabIcon(Tab.DOCS), areaColor(Tab.DOCS), size = 36.dp) }, divider = x != vaults.first(), flush = true) { vault = x.id }
                    }
                    return@Column
                }
                val name = vaults.firstOrNull { it.id == v }?.name.orEmpty()
                HintText((listOf(name) + path.map { it.second }).joinToString(" › "))
                if (path.isNotEmpty() || canLeaveVault) {
                    ListRow(stringResource(Res.string.docs_up), leading = { IconTile(Icons.Filled.Folder, g.faint, size = 36.dp) }, flush = true) {
                        if (path.isNotEmpty()) path = path.dropLast(1) else vault = null
                    }
                }
                for (f in folders) {
                    Box(Modifier.testTag("docs-folder-" + f.name)) {
                        ListRow(f.name, leading = { IconTile(Icons.Filled.Folder, paletteColor(1), size = 36.dp) }, divider = true, flush = true,
                            trailing = { ChevronEnd() }) { path = path + (f.id to f.name) }
                    }
                }
                if (folders.isEmpty()) HintText(stringResource(Res.string.docs_no_subfolders))
            }
        },
        confirmButton = {
            TextButton(onClick = { vault?.let { onPick(it, path) } }, enabled = vault != null, modifier = Modifier.testTag("docs-pick-here")) { Text(confirm) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(Res.string.a_cancel)) } },
    )
}

/**
 * Files shared from another app ("Save in GhulDocs"): into the vault and folder shown, or
 * another one chosen here. "Cancel" lets them go (nothing was read).
 */
@Composable
private fun IncomingDocsSheet(state: AppState, files: List<ByteSource>, vaults: List<VaultUi>, vault: String?, path: List<Pair<String, String>>) {
    var target by remember { mutableStateOf(vault ?: vaults.firstOrNull()?.id) }
    var folder by remember { mutableStateOf(if (vault != null) path else emptyList()) }
    var choosing by remember { mutableStateOf(false) }
    val close = {
        files.forEach { runCatching { it.close() } }
        state.docsIncoming = null
    }
    GhulSheet(close, stringResource(Res.string.docs_incoming_title)) {
        HintText(
            if (files.size == 1) stringResource(Res.string.docs_incoming_one, files[0].name)
            else pluralStringResource(Res.plurals.docs_incoming_n, files.size, files.size.toString()),
        )
        val name = vaults.firstOrNull { it.id == target }?.name
        SheetList {
            SheetRow(
                stringResource(Res.string.docs_incoming_into),
                if (name == null) stringResource(Res.string.docs_no_vault) else (listOf(name) + folder.map { it.second }).joinToString(" › "),
                leading = { IconTile(Icons.Filled.Folder, areaColor(Tab.DOCS)) },
                end = { Text(stringResource(Res.string.docs_change), color = g.accent, fontSize = 14.sp) },
                // Without a GhulDocs vault there is nothing to choose.
                enabled = target != null,
            ) { choosing = true }
        }
        ButtonRow {
            OutlinedButton(onClick = close, modifier = Modifier.weight(1f)) { Text(stringResource(Res.string.a_cancel)) }
            Button(onClick = {
                val v = target ?: return@Button
                state.docsIncoming = null
                state.addDocs(v, folder.lastOrNull()?.first, files)
            }, enabled = target != null, modifier = Modifier.weight(1f).testTag("docs-incoming-save")) { Text(stringResource(Res.string.docs_save_here)) }
        }
    }
    if (choosing) {
        FolderPickerDialog(
            state, fixedVault = null, startVault = target, startFolder = folder,
            title = stringResource(Res.string.docs_incoming_into), confirm = stringResource(Res.string.docs_choose_here),
            onDismiss = { choosing = false },
        ) { v, picked ->
            choosing = false
            target = v
            folder = picked
        }
    }
}

/** Options → GhulDocs: what it holds, a check of every document, the trash. */
@Composable
fun DocsSettingsSection(state: AppState) {
    val docs = state.backend.docs
    var usage by remember { mutableStateOf<DocsUsageUi?>(null) }
    var trashCount by remember { mutableStateOf(0) }
    var verified by remember { mutableStateOf<Map<String, List<DocsItemUi>>>(emptyMap()) }
    var emptyTrash by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(state.liveRevision()) {
        usage = catching { docs.usage(null) }.getOrNull()
        trashCount = catching { docs.trash(null).size }.getOrDefault(0)
    }
    val vaults = state.vaultsOf(VaultKindUi.DOCS)
    SectionCard(stringResource(Res.string.docs_usage_title)) {
        DocsUsageCardFlat(usage, several = vaults.size > 1)
        // Each vault on its own, against its own limit.
        if (vaults.size > 1) {
            for (v in vaults) DocsVaultUsageRow(state, v)
        }
        HintText(stringResource(Res.string.docs_limit_hint))
    }
    SectionCard(stringResource(Res.string.docs_verify_title)) {
        HintText(stringResource(Res.string.docs_verify_hint))
        for (v in vaults) {
            val result = verified[v.id]
            ListRow(
                v.name,
                when {
                    result == null -> pluralStringResource(Res.plurals.docs_vault_items, v.entries, v.entries.toString())
                    result.isEmpty() -> stringResource(Res.string.docs_verify_ok)
                    else -> pluralStringResource(Res.plurals.docs_verify_bad, result.size, result.size.toString())
                },
                leading = {
                    when {
                        result == null -> IconTile(tabIcon(Tab.DOCS), areaColor(Tab.DOCS))
                        result.isEmpty() -> IconTile(Icons.Filled.CheckCircle, okColor())
                        else -> IconTile(Icons.Filled.Warning, g.danger)
                    }
                },
                trailing = {
                    TextButton(onClick = {
                        state.run(onError = { error = it }, block = { docs.verify(v.id) }) { verified = verified + (v.id to it) }
                    }, enabled = !state.busy && v.available, modifier = Modifier.testTag("docs-verify-" + v.id)) { Text(stringResource(Res.string.docs_verify)) }
                },
                divider = v != vaults.first(), flush = true,
            )
            result?.takeIf { it.isNotEmpty() }?.let { bad ->
                Text(bad.joinToString(", ") { (if (it.path.isBlank()) "" else it.path + "/") + it.name }, fontSize = 13.sp, color = g.danger)
                HintText(stringResource(Res.string.docs_verify_bad_hint))
            }
        }
        ErrorText(error)
    }
    SectionCard(stringResource(Res.string.docs_trash)) {
        HintText(pluralStringResource(Res.plurals.docs_trash_count, trashCount, trashCount.toString()))
        OutlinedButton(onClick = { emptyTrash = true }, enabled = trashCount > 0 && !state.busy, modifier = Modifier.fillMaxWidth().testTag("docs-settings-empty-trash")) {
            Text(stringResource(Res.string.docs_empty_trash_title))
        }
    }
    if (emptyTrash) {
        AlertDialog(
            danger = true,
            onDismissRequest = { emptyTrash = false },
            title = { Text(stringResource(Res.string.docs_empty_trash_title)) },
            text = { Text(stringResource(Res.string.docs_empty_trash_text)) },
            confirmButton = {
                TextButton(onClick = {
                    emptyTrash = false
                    state.run(onError = { error = it }, block = { docs.emptyTrash(null) }) { state.changed() }
                }, modifier = Modifier.testTag("ask-confirm")) { Text(stringResource(Res.string.docs_delete_forever)) }
            },
            dismissButton = { TextButton(onClick = { emptyTrash = false }) { Text(stringResource(Res.string.a_cancel)) } },
        )
    }
}

/** One GhulDocs vault in Options: its name and "412 MB of 1 GB". */
@Composable
private fun DocsVaultUsageRow(state: AppState, v: de.danerd.ghulsuite.model.VaultUi) {
    var u by remember { mutableStateOf<DocsUsageUi?>(null) }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(v.id, state.liveRevision()) {
        val r = if (v.available) catching { state.backend.docs.usage(v.id) }.getOrNull() else null
        u = r
        failed = r == null
    }
    ListRow(
        v.name,
        when {
            !v.available -> stringResource(Res.string.vault_missing, v.file)
            u != null -> stringResource(Res.string.docs_usage, formatSizeSi(u!!.used), formatSizeSi(u!!.limit))
            failed -> stringResource(Res.string.docs_usage_failed)
            else -> stringResource(Res.string.loading)
        },
        leading = { IconTile(tabIcon(Tab.DOCS), areaColor(Tab.DOCS), size = 36.dp) }, flush = true,
    )
}

/** The usage of all GhulDocs vaults inside a card of its own (Options): the same texts and bar as above the list. */
@Composable
private fun DocsUsageCardFlat(usage: DocsUsageUi?, several: Boolean) {
    val t = g
    Text(
        when {
            usage == null -> stringResource(Res.string.loading)
            // One vault: its own limit, as over the list.
            !several -> stringResource(Res.string.docs_usage, formatSizeSi(usage.used), formatSizeSi(usage.limit))
            else -> stringResource(Res.string.docs_usage, formatSizeSi(usage.suiteUsed), formatSizeSi(usage.suiteLimit))
        },
        fontWeight = FontWeight.Bold,
    )
    val part = usage?.let { if (several) it.suiteUsed.toFloat() / it.suiteLimit.coerceAtLeast(1) else it.used.toFloat() / it.limit.coerceAtLeast(1) }?.coerceIn(0f, 1f) ?: 0f
    Box(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)).background(t.surface3)) {
        if (part > 0f) Box(Modifier.fillMaxWidth(part.coerceAtLeast(0.01f)).fillMaxHeight().background(if (part >= 0.9f) t.warn else areaColor(Tab.DOCS)))
    }
    usage?.let { Text(pluralStringResource(Res.plurals.docs_n_documents, it.suiteFiles, it.suiteFiles.toString()), fontSize = 13.sp, color = t.muted) }
}
