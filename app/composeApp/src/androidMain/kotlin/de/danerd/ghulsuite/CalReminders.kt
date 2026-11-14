// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.CalAlarmUi
import de.danerd.ghulsuite.ui.epochMillisOf
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONArray
import org.json.JSONObject

/**
 * GhulCal reminders on Android: exact alarms for the next 30 days, set while the suite is unlocked,
 * so they arrive while it is locked or the app is closed. The prepared texts are kept encrypted
 * with a key of the Android Keystore that never leaves the device; the panic lock deletes them.
 */
class CalReminders(private val context: Context) {
    private val prefs = context.getSharedPreferences("cal_alarms", Context.MODE_PRIVATE)
    private val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val alarms = context.getSystemService(AlarmManager::class.java)

    fun schedule(list: List<CalAlarmUi>, sound: Boolean) {
        cancelAll()
        val now = System.currentTimeMillis()
        val future = list.filter { it.atMillis > now - 60_000 }.take(MAX)
        val json = JSONArray()
        future.forEach { json.put(it.toJson()) }
        store(json.toString())
        prefs.edit().putInt(COUNT, future.size).putBoolean(SOUND, sound).apply()
        future.forEachIndexed { i, a -> setAlarm(i, a.atMillis) }
    }

    /**
     * After a reboot or a change of the time zone: the stored alarms again. A reminder belongs to
     * a time on the clock ("09:45"), so its moment is worked out anew for the zone the device is in now.
     */
    fun restore() {
        val list = load()
        val now = System.currentTimeMillis()
        list.forEachIndexed { i, a ->
            val at = if (a.atLocal.isEmpty()) a.atMillis else runCatching { epochMillisOf(a.atLocal) }.getOrDefault(a.atMillis)
            if (at > now - 10 * 60_000) setAlarm(i, maxOf(at, now + 5_000))
        }
    }

    fun clear() {
        cancelAll()
        prefs.edit().clear().apply()
        runCatching { keyStore.deleteEntry(ALIAS) }
    }

    fun alarm(index: Int): CalAlarmUi? = load().getOrNull(index)

    val sound: Boolean get() = prefs.getBoolean(SOUND, true)

    private fun setAlarm(index: Int, at: Long) {
        val pi = PendingIntent.getBroadcast(
            context, REQUEST + index,
            Intent(context, CalAlarmReceiver::class.java).setAction(ACTION_FIRE).putExtra(EXTRA_INDEX, index),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val exact = Build.VERSION.SDK_INT < 31 || alarms.canScheduleExactAlarms()
        if (exact) alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
        else alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
    }

    private fun cancelAll() {
        val n = prefs.getInt(COUNT, 0)
        for (i in 0 until maxOf(n, 0)) {
            val pi = PendingIntent.getBroadcast(
                context, REQUEST + i, Intent(context, CalAlarmReceiver::class.java).setAction(ACTION_FIRE),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE,
            ) ?: continue
            alarms.cancel(pi)
        }
    }

    private fun load(): List<CalAlarmUi> = runCatching {
        val raw = Base64.decode(prefs.getString(DATA, null) ?: return emptyList(), Base64.NO_WRAP)
        val key = keyStore.getKey(ALIAS, null) as? SecretKey ?: return emptyList()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw, 0, 12))
        val arr = JSONArray(String(cipher.doFinal(raw, 12, raw.size - 12)))
        (0 until arr.length()).map { arr.getJSONObject(it).toAlarm() }
    }.getOrDefault(emptyList())

    private fun store(json: String) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val ct = cipher.doFinal(json.toByteArray())
        prefs.edit().putString(DATA, Base64.encodeToString(cipher.iv + ct, Base64.NO_WRAP)).apply()
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

    companion object {
        private const val ALIAS = "ghulsuite.cal.alarms"
        private const val DATA = "data"
        private const val COUNT = "count"
        private const val SOUND = "sound"
        private const val REQUEST = 7000
        private const val MAX = 400
        const val ACTION_FIRE = "de.danerd.ghulsuite.CAL_FIRE"
        const val ACTION_SNOOZE = "de.danerd.ghulsuite.CAL_SNOOZE"
        const val ACTION_DONE = "de.danerd.ghulsuite.CAL_DONE"
        const val EXTRA_INDEX = "index"
        const val EXTRA_KEY = "key"
        // B4: new IDs, private on the lock screen also with Android's default settings.
        const val CHANNEL = "cal_private"
        const val CHANNEL_SILENT = "cal_silent_private"
        val OLD_CHANNELS = listOf("cal", "cal_silent")
    }
}

private fun CalAlarmUi.toJson() = JSONObject()
    .put("k", key).put("at", atMillis).put("t", title).put("x", text).put("pt", publicTitle).put("px", publicText).put("al", atLocal)

private fun JSONObject.toAlarm() =
    CalAlarmUi(getString("k"), getLong("at"), getString("t"), getString("x"), getString("pt"), getString("px"), optString("al"))

/**
 * B4: whether the device is locked right now (secure lock screen). Android lets only the user decide
 * whether a channel shows details on the lock screen; with "show all content" it shows the full
 * notification. So a notification that arrives while locked carries no more than its lock screen
 * version: nothing private can appear there, whatever the Android setting is.
 */
fun deviceLocked(ctx: Context): Boolean =
    runCatching { ctx.getSystemService(android.app.KeyguardManager::class.java)?.isDeviceLocked == true }.getOrDefault(true)

/** Posts a reminder: full text while unlocked, only the chosen lock screen detail while locked. */
fun postCalNotification(ctx: Context, a: CalAlarmUi, sound: Boolean, index: Int) {
    val nm = ctx.getSystemService(NotificationManager::class.java)
    CalReminders.OLD_CHANNELS.forEach { nm.deleteNotificationChannel(it) }
    nm.createNotificationChannel(
        NotificationChannel(CalReminders.CHANNEL, ctx.getString(R.string.cal_channel), NotificationManager.IMPORTANCE_HIGH)
    )
    nm.createNotificationChannel(
        NotificationChannel(CalReminders.CHANNEL_SILENT, ctx.getString(R.string.cal_channel_silent), NotificationManager.IMPORTANCE_LOW)
    )
    val channel = if (sound) CalReminders.CHANNEL else CalReminders.CHANNEL_SILENT
    val id = NOTIFY_BASE + (a.key.hashCode() and 0xffff)
    val open = PendingIntent.getActivity(
        ctx, id,
        Intent(ctx, MainActivity::class.java).setAction(MainActivity.ACTION_OPEN_EVENT).putExtra(CalReminders.EXTRA_KEY, a.key)
            .putExtra(IntentToken.EXTRA, IntentToken.get(ctx))
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    fun action(act: String, req: Int) = PendingIntent.getBroadcast(
        ctx, req,
        Intent(ctx, CalAlarmReceiver::class.java).setAction(act).putExtra(CalReminders.EXTRA_INDEX, index).putExtra("notify_id", id),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    val color = 0xFFD6336C.toInt()
    val public = Notification.Builder(ctx, channel)
        .setSmallIcon(R.drawable.ic_event).setColor(color)
        .setContentTitle(a.publicTitle).setContentText(a.publicText)
        .build()
    val locked = deviceLocked(ctx)
    val title = if (locked) a.publicTitle else a.title
    val text = if (locked) a.publicText else a.text
    val n = Notification.Builder(ctx, channel)
        .setSmallIcon(R.drawable.ic_event).setColor(color)
        .setContentTitle(title).setContentText(text)
        .setStyle(Notification.BigTextStyle().bigText(text))
        .setCategory(Notification.CATEGORY_REMINDER)
        .setContentIntent(open)
        .setAutoCancel(true)
        .setVisibility(Notification.VISIBILITY_PRIVATE)
        .setPublicVersion(public)
        .addAction(Notification.Action.Builder(null, ctx.getString(R.string.cal_snooze), action(CalReminders.ACTION_SNOOZE, id + 1)).build())
        .addAction(Notification.Action.Builder(null, ctx.getString(R.string.cal_done), action(CalReminders.ACTION_DONE, id + 2)).build())
        .build()
    runCatching { nm.notify(id, n) }
}

private const val NOTIFY_BASE = 0x40000

/** Alarm went off, "again in 5 minutes", "done". */
class CalAlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val r = CalReminders(context)
        val index = intent.getIntExtra(CalReminders.EXTRA_INDEX, -1)
        when (intent.action) {
            CalReminders.ACTION_FIRE -> r.alarm(index)?.let { postCalNotification(context, it, r.sound, index) }
            CalReminders.ACTION_DONE -> context.getSystemService(NotificationManager::class.java).cancel(intent.getIntExtra("notify_id", 0))
            CalReminders.ACTION_SNOOZE -> {
                context.getSystemService(NotificationManager::class.java).cancel(intent.getIntExtra("notify_id", 0))
                val pi = PendingIntent.getBroadcast(
                    context, 6000 + index,
                    Intent(context, CalAlarmReceiver::class.java).setAction(CalReminders.ACTION_FIRE).putExtra(CalReminders.EXTRA_INDEX, index),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
                val am = context.getSystemService(AlarmManager::class.java)
                val at = System.currentTimeMillis() + 5 * 60_000
                if (Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()) am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
                else am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
            }
            Intent.ACTION_TIMEZONE_CHANGED -> {
                // Forget the remembered zone of this process, so the new one of the system counts.
                java.util.TimeZone.setDefault(null)
                r.restore()
            }
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED,
            "android.app.action.SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED" -> r.restore()
        }
    }
}

/** GhulCal on Android: the core plus exact alarms. */
class AndroidCalBackend(private val context: Context, suite: () -> GhulSuite) : CoreCalBackend(suite) {
    private val reminders = CalReminders(context)

    override fun scheduleReminders(alarms: List<CalAlarmUi>, sound: Boolean) = reminders.schedule(alarms, sound)

    override fun clearReminders() = reminders.clear()

    override fun showReminder(alarm: CalAlarmUi, sound: Boolean) = postCalNotification(context, alarm, sound, -1)
}
