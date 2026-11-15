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
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.JobInfo
import de.danerd.ghulsuite.ui.JobStatus
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Runs only while the vault is unlocked: shows a notification with a "Lock" action and
 * detects shaking (panic lock). Stops as soon as the vault is locked. While a background job runs
 * (upload, mail archive), the notification shows it with its progress and "Cancel"; a job that
 * ends while the app is not in front leaves a short notification.
 */
class UnlockedService : Service() {
    private var shake: ShakeDetector? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var watching = false

    override fun onBind(intent: android.content.Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_LOCK) {
            ghul.backend.lock()
            stopSelf()
            return START_NOT_STICKY
        }
        if (intent?.action == ACTION_CANCEL_JOBS) {
            JobStatus.cancelAll()
            return START_NOT_STICKY
        }
        if (!ghul.backend.unlocked.value) {
            stopSelf()
            return START_NOT_STICKY
        }
        val notification = buildNotification(JobStatus.current.value)
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        startShakeDetection()
        watchJobs()
        return START_NOT_STICKY
    }

    /** The notification follows the running job; a finished one is announced while the app is away. */
    private fun watchJobs() {
        if (watching) return
        watching = true
        val nm = getSystemService(NotificationManager::class.java)
        scope.launch {
            JobStatus.current.collect { job ->
                if (ghul.backend.unlocked.value) runCatching { nm.notify(NOTIFICATION_ID, buildNotification(job)) }
                // Android drops updates that come faster; the newest state is taken after the pause.
                delay(700)
            }
        }
        scope.launch {
            JobStatus.finished.collect { end -> if (!AppLaunch.inFront.value) postDone(end) }
        }
    }

    private fun postDone(end: de.danerd.ghulsuite.ui.JobEnd) {
        val text = end.text
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(JOBS_CHANNEL, getString(R.string.notif_job_channel), NotificationManager.IMPORTANCE_DEFAULT))
        val open = PendingIntent.getActivity(
            this, 2, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        // On the lock screen only that something is done (names of files stay private).
        val public = Notification.Builder(this, JOBS_CHANNEL)
            .setSmallIcon(R.drawable.ic_lock)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(end.publicText)
            .build()
        val n = Notification.Builder(this, JOBS_CHANNEL)
            .setSmallIcon(R.drawable.ic_lock)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(text)
            .setStyle(Notification.BigTextStyle().bigText(text))
            .setContentIntent(open)
            .setAutoCancel(true)
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
            .build()
        // Without the permission for notifications (Android 13+, asked once after unlocking) nothing shows.
        runCatching { nm.notify(DONE_ID, n) }
    }

    private fun buildNotification(job: JobInfo?): Notification {
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
        val plain = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_lock)
            .setContentTitle(getString(R.string.notif_title))
            .setContentText(getString(R.string.notif_text))
            .setContentIntent(open)
            .setOngoing(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(Notification.Action.Builder(null, getString(R.string.notif_lock), lock).build())
        if (job == null || job.title.isEmpty()) return plain.build()
        // A job runs: its title, how far, "Cancel" and "Lock". The lock screen shows the plain version.
        val cancel = PendingIntent.getService(
            this, 3, Intent(this, UnlockedService::class.java).setAction(ACTION_CANCEL_JOBS), PendingIntent.FLAG_IMMUTABLE,
        )
        val detail = listOfNotNull(
            if (job.total > 1) getString(R.string.notif_job_of, job.done, job.total) else null,
            job.item.takeIf { it.isNotEmpty() },
        ).joinToString(" · ")
        val p = job.progress
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_lock)
            .setContentTitle(job.title)
            .setContentText(detail)
            .setProgress(100, ((p ?: 0f) * 100).toInt(), p == null)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(plain.build())
            .addAction(Notification.Action.Builder(null, getString(R.string.notif_job_cancel), cancel).build())
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
        scope.cancel()
        shake?.stop()
        shake = null
        super.onDestroy()
    }

    companion object {
        const val ACTION_LOCK = "de.danerd.ghulsuite.LOCK"
        const val ACTION_CANCEL_JOBS = "de.danerd.ghulsuite.CANCEL_JOBS"
        private const val CHANNEL = "unlocked"
        private const val JOBS_CHANNEL = "jobs"
        private const val NOTIFICATION_ID = 1
        private const val DONE_ID = 3
    }
}
