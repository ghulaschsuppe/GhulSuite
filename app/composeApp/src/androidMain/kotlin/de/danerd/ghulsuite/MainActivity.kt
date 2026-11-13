// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.Manifest
import android.app.Application
import android.content.pm.PackageManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import androidx.compose.runtime.snapshotFlow
import de.danerd.ghulsuite.ui.AppLaunch
import de.danerd.ghulsuite.ui.AppState
import de.danerd.ghulsuite.ui.Tab
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
        lifecycleScope.launch {
            snapshotFlow { ghul.backend.settings.allowScreenshots }.collect { allowed ->
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
        setContent { GhulSuiteApp(vm.state) }
        handleLaunch(intent)

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

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLaunch(intent)
    }

    /** A tapped mail notification opens GhulMail (right away, or once the suite is unlocked). */
    private fun handleLaunch(intent: Intent?) {
        if (intent?.action == ACTION_OPEN_MAIL) {
            AppLaunch.openTab.value = Tab.MAIL
            intent.action = null
        }
        if (intent?.action == ACTION_OPEN_EVENT) {
            AppLaunch.openEvent.value = intent.getStringExtra(CalReminders.EXTRA_KEY)
            intent.action = null
        }
    }

    companion object {
        const val ACTION_OPEN_MAIL = "de.danerd.ghulsuite.OPEN_MAIL"
        const val ACTION_OPEN_EVENT = "de.danerd.ghulsuite.OPEN_EVENT"
        /** The running activity (language switch). */
        @Volatile
        var current: java.lang.ref.WeakReference<android.app.Activity>? = null
    }
}
