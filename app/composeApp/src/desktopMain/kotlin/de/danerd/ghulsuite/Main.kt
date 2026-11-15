// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.DesktopBack
import de.danerd.ghulsuite.ui.GhulSuiteApp
import java.awt.AWTEvent
import java.awt.Toolkit
import java.awt.event.WindowEvent
import java.awt.event.WindowFocusListener
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay

/** Without any input in the window for this long, GhulSuite locks itself (also while in front). */
private const val IDLE_LOCK_MS = 15 * 60 * 1000L

fun main() {
    WinNative.hardenDllSearch()
    // B10: decrypted copies a program still held open last time are removed now.
    ShareFiles.wipe()
    // The core library lies next to the program (installer, portable folder, Gradle run).
    System.getProperty("compose.application.resources.dir")?.let { System.setProperty("jna.library.path", it) }
    Thread.setDefaultUncaughtExceptionHandler { _, e ->
        runCatching { File(DesktopPaths.dataDir, "absturz.txt").writeText(e.stackTraceToString()) }
    }

    var awtWindow: java.awt.Window? = null
    val backend = DesktopVaultBackend { awtWindow }
    // The language chosen in the app (otherwise the system's).
    backend.settings.appLanguage.takeIf { it.isNotEmpty() }?.let { java.util.Locale.setDefault(java.util.Locale.forLanguageTag(it)) }
    val state = AppState(backend, CoroutineScope(SupervisorJob() + Dispatchers.Main))

    // Activity tracking for the idle lock.
    var lastInput = System.currentTimeMillis()
    Toolkit.getDefaultToolkit().addAWTEventListener(
        { lastInput = System.currentTimeMillis() },
        AWTEvent.KEY_EVENT_MASK or AWTEvent.MOUSE_EVENT_MASK or AWTEvent.MOUSE_WHEEL_EVENT_MASK,
    )
    // The back button of the mouse in GhulSuite's window acts as Esc there: an open dialog, sheet
    // or menu closes first, as with the back key on Android; then a page. (It arrives as an AWT
    // event only; in other windows, e.g. choosing a file, it does nothing here.)
    Toolkit.getDefaultToolkit().addAWTEventListener(
        { e ->
            if (e is java.awt.event.MouseEvent && DesktopBack.isBackButton(e)) {
                val w = awtWindow
                if (w != null && (e.component === w || javax.swing.SwingUtilities.getWindowAncestor(e.component) === w)) {
                    val target = w.mostRecentFocusOwner ?: w
                    for (id in intArrayOf(java.awt.event.KeyEvent.KEY_PRESSED, java.awt.event.KeyEvent.KEY_RELEASED)) {
                        target.dispatchEvent(java.awt.event.KeyEvent(target, id, System.currentTimeMillis(), 0, java.awt.event.KeyEvent.VK_ESCAPE, java.awt.event.KeyEvent.CHAR_UNDEFINED))
                    }
                }
            }
        },
        AWTEvent.MOUSE_EVENT_MASK,
    )

    application {
        // Unread mails appear as a number on a small envelope in the taskbar icon (only while unlocked).
        val unread = if (backend.unlocked.collectAsState().value) state.mailUnread else 0
        val icon = remember(unread) { badgeIcon(unread) }
        Window(
            onCloseRequest = {
                backend.lock()
                exitApplication()
            },
            title = "GhulSuite",
            icon = icon,
            state = rememberWindowState(size = DpSize(1280.dp, 820.dp)),
            // Esc and Alt+Left go back one step, as the back key on Android (after a text field,
            // menu or dialog had the chance to use Esc itself).
            onKeyEvent = { e -> DesktopBack.onKey(e) },
            onPreviewKeyEvent = { e ->
                // Ctrl+L locks at once (like Win+L for Windows).
                if (e.type == KeyEventType.KeyDown && e.isCtrlPressed && e.key == Key.L) {
                    // What waits for "Undo" is carried out first, as with every lock button.
                    state.lockNow()
                    true
                } else {
                    false
                }
            },
        ) {
            awtWindow = window
            window.minimumSize = java.awt.Dimension(380, 560)

            // Keep vault contents out of screenshots and screen sharing unless allowed.
            LaunchedEffect(Unit) {
                snapshotFlow { backend.settings.allowScreenshots }.collect { allowed ->
                    WinNative.setCaptureProtection(window, !allowed)
                }
            }

            // Auto-lock: window in the background, Windows locked, or idle for long.
            LaunchedEffect(Unit) {
                var lostFocusAt: Long? = null
                window.addWindowFocusListener(object : WindowFocusListener {
                    override fun windowGainedFocus(e: WindowEvent) {
                        lostFocusAt = null
                    }

                    override fun windowLostFocus(e: WindowEvent) {
                        // Our own dialogs (choose file, print) are not "in the background".
                        if (e.oppositeWindow == null) lostFocusAt = System.currentTimeMillis()
                    }
                })
                while (true) {
                    delay(1000)
                    if (!backend.unlocked.value) continue
                    val now = System.currentTimeMillis()
                    val away = lostFocusAt?.let { now - it } ?: 0L
                    // A system dialog (e.g. Windows asking for the key PIN) must not lock.
                    val waitingForKey = state.busy
                    // A few seconds of grace, so that a short switch (e.g. to a system dialog) does not lock.
                    val autoLock = maxOf(backend.settings.autoLockSeconds * 1000L, 3000L)
                    when {
                        backend.settings.lockOnScreenOff && WinNative.workstationLocked() -> state.lockNow()
                        !waitingForKey && lostFocusAt != null && away >= autoLock -> state.lockNow()
                        !waitingForKey && now - lastInput >= maxOf(IDLE_LOCK_MS, autoLock) -> state.lockNow()
                    }
                }
            }

            GhulSuiteApp(state)
        }
    }
}
