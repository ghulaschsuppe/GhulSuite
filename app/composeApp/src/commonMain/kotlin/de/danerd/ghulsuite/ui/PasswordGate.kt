// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.stringResource

// M13, M14: who finds the app unlocked for a moment must not be able to weaken its protection
// (auto-lock, lock on screen off, shake, screenshots, clipboard) or photograph a 2FA secret. These
// steps ask for the master password first; nothing changes before it was checked by the core.

/** Whether a new auto-lock time protects less than [old] (longer). */
fun autoLockWeakens(old: Int, new: Int): Boolean = new > old

/** Whether a new clipboard time protects less than [old] (longer, or "never", which is 0). */
fun clipboardWeakens(old: Int, new: Int): Boolean = when {
    new == old -> false
    new == 0 -> true
    old == 0 -> false
    else -> new > old
}

/**
 * Asks for the master password and runs [onConfirmed] once the core confirmed it. [text] says what
 * is about to happen (and why it matters). A wrong password changes nothing.
 */
@Composable
fun PasswordGateDialog(
    state: AppState,
    title: String,
    text: String,
    confirmLabel: String = stringResource(Res.string.a_apply),
    onDismiss: () -> Unit,
    /** Gets the password that was confirmed (for a call of the core that wants it as well). */
    onConfirmed: (password: String) -> Unit,
) {
    var password by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val wrong = stringResource(Res.string.err_wrong_password)
    AlertDialog(
        onDismissRequest = { if (!state.busy) onDismiss() },
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(text)
                Text(stringResource(Res.string.gate_password_hint))
                SecretTextField(password, { password = it; error = null }, stringResource(Res.string.l_master_password),
                    modifier = Modifier.fillMaxWidth().testTag("gate-password"))
                BusyBanner(state)
                ErrorText(error)
            }
        },
        confirmButton = {
            TextButton(onClick = {
                state.run(onError = { error = it }, block = { state.backend.verifyPassword(password) }) { ok ->
                    if (ok) {
                        val confirmed = password
                        password = ""
                        onConfirmed(confirmed)
                    } else {
                        error = wrong
                    }
                }
            }, enabled = !state.busy && password.isNotEmpty(), modifier = Modifier.testTag("gate-confirm")) { Text(confirmLabel) }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !state.busy) { Text(stringResource(Res.string.a_cancel)) } },
    )
}
