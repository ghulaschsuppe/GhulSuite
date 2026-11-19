// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.scan

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap

// What the platform does for scanning documents (Android: CameraX and android.graphics): the camera
// in the app, decoding and drawing pictures, JPEG. Everything stays in memory; nothing here writes
// a file.

/**
 * A photo as the camera took it: its JPEG in memory ([width] x [height] as stored), turned
 * clockwise by [rotation] degrees to be upright. [wipe] overwrites it.
 */
class ScanShot(val jpeg: ByteArray, val width: Int, val height: Int, val rotation: Int) {
    val uprightWidth: Int get() = uprightSize(rotation, width, height).first
    val uprightHeight: Int get() = uprightSize(rotation, width, height).second

    var wiped = false
        private set

    fun wipe() {
        wiped = true
        jpeg.fill(0)
    }
}

/** A picture held by the platform for the screen; [release] frees (and clears) its pixels. */
interface ScanBitmap {
    val image: ImageBitmap
    val width: Int
    val height: Int
    fun release()
}

/** A finished page: its JPEG (as it goes into the PDF), its size and a small picture for the lists. */
class ScanRendered(val jpeg: ByteArray, val width: Int, val height: Int, val thumb: ScanBitmap) {
    fun wipe() {
        jpeg.fill(0)
        thumb.release()
    }
}

/**
 * The camera on the screen ([ScanEngine.Camera]) and what the screen asks of it. [take] is set
 * while the camera is ready; it takes a photo into memory. [denied]: the user did not allow the
 * camera (the camera view says so itself).
 */
class ScanCamera {
    var take by mutableStateOf<(suspend () -> ScanShot)?>(null)
    var hasTorch by mutableStateOf(false)
    var torch by mutableStateOf(false)
    var denied by mutableStateOf(false)
    var failed by mutableStateOf(false)
}

interface ScanEngine {
    /** The live picture of the back camera, with the question for the camera permission. */
    @Composable
    fun Camera(camera: ScanCamera, modifier: Modifier)

    /** The photo upright, its longer side at most [maxSide] pixels (for cropping and finding the sheet). */
    fun preview(shot: ScanShot, maxSide: Int): ScanBitmap

    /** The pixels of [bitmap] as ARGB (the caller wipes them). */
    fun pixels(bitmap: ScanBitmap): IntArray

    /**
     * The page: [corners] (parts of the upright photo) cut out and straightened, turned by
     * [turns] quarter turns, with the "Document" filter if [filter]; its longer side at most
     * [SCAN_PAGE_MAX_SIDE]. Encoded as JPEG (about 200 to 600 KB).
     */
    fun render(shot: ScanShot, corners: Quad, turns: Int, filter: Boolean): ScanRendered

    /** A finished page for the screen, its longer side at most [maxSide] pixels. */
    fun decode(jpeg: ByteArray, maxSide: Int): ScanBitmap

    /** Too little memory is left for another photo. */
    fun memoryLow(): Boolean
}
