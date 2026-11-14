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

    override fun archiveRunning(progress: String?) {
        ArchiveRun.set(progress)
        // The notification of the unlocked suite shows the progress.
        if (context.ghul.backend.unlocked.value) {
            runCatching {
                context.startForegroundService(android.content.Intent(context, UnlockedService::class.java).setAction(UnlockedService.ACTION_UPDATE))
            }
        }
    }

    override fun unmeteredNetwork(): Boolean {
        val cm = context.getSystemService(android.net.ConnectivityManager::class.java) ?: return false
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
        return caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
    }

    override val photoBackupAvailable: Boolean get() = true

    override suspend fun photoBackup(sinceSeconds: Long, wifiOnly: Boolean, onFile: (String) -> Unit): Long = withContext(Dispatchers.IO) {
        if (wifiOnly) {
            val cm = context.getSystemService(android.net.ConnectivityManager::class.java)
            val caps = cm.getNetworkCapabilities(cm.activeNetwork)
            if (caps == null || !caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)) return@withContext sinceSeconds
        }
        val perm = if (android.os.Build.VERSION.SDK_INT >= 33) android.Manifest.permission.READ_MEDIA_IMAGES else android.Manifest.permission.READ_EXTERNAL_STORAGE
        if (androidx.core.content.ContextCompat.checkSelfPermission(context, perm) != android.content.pm.PackageManager.PERMISSION_GRANTED) return@withContext sinceSeconds
        val uri = android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        val cols = arrayOf(android.provider.MediaStore.Images.Media._ID, android.provider.MediaStore.Images.Media.DISPLAY_NAME,
            android.provider.MediaStore.Images.Media.DATE_ADDED, android.provider.MediaStore.Images.Media.SIZE)
        val found = mutableListOf<Triple<Long, String, Long>>()
        var sizes = mutableMapOf<Long, Long>()
        context.contentResolver.query(uri, cols, "${android.provider.MediaStore.Images.Media.DATE_ADDED} > ?", arrayOf(sinceSeconds.toString()),
            "${android.provider.MediaStore.Images.Media.DATE_ADDED} ASC")?.use { c ->
            while (c.moveToNext() && found.size < 200) {
                found += Triple(c.getLong(0), c.getString(1) ?: "foto.jpg", c.getLong(2))
                sizes[c.getLong(0)] = c.getLong(3)
            }
        }
        if (found.isEmpty()) return@withContext sinceSeconds
        val folder = driveList(null, true).firstOrNull { it.isDir && it.name == PHOTO_FOLDER }?.id ?: driveCreateFolder(null, PHOTO_FOLDER)
        var newest = sinceSeconds
        for ((id, name, added) in found) {
            val item = android.content.ContentUris.withAppendedId(uri, id)
            val stream = context.contentResolver.openInputStream(item) ?: continue
            val source = object : de.danerd.ghulsuite.model.ByteSource {
                override val name = name
                override val size = sizes[id] ?: -1L
                override fun read(buffer: ByteArray, offset: Int, length: Int) = stream.read(buffer, offset, length)
                override fun close() = stream.close()
            }
            driveUpload(folder, source) {}
            newest = maxOf(newest, added)
            withContext(Dispatchers.Main) { onFile(name) }
        }
        newest
    }

    private companion object {
        const val PHOTO_FOLDER = "Fotos"
    }
}
