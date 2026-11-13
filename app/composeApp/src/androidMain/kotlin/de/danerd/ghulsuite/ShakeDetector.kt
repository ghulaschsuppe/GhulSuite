// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Build
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * Recognises a deliberate shake of the phone (panic lock).
 *
 * The old check looked for two single peaks above 2.7 g, sampled about 16 times a second: short
 * peaks were often missed, and a gentle but clear shake never reached the threshold. Now the
 * sensor is read about 50 times a second and a shake is a *sustained* movement: within the last
 * half second (at least a quarter second of samples) three quarters of all samples show strong
 * acceleration. Walking, a bumpy bus or putting the phone down do not do that, a shake does.
 *
 * Used by [UnlockedService] (also with the app in the background) and by [MainActivity] while it
 * is visible, so shaking works even if Android stopped the service.
 */
/** [onShake] returns whether it locked something; only then the phone buzzes. */
class ShakeDetector(private val context: Context, private val onShake: () -> Boolean) : SensorEventListener {
    private var manager: SensorManager? = null
    private val samples = ArrayDeque<Pair<Long, Boolean>>()

    /** Starts listening; returns false if the phone has no accelerometer. */
    fun start(): Boolean {
        if (manager != null) return true
        val sm = context.getSystemService(SensorManager::class.java) ?: return false
        val acc = sm.getDefaultSensor(Sensor.TYPE_ACCELEROMETER) ?: return false
        sm.registerListener(this, acc, SensorManager.SENSOR_DELAY_GAME)
        manager = sm
        return true
    }

    fun stop() {
        manager?.unregisterListener(this)
        manager = null
        samples.clear()
    }

    override fun onSensorChanged(event: SensorEvent) {
        val settings = context.ghul.backend.settings
        if (!settings.shakeToLock) return
        val (x, y, z) = event.values
        val threshold = THRESHOLDS[settings.shakeSensitivity.coerceIn(0, 2)]
        val strong = x * x + y * y + z * z > threshold * threshold
        val now = SystemClock.elapsedRealtime()
        if (shaking(now, strong) && now - lastShake > PAUSE_MS) {
            lastShake = now
            samples.clear()
            if (onShake()) confirm()
        }
    }

    /** Adds a sample and tells whether the last half second was a shake. */
    internal fun shaking(now: Long, strong: Boolean): Boolean {
        samples.addLast(now to strong)
        while (samples.isNotEmpty() && now - samples.first().first > WINDOW_MS) samples.removeFirst()
        if (samples.size < MIN_SAMPLES) return false
        if (now - samples.first().first < MIN_WINDOW_MS) return false
        val strongCount = samples.count { it.second }
        return strongCount * 4 >= samples.size * 3
    }

    /** A short buzz: the lock happened (the screen may not be visible while shaking). */
    private fun confirm() {
        runCatching {
            val vibrator = if (Build.VERSION.SDK_INT >= 31) {
                context.getSystemService(VibratorManager::class.java)?.defaultVibrator
            } else {
                @Suppress("DEPRECATION")
                context.getSystemService(Vibrator::class.java)
            }
            vibrator?.vibrate(VibrationEffect.createOneShot(120, VibrationEffect.DEFAULT_AMPLITUDE))
        }
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}

    companion object {
        /** Acceleration including gravity (m/s²) that counts as strong: low, medium, high sensitivity. */
        private val THRESHOLDS = floatArrayOf(15f, 13f, 11f)
        private const val WINDOW_MS = 500L
        private const val MIN_WINDOW_MS = 250L
        private const val MIN_SAMPLES = 4
        private const val PAUSE_MS = 1500L

        /** Shared by the service and the activity: one shake locks (and buzzes) once. */
        @Volatile
        private var lastShake = 0L
    }
}
