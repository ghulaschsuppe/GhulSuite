// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.CalAlarmUi
import de.danerd.ghulsuite.ui.AppLaunch
import java.awt.SystemTray
import java.awt.TrayIcon
import java.util.Timer
import kotlin.concurrent.timerTask

/**
 * GhulCal on Windows: reminders appear as Windows notifications (via the notification area)
 * while GhulSuite runs, also when it is locked. Nothing is written to disk.
 */
class DesktopCalBackend(private val window: () -> java.awt.Window?, suite: () -> GhulSuite) : CoreCalBackend(suite) {
    @Volatile private var pending: List<CalAlarmUi> = emptyList()
    @Volatile private var sound = true
    private var tray: TrayIcon? = null
    private var lastKey: String? = null
    private val timer = Timer("ghulcal-reminders", true).apply {
        scheduleAtFixedRate(timerTask { fireDue() }, 20_000, 20_000)
    }

    override fun scheduleReminders(alarms: List<CalAlarmUi>, sound: Boolean) {
        val now = System.currentTimeMillis()
        pending = alarms.filter { it.atMillis > now - 60_000 }.sortedBy { it.atMillis }
        this.sound = sound
    }

    override fun clearReminders() {
        pending = emptyList()
    }

    override fun showReminder(alarm: CalAlarmUi, sound: Boolean) = show(alarm)

    private fun fireDue() {
        val now = System.currentTimeMillis()
        val due = pending.filter { it.atMillis <= now }
        if (due.isEmpty()) return
        pending = pending - due.toSet()
        // Windows shows the full text only while the session is unlocked; when Windows is locked,
        // the lock-screen version is used.
        due.forEach { show(it) }
    }

    private fun show(a: CalAlarmUi) {
        if (!SystemTray.isSupported()) return
        val icon = tray ?: runCatching {
            val img = baseIcon ?: return
            TrayIcon(img, "GhulSuite").apply {
                isImageAutoSize = true
                addActionListener { open() }
                SystemTray.getSystemTray().add(this)
            }
        }.getOrNull()?.also { tray = it } ?: return
        lastKey = a.key
        val locked = WinNative.workstationLocked()
        val title = if (locked) a.publicTitle else a.title
        val text = if (locked) a.publicText else a.text
        icon.displayMessage(title, text, if (sound) TrayIcon.MessageType.INFO else TrayIcon.MessageType.NONE)
    }

    /** Click on the notification: bring GhulSuite to the front and open the event. */
    private fun open() {
        lastKey?.takeIf { it.substringBefore('|').isNotEmpty() }?.let { AppLaunch.openEvent.value = it }
        java.awt.EventQueue.invokeLater {
            window()?.let { w ->
                (w as? java.awt.Frame)?.let { f -> if (f.state == java.awt.Frame.ICONIFIED) f.state = java.awt.Frame.NORMAL }
                w.toFront()
                w.requestFocus()
            }
        }
    }

    @Suppress("unused")
    private val keep = timer
}
