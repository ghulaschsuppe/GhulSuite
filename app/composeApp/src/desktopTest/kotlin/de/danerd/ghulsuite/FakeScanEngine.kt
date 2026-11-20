// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toAwtImage
import androidx.compose.ui.graphics.toComposeImageBitmap
import de.danerd.ghulsuite.scan.Quad
import de.danerd.ghulsuite.scan.SCAN_PAGE_MAX_SIDE
import de.danerd.ghulsuite.scan.ScanBitmap
import de.danerd.ghulsuite.scan.ScanCamera
import de.danerd.ghulsuite.scan.ScanEngine
import de.danerd.ghulsuite.scan.ScanRendered
import de.danerd.ghulsuite.scan.ScanShot
import de.danerd.ghulsuite.scan.documentFilter
import de.danerd.ghulsuite.scan.jpegInfo
import de.danerd.ghulsuite.scan.pageTransform
import de.danerd.ghulsuite.scan.uprightSize
import java.awt.image.BufferedImage
import kotlinx.coroutines.delay
import org.jetbrains.skia.Image
import org.jetbrains.skia.Matrix33
import org.jetbrains.skia.Paint
import org.jetbrains.skia.PaintMode
import org.jetbrains.skia.Path
import org.jetbrains.skia.SamplingMode
import org.jetbrains.skia.Surface

/**
 * The scanning of Android on the desktop, for the UI tests: the "camera" takes drawn photos of a
 * tilted sheet (stored turned, as phones store them, with the rotation to show them upright),
 * the pages are cut out and straightened with the same transform as on Android (Skia instead of
 * android.graphics), JPEGs come from ImageIO. Like the camera it takes a moment; it remembers
 * every photo, page and picture it handed out, so the tests can check they were wiped.
 */
class FakeScanEngine(var delayMs: Long = 40) : ScanEngine {
    val shots = java.util.Collections.synchronizedList(mutableListOf<ScanShot>())
    val pages = java.util.Collections.synchronizedList(mutableListOf<ScanRendered>())
    val bitmaps = java.util.Collections.synchronizedList(mutableListOf<FakeBitmap>())
    var lowMemory = false

    class FakeBitmap(override val image: ImageBitmap) : ScanBitmap {
        override val width get() = image.width
        override val height get() = image.height
        @Volatile var released = false
        override fun release() { released = true }
    }

    @Composable
    override fun Camera(camera: ScanCamera, modifier: Modifier) {
        Box(modifier.background(Color(0xFF202124)), contentAlignment = Alignment.Center) { Text("camera", color = Color.Gray) }
        DisposableEffect(camera) {
            camera.hasTorch = true
            camera.take = {
                delay(delayMs)
                photo()
            }
            onDispose { camera.take = null }
        }
    }

    /** A photo of a sheet on a dark table: 900 x 1200 upright, stored turned (1200 x 900, rotation 90). */
    fun photo(): ScanShot {
        val uw = 900
        val uh = 1200
        val upright = Surface.makeRasterN32Premul(uw, uh)
        val c = upright.canvas
        c.clear(0xFF3A3A40.toInt())
        val sheet = Path().moveTo(160f, 120f).lineTo(760f, 180f).lineTo(720f, 1080f).lineTo(110f, 1020f).closePath()
        c.drawPath(sheet, Paint().apply { color = 0xFFF2F0EA.toInt() })
        val ink = Paint().apply { color = 0xFF30303A.toInt(); mode = PaintMode.FILL }
        for (line in 0 until 16) {
            val y = 260f + line * 46f
            c.drawRect(org.jetbrains.skia.Rect.makeXYWH(230f + line * 1.2f, y + line * 2.6f, 380f - (line % 4) * 40f, 14f), ink)
        }
        val shown = upright.makeImageSnapshot()
        val raw = Surface.makeRasterN32Premul(uh, uw)
        // Stored turned back: the upright picture is the stored one turned 90 degrees clockwise.
        raw.canvas.translate(0f, uw.toFloat())
        raw.canvas.rotate(-90f)
        raw.canvas.drawImage(shown, 0f, 0f)
        val jpeg = encode(raw.makeImageSnapshot().toComposeImageBitmap().toAwtImage(), 90)
        val info = jpegInfo(jpeg)!!
        return ScanShot(jpeg, info.width, info.height, 90).also { shots += it }
    }

    private fun skia(jpeg: ByteArray): Image = Image.makeFromEncoded(jpeg)

    private fun whole(i: Image) = org.jetbrains.skia.Rect.makeWH(i.width.toFloat(), i.height.toFloat())

    override fun preview(shot: ScanShot, maxSide: Int): ScanBitmap {
        val raw = skia(shot.jpeg)
        val (uw, uh) = uprightSize(shot.rotation, raw.width, raw.height)
        val f = minOf(1f, maxSide.toFloat() / maxOf(uw, uh))
        val s = Surface.makeRasterN32Premul((uw * f).toInt(), (uh * f).toInt())
        val c = s.canvas
        c.scale(f, f)
        when (shot.rotation) {
            90 -> { c.translate(raw.height.toFloat(), 0f); c.rotate(90f) }
            180 -> { c.translate(raw.width.toFloat(), raw.height.toFloat()); c.rotate(180f) }
            270 -> { c.translate(0f, raw.width.toFloat()); c.rotate(270f) }
        }
        c.drawImageRect(raw, whole(raw), whole(raw), SamplingMode.LINEAR, null, true)
        return FakeBitmap(s.makeImageSnapshot().toComposeImageBitmap()).also { bitmaps += it }
    }

    override fun pixels(bitmap: ScanBitmap): IntArray {
        val img = bitmap.image
        return IntArray(img.width * img.height).also { img.readPixels(it) }
    }

    override fun render(shot: ScanShot, corners: Quad, turns: Int, filter: Boolean): ScanRendered {
        val raw = skia(shot.jpeg)
        val t = pageTransform(corners, shot.rotation, raw.width, raw.height, turns, SCAN_PAGE_MAX_SIDE)
        val s = Surface.makeRasterN32Premul(t.width, t.height)
        s.canvas.clear(-1)
        s.canvas.concat(Matrix33(*t.matrix))
        s.canvas.drawImageRect(raw, whole(raw), whole(raw), SamplingMode.LINEAR, null, true)
        val img = s.makeImageSnapshot().toComposeImageBitmap()
        val px = IntArray(t.width * t.height).also { img.readPixels(it) }
        if (filter) documentFilter(px, t.width, t.height)
        val page = BufferedImage(t.width, t.height, BufferedImage.TYPE_INT_RGB)
        page.setRGB(0, 0, t.width, t.height, px, 0, t.width)
        px.fill(0)
        val jpeg = encode(page, 82)
        val f = minOf(1f, 320f / maxOf(t.width, t.height))
        val thumb = BufferedImage(maxOf(1, (t.width * f).toInt()), maxOf(1, (t.height * f).toInt()), BufferedImage.TYPE_INT_RGB)
        thumb.createGraphics().apply {
            setRenderingHint(java.awt.RenderingHints.KEY_INTERPOLATION, java.awt.RenderingHints.VALUE_INTERPOLATION_BILINEAR)
            drawImage(page, 0, 0, thumb.width, thumb.height, null)
            dispose()
        }
        val tb = FakeBitmap(thumb.toComposeImageBitmap()).also { bitmaps += it }
        return ScanRendered(jpeg, t.width, t.height, tb).also { pages += it }
    }

    override fun decode(jpeg: ByteArray, maxSide: Int): ScanBitmap {
        val img = de.danerd.ghulsuite.ui.decodeImage(jpeg, maxSide) ?: throw IllegalArgumentException("not decodable")
        return FakeBitmap(img).also { bitmaps += it }
    }

    override fun memoryLow() = lowMemory

    private fun encode(img: BufferedImage, quality: Int): ByteArray {
        val rgb = if (img.type == BufferedImage.TYPE_INT_RGB) img else BufferedImage(img.width, img.height, BufferedImage.TYPE_INT_RGB).also {
            it.createGraphics().apply { drawImage(img, 0, 0, null); dispose() }
        }
        val bytes = java.io.ByteArrayOutputStream()
        val writer = javax.imageio.ImageIO.getImageWritersByFormatName("jpeg").next()
        val param = writer.defaultWriteParam.apply {
            compressionMode = javax.imageio.ImageWriteParam.MODE_EXPLICIT
            compressionQuality = quality / 100f
        }
        javax.imageio.ImageIO.createImageOutputStream(bytes).use { s ->
            writer.output = s
            writer.write(null, javax.imageio.IIOImage(rgb, null, null), param)
        }
        writer.dispose()
        return bytes.toByteArray()
    }

    /** What is left (for the message of a failed test). */
    fun left(): String = "shots ${shots.count { s -> !s.wiped || s.jpeg.any { it != 0.toByte() } }}/${shots.size}, " +
        "pages ${pages.count { p -> p.jpeg.any { it != 0.toByte() } }}/${pages.size}, " +
        "pictures ${bitmaps.count { !it.released }}/${bitmaps.size}"

    /** Whether nothing of the scan is left: every photo and page overwritten, every picture released. */
    fun allWiped(): Boolean =
        shots.all { s -> s.wiped && s.jpeg.all { it == 0.toByte() } } &&
            pages.all { p -> p.jpeg.all { it == 0.toByte() } } &&
            bitmaps.all { it.released }
}
