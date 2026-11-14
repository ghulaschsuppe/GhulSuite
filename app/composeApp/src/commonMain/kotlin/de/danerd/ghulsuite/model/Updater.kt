// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

/** A newer version on GitHub (signature of its checksums already checked by the core). */
data class UpdateUi(val version: String, val notes: String, val page: String, val size: Long)

/**
 * Updates from the project's releases on GitHub. Only signed release builds update themselves:
 * test versions, builds without the update key (local, F-Droid, forks) and installs from a store
 * leave updating to whoever installed them.
 */
interface AppUpdater {
    /** This build looks for updates itself. */
    val available: Boolean get() = false
    /** The update can be installed from the app (otherwise only the release page is opened). */
    val installs: Boolean get() = false
    /** Newer version, or null. [prerelease]: also development versions. Network. */
    suspend fun check(prerelease: Boolean): UpdateUi? = null
    /** Downloads, checks size and SHA-256 and hands the file to the installer of the system. */
    suspend fun install(update: UpdateUi): UpdateStep = UpdateStep.STARTED
}

enum class UpdateStep {
    /** The installer of the system takes over (Android asks, Windows closes the app). */
    STARTED,
    /** Android: installing from GhulSuite must be allowed once; the settings page is open. */
    ALLOW_FIRST,
}

object NoUpdater : AppUpdater
