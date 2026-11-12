// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.Manifest
import android.app.Application
import android.content.pm.PackageManager
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
    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { /* optional */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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
}
