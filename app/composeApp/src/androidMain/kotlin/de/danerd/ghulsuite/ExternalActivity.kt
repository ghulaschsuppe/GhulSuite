// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

/**
 * Marks that the app opened a system dialog (file or folder picker) on purpose. While it is
 * open, the immediate auto-lock is postponed for a short grace period so that the chosen file
 * can still be read or written. The timer lock still applies after [GRACE_SECONDS].
 */
object ExternalActivity {
    const val GRACE_SECONDS = 120

    @Volatile
    var active: Boolean = false
        private set

    fun started() {
        active = true
    }

    fun finished() {
        active = false
    }
}
