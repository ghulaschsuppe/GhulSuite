// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.SelectAll
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

/**
 * An action of the selection: in the header as an icon, or in its sheet as a row with a tile.
 * [danger] actions (delete) are red and always come last.
 */
class SelAction(val icon: ImageVector, val label: String, val danger: Boolean = false, val onClick: () -> Unit)

/**
 * Bar above every list while items are selected (a long press starts it, a tap then adds or
 * removes): close, count, "all", and the actions of the area. Up to three actions are icons;
 * with more, the first and the one that deletes stay icons and the rest is in a sheet ("More")
 * in the style of all action sheets.
 */
@Composable
fun SelectionHeader(count: Int, onClose: () -> Unit, onAll: () -> Unit, actions: List<SelAction>) {
    val t = g
    var more by remember { mutableStateOf(false) }
    val ordered = actions.filter { !it.danger } + actions.filter { it.danger }
    val icons = if (ordered.size <= 3) ordered else listOfNotNull(ordered.first(), ordered.lastOrNull { it.danger })
    val rest = ordered - icons.toSet()
    val title = stringResource(Res.string.n_selected, count.toString())
    Row(
        Modifier.fillMaxWidth().background(t.accentSoft).padding(horizontal = 4.dp).height(56.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        IconButton(onClick = onClose) { Icon(Icons.Filled.Close, stringResource(Res.string.a_cancel), tint = t.fg) }
        Text(title, Modifier.weight(1f), fontWeight = FontWeight.SemiBold, fontSize = 16.sp, color = t.fg, maxLines = 1)
        IconButton(onClick = onAll) { Icon(Icons.Filled.SelectAll, stringResource(Res.string.a_select_all), tint = t.fg) }
        for (a in icons) {
            IconButton(onClick = a.onClick, enabled = count > 0) { Icon(a.icon, a.label, tint = if (a.danger) t.danger else t.fg) }
        }
        if (rest.isNotEmpty()) {
            IconButton(onClick = { more = true }, enabled = count > 0) { Icon(Icons.Filled.MoreVert, stringResource(Res.string.a_more), tint = t.fg) }
        }
    }
    if (more) {
        GhulSheet({ more = false }, title) {
            SheetList {
                rest.forEachIndexed { i, a ->
                    SheetRow(
                        a.label, leading = { IconTile(a.icon, if (a.danger) t.danger else t.accent) },
                        color = if (a.danger) t.danger else androidx.compose.ui.graphics.Color.Unspecified, divider = i > 0,
                    ) { more = false; a.onClick() }
                }
            }
        }
    }
}

/** In front of a row: its tile, or the check of a chosen row (the same in every list). */
@Composable
fun SelectableTile(selected: Boolean, content: @Composable () -> Unit) {
    if (!selected) return content()
    val t = g
    Box(Modifier.size(42.dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(13.dp)).background(t.accent), contentAlignment = Alignment.Center) {
        Icon(Icons.Filled.Check, null, Modifier.size(22.dp), tint = t.onAccent)
    }
}

/** Selection of a list by keys, with the back button leaving it. */
class ListSelectionState {
    val keys = androidx.compose.runtime.mutableStateListOf<String>()
    val active: Boolean get() = keys.isNotEmpty()
    operator fun contains(key: String) = key in keys
    fun toggle(key: String) { if (key in keys) keys.remove(key) else keys.add(key) }
    fun all(list: List<String>) { list.forEach { if (it !in keys) keys.add(it) } }
    fun clear() = keys.clear()
}

@Composable
fun rememberListSelection(): ListSelectionState {
    val sel = remember { ListSelectionState() }
    PlatformBackHandler(enabled = sel.active && LocalTabActive.current) { sel.clear() }
    return sel
}

/** A row that can be swiped: to the right edits, to the left deletes (with "Undo" afterwards). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EditDeleteSwipe(enabled: Boolean, onEdit: () -> Unit, onDelete: () -> Unit, content: @Composable () -> Unit) {
    if (!enabled) {
        content()
        return
    }
    val st = rememberSwipeToDismissBoxState(confirmValueChange = { v ->
        when (v) {
            SwipeToDismissBoxValue.EndToStart -> onDelete()
            SwipeToDismissBoxValue.StartToEnd -> onEdit()
            else -> {}
        }
        // The row springs back; editing opens on top, a deleted row disappears (Undo brings it back).
        false
    })
    val t = g
    SwipeToDismissBox(
        st,
        backgroundContent = {
            val left = st.dismissDirection == SwipeToDismissBoxValue.EndToStart
            Row(
                Modifier.fillMaxSize().background(if (left) t.danger else t.accent, androidx.compose.foundation.shape.RoundedCornerShape(20.dp)).padding(horizontal = 24.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = if (left) Arrangement.End else Arrangement.Start,
            ) {
                Icon(if (left) Icons.Filled.Delete else Icons.Filled.Edit, null, tint = Color.White)
            }
        },
    ) { Box { content() } }
}
