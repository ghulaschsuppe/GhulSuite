// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import java.io.File

/**
 * What removed features (GhulMail, GhulDrive, servers, the automatic backup, the system camera
 * for scans, WorkManager) left on a device, removed once after the update. Only these names are
 * touched: never the working copy, the device's own data (quick unlock, what was seen of a suite
 * file) or the settings still in use.
 */
object Leftovers {
    /** Settings of removed features. */
    val SETTINGS_KEYS = listOf(
        "auto_sync", "module_drive", "module_mail", "panic_stops_mail", "drive_trash_days", "last_auto_backup_s",
        "mail_swipe_left", "archive_auto_last_s", "auto_backup_folder", "mail_image_senders",
    )

    /** The setting holding the folder of the old automatic backup (its access is given back). */
    const val AUTO_BACKUP_FOLDER = "auto_backup_folder"

    /** Marks that the cleanup ran (in the device's settings). */
    const val DONE_KEY = "leftovers_removed_1"

    /** In the core's own folder: the mail cache, GhulDrive's offline copies and its open uploads. */
    private val LOCAL = listOf("mail-cache", "drive-offline", "drive-pending")

    /** In the app's cache: the photo of the system camera (scans now stay in memory). */
    private val CACHE = listOf("scan")

    /** WorkManager's database (the mail and Drive jobs), with its journal files. */
    private val WORK_DB = listOf("androidx.work.workdb", "androidx.work.workdb-shm", "androidx.work.workdb-wal", "androidx.work.workdb-journal")

    /**
     * Removes the files and folders: [local] is the core's folder, [cache] the app's cache,
     * [databases] the folders WorkManager kept its database in. Returns what was removed.
     */
    fun removeFiles(local: File, cache: File, databases: List<File>): List<File> {
        val gone = LOCAL.map { File(local, it) } + CACHE.map { File(cache, it) } +
            databases.flatMap { d -> WORK_DB.map { File(d, it) } }
        return gone.filter { it.exists() && it.deleteRecursively() }
    }
}
