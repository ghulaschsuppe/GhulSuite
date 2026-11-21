// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.scan.GrayImage
import de.danerd.ghulsuite.scan.Pt
import de.danerd.ghulsuite.scan.Quad
import de.danerd.ghulsuite.scan.SCAN_PAGE_MAX_SIDE
import de.danerd.ghulsuite.scan.buildPdf
import de.danerd.ghulsuite.scan.detectSheet
import de.danerd.ghulsuite.scan.documentFilter
import de.danerd.ghulsuite.scan.grayFromArgb
import de.danerd.ghulsuite.scan.isValidQuad
import de.danerd.ghulsuite.scan.jpegInfo
import de.danerd.ghulsuite.scan.mapPoint
import de.danerd.ghulsuite.scan.moveCorner
import de.danerd.ghulsuite.scan.orderCorners
import de.danerd.ghulsuite.scan.pageSize
import de.danerd.ghulsuite.scan.pageTransform
import de.danerd.ghulsuite.scan.pdfName
import de.danerd.ghulsuite.scan.pdfPageSize
import de.danerd.ghulsuite.scan.perspective
import de.danerd.ghulsuite.scan.scanFileName
import de.danerd.ghulsuite.scan.sheetAspect
import de.danerd.ghulsuite.scan.trueSize
import de.danerd.ghulsuite.scan.uprightSize
import de.danerd.ghulsuite.scan.uprightToRaw
import java.io.File
import kotlin.math.abs
import kotlin.random.Random
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The plain Kotlin parts of scanning documents (G7): finding the sheet in drawn photos, the
 * frame a finger moves, the size and transform of the straightened page, the "Document" filter
 * and the PDF (read back with PDFBox, as the Windows app shows PDFs).
 */
class ScanAlgorithmsTest {
    private val out = File("build/screenshots").apply { mkdirs() }

    // --- drawn photos ------------------------------------------------------------------------

    /** A photo [w] x [h]: background [bg], the sheet [q] (pixels, may reach outside) in [sheet], noise [noise], text lines. */
    private fun photo(
        w: Int, h: Int, q: Quad?, bg: Int, sheet: Int, noise: Double, seed: Int = 1,
        text: Boolean = true, shadow: Boolean = false,
    ): GrayImage {
        val r = Random(seed)
        val px = IntArray(w * h)
        val inv = q?.let { perspective(it, Quad(Pt(0f, 0f), Pt(1f, 0f), Pt(1f, 1f), Pt(0f, 1f))) }
        for (y in 0 until h) for (x in 0 until w) {
            // 4 x 4 samples per pixel: soft edges as a camera has them.
            var inside = 0
            var ink = 0
            for (sy in 0 until 4) for (sx in 0 until 4) {
                val p = Pt(x + (sx + 0.5f) / 4, y + (sy + 0.5f) / 4)
                if (q != null && insideQuad(q, p)) {
                    inside++
                    if (text) {
                        val u = mapPoint(inv!!, p)
                        // Lines of "text" in the middle of the sheet.
                        val line = ((u.y - 0.12f) / 0.045f)
                        if (u.x in 0.12f..0.85f && u.y in 0.12f..0.8f && line - kotlin.math.floor(line) < 0.35f &&
                            ((u.x * 37).toInt() % 5 != 0)
                        ) ink++
                    }
                }
            }
            val shade = if (shadow) 1f - 0.35f * x / w else 1f
            var v = bg + (sheet - bg) * inside / 16.0
            v -= (sheet - 40) * 0.8 * ink / 16.0
            v = v * shade + r.nextGaussianLike() * noise
            px[y * w + x] = v.toInt().coerceIn(0, 255)
        }
        return GrayImage(w, h, px)
    }

    private fun Random.nextGaussianLike(): Double {
        // Sum of uniform values: close enough to a normal distribution, reproducible.
        var s = 0.0
        repeat(12) { s += nextDouble() }
        return s - 6.0
    }

    private fun insideQuad(q: Quad, p: Pt): Boolean {
        val pts = q.points
        for (i in 0 until 4) {
            val a = pts[i]
            val b = pts[(i + 1) % 4]
            if ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) < 0) return false
        }
        return true
    }

    private fun save(name: String, img: GrayImage, guess: Quad) {
        val bmp = java.awt.image.BufferedImage(img.width, img.height, java.awt.image.BufferedImage.TYPE_INT_RGB)
        for (y in 0 until img.height) for (x in 0 until img.width) {
            val v = img.pixels[y * img.width + x]
            bmp.setRGB(x, y, (v shl 16) or (v shl 8) or v)
        }
        val g = bmp.createGraphics()
        g.color = java.awt.Color(255, 60, 60)
        g.stroke = java.awt.BasicStroke(2f)
        val p = guess.scale(img.width.toFloat(), img.height.toFloat()).points
        for (i in 0 until 4) g.drawLine(p[i].x.toInt(), p[i].y.toInt(), p[(i + 1) % 4].x.toInt(), p[(i + 1) % 4].y.toInt())
        g.dispose()
        javax.imageio.ImageIO.write(bmp, "png", File(out, "scan-detect-$name.png"))
    }

    /** Each corner of [found] (parts of the picture) within [tol] pixels of [expected] (pixels). */
    private fun assertCorners(expected: Quad, found: Quad, w: Int, h: Int, tol: Float) {
        val f = found.scale(w.toFloat(), h.toFloat())
        for (i in 0 until 4) {
            val d = expected[i].dist(f[i])
            assertTrue(d <= tol, "corner $i: expected ${expected[i]}, found ${f[i]} ($d px away)")
        }
    }

    // --- detection ---------------------------------------------------------------------------

    @Test
    fun findsTiltedSheetOnDarkNoisyBackground() {
        val q = Quad(Pt(70f, 52f), Pt(296f, 80f), Pt(281f, 408f), Pt(38f, 376f))
        val img = photo(360, 480, q, bg = 52, sheet = 205, noise = 14.0)
        val guess = detectSheet(img)
        save("tilted", img, guess.corners)
        assertTrue(guess.found)
        assertCorners(q, guess.corners, 360, 480, tol = 4f)
    }

    @Test
    fun findsSheetWithLowContrastAndShadow() {
        val q = Quad(Pt(52f, 60f), Pt(300f, 44f), Pt(318f, 420f), Pt(60f, 440f))
        val img = photo(360, 480, q, bg = 112, sheet = 148, noise = 8.0, seed = 3, shadow = true)
        val guess = detectSheet(img)
        save("low-contrast", img, guess.corners)
        assertTrue(guess.found)
        assertCorners(q, guess.corners, 360, 480, tol = 5f)
    }

    @Test
    fun findsSheetLargerThanThePhoto() {
        // The lower part of the sheet is outside the photo: its lower corners are where its sides leave the picture.
        val q = Quad(Pt(64f, 50f), Pt(302f, 36f), Pt(330f, 560f), Pt(40f, 580f))
        val img = photo(360, 480, q, bg = 40, sheet = 200, noise = 10.0, seed = 5)
        val guess = detectSheet(img)
        save("partial", img, guess.corners)
        assertTrue(guess.found)
        fun xAt(a: Pt, b: Pt, y: Float) = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y)
        val visible = Quad(q.tl, q.tr, Pt(xAt(q.tr, q.br, 480f), 480f), Pt(xAt(q.tl, q.bl, 480f), 480f))
        assertCorners(visible, guess.corners, 360, 480, tol = 5f)
    }

    @Test
    fun findsRotatedSheetInLandscapePhoto() {
        val q = Quad(Pt(150f, 30f), Pt(420f, 90f), Pt(370f, 330f), Pt(100f, 262f))
        val img = photo(480, 360, q, bg = 70, sheet = 220, noise = 12.0, seed = 9)
        val guess = detectSheet(img)
        save("landscape", img, guess.corners)
        assertTrue(guess.found)
        assertCorners(q, guess.corners, 480, 360, tol = 4f)
    }

    @Test
    fun findsSheetInLargePhoto() {
        // A photo larger than the detection works on (made smaller first), here at 2 x.
        val q = Quad(Pt(140f, 104f), Pt(592f, 160f), Pt(562f, 816f), Pt(76f, 752f))
        val img = photo(720, 960, q, bg = 52, sheet = 205, noise = 14.0, seed = 11, text = false)
        val guess = detectSheet(img)
        assertTrue(guess.found)
        assertCorners(q, guess.corners, 720, 960, tol = 8f)
    }

    @Test
    fun noSheetGivesTheInsetFrame() {
        val cases = listOf(
            "noise" to photo(360, 480, null, bg = 120, sheet = 0, noise = 25.0, seed = 7),
            "flat" to GrayImage(360, 480, IntArray(360 * 480) { 128 }),
            "gradient" to GrayImage(360, 480, IntArray(360 * 480) { (it % 360) * 255 / 360 }),
            "sheet-fills-photo" to photo(360, 480, Quad(Pt(-20f, -20f), Pt(380f, -20f), Pt(380f, 500f), Pt(-20f, 500f)), 40, 210, 8.0),
        )
        for ((name, img) in cases) {
            val guess = detectSheet(img)
            save("none-$name", img, guess.corners)
            assertFalse(guess.found, name)
            assertEquals(Quad.inset(), guess.corners, name)
        }
    }

    @Test
    fun grayFromArgbAveragesBlocks() {
        val w = 1000
        val h = 500
        val argb = IntArray(w * h) { i -> if ((i % w) < w / 2) 0xFFFFFFFF.toInt() else 0xFF000000.toInt() }
        val g = grayFromArgb(argb, w, h)
        assertTrue(g.width <= 360 && g.height <= 360)
        assertEquals(255, g.pixels[0])
        assertEquals(0, g.pixels[g.width - 1])
    }

    // --- the frame ---------------------------------------------------------------------------

    @Test
    fun cornersAreOrderedClockwiseFromTopLeft() {
        val tl = Pt(10f, 12f)
        val tr = Pt(200f, 5f)
        val br = Pt(210f, 300f)
        val bl = Pt(3f, 280f)
        for (perm in listOf(listOf(br, tl, bl, tr), listOf(bl, br, tr, tl), listOf(tr, bl, tl, br))) {
            assertEquals(Quad(tl, tr, br, bl), orderCorners(perm))
        }
        // A sheet turned by about 40 degrees.
        val q = orderCorners(listOf(Pt(100f, 0f), Pt(200f, 100f), Pt(100f, 200f), Pt(5f, 95f)))
        assertTrue(isValidQuad(q))
    }

    @Test
    fun invalidShapesAreRecognised() {
        val ok = Quad(Pt(0f, 0f), Pt(100f, 0f), Pt(100f, 100f), Pt(0f, 100f))
        assertTrue(isValidQuad(ok))
        // Crossed (bow tie), counter-clockwise, concave, a side too short, a flat angle.
        assertFalse(isValidQuad(Quad(Pt(0f, 0f), Pt(100f, 100f), Pt(100f, 0f), Pt(0f, 100f))))
        assertFalse(isValidQuad(Quad(Pt(0f, 0f), Pt(0f, 100f), Pt(100f, 100f), Pt(100f, 0f))))
        assertFalse(isValidQuad(Quad(Pt(0f, 0f), Pt(100f, 0f), Pt(30f, 30f), Pt(0f, 100f))))
        assertFalse(isValidQuad(ok.with(1, Pt(2f, 0f)), minSide = 10f))
        assertFalse(isValidQuad(Quad(Pt(0f, 0f), Pt(100f, 0f), Pt(100f, 100f), Pt(5f, 6f))))
    }

    @Test
    fun aCornerCannotBePulledIntoAnInvalidShape() {
        val q = Quad.inset(0.1f)
        val w = 400f
        val h = 600f
        // Into the middle of the frame and beyond the opposite corner: it stops where the frame is still valid.
        for (target in listOf(Pt(0.5f, 0.5f), Pt(0.95f, 0.95f), Pt(0.9f, 0.1f), Pt(0.1f, 0.9f), Pt(1.2f, 1.4f))) {
            val m = moveCorner(q, 0, target, w, h)
            assertTrue(isValidQuad(m.scale(w, h), 24f), "target $target gave $m")
            assertEquals(q.tr, m.tr)
            assertEquals(q.br, m.br)
            assertEquals(q.bl, m.bl)
        }
        // An allowed move goes all the way; outside the picture it stops at the edge.
        assertEquals(Pt(0.2f, 0.15f), moveCorner(q, 0, Pt(0.2f, 0.15f), w, h).tl)
        assertEquals(Pt(0f, 0f), moveCorner(q, 0, Pt(-0.3f, -1f), w, h).tl)
        assertEquals(Pt(1f, 1f), moveCorner(q, 2, Pt(1.5f, 1.1f), w, h).br)
        // Towards the centre it gets as close as the shape allows (the frame does not just stand still).
        val near = moveCorner(q, 0, Pt(0.95f, 0.95f), w, h)
        assertTrue(near.tl.x > 0.3f && near.tl.y > 0.3f, "moved only to ${near.tl}")
    }

    // --- the page ----------------------------------------------------------------------------

    @Test
    fun pageSizeFollowsTheLongerSidesAndTheLimit() {
        val q = Quad(Pt(0f, 0f), Pt(1000f, 40f), Pt(980f, 1400f), Pt(10f, 1414f))
        val (w, h) = pageSize(q, 5000)
        assertEquals(1001, w)
        assertEquals(1414, h)
        val (sw, sh) = pageSize(q, SCAN_PAGE_MAX_SIDE)
        assertEquals(1001 to 1414, sw to sh)
        val big = q.scale(3f, 3f)
        val (bw, bh) = pageSize(big, SCAN_PAGE_MAX_SIDE)
        assertEquals(SCAN_PAGE_MAX_SIDE, bh)
        assertTrue(abs(bw - 2500 * 1001 / 1414) <= 1)
        // Never smaller than one pixel.
        assertEquals(1 to 1, pageSize(Quad(Pt(0f, 0f), Pt(0f, 0f), Pt(0f, 0f), Pt(0f, 0f)), 2500))
    }

    /**
     * A sheet photographed by a camera ([f] focal length in pixels, the photo [w] x [h]): turned by
     * [tiltX] about the horizontal and [tiltY] about the vertical axis (degrees), [dist] away.
     * Returns its corners in the photo (top left, top right, bottom right, bottom left).
     */
    private fun photographed(sheetW: Double, sheetH: Double, tiltX: Double, tiltY: Double, dist: Double, f: Double, w: Int, h: Int): Quad {
        val ax = Math.toRadians(tiltX)
        val ay = Math.toRadians(tiltY)
        fun project(x: Double, y: Double): Pt {
            // Rotate about x, then about y, then move away from the camera.
            val y1 = y * kotlin.math.cos(ax)
            val z1 = y * kotlin.math.sin(ax)
            val x2 = x * kotlin.math.cos(ay) + z1 * kotlin.math.sin(ay)
            val z2 = -x * kotlin.math.sin(ay) + z1 * kotlin.math.cos(ay) + dist
            return Pt((w / 2.0 + f * x2 / z2).toFloat(), (h / 2.0 + f * y1 / z2).toFloat())
        }
        val hw = sheetW / 2
        val hh = sheetH / 2
        return Quad(project(-hw, -hh), project(hw, -hh), project(hw, hh), project(-hw, hh))
    }

    /**
     * An A4 sheet photographed at an angle keeps its shape (1 : 1.414): the sides measured in the
     * photo are shortened along the tilt (a sheet taken from below came out almost square).
     */
    @Test
    fun aSheetPhotographedAtAnAngleKeepsItsShape() {
        val (w, h) = 3000 to 4000
        val tilts = listOf(0.0 to 0.0, 20.0 to 0.0, 35.0 to 0.0, 45.0 to 0.0, 0.0 to 30.0, 45.0 to 10.0, -30.0 to 20.0, 20.0 to -25.0)
        // Phone cameras: about 23 to 28 mm (35 mm film); tilted about one axis only, the focal
        // length cannot be found and a typical one is taken.
        val focals = listOf(0.68 * h, 0.75 * h, 0.82 * h)
        // Every shape: A4 upright and lying, square, 16 : 9, a card, 4 : 3.
        for ((sw, sh) in listOf(210.0 to 297.0, 297.0 to 210.0, 200.0 to 200.0, 160.0 to 90.0, 85.6 to 54.0, 120.0 to 90.0)) {
            val real = sw / sh
            for ((tx, ty) in tilts) for (f in focals) {
                val q = photographed(sw, sh, tx, ty, 400.0, f, w, h)
                val r = assertNotNull(sheetAspect(q, w / 2f, h / 2f, h.toFloat()))
                assertTrue(abs(r / real - 1) < 0.055, "$sw x $sh, tilt $tx/$ty, f $f: $r instead of $real")
                // The page has that shape; no shape (DIN A or any other) is preferred.
                val (pw, ph) = trueSize(q, w / 2f, h / 2f, h.toFloat(), 2500)
                assertTrue(abs(pw.toDouble() / ph / real - 1) < 0.055, "$sw x $sh, tilt $tx/$ty, f $f: $pw x $ph")
            }
        }
        // The old way: 35 degrees from below made an A4 sheet far too wide.
        val q = photographed(210.0, 297.0, 35.0, 0.0, 400.0, 0.75 * h, w, h)
        val (mw, mh) = pageSize(q, Int.MAX_VALUE)
        assertTrue(mw.toDouble() / mh > 210.0 / 297.0 * 1.15, "the measured sides alone are wrong: ${mw.toDouble() / mh}")
    }

    @Test
    fun perspectiveMapsTheCorners() {
        val src = Quad(Pt(10f, 20f), Pt(310f, 5f), Pt(330f, 420f), Pt(-4f, 400f))
        val dst = Quad(Pt(0f, 0f), Pt(800f, 0f), Pt(800f, 1100f), Pt(0f, 1100f))
        val m = perspective(src, dst)
        for (i in 0 until 4) {
            val p = mapPoint(m, src[i])
            assertTrue(p.dist(dst[i]) < 0.05f, "corner $i went to $p")
        }
        // The middle of the sheet (where its diagonals cross) lands in the middle of the page.
        val c = mapPoint(m, diagonalsCross(src))
        assertTrue(c.dist(Pt(400f, 550f)) < 0.5f, "centre went to $c")
    }

    private fun diagonalsCross(q: Quad): Pt {
        val a = q.tl; val b = q.br; val c = q.tr; val d = q.bl
        val den = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x)
        val t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / den
        return Pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    @Test
    fun uprightAndStoredPhotoPointsAgree() {
        // A stored photo 400 x 300 turned 90 degrees clockwise is 300 x 400 upright.
        assertEquals(300 to 400, uprightSize(90, 400, 300))
        assertEquals(400 to 300, uprightSize(180, 400, 300))
        // The top left of the upright photo is the bottom left of the stored one (turned clockwise).
        assertEquals(Pt(0f, 300f), uprightToRaw(Pt(0f, 0f), 90, 400f, 300f))
        assertEquals(Pt(400f, 300f), uprightToRaw(Pt(0f, 0f), 180, 400f, 300f))
        assertEquals(Pt(400f, 0f), uprightToRaw(Pt(0f, 0f), 270, 400f, 300f))
        assertEquals(Pt(5f, 7f), uprightToRaw(Pt(5f, 7f), 0, 400f, 300f))
    }

    @Test
    fun pageTransformCutsStraightensAndTurns() {
        val corners = Quad(Pt(0.1f, 0.1f), Pt(0.9f, 0.1f), Pt(0.9f, 0.9f), Pt(0.1f, 0.9f))
        // Stored 400 x 300, shown upright turned 90 degrees: 300 x 400.
        val t = pageTransform(corners, 90, 400, 300, 0, 2500)
        assertEquals(240, t.width)
        assertEquals(320, t.height)
        // The top left of the sheet (upright 30, 40) is (40, 270) in the stored photo.
        assertTrue(mapPoint(t.matrix, Pt(40f, 270f)).dist(Pt(0f, 0f)) < 0.05f)
        // A quarter turn more: the page lies, its top left at the top right.
        val turned = pageTransform(corners, 90, 400, 300, 1, 2500)
        assertEquals(320, turned.width)
        assertEquals(240, turned.height)
        assertTrue(mapPoint(turned.matrix, Pt(40f, 270f)).dist(Pt(320f, 0f)) < 0.05f)
        // Four quarter turns are none; the limit keeps the longer side.
        assertEquals(t.width, pageTransform(corners, 90, 400, 300, 4, 2500).width)
        val small = pageTransform(corners, 0, 4000, 3000, 0, 2500)
        assertEquals(2500, small.width)
        assertEquals(1875, small.height)
    }

    @Test
    fun fileNamesOfScans() {
        assertEquals("Scan 2026-11-19 14.05.pdf", scanFileName("2026-11-19T14:05"))
        assertEquals("Scan 2026-11-19.pdf", scanFileName("2026-11-19"))
        assertEquals("Steuer 2025.pdf", pdfName("  Steuer 2025 ", "x.pdf"))
        assertEquals("Brief.PDF", pdfName("Brief.PDF", "x.pdf"))
        assertEquals("Scan.pdf", pdfName("   ", "Scan.pdf"))
    }

    // --- filter and PDF ----------------------------------------------------------------------

    @Test
    fun documentFilterMakesPaperWhiteAndTextBlack() {
        val w = 600
        val h = 800
        val px = IntArray(w * h)
        for (y in 0 until h) for (x in 0 until w) {
            // Paper in a shadow (darker to the right) with lines of text, slightly yellow.
            val paper = 215 - 95 * x / w
            val text = y % 40 in 10..17 && x % 60 in 5..50
            val v = if (text) (paper * 0.3).toInt() else paper
            px[y * w + x] = (0xFF shl 24) or (v shl 16) or (v shl 8) or (v * 9 / 10)
        }
        documentFilter(px, w, h)
        fun gray(x: Int, y: Int): Int {
            val c = px[y * w + x]
            assertEquals((c shr 16) and 255, c and 255, "gray")
            return c and 255
        }
        // Paper in the light and in the shadow.
        assertTrue(gray(30, 30) >= 245, "paper left ${gray(30, 30)}")
        assertTrue(gray(560, 30) >= 245, "paper right ${gray(560, 30)}")
        // Text in the light and in the shadow.
        assertTrue(gray(20, 12) <= 60, "text left ${gray(20, 12)}")
        assertTrue(gray(560, 12) <= 60, "text right ${gray(560, 12)}")
    }

    private fun jpeg(w: Int, h: Int, color: Int, quality: Int = 80): ByteArray {
        val bmp = java.awt.image.BufferedImage(w, h, java.awt.image.BufferedImage.TYPE_INT_RGB)
        val g = bmp.createGraphics()
        g.color = java.awt.Color(color)
        g.fillRect(0, 0, w, h)
        g.color = java.awt.Color.BLACK
        g.drawString("GhulDocs", w / 4, h / 2)
        g.dispose()
        val bytes = java.io.ByteArrayOutputStream()
        val writer = javax.imageio.ImageIO.getImageWritersByFormatName("jpeg").next()
        val param = writer.defaultWriteParam.apply {
            compressionMode = javax.imageio.ImageWriteParam.MODE_EXPLICIT
            compressionQuality = quality / 100f
        }
        javax.imageio.ImageIO.createImageOutputStream(bytes).use { s ->
            writer.output = s
            writer.write(null, javax.imageio.IIOImage(bmp, null, null), param)
        }
        writer.dispose()
        return bytes.toByteArray()
    }

    @Test
    fun jpegInfoReadsTheFrame() {
        val j = jpeg(321, 123, 0xEEEEEE)
        val info = assertNotNull(jpegInfo(j))
        assertEquals(321, info.width)
        assertEquals(123, info.height)
        assertEquals(3, info.components)
        assertNull(jpegInfo(byteArrayOf(1, 2, 3, 4, 5)))
        assertNull(jpegInfo("%PDF-1.4".encodeToByteArray()))
    }

    @Test
    fun pdfPageSizesFitA4() {
        // A4 itself (as a scanned A4 sheet is about) and turned.
        val (w, h) = pdfPageSize(1768, 2500)
        assertTrue(abs(h - 841.89f) < 0.5f && abs(w - 595.28f) < 0.5f, "$w x $h")
        val (lw, lh) = pdfPageSize(2500, 1768)
        assertTrue(abs(lw - 841.89f) < 0.5f && abs(lh - 595.28f) < 0.5f, "$lw x $lh")
        // Wider than A4: as wide as A4, less high.
        val (ww, wh) = pdfPageSize(2000, 2000)
        assertTrue(abs(ww - 595.28f) < 0.01f && abs(wh - 595.28f) < 0.01f, "$ww x $wh")
        // A receipt: tall and narrow, as high as A4.
        val (rw, rh) = pdfPageSize(600, 2400)
        assertTrue(abs(rh - 841.89f) < 0.01f && abs(rw - 210.47f) < 0.1f, "$rw x $rh")
    }

    @Test
    fun pdfHoldsEveryPageAsItsJpeg() {
        val pages = listOf(jpeg(1768, 2500, 0xF4F4F0), jpeg(2500, 1768, 0xFFFFFF), jpeg(800, 2400, 0xDDDDDD))
        val pdf = buildPdf(pages)
        assertTrue(pdf.decodeToString(0, 8) == "%PDF-1.4")
        // The JPEGs are inside as they are (not encoded again): the file is about their size.
        assertTrue(pdf.size < pages.sumOf { it.size } + 4000, "PDF ${pdf.size} bytes")
        org.apache.pdfbox.Loader.loadPDF(pdf).use { doc ->
            assertEquals(3, doc.numberOfPages)
            val boxes = doc.pages.map { it.mediaBox }
            assertTrue(abs(boxes[0].height - 841.89f) < 0.5f)
            assertTrue(abs(boxes[1].width - 841.89f) < 0.5f)
            assertTrue(abs(boxes[2].height - 841.89f) < 0.02f && abs(boxes[2].width - 280.63f) < 0.1f)
            val image = doc.getPage(0).resources.xObjectNames.map { doc.getPage(0).resources.getXObject(it) }.single()
                as org.apache.pdfbox.pdmodel.graphics.image.PDImageXObject
            assertEquals(1768, image.width)
            assertEquals(2500, image.height)
            // Drawn: the page is light where the picture is light.
            val shown = org.apache.pdfbox.rendering.PDFRenderer(doc).renderImage(0, 0.5f)
            val c = shown.getRGB(20, 20)
            assertTrue((c and 255) > 220, "page drawn as ${Integer.toHexString(c)}")
        }
    }
}
