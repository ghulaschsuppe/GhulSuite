// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

/**
 * A mail archive run is going on: the auto-lock (time away, screen off) waits for its end, so the
 * long first run is not cut off whenever the user switches apps. The notification of the unlocked
 * suite shows it; "Lock", shaking and the tile still lock at once and stop the run.
 */
object ArchiveRun {
    /** Progress text while a run goes on; null otherwise. */
    @Volatile
    var progress: String? = null
        private set

    /** Called when a run ends (to carry out an auto-lock that waited for it). */
    @Volatile
    var onEnded: (() -> Unit)? = null

    fun set(text: String?) {
        val ended = progress != null && text == null
        progress = text
        if (ended) onEnded?.invoke()
    }
}
