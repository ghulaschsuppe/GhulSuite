// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared by Android and Windows: update check and download through the core.
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.UpdateInfo
import de.danerd.ghulsuite.core.updateCheck
import de.danerd.ghulsuite.core.updateDownload
import de.danerd.ghulsuite.model.AppUpdater
import de.danerd.ghulsuite.model.UpdateUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * [suffix]: the file of this platform in a release (".apk", ".msi", "-portable.zip"). The core
 * accepts a release only if `SHA256SUMS` carries a valid signature of the key built into this app
 * ([BuildInfo.UPDATE_KEY]) and the file has exactly the size and SHA-256 listed there.
 */
abstract class CoreUpdater(private val suffix: String) : AppUpdater {
    private var found: UpdateInfo? = null

    /** Builds with an update key only (releases from CI). */
    protected val hasKey: Boolean get() = BuildInfo.UPDATE_KEY.isNotBlank()

    override suspend fun check(prerelease: Boolean): UpdateUi? = withContext(Dispatchers.IO) {
        mapped { updateCheck(BuildInfo.UPDATE_REPO, BuildInfo.VERSION, suffix, BuildInfo.UPDATE_KEY, prerelease) }
    }.also { found = it }?.let { UpdateUi(it.version, it.notes, it.page, it.size.toLong()) }

    /** Downloads the update found by [check] into [dir]; the checked file. */
    protected suspend fun download(update: UpdateUi, dir: File): File = withContext(Dispatchers.IO) {
        val info = found?.takeIf { it.version == update.version } ?: error("no update checked")
        dir.mkdirs()
        // Leftovers of earlier updates.
        dir.listFiles()?.forEach { it.delete() }
        val name = info.asset.filter { it.isLetterOrDigit() || it in "._-" }.ifEmpty { "update$suffix" }
        val file = File(dir, name)
        mapped { updateDownload(info, file.path) }
        file
    }
}
