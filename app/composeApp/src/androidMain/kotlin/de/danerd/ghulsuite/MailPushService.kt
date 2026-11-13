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

    /**
     * Bumped when the network changes: every account gets a fresh connection at once instead of
     * waiting on one that silently died (Wi-Fi to mobile data, router restart) or in a back-off.
     */
    @Volatile private var generation = 0
    private var network: android.net.ConnectivityManager.NetworkCallback? = null

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
        watchNetwork()
        startWatchers(accounts)
        return START_STICKY
    }

    private fun startWatchers(accounts: List<String>) {
        synchronized(watchers) {
            watchers.entries.removeAll { !it.value.isAlive || it.value.name != threadName(it.key) }
            for (id in accounts) {
                if (id !in watchers) {
                    val gen = generation
                    watchers[id] = Thread({ watch(id, gen) }, threadName(id)).apply { isDaemon = true; start() }
                }
            }
        }
    }

    private fun threadName(id: String) = "mail-push-$id-$generation"

    /** A new or changed network: reconnect every account right away. */
    private fun watchNetwork() {
        if (network != null) return
        val cm = getSystemService(android.net.ConnectivityManager::class.java) ?: return
        val cb = object : android.net.ConnectivityManager.NetworkCallback() {
            private var last: android.net.Network? = cm.activeNetwork

            // Called at once for the current network too: only a different one reconnects.
            override fun onAvailable(net: android.net.Network) {
                if (net != last) reconnect()
                last = net
            }
        }
        runCatching { cm.registerDefaultNetworkCallback(cb) }.onSuccess { network = cb }
    }

    private fun reconnect() {
        if (!running) return
        synchronized(watchers) {
            generation++
            watchers.clear()
        }
        startWatchers(MailNotifications(this).accounts())
    }

    override fun onDestroy() {
        running = false
        network?.let { cb -> runCatching { getSystemService(android.net.ConnectivityManager::class.java).unregisterNetworkCallback(cb) } }
        network = null
        super.onDestroy()
    }

    private fun watch(accountId: String, gen: Int) {
        val n = MailNotifications(this)
        var backoff = 5_000L
        while (running && gen == generation && accountId in n.accounts()) {
            val config = n.config(accountId) ?: break
            val since = n.lastUid(accountId)
            try {
                // A few minutes per round: keeps the connection alive behind routers and mobile
                // networks that drop idle connections after about five minutes.
                val r = mailWait(config, since.toUInt(), WAIT_SECONDS)
                if (!running || accountId !in n.accounts()) break
                // Claimed atomically: an older connection or the periodic check may report too.
                if (n.claim(accountId, since, r.nextUid.toInt()) && since > 0 && r.newMessages > 0u) {
                    postMailNotification(this, config, r.newMessages.toInt(), r.newestFrom, r.newestSubject, r.unseen.toInt(), r.newestUid.toInt())
                }
                backoff = 5_000L
            } catch (e: Exception) {
                // Offline or server trouble: try again later, up to every 5 minutes (a new
                // network starts a fresh connection at once, see [watchNetwork]).
                var waited = 0L
                while (waited < backoff && running && gen == generation) {
                    Thread.sleep(1_000L)
                    waited += 1_000L
                }
                backoff = (backoff * 2).coerceAtMost(300_000L)
            }
        }
        synchronized(watchers) {
            if (watchers[accountId]?.name == Thread.currentThread().name) watchers.remove(accountId)
            if (watchers.isEmpty() && gen == generation) stopSelf()
        }
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
        private const val WAIT_SECONDS = 240u

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

/**
 * Restarts mail reception after a restart of the phone or an app update: the instant service and
 * the periodic check (without opening the app).
 */
class MailPushBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action in BOOT_ACTIONS) {
            val n = MailNotifications(context)
            if (n.accounts().isNotEmpty()) n.ensureRunning()
        }
    }

    companion object {
        private val BOOT_ACTIONS = setOf(
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED,
            // Some manufacturers send their own "fast boot" broadcasts instead.
            "android.intent.action.QUICKBOOT_POWERON",
            "com.htc.intent.action.QUICKBOOT_POWERON",
        )
    }
}
