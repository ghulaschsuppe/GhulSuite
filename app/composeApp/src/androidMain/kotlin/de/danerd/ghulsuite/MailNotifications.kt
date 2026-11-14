// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import de.danerd.ghulsuite.core.MailPollConfig
import de.danerd.ghulsuite.core.MailSecurity
import de.danerd.ghulsuite.core.mailPoll
import java.security.KeyStore
import java.util.concurrent.TimeUnit
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * New-mail notifications, also while GhulSuite is locked, after a restart of the phone and when
 * the app was swiped away. On by default for every account on this device (it can be switched
 * off per account). Instant via [MailPushService]; a check every 15 minutes remains as a
 * fallback and brings the instant notifications back when Android stopped them.
 *
 * The suite stays locked; the worker only needs the inbox login. A copy of it is kept encrypted
 * with a key of the Android Keystore that never leaves the device. It is refreshed at every
 * unlock (a password changed on another device arrives this way) and removed when notifications
 * are turned off or the account is removed. By default notifications show only the account name
 * and the number of new messages.
 */
class MailNotifications(private val context: Context) {
    private val prefs = context.getSharedPreferences("mail_notify", Context.MODE_PRIVATE)
    private val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    fun isOn(accountId: String): Boolean = prefs.contains("$accountId.cfg")

    /** 0 = only "new mail", 1 = with sender, 2 = with sender and subject (device setting). */
    var detail: Int
        get() = prefs.getInt(DETAIL, 0)
        set(v) = prefs.edit().putInt(DETAIL, v.coerceIn(0, 2)).apply()

    fun accounts(): List<String> = prefs.all.keys.filter { it.endsWith(".cfg") }.map { it.removeSuffix(".cfg") }

    fun enable(config: MailPollConfig) {
        val json = JSONObject()
            .put("id", config.accountId)
            .put("name", config.accountName)
            .put("host", config.host)
            .put("port", config.port.toInt())
            .put("tls", config.security == MailSecurity.TLS)
            .put("user", config.username)
            .put("password", config.password)
            .put("cert", config.cert ?: "")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val ct = cipher.doFinal(json.toString().toByteArray())
        prefs.edit().putString("${config.accountId}.cfg", Base64.encodeToString(cipher.iv + ct, Base64.NO_WRAP)).apply()
        schedule()
        MailPushService.start(context)
    }

    fun disable(accountId: String) {
        prefs.edit().remove("$accountId.cfg").remove("$accountId.uid").apply()
        if (accounts().isEmpty()) {
            WorkManager.getInstance(context).cancelUniqueWork(WORK)
            MailPushService.stop(context)
        }
    }

    /** The user chose on this device whether this account notifies (otherwise: on by default). */
    fun decided(accountId: String): Boolean = prefs.getBoolean("$accountId.decided", false)

    fun setDecided(accountId: String) = prefs.edit().putBoolean("$accountId.decided", true).apply()

    /**
     * After unlocking: every account notifies unless switched off on this device; stored login
     * copies follow changes from other devices; removed accounts lose theirs. Returns whether an
     * account was switched on for the first time.
     */
    fun refresh(configs: List<MailPollConfig>): Boolean {
        var newlyOn = false
        val ids = configs.map { it.accountId }.toSet()
        for (gone in accounts().filter { it !in ids }) disable(gone)
        for (c in configs) {
            when {
                isOn(c.accountId) -> if (config(c.accountId) != c) enable(c)
                !decided(c.accountId) -> {
                    enable(c)
                    setDecided(c.accountId)
                    newlyOn = true
                }
            }
        }
        // Also when nothing changed: make sure the check and the instant service are running.
        if (accounts().isNotEmpty()) ensureRunning()
        return newlyOn
    }

    fun config(accountId: String): MailPollConfig? = runCatching {
        val raw = Base64.decode(prefs.getString("$accountId.cfg", null) ?: return null, Base64.NO_WRAP)
        val key = keyStore.getKey(ALIAS, null) as? SecretKey ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw, 0, 12))
        val j = JSONObject(String(cipher.doFinal(raw, 12, raw.size - 12)))
        MailPollConfig(
            accountId = j.getString("id"),
            accountName = j.getString("name"),
            host = j.getString("host"),
            port = j.getInt("port").toUShort(),
            security = if (j.getBoolean("tls")) MailSecurity.TLS else MailSecurity.START_TLS,
            username = j.getString("user"),
            password = j.getString("password"),
            cert = j.getString("cert").ifEmpty { null },
        )
    }.getOrNull()

    fun lastUid(accountId: String): Int = prefs.getInt("$accountId.uid", 0)

    fun setLastUid(accountId: String, uid: Int) = prefs.edit().putInt("$accountId.uid", uid).apply()

    /**
     * Moves the account from [since] to [next] if nobody else did in the meantime. The periodic
     * check and the instant notifications run side by side; only the one that claims the new
     * mails reports them, so a mail never rings twice.
     */
    fun claim(accountId: String, since: Int, next: Int): Boolean = synchronized(CLAIM_LOCK) {
        if (lastUid(accountId) != since) return false
        prefs.edit().putInt("$accountId.uid", next).commit()
        true
    }

    private fun key(): SecretKey {
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    /** The periodic check and the instant service (after a restart of the phone, for example). */
    fun ensureRunning() {
        schedule()
        MailPushService.start(context)
    }

    private fun schedule() {
        val request = PeriodicWorkRequestBuilder<MailCheckWorker>(15, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    companion object {
        private val CLAIM_LOCK = Any()
        private const val ALIAS = "ghulsuite.mail.notify"
        private const val WORK = "ghulsuite.mail.check"
        private const val DETAIL = "detail"
        // B4: new IDs (a channel's lock screen setting is fixed once created), private on the
        // lock screen also with Android's default "show all content".
        const val CHANNEL = "mail_private"
        const val OLD_CHANNEL = "mail"
    }
}

/** Checks the inboxes with notifications on and posts one neutral notification per account. */
class MailCheckWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val n = MailNotifications(applicationContext)
        for (id in n.accounts()) {
            val config = n.config(id) ?: continue
            val since = n.lastUid(id)
            val r = runCatching { mailPoll(config, since.toUInt()) }.getOrNull() ?: continue
            if (!n.claim(id, since, r.nextUid.toInt())) continue
            if (since > 0 && r.newMessages > 0u) postMailNotification(applicationContext, config, r.newMessages.toInt(), r.newestFrom, r.newestSubject, r.unseen.toInt(), r.newestUid.toInt())
        }
        // Brings the instant notifications back if Android stopped them (works only when allowed).
        MailPushService.start(applicationContext)
        Result.success()
    }
}

/**
 * One notification per account. By default neutral (account name and number of new messages); on
 * request with the sender or sender and subject of the newest mail. Encrypted mails have no
 * readable subject ("..."). The lock screen always shows only the neutral version.
 */
fun postMailNotification(
    ctx: Context,
    config: MailPollConfig,
    count: Int,
    from: String = "",
    subject: String = "",
    unseen: Int = count,
    newestUid: Int = 0,
) {
    val nm = ctx.getSystemService(NotificationManager::class.java)
    nm.deleteNotificationChannel(MailNotifications.OLD_CHANNEL)
    nm.createNotificationChannel(
        NotificationChannel(MailNotifications.CHANNEL, ctx.getString(R.string.mail_channel), NotificationManager.IMPORTANCE_DEFAULT)
            .apply { lockscreenVisibility = Notification.VISIBILITY_PRIVATE },
    )
    // One new mail opens that mail; several open the account's inbox. One request code per
    // account, so the notifications of two accounts do not overwrite each other's target.
    val open = PendingIntent.getActivity(
        ctx, config.accountId.hashCode(),
        Intent(ctx, MainActivity::class.java)
            .setAction(MainActivity.ACTION_OPEN_MAIL)
            .putExtra(MainActivity.EXTRA_MAIL_ACCOUNT, config.accountId)
            .putExtra(MainActivity.EXTRA_MAIL_UID, if (count == 1) newestUid else 0)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    val neutral = ctx.resources.getQuantityString(R.plurals.mail_new, count, count)
    val detail = MailNotifications(ctx).detail
    val readableSubject = subject.takeIf { it.isNotBlank() && it != "..." }
    val title = if (detail >= 1 && from.isNotBlank()) from else config.accountName
    val text = when {
        detail >= 2 && readableSubject != null -> readableSubject + if (count > 1) " (+${count - 1})" else ""
        detail >= 1 && from.isNotBlank() -> neutral + " · " + config.accountName
        else -> neutral
    }
    val public = Notification.Builder(ctx, MailNotifications.CHANNEL)
        .setSmallIcon(R.drawable.ic_mail)
        .setContentTitle(config.accountName)
        .setContentText(neutral)
        .build()
    val notification = Notification.Builder(ctx, MailNotifications.CHANNEL)
        .setSmallIcon(R.drawable.ic_mail)
        .setContentTitle(title)
        .setContentText(text)
        .setContentIntent(open)
        .setAutoCancel(true)
        // Launchers show this as the number on the app icon (with the mail symbol of the notification).
        .setNumber(unseen.coerceAtLeast(count))
        .setCategory(Notification.CATEGORY_EMAIL)
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(public)
        .build()
    runCatching { nm.notify(config.accountId.hashCode(), notification) }
    de.danerd.ghulsuite.ui.AppLaunch.newMail.value++
}
