// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

/**
 * Runs only while the vault is unlocked: shows a notification with a "Lock" action and
 * detects shaking (panic lock). Stops as soon as the vault is locked.
 */
class UnlockedService : Service() {
    private var shake: ShakeDetector? = null

    override fun onBind(intent: android.content.Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_LOCK) {
            ghul.backend.lock()
            stopSelf()
            return START_NOT_STICKY
        }
        if (!ghul.backend.unlocked.value) {
            stopSelf()
            return START_NOT_STICKY
        }
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        // Also on ACTION_UPDATE: every start of the service sets its notification anew (archive progress).
        startShakeDetection()
        return START_NOT_STICKY
    }

    private fun buildNotification(): Notification {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.notif_channel), NotificationManager.IMPORTANCE_LOW).apply {
                setShowBadge(false)
                lockscreenVisibility = Notification.VISIBILITY_PUBLIC
            },
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val lock = PendingIntent.getService(
            this, 1, Intent(this, UnlockedService::class.java).setAction(ACTION_LOCK), PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_lock)
            .setContentTitle(getString(R.string.notif_title))
            .setContentText(ArchiveRun.progress?.let { getString(R.string.notif_archive, it) } ?: getString(R.string.notif_text))
            .setContentIntent(open)
            .setOngoing(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(Notification.Action.Builder(null, getString(R.string.notif_lock), lock).build())
            .build()
    }

    /**
     * Listens always while unlocked; [ShakeDetector] checks the setting on each sample, so
     * switching "shake to lock" on or off takes effect at once.
     */
    private fun startShakeDetection() {
        if (shake != null) return
        shake = ShakeDetector(this) {
            val wasUnlocked = ghul.backend.unlocked.value
            ghul.backend.panicLock()
            stopSelf()
            wasUnlocked
        }.also { it.start() }
    }

    override fun onDestroy() {
        shake?.stop()
        shake = null
        super.onDestroy()
    }

    companion object {
        const val ACTION_LOCK = "de.danerd.ghulsuite.LOCK"
        const val ACTION_UPDATE = "de.danerd.ghulsuite.UPDATE"
        private const val CHANNEL = "unlocked"
        private const val NOTIFICATION_ID = 1
    }
}
