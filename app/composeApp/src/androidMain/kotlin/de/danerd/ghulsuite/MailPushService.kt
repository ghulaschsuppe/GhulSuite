// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import de.danerd.ghulsuite.core.mailWait

/**
 * Instant new-mail notifications: keeps one connection per account open and lets the mail server
 * report new mail at once (IMAP IDLE) instead of asking every 15 minutes. Runs only while
 * notifications are on for at least one account, with a silent notification (Android requires one).
 * Uses the same encrypted login copy as [MailNotifications]; the suite stays locked.
 */
class MailPushService : Service() {
    @Volatile private var running = true
    private val watchers = mutableMapOf<String, Thread>()

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val accounts = MailNotifications(this).accounts()
        if (accounts.isEmpty()) {
            stopSelf()
            return START_NOT_STICKY
        }
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        synchronized(watchers) {
            watchers.entries.removeAll { !it.value.isAlive }
            for (id in accounts) if (id !in watchers) watchers[id] = Thread({ watch(id) }, "mail-push").apply { isDaemon = true; start() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        super.onDestroy()
    }

    private fun watch(accountId: String) {
        val n = MailNotifications(this)
        var backoff = 5_000L
        while (running && accountId in n.accounts()) {
            val config = n.config(accountId) ?: break
            val since = n.lastUid(accountId)
            try {
                // A few minutes per round: keeps the connection alive behind routers and notices changed logins.
                val r = mailWait(config, since.toUInt(), WAIT_SECONDS)
                if (!running || accountId !in n.accounts()) break
                if (since > 0 && r.newMessages > 0u) postMailNotification(this, config, r.newMessages.toInt(), r.newestFrom, r.newestSubject)
                n.setLastUid(accountId, r.nextUid.toInt())
                backoff = 5_000L
            } catch (e: Exception) {
                // Offline or server trouble: try again later, up to every 5 minutes.
                Thread.sleep(backoff)
                backoff = (backoff * 2).coerceAtMost(300_000L)
            }
        }
        synchronized(watchers) { watchers.remove(accountId) }
        if (synchronized(watchers) { watchers.isEmpty() }) stopSelf()
    }

    private fun buildNotification(): Notification {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.mail_push_channel), NotificationManager.IMPORTANCE_MIN).apply {
                setShowBadge(false)
            },
        )
        val open = PendingIntent.getActivity(
            this, 3, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_mail)
            .setContentTitle(getString(R.string.mail_push_title))
            .setContentText(getString(R.string.mail_push_text))
            .setContentIntent(open)
            .setOngoing(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .build()
    }

    companion object {
        private const val CHANNEL = "mail_push"
        private const val NOTIFICATION_ID = 7
        private const val WAIT_SECONDS = 540u

        fun start(context: Context) {
            if (MailNotifications(context).accounts().isEmpty()) return
            // Not allowed from the background on newer Android; the periodic check then still runs.
            runCatching { context.startForegroundService(Intent(context, MailPushService::class.java)) }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, MailPushService::class.java))
        }
    }
}

/** Restarts the instant notifications after a restart of the phone or an app update. */
class MailPushBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) {
            MailPushService.start(context)
        }
    }
}
