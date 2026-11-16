// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.model.DriveItemUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Android: share sheet for decrypted files, mail notifications while locked. */
class AndroidNetBackend(private val context: Context, suite: () -> GhulSuite) : CoreNetBackend(suite) {
    private val notifications = MailNotifications(context)

    override suspend fun onMailAccountSaved(id: String) {
        // A stored notification copy follows password or server changes; a new account notifies.
        refreshMailNotifications()
    }

    override fun onMailAccountRemoved(id: String) = notifications.disable(id)

    override suspend fun shareDriveToApps(files: List<DriveItemUi>) {
        val uris = withContext(Dispatchers.IO) {
            val dir = java.io.File(context.cacheDir, "shared").apply { deleteRecursively(); mkdirs() }
            val used = mutableSetOf<String>()
            files.filter { !it.isDir }.map { f ->
                val target = java.io.File(dir, safeFileName(f.name, used))
                writeDriveFile(f.id, target)
                androidx.core.content.FileProvider.getUriForFile(context, context.packageName + ".files", target)
            }
        }
        if (uris.isEmpty()) return
        val intent = if (uris.size == 1) {
            android.content.Intent(android.content.Intent.ACTION_SEND).putExtra(android.content.Intent.EXTRA_STREAM, uris[0])
        } else {
            android.content.Intent(android.content.Intent.ACTION_SEND_MULTIPLE)
                .putParcelableArrayListExtra(android.content.Intent.EXTRA_STREAM, ArrayList(uris))
        }
        intent.type = if (uris.size == 1) mimeOf(files.first { !it.isDir }.name) else "*/*"
        intent.clipData = android.content.ClipData.newRawUri("", uris[0]).apply { uris.drop(1).forEach { addItem(android.content.ClipData.Item(it)) } }
        intent.addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ExternalActivity.started()
        context.startActivity(
            android.content.Intent.createChooser(intent, null)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK or android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION),
        )
    }

    override suspend fun shareBytesToApps(name: String, mime: String, data: ByteArray): Boolean {
        val uri = withContext(Dispatchers.IO) {
            val dir = java.io.File(context.cacheDir, "shared").apply { deleteRecursively(); mkdirs() }
            val target = java.io.File(dir, safeFileName(name, mutableSetOf())).apply { writeBytes(data) }
            androidx.core.content.FileProvider.getUriForFile(context, context.packageName + ".files", target)
        }
        val intent = android.content.Intent(android.content.Intent.ACTION_SEND).putExtra(android.content.Intent.EXTRA_STREAM, uri)
        intent.type = mime
        intent.clipData = android.content.ClipData.newRawUri("", uri)
        intent.addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ExternalActivity.started()
        context.startActivity(
            android.content.Intent.createChooser(intent, null)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK or android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION),
        )
        return true
    }

    override fun mailNotificationsOn(accountId: String): Boolean = notifications.isOn(accountId)

    override fun clearMailNotification(accountId: String) {
        runCatching { context.getSystemService(android.app.NotificationManager::class.java).cancel(accountId.hashCode()) }
    }

    override fun mailNotifyDetail(): Int = notifications.detail

    override fun setMailNotifyDetail(level: Int) {
        notifications.detail = level
    }

    override suspend fun refreshMailNotifications() {
        val configs = io { suite().mailAccounts().map { suite().mailPollConfig(it.id) } }
        if (notifications.refresh(configs) && !askedBattery()) askToIgnoreBatteryOptimization()
    }

    override fun mailBackgroundAllowed(): Boolean? =
        context.getSystemService(android.os.PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

    override fun openMailBackgroundSettings() = askToIgnoreBatteryOptimization(force = true)

    private fun askedBattery(): Boolean {
        val p = context.getSharedPreferences("mail_notify_ui", android.content.Context.MODE_PRIVATE)
        if (p.getBoolean("asked_battery", false)) return true
        p.edit().putBoolean("asked_battery", true).apply()
        return false
    }

    override suspend fun setMailNotifications(accountId: String, on: Boolean) {
        notifications.setDecided(accountId)
        if (on) {
            notifications.enable(io { suite().mailPollConfig(accountId) })
            askToIgnoreBatteryOptimization()
        } else {
            notifications.disable(accountId)
        }
    }

    /** Without this, Android may cut the waiting connection in deep sleep and mail arrives late. */
    private fun askToIgnoreBatteryOptimization(force: Boolean = false) {
        val power = context.getSystemService(android.os.PowerManager::class.java)
        if (power.isIgnoringBatteryOptimizations(context.packageName) && !force) {
            return
        }
        if (power.isIgnoringBatteryOptimizations(context.packageName)) {
            // Already allowed: show the list where it can be checked (and switched back).
            runCatching {
                ExternalActivity.started()
                context.startActivity(
                    android.content.Intent(android.provider.Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
                        .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }
            return
        }
        runCatching {
            ExternalActivity.started()
            context.startActivity(
                android.content.Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
                    .setData(android.net.Uri.parse("package:" + context.packageName))
                    .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK),
            )
        }
    }

    override fun unmeteredNetwork(): Boolean {
        val cm = context.getSystemService(android.net.ConnectivityManager::class.java) ?: return false
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
        return caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
    }
}
