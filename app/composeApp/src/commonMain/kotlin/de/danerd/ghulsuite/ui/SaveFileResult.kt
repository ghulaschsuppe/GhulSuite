// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.runtime.Composable
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.err_no_space_unknown
import de.danerd.ghulsuite.resources.file_save_failed
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource

/** M8: how saving a file through the system's save dialog ended ([rememberSaveFile]). */
enum class SaveFileResult {
    SAVED,

    /** The user closed the dialog: nothing to say. */
    CANCELLED,

    /** Chosen, but the file could not be written. */
    FAILED,

    /** Chosen, but the storage there is full (see isNoSpace). */
    NO_SPACE,
}

/** The message for a save that did not work, null when there is nothing to say. */
fun saveFileFailedText(r: SaveFileResult): StringResource? = when (r) {
    SaveFileResult.FAILED -> Res.string.file_save_failed
    SaveFileResult.NO_SPACE -> Res.string.err_no_space_unknown
    SaveFileResult.SAVED, SaveFileResult.CANCELLED -> null
}

/** Shows [saved] (when given) after a save, the error when it failed, nothing when cancelled. */
fun AppState.reportSaveFile(r: SaveFileResult, saved: String? = null) {
    if (r == SaveFileResult.SAVED) {
        saved?.let { message(it) }
    } else {
        saveFileFailedText(r)?.let { messageRes(it) }
    }
}

/** The texts of [saveFileFailedText] in the language of the screen, for an error shown in a dialog. */
@Composable
fun rememberSaveFileFailedText(): (SaveFileResult) -> String? {
    val failed = stringResource(Res.string.file_save_failed)
    val full = stringResource(Res.string.err_no_space_unknown)
    return { r ->
        when (r) {
            SaveFileResult.FAILED -> failed
            SaveFileResult.NO_SPACE -> full
            SaveFileResult.SAVED, SaveFileResult.CANCELLED -> null
        }
    }
}
