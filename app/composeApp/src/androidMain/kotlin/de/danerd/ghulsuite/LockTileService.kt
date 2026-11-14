// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/** Quick settings tile "Lock GhulSuite": panic lock, locks and destroys fingerprint/PIN unlock. */
class LockTileService : TileService() {
    override fun onStartListening() {
        update()
    }

    override fun onClick() {
        ghul.backend.panicLock()
        update()
    }

    private fun update() {
        val tile = qsTile ?: return
        tile.state = if (ghul.backend.unlocked.value) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.updateTile()
    }
}
