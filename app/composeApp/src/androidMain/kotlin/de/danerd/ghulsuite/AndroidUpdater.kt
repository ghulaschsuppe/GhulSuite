// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import de.danerd.ghulsuite.model.UpdateUi
import de.danerd.ghulsuite.model.UpdateStep
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Android: the APK of the release is checked by the core (signed checksums), then handed to the
 * system installer, which in turn only accepts it when it is signed with the same key as the
 * installed app. The user confirms the installation in the system's own dialog.
 */
class AndroidUpdater(private val context: Context) : CoreUpdater(".apk") {
    override val available: Boolean
        get() = hasKey && !context.packageName.endsWith(".dev") && !fromStore()
    override val installs: Boolean get() = true

    /** Installed by an app store: that store brings the updates (and may require it). */
    private fun fromStore(): Boolean {
        val installer = runCatching {
            if (Build.VERSION.SDK_INT >= 30) {
                context.packageManager.getInstallSourceInfo(context.packageName).installingPackageName
            } else {
                @Suppress("DEPRECATION")
                context.packageManager.getInstallerPackageName(context.packageName)
            }
        }.getOrNull() ?: return false
        return installer in STORES
    }

    override suspend fun install(update: UpdateUi): UpdateStep {
        val pm = context.packageManager
        if (!pm.canRequestPackageInstalls()) {
            // Once: "Allow from this source" for GhulSuite in the system settings.
            context.startActivity(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            return UpdateStep.ALLOW_FIRST
        }
        val apk = download(update, File(context.cacheDir, "update"))
        withContext(Dispatchers.IO) {
            val installer = pm.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
                setAppPackageName(context.packageName)
                setSize(apk.length())
            }
            val id = installer.createSession(params)
            installer.openSession(id).use { session ->
                session.openWrite("GhulSuite.apk", 0, apk.length()).use { out ->
                    apk.inputStream().use { it.copyTo(out) }
                    session.fsync(out)
                }
                apk.delete()
                val result = PendingIntent.getBroadcast(
                    context, id,
                    Intent(context, UpdateResultReceiver::class.java).setPackage(context.packageName),
                    // The installer fills in the result: must be mutable.
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
                )
                session.commit(result.intentSender)
            }
        }
        return UpdateStep.STARTED
    }

    companion object {
        private val STORES = setOf(
            "com.android.vending", "org.fdroid.fdroid", "org.fdroid.basic", "com.looker.droidify",
            "com.machiav3lli.fdroid", "dev.imranr.obtainium", "com.aurora.store", "in.sunilpaulmathew.izzyondroid",
        )
    }
}

/** Result of the installation: the confirmation of the system, or what went wrong. */
class UpdateResultReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = if (Build.VERSION.SDK_INT >= 33) {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(Intent.EXTRA_INTENT)
                }
                confirm?.let { context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            }
            PackageInstaller.STATUS_SUCCESS -> {}
            PackageInstaller.STATUS_FAILURE_ABORTED -> {}
            else -> Toast.makeText(context, context.getString(R.string.update_failed), Toast.LENGTH_LONG).show()
        }
    }
}
