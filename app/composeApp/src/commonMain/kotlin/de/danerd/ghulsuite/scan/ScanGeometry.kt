// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.scan

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt

// The geometry of a scanned page, in plain Kotlin (tested on the desktop): the four corners of the
// sheet, keeping them a sensible shape while a finger moves them, the size of the straightened
// page and the perspective transform that straightens it.

/** A point; in pixels or in parts of the picture (0 to 1), as the caller uses it. */
data class Pt(val x: Float, val y: Float) {
    operator fun minus(o: Pt) = Pt(x - o.x, y - o.y)
    operator fun plus(o: Pt) = Pt(x + o.x, y + o.y)
    operator fun times(f: Float) = Pt(x * f, y * f)
    fun dist(o: Pt): Float = hypot(x - o.x, y - o.y)
}

/** The corners of a sheet, clockwise on the screen (y points down): top left, top right, bottom right, bottom left. */
data class Quad(val tl: Pt, val tr: Pt, val br: Pt, val bl: Pt) {
    val points: List<Pt> get() = listOf(tl, tr, br, bl)

    operator fun get(i: Int): Pt = when (i) {
        0 -> tl
        1 -> tr
        2 -> br
        3 -> bl
        else -> throw IndexOutOfBoundsException("corner $i")
    }

    /** The same with corner [i] at [p]. */
    fun with(i: Int, p: Pt): Quad = when (i) {
        0 -> copy(tl = p)
        1 -> copy(tr = p)
        2 -> copy(br = p)
        3 -> copy(bl = p)
        else -> throw IndexOutOfBoundsException("corner $i")
    }

    fun scale(sx: Float, sy: Float) = Quad(
        Pt(tl.x * sx, tl.y * sy), Pt(tr.x * sx, tr.y * sy), Pt(br.x * sx, br.y * sy), Pt(bl.x * sx, bl.y * sy),
    )

    /** Signed area (shoelace): positive when the corners go clockwise on the screen. */
    fun signedArea(): Float {
        val p = points
        var s = 0f
        for (i in 0 until 4) {
            val a = p[i]
            val b = p[(i + 1) % 4]
            s += a.x * b.y - b.x * a.y
        }
        return s / 2f
    }

    fun area(): Float = abs(signedArea())

    companion object {
        /** The whole picture (in parts of it). */
        val FULL = Quad(Pt(0f, 0f), Pt(1f, 0f), Pt(1f, 1f), Pt(0f, 1f))

        /** A frame [inset] (part of the picture) inside its edges: the suggestion when no sheet is found. */
        fun inset(inset: Float = 0.08f) = Quad(Pt(inset, inset), Pt(1 - inset, inset), Pt(1 - inset, 1 - inset), Pt(inset, 1 - inset))
    }
}

/**
 * Four points in any order as a [Quad]: around their centre clockwise, starting with the one
 * nearest the top left (the smallest x + y).
 */
fun orderCorners(pts: List<Pt>): Quad {
    require(pts.size == 4) { "four corners" }
    val cx = pts.sumOf { it.x.toDouble() } / 4
    val cy = pts.sumOf { it.y.toDouble() } / 4
    // Screen coordinates (y down): a growing angle goes clockwise.
    val around = pts.sortedBy { atan2(it.y - cy, it.x - cx) }
    val first = around.indices.minBy { around[it].x + around[it].y }
    val o = List(4) { around[(first + it) % 4] }
    return Quad(o[0], o[1], o[2], o[3])
}

private fun cross(o: Pt, a: Pt, b: Pt): Float = (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)

/**
 * Whether [q] is a shape a sheet can have in a photo: convex, its corners clockwise in their
 * roles (top left stays top left), every side at least [minSide] long and every angle between
 * 15 and 165 degrees. Coordinates in pixels (or any units with the same scale on both axes).
 */
fun isValidQuad(q: Quad, minSide: Float = 1f): Boolean {
    val p = q.points
    for (i in 0 until 4) {
        val a = p[i]
        val b = p[(i + 1) % 4]
        val c = p[(i + 2) % 4]
        if (a.dist(b) < minSide) return false
        // Clockwise on the screen: every turn goes the same way (positive cross product, y down).
        val turn = cross(a, b, c)
        if (turn <= 0f) return false
        // The angle at b: sin = cross / (|ab| |bc|); too flat or too sharp is no sheet.
        val sin = turn / (a.dist(b) * b.dist(c))
        if (sin < MIN_SIN) return false
    }
    return true
}

/** sin(15 degrees): angles of a corner between 15 and 165 degrees. */
private const val MIN_SIN = 0.2588f

/**
 * Moves corner [i] of [q] (coordinates in parts of a picture [w] x [h] pixels) towards [target]:
 * kept inside the picture, and only as far as the frame stays a valid shape ([isValidQuad]) with
 * sides of at least [minSidePx]. A finger that pulls a corner across the frame leaves it at the
 * last valid place instead of folding the frame.
 */
fun moveCorner(q: Quad, i: Int, target: Pt, w: Float, h: Float, minSidePx: Float = 24f): Quad {
    val to = Pt(target.x.coerceIn(0f, 1f), target.y.coerceIn(0f, 1f))
    fun ok(c: Quad) = isValidQuad(c.scale(w, h), minSidePx)
    val moved = q.with(i, to)
    if (ok(moved)) return moved
    if (!ok(q)) return q
    // As far as it goes on the way from the old place to the target.
    val from = q[i]
    var lo = 0f
    var hi = 1f
    repeat(14) {
        val mid = (lo + hi) / 2
        if (ok(q.with(i, from + (to - from) * mid))) lo = mid else hi = mid
    }
    return q.with(i, from + (to - from) * lo)
}

/**
 * Width and height of the straightened page for corners [q] in pixels: the longer of each pair
 * of opposite sides, made smaller (never larger) so the longer side is at most [maxSide].
 */
fun pageSize(q: Quad, maxSide: Int): Pair<Int, Int> {
    val w = max(q.tl.dist(q.tr), q.bl.dist(q.br)).toDouble()
    val h = max(q.tl.dist(q.bl), q.tr.dist(q.br)).toDouble()
    val f = minOf(1.0, maxSide / max(max(w, h), 1.0))
    return max(1, (w * f).roundToInt()) to max(1, (h * f).roundToInt())
}

/**
 * The real width : height of the sheet whose corners [q] (pixels of the upright photo, its centre
 * at [cx], [cy]) were photographed at an angle: the sides measured in the photo are shortened by
 * the perspective (both sides along the tilt alike), so their lengths alone give a wrong shape (an
 * A4 sheet taken at an angle came out almost square). The perspective of the quad gives the
 * camera's focal length and with it the shape of the sheet (Z. Zhang, L. He, "Whiteboard scanning
 * and image enhancement", 2007). A focal length that cannot be found reliably (almost no
 * perspective, or a value no phone camera has) is taken as a typical one of a phone ([longSide]
 * of the photo times 0.75). Null if the quad gives no shape at all.
 */
fun sheetAspect(q: Quad, cx: Float, cy: Float, longSide: Float): Double? {
    fun h(p: Pt) = doubleArrayOf(p.x.toDouble(), p.y.toDouble(), 1.0)
    fun crossV(a: DoubleArray, b: DoubleArray) =
        doubleArrayOf(a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
    fun dot(a: DoubleArray, b: DoubleArray) = a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    // m1 top left, m2 top right (along the width), m3 bottom left (along the height), m4 bottom right.
    val m1 = h(q.tl)
    val m2 = h(q.tr)
    val m3 = h(q.bl)
    val m4 = h(q.br)
    val c14 = crossV(m1, m4)
    val d2 = dot(crossV(m2, m4), m3)
    val d3 = dot(crossV(m3, m4), m2)
    if (abs(d2) < 1e-9 || abs(d3) < 1e-9) return null
    val k2 = dot(c14, m3) / d2
    val k3 = dot(c14, m2) / d3
    val n2 = DoubleArray(3) { k2 * m2[it] - m1[it] }
    val n3 = DoubleArray(3) { k3 * m3[it] - m1[it] }
    val u0 = cx.toDouble()
    val v0 = cy.toDouble()
    val typical = 0.75 * longSide
    val denom = n2[2] * n3[2]
    var f = typical
    if (abs(denom) > 1e-12) {
        val f2 = -((n2[0] * n3[0] - (n2[0] * n3[2] + n2[2] * n3[0]) * u0 + n2[2] * n3[2] * u0 * u0) +
            (n2[1] * n3[1] - (n2[1] * n3[2] + n2[2] * n3[1]) * v0 + n2[2] * n3[2] * v0 * v0)) / denom
        val found = if (f2 > 0) kotlin.math.sqrt(f2) else Double.NaN
        // Phones: roughly 0.5 (wide) to 2.5 (tele) times the long side of the photo.
        if (found.isFinite() && found in 0.5 * longSide..2.5 * longSide) f = found
    }
    // n^T K^-T K^-1 n with K = [[f, 0, u0], [0, f, v0], [0, 0, 1]].
    fun len2(n: DoubleArray): Double {
        val x = (n[0] - u0 * n[2]) / f
        val y = (n[1] - v0 * n[2]) / f
        return x * x + y * y + n[2] * n[2]
    }
    val a = len2(n2)
    val b = len2(n3)
    if (a <= 0 || b <= 0) return null
    val r = kotlin.math.sqrt(a / b)
    return r.takeIf { it.isFinite() && it > 0.05 && it < 20 }
}

/**
 * The size of the straightened page: the shape from [sheetAspect], whatever it is (upright, lying,
 * square, any other; no shape is preferred), the longer side as long as the longer side measured
 * in the photo; at most [maxSide].
 */
fun trueSize(q: Quad, cx: Float, cy: Float, longSide: Float, maxSide: Int): Pair<Int, Int> {
    val measured = pageSize(q, Int.MAX_VALUE)
    val aspect = sheetAspect(q, cx, cy, longSide) ?: return pageSize(q, maxSide)
    val long = max(measured.first, measured.second).toDouble()
    val (w, h) = if (aspect >= 1) long to long / aspect else long * aspect to long
    val f = minOf(1.0, maxSide / max(max(w, h), 1.0))
    return max(1, (w * f).roundToInt()) to max(1, (h * f).roundToInt())
}

/**
 * The 3 x 3 perspective transform (row-major, the last value 1) that maps the corners of [src]
 * onto those of [dst]: as Android's `Matrix.setPolyToPoly` with four points, and Skia's Matrix33.
 */
fun perspective(src: Quad, dst: Quad): FloatArray {
    // a x + b y + c - g x X - h y X = X;  d x + e y + f - g x Y - h y Y = Y
    val m = Array(8) { DoubleArray(9) }
    for (i in 0 until 4) {
        val s = src[i]
        val d = dst[i]
        val x = s.x.toDouble()
        val y = s.y.toDouble()
        val u = d.x.toDouble()
        val v = d.y.toDouble()
        m[2 * i] = doubleArrayOf(x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u)
        m[2 * i + 1] = doubleArrayOf(0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v)
    }
    // Gaussian elimination with partial pivoting.
    for (col in 0 until 8) {
        var pivot = col
        for (r in col + 1 until 8) if (abs(m[r][col]) > abs(m[pivot][col])) pivot = r
        require(abs(m[pivot][col]) > 1e-12) { "degenerate corners" }
        val t = m[col]; m[col] = m[pivot]; m[pivot] = t
        for (r in 0 until 8) {
            if (r == col) continue
            val f = m[r][col] / m[col][col]
            if (f == 0.0) continue
            for (c in col until 9) m[r][c] -= f * m[col][c]
        }
    }
    val h = DoubleArray(8) { m[it][8] / m[it][it] }
    return floatArrayOf(
        h[0].toFloat(), h[1].toFloat(), h[2].toFloat(),
        h[3].toFloat(), h[4].toFloat(), h[5].toFloat(),
        h[6].toFloat(), h[7].toFloat(), 1f,
    )
}

/** [p] mapped by the 3 x 3 transform [m]. */
fun mapPoint(m: FloatArray, p: Pt): Pt {
    val w = m[6] * p.x + m[7] * p.y + m[8]
    return Pt((m[0] * p.x + m[1] * p.y + m[2]) / w, (m[3] * p.x + m[4] * p.y + m[5]) / w)
}

/**
 * A point of the photo as it is shown (upright) in the pixels of the photo as the camera stored it.
 * [rotation] (0, 90, 180, 270) turns the stored photo clockwise to show it upright; [rawW] x
 * [rawH] is the size of the stored photo.
 */
fun uprightToRaw(p: Pt, rotation: Int, rawW: Float, rawH: Float): Pt = when (((rotation % 360) + 360) % 360) {
    90 -> Pt(p.y, rawH - p.x)
    180 -> Pt(rawW - p.x, rawH - p.y)
    270 -> Pt(rawW - p.y, p.x)
    else -> p
}

/** The size of a stored photo [rawW] x [rawH] when shown upright after [rotation]. */
fun uprightSize(rotation: Int, rawW: Int, rawH: Int): Pair<Int, Int> =
    if ((((rotation % 360) + 360) % 360) % 180 == 90) rawH to rawW else rawW to rawH

/** How a page is drawn from its photo: the size of the page and the transform from the stored photo. */
class PageTransform(val width: Int, val height: Int, val matrix: FloatArray)

/**
 * The transform that cuts the sheet [corners] (parts of the upright photo) out of the stored photo
 * ([rawW] x [rawH], turned upright by [rotation]), straightens it and turns it by [turns] quarter
 * turns clockwise; the longer side of the page at most [maxSide] pixels.
 */
fun pageTransform(corners: Quad, rotation: Int, rawW: Int, rawH: Int, turns: Int, maxSide: Int): PageTransform {
    val (uw, uh) = uprightSize(rotation, rawW, rawH)
    val px = corners.scale(uw.toFloat(), uh.toFloat())
    // The real shape of the sheet, not the shortened sides of the photo.
    val (pw, ph) = trueSize(px, uw / 2f, uh / 2f, max(uw, uh).toFloat(), maxSide)
    val k = ((turns % 4) + 4) % 4
    val (ow, oh) = if (k % 2 == 1) ph to pw else pw to ph
    val rect = listOf(Pt(0f, 0f), Pt(ow.toFloat(), 0f), Pt(ow.toFloat(), oh.toFloat()), Pt(0f, oh.toFloat()))
    // A quarter turn clockwise: the top left of the page goes to the top right of the picture.
    val dst = Quad(rect[k % 4], rect[(1 + k) % 4], rect[(2 + k) % 4], rect[(3 + k) % 4])
    val src = Quad(
        uprightToRaw(px.tl, rotation, rawW.toFloat(), rawH.toFloat()),
        uprightToRaw(px.tr, rotation, rawW.toFloat(), rawH.toFloat()),
        uprightToRaw(px.br, rotation, rawW.toFloat(), rawH.toFloat()),
        uprightToRaw(px.bl, rotation, rawW.toFloat(), rawH.toFloat()),
    )
    return PageTransform(ow, oh, perspective(src, dst))
}

/** The longer side of a finished page at most (pixels). */
const val SCAN_PAGE_MAX_SIDE = 2500

/** "Scan 2026-11-19 14.05.pdf" for the local time "2026-11-19T14:05" (see `localNow`). */
fun scanFileName(local: String): String {
    val date = local.substringBefore('T')
    val time = local.substringAfter('T', "").take(5).replace(':', '.')
    return if (time.isEmpty()) "Scan $date.pdf" else "Scan $date $time.pdf"
}

/** [name] as the name of a PDF: trimmed, ".pdf" added when it does not end with it. */
fun pdfName(name: String, fallback: String): String {
    val n = name.trim().ifEmpty { fallback }
    return if (n.lowercase().endsWith(".pdf")) n else "$n.pdf"
}
