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
 * New-mail notifications while GhulSuite is locked (opt-in per account). Instant via
 * [MailPushService]; a check every 15 minutes remains as a fallback.
 *
 * The suite stays locked; the worker only needs the inbox login. A copy of it is kept encrypted
 * with a key of the Android Keystore that never leaves the device and is removed when
 * notifications are turned off, the account is removed or the panic lock is used. Notifications
 * show only the account name and the number of new messages – no sender, no subject.
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

    /** Panic lock: no login copy remains on the device. */
    fun disableAll() {
        prefs.edit().clear().apply()
        runCatching { keyStore.deleteEntry(ALIAS) }
        WorkManager.getInstance(context).cancelUniqueWork(WORK)
        MailPushService.stop(context)
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

    private fun schedule() {
        val request = PeriodicWorkRequestBuilder<MailCheckWorker>(15, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    companion object {
        private const val ALIAS = "ghulsuite.mail.notify"
        private const val WORK = "ghulsuite.mail.check"
        private const val DETAIL = "detail"
        const val CHANNEL = "mail"
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
            if (since > 0 && r.newMessages > 0u) postMailNotification(applicationContext, config, r.newMessages.toInt(), r.newestFrom, r.newestSubject, r.unseen.toInt())
            n.setLastUid(id, r.nextUid.toInt())
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
fun postMailNotification(ctx: Context, config: MailPollConfig, count: Int, from: String = "", subject: String = "", unseen: Int = count) {
    val nm = ctx.getSystemService(NotificationManager::class.java)
    nm.createNotificationChannel(
        NotificationChannel(MailNotifications.CHANNEL, ctx.getString(R.string.mail_channel), NotificationManager.IMPORTANCE_DEFAULT),
    )
    val open = PendingIntent.getActivity(
        ctx, 2,
        Intent(ctx, MainActivity::class.java)
            .setAction(MainActivity.ACTION_OPEN_MAIL)
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
}
