// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.app.Application
import android.os.Bundle
import java.lang.ref.WeakReference
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.service.quicksettings.TileService
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * Owns the vault backend and enforces automatic locking:
 * - after a configurable time in the background,
 * - when the screen turns off (optional),
 * - on demand from the notification, the quick settings tile or by shaking ([UnlockedService]).
 */
class GhulApplication : Application() {
    lateinit var backend: AndroidVaultBackend
        private set

    /** The start failed; [MainActivity] shows the crash report instead of the app. */
    var startFailed = false
        private set

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val handler = Handler(Looper.getMainLooper())
    private val lockRunnable = Runnable { backend.lock() }
    private var screenReceiverRegistered = false
    private var current: WeakReference<Activity>? = null

    private val screenOffReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == Intent.ACTION_SCREEN_OFF && backend.settings.lockOnScreenOff) backend.lock()
        }
    }

    override fun onCreate() {
        super.onCreate()
        CrashLog.install(this)
        try {
            start()
        } catch (e: Throwable) {
            CrashLog.write(this, e)
            startFailed = true
        }
    }

    private fun start() {
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityResumed(activity: Activity) { current = WeakReference(activity) }
            override fun onActivityPaused(activity: Activity) {}
            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) { current = WeakReference(activity) }
            override fun onActivityStarted(activity: Activity) {}
            override fun onActivityStopped(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) { if (current?.get() === activity) current = null }
        })
        backend = AndroidVaultBackend(this) { current?.get() }
        // Instant mail notifications, if switched on (also after the app was closed by Android).
        MailPushService.start(this)

        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                handler.removeCallbacks(lockRunnable)
                // B8: the handler's clock stops in deep sleep; the real time away decides.
                if (lockDueAt > 0 && android.os.SystemClock.elapsedRealtime() >= lockDueAt && backend.unlocked.value) {
                    backend.lock()
                }
                lockDueAt = 0
                // Back in the app: the grace period of a file picker, share or camera is over.
                ExternalActivity.finished()
            }

            override fun onStop(owner: LifecycleOwner) {
                if (!backend.unlocked.value) return
                var delay = backend.settings.autoLockSeconds
                if (ExternalActivity.active) delay = maxOf(delay, ExternalActivity.GRACE_SECONDS)
                if (delay <= 0) {
                    backend.lock()
                } else {
                    lockDueAt = android.os.SystemClock.elapsedRealtime() + delay * 1000L
                    handler.postDelayed(lockRunnable, delay * 1000L)
                }
            }
        })

        scope.launch {
            backend.unlocked.collect { unlocked ->
                if (unlocked) onUnlocked() else onLocked()
                TileService.requestListeningState(this@GhulApplication, ComponentName(this@GhulApplication, LockTileService::class.java))
            }
        }
    }

    /** When the app left the screen plus the auto-lock time (elapsed realtime, counts in deep sleep); 0 = none. */
    @Volatile
    private var lockDueAt = 0L

    private fun onUnlocked() {
        if (!screenReceiverRegistered) {
            val filter = IntentFilter(Intent.ACTION_SCREEN_OFF)
            if (Build.VERSION.SDK_INT >= 33) {
                registerReceiver(screenOffReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                registerReceiver(screenOffReceiver, filter)
            }
            screenReceiverRegistered = true
        }
        runCatching { startForegroundService(Intent(this, UnlockedService::class.java)) }
    }

    private fun onLocked() {
        handler.removeCallbacks(lockRunnable)
        if (screenReceiverRegistered) {
            runCatching { unregisterReceiver(screenOffReceiver) }
            screenReceiverRegistered = false
        }
        stopService(Intent(this, UnlockedService::class.java))
    }
}

val Context.ghul: GhulApplication get() = applicationContext as GhulApplication
