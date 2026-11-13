// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
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
 * Bar above a list while items are selected (long press starts it): close, count, "all",
 * extra actions and delete.
 */
@Composable
fun SelectionHeader(
    count: Int,
    onClose: () -> Unit,
    onAll: () -> Unit,
    onDelete: () -> Unit,
    extra: List<Triple<ImageVector, String, () -> Unit>> = emptyList(),
) {
    val t = g
    Row(
        Modifier.fillMaxWidth().background(t.surface).padding(horizontal = 4.dp).height(56.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        IconButton(onClick = onClose) { Icon(Icons.Filled.Close, stringResource(Res.string.a_cancel)) }
        Text(stringResource(Res.string.drive_selected, count.toString()), Modifier.weight(1f), fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
        TextButton(onClick = onAll) { Text(stringResource(Res.string.a_select_all)) }
        for ((icon, label, action) in extra) IconButton(onClick = action) { Icon(icon, label) }
        IconButton(onClick = onDelete) { Icon(Icons.Filled.Delete, stringResource(Res.string.a_delete), tint = t.danger) }
    }
}

/** A row that can be swiped: to the right edits, to the left deletes (the caller asks first). */
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
        // The row springs back; editing and the delete question open on top.
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
