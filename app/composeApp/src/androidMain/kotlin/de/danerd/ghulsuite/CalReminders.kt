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
 * with a key of the Android Keystore that never leaves the device (not bound to an unlocked
 * device: they must be readable when a reminder comes on the locked phone). What is kept is only
 * the chosen detail (M12): below "everything" the full text is not even prepared; with
 * "everything" the lock screen shows (and the device keeps) title, time and place for 30 days.
 * The part for full texts with a key bound to an unlocked device is only written
 * when they differ from the lock screen part, which the app's detail levels never make; it stays
 * as a guard, not as a protection the app relies on. The reminders stay after a panic lock unless
 * the user chose otherwise (see test plan K11); deleting the suite removes them.
 */
class CalReminders(private val context: Context) {
    private val prefs = context.getSharedPreferences("cal_alarms", Context.MODE_PRIVATE)
    private val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private val alarms = context.getSystemService(AlarmManager::class.java)

    // N5: one at a time (unlocking and saving an event can both plan anew).
    @Synchronized
    fun schedule(list: List<CalAlarmUi>, sound: Boolean) {
        cancelAll()
        val now = System.currentTimeMillis()
        val future = list.filter { it.atMillis > now - 60_000 }.take(MAX)
        store(future)
        prefs.edit().putInt(COUNT, future.size).putBoolean(SOUND, sound).apply()
        future.forEachIndexed { i, a -> setAlarm(i, a.atMillis, a.key) }
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
            if (at > now - 10 * 60_000) setAlarm(i, maxOf(at, now + 5_000), a.key)
        }
    }

    fun clear() {
        cancelAll()
        prefs.edit().clear().apply()
        runCatching { keyStore.deleteEntry(ALIAS) }
        runCatching { keyStore.deleteEntry(ALIAS_FULL) }
    }

    /** The reminder of an alarm: by its key (the list may have been planned anew meanwhile), else its place. */
    fun alarm(index: Int, key: String? = null): CalAlarmUi? {
        val list = load()
        return key?.let { k -> list.firstOrNull { it.key == k } } ?: list.getOrNull(index)
    }

    /**
     * Reminders shown lately, by their key (also the test reminder): "again in 5 minutes" finds
     * its reminder by the key, not by a place in the list (which changes when the list is set anew).
     */
    fun remember(a: CalAlarmUi) {
        val all = loadFrom(SHOWN).filter { it.key != a.key }.takeLast(SHOWN_MAX - 1) + a
        storeTo(SHOWN, all)
    }

    fun shown(key: String): CalAlarmUi? = loadFrom(SHOWN).lastOrNull { it.key == key }

    val sound: Boolean get() = prefs.getBoolean(SOUND, true)

    private fun setAlarm(index: Int, at: Long, key: String) {
        val pi = PendingIntent.getBroadcast(
            context, REQUEST + index,
            Intent(context, CalAlarmReceiver::class.java).setAction(ACTION_FIRE).putExtra(EXTRA_INDEX, index).putExtra(EXTRA_ALARM, key),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val exact = alarms.canScheduleExactAlarms()
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

    private fun load(): List<CalAlarmUi> = loadFrom(DATA)

    /**
     * The reminders kept under [name]: the lock screen part always; the full texts only while the
     * device is unlocked (their key refuses otherwise), else the lock screen part stands in.
     */
    private fun loadFrom(name: String): List<CalAlarmUi> {
        val shown = decrypt(name, ALIAS)?.let { arr -> (0 until arr.length()).map { arr.getJSONObject(it).toAlarm() } }
            ?: return emptyList()
        val full = decrypt(name + FULL, ALIAS_FULL)?.let { arr ->
            (0 until arr.length()).associate { i -> arr.getJSONObject(i).let { it.getString("k") to (it.getString("t") to it.getString("x")) } }
        }.orEmpty()
        return shown.map { a -> full[a.key]?.let { (t, x) -> a.copy(title = t, text = x) } ?: a }
    }

    private fun decrypt(name: String, alias: String): JSONArray? = runCatching {
        val raw = Base64.decode(prefs.getString(name, null) ?: return null, Base64.NO_WRAP)
        val key = keyStore.getKey(alias, null) as? SecretKey ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw, 0, 12))
        JSONArray(String(cipher.doFinal(raw, 12, raw.size - 12)))
    }.getOrNull()

    private fun store(json: List<CalAlarmUi>) = storeTo(DATA, json)

    /**
     * M12: the lock screen part (key, time, public texts) with the key that works while locked;
     * the full texts, where they differ, with the key bound to an unlocked device.
     */
    private fun storeTo(name: String, list: List<CalAlarmUi>) {
        val shown = JSONArray()
        val full = JSONArray()
        for (a in list) {
            shown.put(a.toPublicJson())
            if (a.title != a.publicTitle || a.text != a.publicText) {
                full.put(JSONObject().put("k", a.key).put("t", a.title).put("x", a.text))
            }
        }
        val edit = prefs.edit().putString(name, encrypt(shown.toString(), key(ALIAS, unlockedOnly = false)))
        // Written while the device is locked (a reminder shown at night): the full texts kept so
        // far cannot be read now, and the list holds only their lock screen part. They stay as
        // they are instead of being replaced by nothing.
        val fullUnreadable = prefs.getString(name + FULL, null) != null && decrypt(name + FULL, ALIAS_FULL) == null
        if (!fullUnreadable) {
            val fullKey = if (full.length() > 0) runCatching { key(ALIAS_FULL, unlockedOnly = true) }.getOrNull() else null
            // Without a key bound to an unlocked device, the full texts are not kept at all.
            if (fullKey != null) edit.putString(name + FULL, encrypt(full.toString(), fullKey)) else edit.remove(name + FULL)
        }
        edit.apply()
    }

    private fun encrypt(json: String, key: SecretKey): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key) }
        val ct = cipher.doFinal(json.toByteArray())
        return Base64.encodeToString(cipher.iv + ct, Base64.NO_WRAP)
    }

    private fun key(alias: String, unlockedOnly: Boolean): SecretKey {
        (keyStore.getKey(alias, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
        if (unlockedOnly) spec.setUnlockedDeviceRequired(true)
        gen.init(spec.build())
        return gen.generateKey()
    }

    companion object {
        private const val ALIAS = "ghulsuite.cal.alarms"
        /** M12: the key of the full texts, usable only while the device is unlocked. */
        private const val ALIAS_FULL = "ghulsuite.cal.alarms.full"
        private const val FULL = "_full"
        private const val DATA = "data"
        private const val SHOWN = "shown"
        private const val SHOWN_MAX = 20
        private const val COUNT = "count"
        private const val SOUND = "sound"
        private const val REQUEST = 7000
        private const val MAX = 400
        const val ACTION_FIRE = "de.danerd.ghulsuite.CAL_FIRE"
        const val ACTION_SNOOZE = "de.danerd.ghulsuite.CAL_SNOOZE"
        const val ACTION_DONE = "de.danerd.ghulsuite.CAL_DONE"
        const val EXTRA_INDEX = "index"
        const val EXTRA_KEY = "key"
        /** The key of a planned reminder (N5: found by it, not only by its place in the list). */
        const val EXTRA_ALARM = "alarm"
        // B4: new IDs, private on the lock screen also with Android's default settings.
        const val CHANNEL = "cal_private"
        const val CHANNEL_SILENT = "cal_silent_private"
    }
}

/** What may show on the lock screen: the full texts are not part of it (M12). */
private fun CalAlarmUi.toPublicJson() = JSONObject()
    .put("k", key).put("at", atMillis).put("pt", publicTitle).put("px", publicText).put("al", atLocal)

/** Read back: until the full texts are added, the lock screen texts stand in for them. */
private fun JSONObject.toAlarm(): CalAlarmUi {
    val pt = getString("pt")
    val px = getString("px")
    return CalAlarmUi(getString("k"), getLong("at"), pt, px, pt, px, optString("al"))
}

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
    // "Again in 5 minutes" finds it by its key (see CalReminders.remember).
    runCatching { CalReminders(ctx).remember(a) }
    fun action(act: String, req: Int) = PendingIntent.getBroadcast(
        ctx, req,
        Intent(ctx, CalAlarmReceiver::class.java).setAction(act).putExtra(CalReminders.EXTRA_INDEX, index)
            .putExtra(CalReminders.EXTRA_KEY, a.key).putExtra("notify_id", id),
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
        val key = intent.getStringExtra(CalReminders.EXTRA_KEY)
        when (intent.action) {
            // A snoozed reminder comes by its key; a scheduled one by its place in the list.
            CalReminders.ACTION_FIRE -> (key?.let { r.shown(it) } ?: r.alarm(index, intent.getStringExtra(CalReminders.EXTRA_ALARM)))
                ?.let { postCalNotification(context, it, r.sound, index) }
            CalReminders.ACTION_DONE -> context.getSystemService(NotificationManager::class.java).cancel(intent.getIntExtra("notify_id", 0))
            CalReminders.ACTION_SNOOZE -> {
                context.getSystemService(NotificationManager::class.java).cancel(intent.getIntExtra("notify_id", 0))
                val pi = PendingIntent.getBroadcast(
                    context, 6000 + ((key ?: index.toString()).hashCode() and 0xfff),
                    Intent(context, CalAlarmReceiver::class.java).setAction(CalReminders.ACTION_FIRE).putExtra(CalReminders.EXTRA_INDEX, index)
                        .putExtra(CalReminders.EXTRA_KEY, key),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
                val am = context.getSystemService(AlarmManager::class.java)
                val at = System.currentTimeMillis() + 5 * 60_000
                if (am.canScheduleExactAlarms()) am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
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
