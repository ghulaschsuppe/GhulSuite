// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.scan

import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt

// Finds the corners of a sheet of paper in a photo, in plain Kotlin on a small gray picture (no
// library, tested on the desktop with drawn pictures):
//
// 1. The photo, about 360 pixels on its longer side, is blurred (Gauss) and its brightness
//    gradients are taken (Sobel).
// 2. Candidates from regions: the picture is split into light and dark at a few thresholds (Otsu
//    and Otsu within each half); the largest connected areas give a convex hull, and the four hull
//    points with the largest area between them are a first guess of the corners.
// 3. Each side of a guess is moved onto the strongest edge near it (edge points along the side,
//    a straight line fitted through them); the corners are where the fitted lines meet.
// 4. A guess counts only if its sides lie on real edges (most points along each side have a strong
//    gradient across it, and the sides are clearly lighter on one side than on the other); sides on
//    the edge of the picture are allowed (a sheet larger than the photo). The best one wins.
//
// Nothing good: the caller gets an inset frame and [SheetGuess.found] is false.

/** A gray picture: [pixels] row by row, 0 (black) to 255 (white). */
class GrayImage(val width: Int, val height: Int, val pixels: IntArray) {
    init {
        require(width > 0 && height > 0 && pixels.size == width * height) { "size" }
    }
}

/** What the detection suggests: corners in parts of the picture (0 to 1); [found] false means the inset frame. */
class SheetGuess(val corners: Quad, val found: Boolean)

/** The longer side of the picture the detection works on. */
const val DETECT_SIDE = 360

/** Brightness of ARGB pixels, made smaller (averaged blocks) to about [maxSide] on the longer side. */
fun grayFromArgb(argb: IntArray, w: Int, h: Int, maxSide: Int = DETECT_SIDE): GrayImage {
    require(argb.size >= w * h) { "size" }
    val f = max(1, ceil(max(w, h).toDouble() / maxSide).toInt())
    val ow = max(1, w / f)
    val oh = max(1, h / f)
    val out = IntArray(ow * oh)
    for (y in 0 until oh) for (x in 0 until ow) {
        var sum = 0
        var n = 0
        for (yy in y * f until min(h, y * f + f)) for (xx in x * f until min(w, x * f + f)) {
            val c = argb[yy * w + xx]
            sum += (((c shr 16) and 255) * 77 + ((c shr 8) and 255) * 150 + (c and 255) * 29) shr 8
            n++
        }
        out[y * ow + x] = sum / max(1, n)
    }
    return GrayImage(ow, oh, out)
}

private fun shrink(img: GrayImage, maxSide: Int): GrayImage {
    val f = max(1, ceil(max(img.width, img.height).toDouble() / maxSide).toInt())
    if (f == 1) return img
    val ow = max(1, img.width / f)
    val oh = max(1, img.height / f)
    val out = IntArray(ow * oh)
    for (y in 0 until oh) for (x in 0 until ow) {
        var sum = 0
        for (yy in y * f until y * f + f) for (xx in x * f until x * f + f) sum += img.pixels[yy * img.width + xx]
        out[y * ow + x] = sum / (f * f)
    }
    return GrayImage(ow, oh, out)
}

/** Suggests the corners of the sheet in [image] (any size; made smaller first). */
fun detectSheet(image: GrayImage): SheetGuess {
    val img = shrink(image, DETECT_SIDE)
    val found = runCatching { Detector(img).run() }.getOrNull()
    return if (found != null) SheetGuess(found, true) else SheetGuess(Quad.inset(), false)
}

private class Line(val p: Pt, val d: Pt)

private class Detector(img: GrayImage) {
    val w = img.width
    val h = img.height
    val raw = img.pixels
    val blur = gaussian(raw.map { it.toFloat() }.toFloatArray(), w, h, 1.4f)
    val gx = FloatArray(w * h)
    val gy = FloatArray(w * h)

    /** Noise of the picture (standard deviation of a pixel) and what is left of it after blurring. */
    val noise: Float
    val noiseBlur: Float

    init {
        for (y in 1 until h - 1) for (x in 1 until w - 1) {
            val i = y * w + x
            gx[i] = (blur[i - w + 1] + 2 * blur[i + 1] + blur[i + w + 1]) - (blur[i - w - 1] + 2 * blur[i - 1] + blur[i + w - 1])
            gy[i] = (blur[i + w - 1] + 2 * blur[i + w] + blur[i + w + 1]) - (blur[i - w - 1] + 2 * blur[i - w] + blur[i - w + 1])
        }
        // Median of the differences of neighbours: the noise, little disturbed by edges.
        val hist = IntArray(256)
        var n = 0
        for (y in 0 until h) for (x in 0 until w - 1) {
            hist[abs(raw[y * w + x + 1] - raw[y * w + x]).coerceAtMost(255)]++
            n++
        }
        var acc = 0
        var med = 0
        while (med < 255 && acc + hist[med] < n / 2) acc += hist[med++]
        noise = (med / 0.6745f / sqrt(2f)).coerceAtLeast(0.5f)
        noiseBlur = noise * 0.22f
    }

    fun run(): Quad? {
        val area = (w * h).toFloat()
        val guesses = mutableListOf<Pair<Quad, Float>>()
        fun consider(q0: Quad, polarity: Float) {
            val refined = refine(q0, polarity) ?: q0
            for (q in listOf(refined, q0).distinct()) {
                val s = score(q, polarity) ?: continue
                guesses += q to s
            }
        }
        for (t in thresholds()) for (bright in listOf(true, false)) {
            for (hull in regions(t, bright)) consider(largestQuad(hull) ?: continue, if (bright) 1f else -1f)
        }
        // Edges alone (light changing across the picture, a background as light as the paper).
        // Many frames: the best few as they are, only those refined.
        val fromEdges = mutableListOf<Triple<Quad, Float, Float>>()
        for (q in fromLines()) for (polarity in listOf(1f, -1f)) score(q, polarity)?.let { fromEdges += Triple(q, polarity, it) }
        for ((q, polarity, _) in fromEdges.sortedByDescending { it.third }.take(6)) consider(q, polarity)
        val best = guesses.maxByOrNull { it.second } ?: return null
        val q = best.first
        if (q.area() < 0.08f * area) return null
        return snap(q).scale(1f / w, 1f / h)
    }

    /**
     * Candidates from straight edges: thin edges (Canny-like: strong gradient, thinned across the
     * edge), a Hough transform voting only near each edge's own direction, the strongest lines
     * (and the borders of the picture) combined into frames of two nearly parallel pairs.
     */
    fun fromLines(): List<Quad> {
        val sobelNoise = 3.5f * noiseBlur
        val strong = max(12f, 3f * sobelNoise)
        val mag = FloatArray(w * h) { sqrt(gx[it] * gx[it] + gy[it] * gy[it]) }
        val diag = sqrt((w * w + h * h).toFloat()).toInt() + 1
        val thetas = 180
        val acc = FloatArray(thetas * (2 * diag + 1))
        val cosT = FloatArray(thetas) { kotlin.math.cos(it * PI_F / thetas) }
        val sinT = FloatArray(thetas) { kotlin.math.sin(it * PI_F / thetas) }
        for (y in 2 until h - 2) for (x in 2 until w - 2) {
            val i = y * w + x
            val m = mag[i]
            if (m < strong) continue
            // Thin: only the strongest point across the edge votes.
            val ax = abs(gx[i])
            val ay = abs(gy[i])
            val (a, b) = when {
                ax > 2.414f * ay -> mag[i - 1] to mag[i + 1]
                ay > 2.414f * ax -> mag[i - w] to mag[i + w]
                (gx[i] > 0) == (gy[i] > 0) -> mag[i - w - 1] to mag[i + w + 1]
                else -> mag[i - w + 1] to mag[i + w - 1]
            }
            if (m < a || m < b) continue
            var dir = (kotlin.math.atan2(gy[i], gx[i]) * thetas / PI_F).roundToInt()
            dir = ((dir % thetas) + thetas) % thetas
            for (dt in -6..6) {
                val th = ((dir + dt) % thetas + thetas) % thetas
                val r = (x * cosT[th] + y * sinT[th]).roundToInt() + diag
                acc[th * (2 * diag + 1) + r] += 1f
            }
        }
        // The strongest lines: local maxima, at least a sixth of the shorter side long.
        val rows = 2 * diag + 1
        val peaks = mutableListOf<Triple<Int, Int, Float>>()
        val minVotes = 0.16f * min(w, h)
        for (th in 0 until thetas) for (r in 0 until rows) {
            val v = acc[th * rows + r]
            if (v < minVotes) continue
            var top = true
            loop@ for (dt in -3..3) for (dr in -4..4) {
                if (dt == 0 && dr == 0) continue
                val t2 = th + dt
                val r2 = r + dr
                if (r2 < 0 || r2 >= rows) continue
                // Across 0 degrees the line is the same with the opposite distance.
                val o = if (t2 < 0) acc[(t2 + thetas) * rows + (rows - 1 - r2)] else if (t2 >= thetas) acc[(t2 - thetas) * rows + (rows - 1 - r2)] else acc[t2 * rows + r2]
                if (o > v || (o == v && (dt < 0 || (dt == 0 && dr < 0)))) { top = false; break@loop }
            }
            if (top) peaks += Triple(th, r - diag, v)
        }
        val lines = peaks.sortedByDescending { it.third }.take(16).map { (th, r, _) ->
            val n = Pt(cosT[th], sinT[th])
            Line(Pt(n.x * r, n.y * r), Pt(-n.y, n.x)) to th
        }.toMutableList()
        // The borders: a sheet larger than the photo ends there.
        lines += Line(Pt(0f, 0f), Pt(1f, 0f)) to 90
        lines += Line(Pt(0f, h.toFloat()), Pt(1f, 0f)) to 90
        lines += Line(Pt(0f, 0f), Pt(0f, 1f)) to 0
        lines += Line(Pt(w.toFloat(), 0f), Pt(0f, 1f)) to 0
        fun angle(a: Int, b: Int): Int {
            val d = abs(a - b) % 180
            return min(d, 180 - d)
        }
        val pairs = mutableListOf<Pair<Int, Int>>()
        for (i in lines.indices) for (j in i + 1 until lines.size) {
            if (angle(lines[i].second, lines[j].second) <= 35) pairs += i to j
        }
        val out = mutableListOf<Quad>()
        val m = 0.03f * max(w, h)
        val minSide = 0.1f * min(w, h)
        for (a in pairs.indices) for (b in a + 1 until pairs.size) {
            val (i1, j1) = pairs[a]
            val (i2, j2) = pairs[b]
            if (setOf(i1, j1, i2, j2).size < 4) continue
            if (angle(lines[i1].second, lines[i2].second) < 40) continue
            val c = listOf(
                intersect(lines[i1].first, lines[i2].first), intersect(lines[i1].first, lines[j2].first),
                intersect(lines[j1].first, lines[j2].first), intersect(lines[j1].first, lines[i2].first),
            )
            if (c.any { it == null || it.x < -m || it.y < -m || it.x > w + m || it.y > h + m }) continue
            val q = runCatching { orderCorners(c.map { it!! }) }.getOrNull() ?: continue
            val cq = clamp(q)
            if (cq.area() < 0.08f * w * h || !isValidQuad(cq, minSide)) continue
            out += cq
        }
        return out
    }

    /** Otsu over the whole picture and within each of its two halves. */
    fun thresholds(): List<Int> {
        val hist = IntArray(256)
        for (v in blur) hist[v.roundToInt().coerceIn(0, 255)]++
        val t = otsu(hist, 0, 255) ?: return emptyList()
        return listOfNotNull(t, otsu(hist, t + 1, 255), otsu(hist, 0, t)).distinct()
    }

    /** The convex hulls of the largest areas lighter (or darker) than [t]. */
    fun regions(t: Int, bright: Boolean): List<List<Pt>> {
        val mask = BooleanArray(w * h) { if (bright) blur[it] > t else blur[it] <= t }
        // Eroded once: thin bridges (noise, a shadow line) do not join the sheet to its background.
        val core = BooleanArray(w * h)
        for (y in 1 until h - 1) for (x in 1 until w - 1) {
            val i = y * w + x
            core[i] = mask[i] && mask[i - 1] && mask[i + 1] && mask[i - w] && mask[i + w]
        }
        // Pixels on the edge of the picture keep their value: a sheet larger than the photo touches it.
        for (x in 0 until w) { core[x] = mask[x] && mask[x + w]; core[(h - 1) * w + x] = mask[(h - 1) * w + x] && mask[(h - 2) * w + x] }
        for (y in 0 until h) { core[y * w] = mask[y * w] && mask[y * w + 1]; core[y * w + w - 1] = mask[y * w + w - 1] && mask[y * w + w - 2] }
        val label = IntArray(w * h)
        val sizes = mutableListOf(0)
        val stack = IntArray(w * h)
        for (start in 0 until w * h) {
            if (!core[start] || label[start] != 0) continue
            val id = sizes.size
            var size = 0
            var top = 0
            stack[top++] = start
            label[start] = id
            while (top > 0) {
                val i = stack[--top]
                size++
                val x = i % w
                if (x > 0 && core[i - 1] && label[i - 1] == 0) { label[i - 1] = id; stack[top++] = i - 1 }
                if (x < w - 1 && core[i + 1] && label[i + 1] == 0) { label[i + 1] = id; stack[top++] = i + 1 }
                if (i >= w && core[i - w] && label[i - w] == 0) { label[i - w] = id; stack[top++] = i - w }
                if (i < w * (h - 1) && core[i + w] && label[i + w] == 0) { label[i + w] = id; stack[top++] = i + w }
            }
            sizes += size
        }
        val big = sizes.indices.drop(1).filter { sizes[it] >= 0.05f * w * h }.sortedByDescending { sizes[it] }.take(2)
        return big.mapNotNull { id ->
            val pts = mutableListOf<Pt>()
            for (y in 0 until h) {
                var lo = -1
                var hi = -1
                for (x in 0 until w) if (label[y * w + x] == id) { if (lo < 0) lo = x; hi = x }
                if (lo >= 0) {
                    pts += Pt(lo.toFloat(), y.toFloat())
                    pts += Pt(hi + 1f, y.toFloat())
                    pts += Pt(lo.toFloat(), y + 1f)
                    pts += Pt(hi + 1f, y + 1f)
                }
            }
            val hull = convexHull(pts)
            // Mostly filled: a noisy area or a frayed shadow is no sheet.
            if (hull.size < 3 || sizes[id] < 0.6f * polygonArea(hull)) null else simplify(hull)
        }
    }

    /** Each side moved onto the strongest edge near it; null if the result is no sheet. */
    fun refine(q: Quad, polarity: Float): Quad? {
        val reach = max(6f, 0.035f * sqrt((w * w + h * h).toFloat()))
        val thr = threshold(contrast(q, polarity))
        val lines = (0 until 4).map { i ->
            val a = q[i]
            val b = q[(i + 1) % 4]
            if (onBorder(a, b)) return@map Line(a, b - a)
            val d = unit(b - a)
            val n = Pt(-d.y, d.x)
            val pts = mutableListOf<Pt>()
            val k = 36
            for (s in 0 until k) {
                val p = a + (b - a) * (0.08f + 0.84f * s / (k - 1))
                var best = 0f
                var bestT = 0f
                var t = -reach
                while (t <= reach) {
                    val v = across(p + n * t, n) * polarity
                    if (v > best) { best = v; bestT = t }
                    t += 0.5f
                }
                if (best > thr) pts += p + n * bestT
            }
            fitLine(pts, k) ?: Line(a, b - a)
        }
        val c = (0 until 4).map { i -> intersect(lines[(i + 3) % 4], lines[i]) ?: return null }
        val r = Quad(c[0], c[1], c[2], c[3])
        val minSide = 0.1f * min(w, h)
        if (!isValidQuad(r, minSide)) return null
        if (r.area() < 0.6f * q.area() || r.area() > 1.4f * q.area()) return null
        val m = 0.03f * max(w, h)
        if (r.points.any { it.x < -m || it.y < -m || it.x > w + m || it.y > h + m }) return null
        return clamp(r).takeIf { isValidQuad(it, minSide) }
    }

    /** How strong an edge must be: a part of the step [step] between sheet and background, well above the noise. */
    fun threshold(step: Float): Float =
        // A blurred step of height s gives a Sobel response of about 2.3 s; the noise of the
        // Sobel response is about 3.5 times that of a blurred pixel.
        max(max(4f, 0.6f * abs(step)), 10.5f * noiseBlur)

    /** How much lighter (polarity 1) the inside is than the outside, along the sides that are not on the border. */
    fun contrast(q: Quad, polarity: Float): Float {
        var sum = 0f
        var n = 0
        for (i in 0 until 4) {
            val a = q[i]
            val b = q[(i + 1) % 4]
            if (onBorder(a, b)) continue
            val d = unit(b - a)
            val nn = Pt(-d.y, d.x)
            for (s in 1..15) {
                val p = a + (b - a) * (s / 16f)
                val inside = p + nn * 4f
                val outside = p - nn * 4f
                if (!inImage(inside) || !inImage(outside)) continue
                sum += (sample(blur, inside) - sample(blur, outside)) * polarity
                n++
            }
        }
        return if (n == 0) 0f else sum / n
    }

    /**
     * How well [q] fits a sheet (higher is better), null if it does not: its sides on real edges,
     * a clear step between inside and outside, a sensible size and shape.
     */
    fun score(q: Quad, polarity: Float): Float? {
        val area = q.area() / (w * h)
        if (area < 0.08f || area > 0.985f) return null
        if (!isValidQuad(q, 0.1f * min(w, h))) return null
        val supports = mutableListOf<Float>()
        val minContrast = max(8f, 5f * noiseBlur)
        val far = max(8f, 0.03f * max(w, h))
        var sumIn = 0f
        var sumOut = 0f
        var count = 0
        val borders = mutableListOf<Int>()
        for (i in 0 until 4) {
            val a = q[i]
            val b = q[(i + 1) % 4]
            if (onBorder(a, b)) { borders += i; continue }
            val d = unit(b - a)
            val n = Pt(-d.y, d.x)
            // The step across this side alone, next to it and a little further away: an edge of
            // the sheet has paper on one side and background on the other, a line of text has
            // paper on both sides a few pixels away.
            var step = 0f
            var stepFar = 0f
            var steps = 0
            for (s in 1..15) {
                val p = a + (b - a) * (s / 16f)
                if (!inImage(p + n * far) || !inImage(p - n * far)) continue
                val inside = sample(blur, p + n * 4f)
                val outside = sample(blur, p - n * 4f)
                step += (inside - outside) * polarity
                stepFar += (sample(blur, p + n * far) - sample(blur, p - n * far)) * polarity
                sumIn += inside
                sumOut += outside
                count++
                steps++
            }
            if (steps == 0 || step / steps < minContrast) return null
            if (stepFar / steps < 0.5f * minContrast || stepFar < 0.4f * step) return null
            val t = threshold(step / steps)
            var hit = 0
            val k = 40
            for (s in 0 until k) {
                val p = a + (b - a) * (0.05f + 0.9f * s / (k - 1))
                var best = 0f
                var off = -2f
                while (off <= 2f) {
                    best = max(best, across(p + n * off, n) * polarity)
                    off += 0.5f
                }
                if (best >= t) hit++
            }
            supports += hit.toFloat() / k
        }
        if (borders.size > 2 || supports.isEmpty() || count == 0) return null
        if (supports.any { it < 0.45f } || supports.average() < 0.6f) return null
        // A side on the edge of the picture: the sheet goes on beyond it, so just inside it there
        // is paper (as inside the other sides), not background.
        val mid = (sumIn + sumOut) / (2 * count)
        for (i in borders) {
            val a = q[i]
            val b = q[(i + 1) % 4]
            val n = unit(b - a).let { Pt(-it.y, it.x) }
            var v = 0f
            var c = 0
            for (s in 1..15) {
                val p = a + (b - a) * (s / 16f) + n * 4f
                if (!inImage(p)) continue
                v += sample(blur, p)
                c++
            }
            if (c == 0 || (v / c - mid) * polarity <= 0f) return null
        }
        return supports.average().toFloat() + 0.3f * area
    }

    fun onBorder(a: Pt, b: Pt): Boolean {
        val e = 3f
        return (a.x <= e && b.x <= e) || (a.y <= e && b.y <= e) || (a.x >= w - e && b.x >= w - e) || (a.y >= h - e && b.y >= h - e)
    }

    fun inImage(p: Pt) = p.x >= 0 && p.y >= 0 && p.x <= w - 1 && p.y <= h - 1

    /** The gradient across a side at [p] (along the normal [n]). */
    fun across(p: Pt, n: Pt): Float {
        if (p.x < 1 || p.y < 1 || p.x > w - 2 || p.y > h - 2) return 0f
        return sample(gx, p) * n.x + sample(gy, p) * n.y
    }

    fun sample(a: FloatArray, p: Pt): Float {
        val x = p.x.coerceIn(0f, w - 1.001f)
        val y = p.y.coerceIn(0f, h - 1.001f)
        val x0 = floor(x).toInt()
        val y0 = floor(y).toInt()
        val fx = x - x0
        val fy = y - y0
        val x1 = min(x0 + 1, w - 1)
        val y1 = min(y0 + 1, h - 1)
        val top = a[y0 * w + x0] * (1 - fx) + a[y0 * w + x1] * fx
        val bottom = a[y1 * w + x0] * (1 - fx) + a[y1 * w + x1] * fx
        return top * (1 - fy) + bottom * fy
    }

    /** Corners close to the edge of the picture on it, all inside it. */
    fun snap(q: Quad): Quad {
        fun s(v: Float, size: Int) = when {
            v < 1.5f -> 0f
            v > size - 1.5f -> size.toFloat()
            else -> v
        }
        return Quad(
            Pt(s(q.tl.x, w), s(q.tl.y, h)), Pt(s(q.tr.x, w), s(q.tr.y, h)),
            Pt(s(q.br.x, w), s(q.br.y, h)), Pt(s(q.bl.x, w), s(q.bl.y, h)),
        )
    }

    fun clamp(q: Quad) = Quad(
        Pt(q.tl.x.coerceIn(0f, w.toFloat()), q.tl.y.coerceIn(0f, h.toFloat())),
        Pt(q.tr.x.coerceIn(0f, w.toFloat()), q.tr.y.coerceIn(0f, h.toFloat())),
        Pt(q.br.x.coerceIn(0f, w.toFloat()), q.br.y.coerceIn(0f, h.toFloat())),
        Pt(q.bl.x.coerceIn(0f, w.toFloat()), q.bl.y.coerceIn(0f, h.toFloat())),
    )
}

private const val PI_F = 3.1415927f

private fun unit(v: Pt): Pt {
    val l = sqrt(v.x * v.x + v.y * v.y).coerceAtLeast(1e-6f)
    return Pt(v.x / l, v.y / l)
}

/** A straight line through edge points (total least squares, outliers dropped once); null with too few points. */
private fun fitLine(pts: List<Pt>, samples: Int): Line? {
    fun fit(p: List<Pt>): Line? {
        if (p.size < max(6, samples / 3)) return null
        val cx = p.sumOf { it.x.toDouble() } / p.size
        val cy = p.sumOf { it.y.toDouble() } / p.size
        var sxx = 0.0
        var sxy = 0.0
        var syy = 0.0
        for (q in p) {
            val dx = q.x - cx
            val dy = q.y - cy
            sxx += dx * dx; sxy += dx * dy; syy += dy * dy
        }
        // Direction of the largest spread (eigenvector of the 2 x 2 covariance).
        val angle = 0.5 * kotlin.math.atan2(2 * sxy, sxx - syy)
        return Line(Pt(cx.toFloat(), cy.toFloat()), Pt(kotlin.math.cos(angle).toFloat(), kotlin.math.sin(angle).toFloat()))
    }
    val first = fit(pts) ?: return null
    val n = Pt(-first.d.y, first.d.x)
    val dist = pts.map { abs((it.x - first.p.x) * n.x + (it.y - first.p.y) * n.y) }
    val med = dist.sorted()[dist.size / 2]
    val keep = pts.filterIndexed { i, _ -> dist[i] <= max(1.5f, 2.5f * med) }
    return fit(keep) ?: first
}

private fun intersect(a: Line, b: Line): Pt? {
    val den = a.d.x * b.d.y - a.d.y * b.d.x
    if (abs(den) < 1e-4f * (sqrt(a.d.x * a.d.x + a.d.y * a.d.y) * sqrt(b.d.x * b.d.x + b.d.y * b.d.y))) return null
    val t = ((b.p.x - a.p.x) * b.d.y - (b.p.y - a.p.y) * b.d.x) / den
    return a.p + a.d * t
}

/** Otsu's threshold between [lo] and [hi] of a histogram; null if that part is (nearly) empty or flat. */
private fun otsu(hist: IntArray, lo: Int, hi: Int): Int? {
    if (hi - lo < 2) return null
    var total = 0L
    var sum = 0.0
    for (i in lo..hi) { total += hist[i]; sum += i.toDouble() * hist[i] }
    if (total < 100) return null
    var wB = 0L
    var sumB = 0.0
    var best = -1.0
    var bestT = -1
    for (t in lo until hi) {
        wB += hist[t]
        if (wB == 0L) continue
        val wF = total - wB
        if (wF == 0L) break
        sumB += t.toDouble() * hist[t]
        val mB = sumB / wB
        val mF = (sum - sumB) / wF
        val between = wB.toDouble() * wF * (mB - mF) * (mB - mF)
        if (between > best) { best = between; bestT = t }
    }
    return bestT.takeIf { it >= 0 }
}

private fun gaussian(src: FloatArray, w: Int, h: Int, sigma: Float): FloatArray {
    val r = ceil(3 * sigma).toInt()
    val k = FloatArray(2 * r + 1) { exp(-((it - r) * (it - r)) / (2 * sigma * sigma)) }
    val ks = k.sum()
    for (i in k.indices) k[i] /= ks
    val tmp = FloatArray(w * h)
    val out = FloatArray(w * h)
    for (y in 0 until h) for (x in 0 until w) {
        var s = 0f
        for (j in -r..r) s += src[y * w + (x + j).coerceIn(0, w - 1)] * k[j + r]
        tmp[y * w + x] = s
    }
    for (y in 0 until h) for (x in 0 until w) {
        var s = 0f
        for (j in -r..r) s += tmp[(y + j).coerceIn(0, h - 1) * w + x] * k[j + r]
        out[y * w + x] = s
    }
    return out
}

/** Convex hull (Andrew's monotone chain), clockwise on the screen (y down). */
internal fun convexHull(points: List<Pt>): List<Pt> {
    val p = points.distinct().sortedWith(compareBy({ it.x }, { it.y }))
    if (p.size < 3) return p
    fun cross(o: Pt, a: Pt, b: Pt) = (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    val lower = mutableListOf<Pt>()
    for (q in p) {
        while (lower.size >= 2 && cross(lower[lower.size - 2], lower[lower.size - 1], q) <= 0) lower.removeAt(lower.size - 1)
        lower += q
    }
    val upper = mutableListOf<Pt>()
    for (q in p.asReversed()) {
        while (upper.size >= 2 && cross(upper[upper.size - 2], upper[upper.size - 1], q) <= 0) upper.removeAt(upper.size - 1)
        upper += q
    }
    // Counter-clockwise in y-up terms is clockwise on the screen.
    return lower.dropLast(1) + upper.dropLast(1)
}

internal fun polygonArea(p: List<Pt>): Float {
    var s = 0f
    for (i in p.indices) {
        val a = p[i]
        val b = p[(i + 1) % p.size]
        s += a.x * b.y - b.x * a.y
    }
    return abs(s) / 2
}

/** A hull with its nearly straight points left out, at most about 24 points. */
private fun simplify(hull: List<Pt>): List<Pt> {
    var p = hull
    var tol = 0.75f
    while (true) {
        var changed = true
        while (changed && p.size > 4) {
            changed = false
            for (i in p.indices) {
                val a = p[(i + p.size - 1) % p.size]
                val b = p[i]
                val c = p[(i + 1) % p.size]
                val len = a.dist(c).coerceAtLeast(1e-6f)
                val d = abs((c.x - a.x) * (a.y - b.y) - (a.x - b.x) * (c.y - a.y)) / len
                if (d < tol) {
                    p = p.filterIndexed { j, _ -> j != i }
                    changed = true
                    break
                }
            }
        }
        if (p.size <= 24) return p
        tol *= 2
    }
}

/** The four points of a convex polygon with the largest area between them, as a [Quad]. */
private fun largestQuad(hull: List<Pt>): Quad? {
    val n = hull.size
    if (n < 4) return null
    var best = -1f
    var pick = IntArray(4)
    for (a in 0 until n) for (b in a + 1 until n) for (c in b + 1 until n) for (d in c + 1 until n) {
        val ar = polygonArea(listOf(hull[a], hull[b], hull[c], hull[d]))
        if (ar > best) { best = ar; pick = intArrayOf(a, b, c, d) }
    }
    return orderCorners(pick.map { hull[it] })
}
