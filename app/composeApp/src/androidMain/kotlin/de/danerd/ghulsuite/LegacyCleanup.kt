// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import java.io.File

/**
 * Android's part of [Leftovers]: once after the update, off the main thread. Besides the files,
 * the settings of removed features go, the access to the folder of the old automatic backup is
 * given back, and the notification channels, preference files and Keystore key of the mail
 * notifications are removed. User data (the working copy, quick unlock, settings in use) stays.
 */
object LegacyCleanup {
    fun runOnce(context: Context) {
        val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
        if (prefs.getBoolean(Leftovers.DONE_KEY, false)) return
        runCatching {
            prefs.getString(Leftovers.AUTO_BACKUP_FOLDER, null)?.takeIf { it.isNotBlank() }?.let { folder ->
                val uri = Uri.parse(folder)
                val held = context.contentResolver.persistedUriPermissions.filter { it.uri == uri }
                for (p in held) {
                    val flags = (if (p.isReadPermission) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                        (if (p.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
                    runCatching { context.contentResolver.releasePersistableUriPermission(uri, flags) }
                }
            }
        }
        runCatching {
            Leftovers.removeFiles(
                File(context.filesDir, "local"),
                context.cacheDir,
                listOf(context.noBackupFilesDir, context.getDatabasePath("x").parentFile ?: context.noBackupFilesDir),
            )
        }
        runCatching {
            val nm = context.getSystemService(NotificationManager::class.java)
            for (id in listOf("mail_private", "mail", "mail_push")) nm?.deleteNotificationChannel(id)
        }
        runCatching { for (name in listOf("mail_notify", "mail_notify_ui")) context.deleteSharedPreferences(name) }
        runCatching {
            val ks = java.security.KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            if (ks.containsAlias("ghulsuite.mail.notify")) ks.deleteEntry("ghulsuite.mail.notify")
        }
        prefs.edit().apply {
            Leftovers.SETTINGS_KEYS.forEach { remove(it) }
            putBoolean(Leftovers.DONE_KEY, true)
        }.apply()
    }
}
