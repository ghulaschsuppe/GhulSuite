// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.Manifest
import android.app.Application
import android.content.pm.PackageManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.semantics.semantics
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.compose.runtime.snapshotFlow
import de.danerd.ghulsuite.model.autoLockSecondsNow
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.GhulSuiteApp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/** Keeps the UI state across configuration changes. */
class MainViewModel(application: Application) : AndroidViewModel(application) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val state = AppState(application.ghul.backend, scope)

    override fun onCleared() {
        scope.cancel()
    }
}

class MainActivity : ComponentActivity() {
    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean {
        lastInput = android.os.SystemClock.elapsedRealtime()
        return guardStaleTextKey { super.dispatchKeyEvent(event) }
    }

    /** r9 dieb N4: the last touch or key (for the idle lock while GhulSuite is in front). */
    private var lastInput = android.os.SystemClock.elapsedRealtime()

    override fun dispatchTouchEvent(ev: android.view.MotionEvent): Boolean {
        lastInput = android.os.SystemClock.elapsedRealtime()
        return super.dispatchTouchEvent(ev)
    }

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* optional */ }

    /** Before Android 13: the language chosen in the app (newer versions do this themselves). */
    override fun attachBaseContext(base: android.content.Context) {
        val tag = base.getSharedPreferences("settings", MODE_PRIVATE).getString("app_language", "").orEmpty()
        if (tag.isEmpty() || android.os.Build.VERSION.SDK_INT >= 33) return super.attachBaseContext(base)
        val locale = java.util.Locale.forLanguageTag(tag)
        java.util.Locale.setDefault(locale)
        val conf = android.content.res.Configuration(base.resources.configuration).apply { setLocale(locale) }
        super.attachBaseContext(base.createConfigurationContext(conf))
    }

    override fun onDestroy() {
        if (current?.get() === this) current = null
        // Files shared into GhulDocs may only be read while this window lives (Android's rule for
        // shared files): once it is closed for good, they cannot be added any more.
        if (isFinishing) AppLaunch.docsFiles.value = null
        super.onDestroy()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        current = java.lang.ref.WeakReference(this)
        CrashLog.pending(this)?.let { report ->
            CrashLog.show(this, report)
            return
        }
        // No screenshots, screen recordings or previews in the app switcher (unless allowed in the settings).
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        // L10: no taps while another app draws over GhulSuite (tapjacking). M17: hidden from
        // accessibility services that are not tools (Android 14+), and other apps' overlay
        // windows are hidden while GhulSuite is in front (Android 12+).
        de.danerd.ghulsuite.ui.guardView(window.decorView)
        window.setHideOverlayWindows(true)
        lifecycleScope.launch {
            val testRun = TestDevice.screenshotsAllowed(this@MainActivity)
            snapshotFlow { ghul.backend.settings.allowScreenshots || testRun }.collect { allowed ->
                // Newer Android versions show a live preview in the app switcher despite FLAG_SECURE
                // unless the recents screenshot is switched off as well (P7).
                if (android.os.Build.VERSION.SDK_INT >= 33) setRecentsScreenshotEnabled(allowed)
                if (allowed) {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                } else {
                    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
                }
            }
        }
        enableEdgeToEdge()
        val vm = ViewModelProvider(this)[MainViewModel::class.java]
        setContent {
            // Test tags (e.g. "tab-pass" of the bottom bar) are visible to the emulator flows as ids.
            @OptIn(androidx.compose.ui.ExperimentalComposeUiApi::class)
            androidx.compose.foundation.layout.Box(
                androidx.compose.ui.Modifier.semantics { testTagsAsResourceId = true },
            ) { GhulSuiteApp(vm.state) }
        }
        handleLaunch(intent)

        // r10 M2: while a job the user started runs (adding many scans, an export), the display
        // stays on: it going off by itself would lock firmly (r9 dieb N3) and cut the job off.
        // Switching it off by hand still locks.
        // r11: only while it makes progress (a stalled job does not keep the display on for good).
        lifecycleScope.launch {
            repeatOnLifecycle(androidx.lifecycle.Lifecycle.State.STARTED) {
                var on = false
                while (true) {
                    val keep = de.danerd.ghulsuite.ui.JobStatus.userJobProgressing()
                    if (keep != on) {
                        on = keep
                        if (keep) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                    kotlinx.coroutines.delay(5_000)
                }
            }
        }
        // r9 dieb N4: like on Windows, an app in front without any touch for long locks itself
        // (someone took the unlocked phone and keeps the display on). Time in deep sleep counts.
        lifecycleScope.launch {
            repeatOnLifecycle(androidx.lifecycle.Lifecycle.State.RESUMED) {
                lastInput = android.os.SystemClock.elapsedRealtime()
                while (true) {
                    kotlinx.coroutines.delay(5_000)
                    // Touches and keys of this window, plus what the app saw elsewhere (dialogs,
                    // typing on the screen keyboard).
                    val idle = minOf(
                        android.os.SystemClock.elapsedRealtime() - lastInput,
                        de.danerd.ghulsuite.ui.UserActivity.idleMillis() ?: Long.MAX_VALUE,
                    )
                    vm.state.idleWarning = ghul.backend.unlocked.value &&
                        de.danerd.ghulsuite.model.idleLockWarning(idle, ghul.backend.autoLockSecondsNow(), vm.state.editorDirty)
                    if (ghul.backend.unlocked.value &&
                        de.danerd.ghulsuite.model.idleLockDue(idle, ghul.backend.autoLockSecondsNow(), vm.state.editorDirty, vm.state.busy)
                    ) {
                        ghul.autoLock()
                        lastInput = android.os.SystemClock.elapsedRealtime()
                    }
                }
            }
        }

        // Ask once for notifications so that the "Lock" notification is visible while unlocked.
        lifecycleScope.launch {
            ghul.backend.unlocked.collect { unlocked ->
                if (unlocked && Build.VERSION.SDK_INT >= 33 &&
                    checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED &&
                    !getPreferences(MODE_PRIVATE).getBoolean("asked_notifications", false)
                ) {
                    getPreferences(MODE_PRIVATE).edit().putBoolean("asked_notifications", true).apply()
                    notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }
        }
    }

    /** Shaking also works while the app is visible, even if Android stopped [UnlockedService]. */
    private val shake by lazy { ShakeDetector(this) {
            val unlocked = ghul.backend.unlocked.value
            if (unlocked) ghul.backend.panicLock()
            unlocked
        } }

    override fun onResume() {
        super.onResume()
        if (!ghul.startFailed) shake.start()
        // A locked app asks for the fingerprint as soon as it is back in front.
        AppLaunch.inFront.value = true
        AppLaunch.resumed.value++
    }

    override fun onPause() {
        AppLaunch.inFront.value = false
        shake.stop()
        super.onPause()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLaunch(intent)
    }

    /** A tapped reminder opens its event (right away, or once the suite is unlocked). */
    private fun handleLaunch(intent: Intent?) {
        // B7: only from the app's own notifications.
        if (intent != null && intent.action == ACTION_OPEN_EVENT && !IntentToken.valid(this, intent)) {
            intent.action = null
            return
        }
        if (intent?.action == ACTION_OPEN_EVENT) {
            AppLaunch.openEvent.value = intent.getStringExtra(CalReminders.EXTRA_KEY)
            intent.action = null
        }
    }

    companion object {
        const val ACTION_OPEN_EVENT = "de.danerd.ghulsuite.OPEN_EVENT"
        /** The running activity (language switch). */
        @Volatile
        var current: java.lang.ref.WeakReference<android.app.Activity>? = null
    }
}

/** Drops a key that hit a text field with an outdated layout (see [isStaleTextLayoutError]). */
internal inline fun guardStaleTextKey(dispatch: () -> Boolean): Boolean = try {
    dispatch()
} catch (e: IllegalArgumentException) {
    if (!de.danerd.ghulsuite.ui.isStaleTextLayoutError(e)) throw e
    android.util.Log.w("GhulSuite", "GHUL-STALE-KEY key dropped: ${e.message}")
    true
}
