// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Checks of the development build (Android `…ghulsuite.dev`, desktop tests): mistakes that a user
 * would not see as an error, but that are one, become loud. Each alarm is
 *   - written to the log as a line starting with [MARK] (the emulator runs fail on it),
 *   - shown as a red bar at the top of the app (it is in every screenshot).
 * Alarms:
 *   - an action was dropped because another one was still busy, although it was started by the app
 *     itself (a follow-up action, bug F49), not by a second tap of the user,
 *   - an unexpected error reached the screen: not an error of the core with a clear text, but a
 *     programming error or a cancellation (bug F51),
 * The stable app never does any of this ([enabled] stays false).
 */
object DevChecks {
    const val MARK = "GHUL-DEV-ALARM"

    /** Set at start: true for the development build. */
    var enabled = false

    /** The alarms of this run, newest last. */
    val alarms = mutableStateListOf<String>()

    fun alarm(what: String) {
        if (!enabled) return
        println("$MARK: $what")
        alarms += what
    }

    /** An error is about to be shown: is it one a user should ever see? */
    fun shown(e: Throwable) {
        if (!enabled) return
        val name = e::class.simpleName.orEmpty()
        val expected = name.contains("IOException") || name.contains("Timeout") || name.contains("Network") ||
            name.contains("UnknownHost") || name.contains("Connect") || name.contains("Socket") ||
            // Only on a computer without a display (the desktop tests), never in the app itself.
            name == "HeadlessException"
        if (!expected) alarm("unerwarteter Fehler angezeigt: $name: ${e.message}")
    }
}

/** Red bar with the alarms of the development build; a tap clears it. Nothing in the stable app. */
@Composable
fun DevAlarmBar(modifier: Modifier = Modifier) {
    if (!DevChecks.enabled || DevChecks.alarms.isEmpty()) return
    Column(
        modifier.fillMaxWidth().background(Color(0xFFB00020)).clickable { DevChecks.alarms.clear() }.padding(8.dp),
    ) {
        for (a in DevChecks.alarms.takeLast(3)) {
            Text("${DevChecks.MARK}: $a", color = Color.White, fontSize = 12.sp)
        }
    }
}
