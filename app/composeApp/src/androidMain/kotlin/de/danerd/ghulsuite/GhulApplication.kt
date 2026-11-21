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
import kotlinx.coroutines.flow.first
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
    /**
     * An automatic lock: while background jobs run, the app's state decides (quiet jobs stop, a
     * job the user started is waited for while it makes progress); else it locks at once.
     */
    private fun autoLock() {
        val jobs = de.danerd.ghulsuite.ui.JobStatus
        val hook = jobs.autoLock
        if (hook != null && jobs.current.value != null) hook() else backend.lock()
    }

    private val lockRunnable = Runnable { autoLock() }
    private var screenReceiverRegistered = false
    private var current: WeakReference<Activity>? = null

    private val screenOffReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == Intent.ACTION_SCREEN_OFF && backend.settings.lockOnScreenOff) {
                autoLock()
            }
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
        if (packageName.endsWith(".dev")) {
            // Development build: mistakes become loud (red bar, log lines the emulator runs fail on).
            de.danerd.ghulsuite.ui.DevChecks.enabled = true
            // Disk and network on the main thread make the app stutter: logged ("StrictMode"),
            // tools/check-logcat.sh reports the ones in our own code.
            android.os.StrictMode.setThreadPolicy(
                android.os.StrictMode.ThreadPolicy.Builder().detectDiskReads().detectDiskWrites().detectNetwork().penaltyLog().build(),
            )
        }
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
        // Once after the update: what removed features left on the device (off the main thread).
        scope.launch(Dispatchers.IO) { runCatching { LegacyCleanup.runOnce(this@GhulApplication) } }

        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                handler.removeCallbacks(lockRunnable)
                // B8: the handler's clock stops in deep sleep; the real time away decides. A job
                // the user started that still makes progress keeps the suite open (the user is back).
                if (lockDueAt > 0 && android.os.SystemClock.elapsedRealtime() >= lockDueAt && backend.unlocked.value) {
                    autoLock()
                }
                de.danerd.ghulsuite.ui.JobStatus.keepUnlocked?.invoke()
                lockDueAt = 0
                // Back in the app: the grace period of a file picker, share or camera is over.
                ExternalActivity.finished()
                backend.onFront()
            }

            override fun onStop(owner: LifecycleOwner) {
                if (!backend.unlocked.value) return
                scheduleLock()
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

    /** The app left the screen while unlocked: locks after the auto-lock time (at once with 0). */
    private fun scheduleLock() {
        var delay = backend.settings.autoLockSeconds
        if (ExternalActivity.active) delay = maxOf(delay, ExternalActivity.GRACE_SECONDS)
        if (delay <= 0) {
            autoLock()
        } else {
            lockDueAt = android.os.SystemClock.elapsedRealtime() + delay * 1000L
            handler.postDelayed(lockRunnable, delay * 1000L)
        }
    }

    private fun onUnlocked() {
        // Unlocking ended while the app was away (Home or the power key while the password was
        // derived): what leaving would have done happens now.
        if (!ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(androidx.lifecycle.Lifecycle.State.STARTED)) {
            val screenOn = getSystemService(android.os.PowerManager::class.java)?.isInteractive != false
            if (!screenOn && backend.settings.lockOnScreenOff) {
                autoLock()
                return
            }
            scheduleLock()
        }
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
        // The suite file is still being written: the service (a notification without names)
        // keeps the app alive until it is done, else Android could end it midway and leave an
        // incomplete file on the stick.
        if (de.danerd.ghulsuite.ui.JobStatus.saving.value != null) {
            scope.launch {
                de.danerd.ghulsuite.ui.JobStatus.saving.first { it == null }
                if (!backend.unlocked.value) stopService(Intent(this@GhulApplication, UnlockedService::class.java))
            }
            return
        }
        stopService(Intent(this, UnlockedService::class.java))
    }
}

val Context.ghul: GhulApplication get() = applicationContext as GhulApplication
