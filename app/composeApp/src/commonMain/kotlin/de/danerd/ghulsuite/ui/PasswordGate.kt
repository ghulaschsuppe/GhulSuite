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

/**
 * r9 dieb M2: the core's rule for an app PIN that is too easy to guess (two tries are enough for
 * it), checked here first: one digit or a block of up to four repeated (11111111, 12121212,
 * 12341234), a run up or down (12345678, 87654321), or, with 8 digits, a date (DDMMYYYY,
 * MMDDYYYY, YYYYMMDD, years 1900 to 2099).
 */
fun appPinTooSimple(pin: String): Boolean {
    if (pin.isEmpty() || !pin.all { it in '0'..'9' }) return false
    val d = pin.map { it - '0' }
    val repeated = (1..4).any { n -> d.size % n == 0 && d.chunked(n).all { it == d.subList(0, n) } }
    val run = listOf(1, -1).any { step -> d.zipWithNext().all { (a, b) -> b - a == step } }
    fun num(from: Int, to: Int) = d.subList(from, to).fold(0) { a, x -> a * 10 + x }
    fun date(day: Int, month: Int, year: Int) = day in 1..31 && month in 1..12 && year in 1900..2099
    val dated = d.size == 8 && (date(num(0, 2), num(2, 4), num(4, 8)) || date(num(2, 4), num(0, 2), num(4, 8)) || date(num(6, 8), num(4, 6), num(0, 4)))
    return repeated || run || dated
}

/** How long one confirmation of the master password covers further weakening steps (device test). */
const val WEAKENING_VALID_MS = 2 * 60 * 1000L

/** From this many at once (not into a trash), deleting asks for the master password (r9 dieb N7). */
const val MANY_TO_DELETE = 10

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
    /**
     * A step that weakens a setting: one confirmation covers the next ones for
     * [WEAKENING_VALID_MS] (until a lock). Others (a 2FA secret, deleting for good) always ask.
     */
    weakening: Boolean = false,
    /** Gets the password that was confirmed (for a call of the core that wants it as well; empty when confirmed before). */
    onConfirmed: (password: String) -> Unit,
) {
    if (weakening && state.weakeningStillConfirmed()) {
        androidx.compose.runtime.LaunchedEffect(Unit) { onConfirmed("") }
        return
    }
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
                        if (weakening) state.noteWeakeningConfirmed()
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
