// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.SaveFileResult
import de.danerd.ghulsuite.ui.saveChosenFile
import de.danerd.ghulsuite.ui.saveFileFailedText
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/** M8: a save through the system's dialog tells cancelled, saved and failed apart. */
class SaveFileResultTest {
    @Test
    fun cancelledSavedAndFailedAreTold() {
        val dir = kotlin.io.path.createTempDirectory("save-file").toFile()
        try {
            assertEquals(SaveFileResult.CANCELLED, saveChosenFile(null, byteArrayOf(1)))
            val f = File(dir, "export.csv")
            assertEquals(SaveFileResult.SAVED, saveChosenFile(f, "a;b".toByteArray()))
            assertEquals("a;b", f.readText())
            // A folder that is not there (a stick pulled meanwhile): failed, not "cancelled".
            assertEquals(SaveFileResult.FAILED, saveChosenFile(File(dir, "weg/export.csv"), byteArrayOf(1)))
            // Only a failure has something to say.
            assertNull(saveFileFailedText(SaveFileResult.CANCELLED))
            assertNull(saveFileFailedText(SaveFileResult.SAVED))
            assertNotNull(saveFileFailedText(SaveFileResult.FAILED))
            assertNotNull(saveFileFailedText(SaveFileResult.NO_SPACE))
        } finally {
            dir.deleteRecursively()
        }
    }
}
