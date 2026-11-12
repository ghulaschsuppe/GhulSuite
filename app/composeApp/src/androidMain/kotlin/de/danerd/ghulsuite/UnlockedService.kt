// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Build
import android.os.IBinder
import android.os.SystemClock
import kotlin.math.sqrt

/**
 * Runs only while the vault is unlocked: shows a notification with a "Lock" action and
 * detects shaking (panic lock). Stops as soon as the vault is locked.
 */
class UnlockedService : Service(), SensorEventListener {
    private var sensors: SensorManager? = null
    private var lastShake = 0L
    private var shakeCount = 0

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
            .setContentText(getString(R.string.notif_text))
            .setContentIntent(open)
            .setOngoing(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(Notification.Action.Builder(null, getString(R.string.notif_lock), lock).build())
            .build()
    }

    private fun startShakeDetection() {
        if (sensors != null || !ghul.backend.settings.shakeToLock) return
        val sm = getSystemService(SensorManager::class.java) ?: return
        val acc = sm.getDefaultSensor(Sensor.TYPE_ACCELEROMETER) ?: return
        sm.registerListener(this, acc, SensorManager.SENSOR_DELAY_UI)
        sensors = sm
    }

    /** Two strong shakes (> 2.7 g) within one second lock the vault. */
    override fun onSensorChanged(event: SensorEvent) {
        val (x, y, z) = event.values
        val g = sqrt(x * x + y * y + z * z) / SensorManager.GRAVITY_EARTH
        val threshold = SHAKE_THRESHOLDS_G[ghul.backend.settings.shakeSensitivity.coerceIn(0, 2)]
        if (g < threshold) return
        val now = SystemClock.elapsedRealtime()
        if (now - lastShake < 150) return // same movement
        shakeCount = if (now - lastShake > 1000) 1 else shakeCount + 1
        lastShake = now
        if (shakeCount >= 2) {
            ghul.backend.panicLock()
            stopSelf()
        }
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}

    override fun onDestroy() {
        sensors?.unregisterListener(this)
        sensors = null
        super.onDestroy()
    }

    companion object {
        const val ACTION_LOCK = "de.danerd.ghulsuite.LOCK"
        private const val CHANNEL = "unlocked"
        private const val NOTIFICATION_ID = 1
        /** low, medium, high sensitivity */
        private val SHAKE_THRESHOLDS_G = floatArrayOf(3.3f, 2.7f, 2.2f)
    }
}
