// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Key
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.model.KeyChangeStateUi
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

/**
 * After a change of the protection every server gets a new SSH key. Where the hoster manages
 * keys in its web panel (the app cannot change them there), the user adds the new key and
 * removes the earlier one there; the app checks both.
 */
@Composable
fun KeyChangeDialog(state: AppState) {
    val kc = state.keyChange ?: return
    AlertDialog(
        onDismissRequest = { state.keyChangeLater = true },
        icon = { Icon(Icons.Filled.Key, null, tint = warnColor()) },
        title = { Text(stringResource(Res.string.kc_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(Res.string.kc_text))
                kc.servers.filter { it.state != KeyChangeStateUi.DONE }.forEach { s ->
                    Text(s.name.ifBlank { s.host }, fontWeight = FontWeight.Bold)
                    Text(
                        stringResource(
                            when (s.state) {
                                KeyChangeStateUi.ADD_BY_HAND -> Res.string.kc_add
                                KeyChangeStateUi.REMOVE_BY_HAND -> Res.string.kc_remove
                                else -> Res.string.kc_waiting
                            },
                        ),
                    )
                }
                if (kc.servers.any { it.state == KeyChangeStateUi.ADD_BY_HAND }) PublicKeyBox(state, kc.publicKey)
            }
        },
        confirmButton = {
            TextButton(onClick = { state.continueKeyChange() }, enabled = !state.busy) {
                Text(stringResource(Res.string.kc_check))
            }
        },
        dismissButton = {
            TextButton(onClick = { state.keyChangeLater = true }) { Text(stringResource(Res.string.a_later)) }
        },
    )
}
