// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.scan

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt

// The "Document" look of a page and the PDF that holds the pages, in plain Kotlin (tested on the
// desktop). Everything stays in memory: the PDF is written into a byte array that is wiped when
// it is no longer needed.

/**
 * The "Document" filter, in place on ARGB pixels: gray, the paper evened out (shadows and a
 * darker corner become white, the brightness of the paper is estimated in blocks) and more
 * contrast, so text stands out black on white.
 */
fun documentFilter(px: IntArray, w: Int, h: Int) {
    require(px.size >= w * h) { "size" }
    val n = w * h
    val lum = ByteArray(n)
    for (i in 0 until n) {
        val c = px[i]
        lum[i] = ((((c shr 16) and 255) * 77 + ((c shr 8) and 255) * 150 + (c and 255) * 29) shr 8).toByte()
    }
    // The paper's brightness: a bright part (the 90th percentile) of each block, so the text in it does not count.
    val b = max(8, max(w, h) / 40)
    val bw = ceil(w.toDouble() / b).toInt()
    val bh = ceil(h.toDouble() / b).toInt()
    val paper = FloatArray(bw * bh)
    val hist = IntArray(64)
    for (by in 0 until bh) for (bx in 0 until bw) {
        hist.fill(0)
        var count = 0
        for (y in by * b until min(h, by * b + b)) for (x in bx * b until min(w, bx * b + b)) {
            hist[(lum[y * w + x].toInt() and 255) shr 2]++
            count++
        }
        var acc = 0
        var bin = 63
        while (bin > 0 && acc + hist[bin] < count / 10) acc += hist[bin--]
        paper[by * bw + bx] = (bin * 4 + 2).toFloat()
    }
    // A block covered by a large dark area (a photo, a heading) takes the paper of its neighbours.
    val grown = FloatArray(bw * bh)
    for (by in 0 until bh) for (bx in 0 until bw) {
        var m = 0f
        for (yy in max(0, by - 1)..min(bh - 1, by + 1)) for (xx in max(0, bx - 1)..min(bw - 1, bx + 1)) m = max(m, paper[yy * bw + xx])
        grown[by * bw + bx] = m
    }
    val smooth = FloatArray(bw * bh)
    for (by in 0 until bh) for (bx in 0 until bw) {
        var s = 0f
        var c = 0
        for (yy in max(0, by - 1)..min(bh - 1, by + 1)) for (xx in max(0, bx - 1)..min(bw - 1, bx + 1)) { s += grown[yy * bw + xx]; c++ }
        smooth[by * bw + bx] = max(24f, s / c)
    }
    // Brightness relative to the paper (0 to 255), then black and white points.
    val rel = IntArray(256)
    for (y in 0 until h) {
        val fy = ((y + 0.5f) / b - 0.5f).coerceIn(0f, (bh - 1).toFloat())
        val y0 = fy.toInt()
        val y1 = min(y0 + 1, bh - 1)
        val ty = fy - y0
        for (x in 0 until w) {
            val fx = ((x + 0.5f) / b - 0.5f).coerceIn(0f, (bw - 1).toFloat())
            val x0 = fx.toInt()
            val x1 = min(x0 + 1, bw - 1)
            val tx = fx - x0
            val top = smooth[y0 * bw + x0] * (1 - tx) + smooth[y0 * bw + x1] * tx
            val bottom = smooth[y1 * bw + x0] * (1 - tx) + smooth[y1 * bw + x1] * tx
            val bg = top * (1 - ty) + bottom * ty
            val v = ((lum[y * w + x].toInt() and 255) * 255f / bg).roundToInt().coerceIn(0, 255)
            lum[y * w + x] = v.toByte()
            rel[v]++
        }
    }
    var acc = 0
    var lo = 0
    while (lo < 255 && acc + rel[lo] < n / 100) acc += rel[lo++]
    lo = lo.coerceAtMost(140)
    val hi = 232
    val curve = IntArray(256) { v ->
        val t = ((v - lo).toFloat() / (hi - lo)).coerceIn(0f, 1f)
        (255 * t.pow(1.25f)).roundToInt()
    }
    for (i in 0 until n) {
        val g = curve[lum[i].toInt() and 255]
        px[i] = (0xFF shl 24) or (g shl 16) or (g shl 8) or g
    }
    lum.fill(0)
}

/** Size and colour channels of a JPEG (from its frame header); null if it is no JPEG. */
class JpegInfo(val width: Int, val height: Int, val components: Int)

fun jpegInfo(b: ByteArray): JpegInfo? {
    if (b.size < 4 || (b[0].toInt() and 255) != 0xFF || (b[1].toInt() and 255) != 0xD8) return null
    var i = 2
    fun u(k: Int) = b[k].toInt() and 255
    while (i + 3 < b.size) {
        if (u(i) != 0xFF) return null
        var m = u(i + 1)
        while (m == 0xFF && i + 2 < b.size) { i++; m = u(i + 1) }
        i += 2
        if (m == 0xD8 || m == 0x01 || m in 0xD0..0xD7) continue
        if (i + 1 >= b.size) return null
        val len = (u(i) shl 8) or u(i + 1)
        if (len < 2) return null
        val sof = m in 0xC0..0xCF && m != 0xC4 && m != 0xC8 && m != 0xCC
        if (sof) {
            if (i + 7 >= b.size) return null
            val height = (u(i + 3) shl 8) or u(i + 4)
            val width = (u(i + 5) shl 8) or u(i + 6)
            val comps = u(i + 7)
            return if (width > 0 && height > 0 && comps in listOf(1, 3, 4)) JpegInfo(width, height, comps) else null
        }
        if (m == 0xDA) return null
        i += len
    }
    return null
}

/** Points of A4 (1/72 inch). */
const val A4_SHORT = 595.28f
const val A4_LONG = 841.89f

/** Page size in points for a picture [w] x [h]: as large as fits on an A4 sheet turned like the picture. */
fun pdfPageSize(w: Int, h: Int): Pair<Float, Float> {
    val (aw, ah) = if (w > h) A4_LONG to A4_SHORT else A4_SHORT to A4_LONG
    val s = min(aw / w, ah / h)
    return w * s to h * s
}

/** A number of points with two decimals (the PDF needs no more). */
private fun pt(v: Float): String {
    val c = (v * 100).roundToInt()
    return "${c / 100}.${(c % 100).toString().padStart(2, '0')}"
}

/** A growing buffer that overwrites what it leaves behind (a larger copy replaces it). */
private class WipingBuffer(initial: Int) {
    var buf = ByteArray(max(64, initial))
    var size = 0

    fun write(data: ByteArray) {
        if (size + data.size > buf.size) {
            val grown = ByteArray(max(buf.size * 2, size + data.size))
            buf.copyInto(grown, 0, 0, size)
            buf.fill(0)
            buf = grown
        }
        data.copyInto(buf, size)
        size += data.size
    }

    fun text(s: String) = write(s.encodeToByteArray())
}

/**
 * One PDF with a page for each JPEG of [pages] (in this order), each page sized by [pdfPageSize]
 * and filled by its picture; the JPEGs are embedded as they are (DCTDecode), not encoded again.
 */
fun buildPdf(pages: List<ByteArray>): ByteArray {
    require(pages.isNotEmpty()) { "no pages" }
    val infos = pages.map { requireNotNull(jpegInfo(it)) { "not a JPEG" } }
    val out = WipingBuffer(pages.sumOf { it.size } + 1024 + 600 * pages.size)
    val offsets = mutableListOf<Int>()
    fun obj(num: Int, body: () -> Unit) {
        while (offsets.size < num) offsets += 0
        offsets[num - 1] = out.size
        out.text("$num 0 obj\n")
        body()
        out.text("\nendobj\n")
    }
    out.text("%PDF-1.4\n")
    // A comment with bytes above 127: the file counts as binary for programs that copy it.
    out.write(byteArrayOf(0x25, 0xE2.toByte(), 0xE3.toByte(), 0xCF.toByte(), 0xD3.toByte(), 0x0A))
    val kids = pages.indices.joinToString(" ") { "${3 + 3 * it} 0 R" }
    obj(1) { out.text("<< /Type /Catalog /Pages 2 0 R >>") }
    obj(2) { out.text("<< /Type /Pages /Kids [$kids] /Count ${pages.size} >>") }
    for ((i, jpeg) in pages.withIndex()) {
        val info = infos[i]
        val (pw, ph) = pdfPageSize(info.width, info.height)
        val page = 3 + 3 * i
        val content = "q\n${pt(pw)} 0 0 ${pt(ph)} 0 0 cm\n/Im0 Do\nQ\n".encodeToByteArray()
        obj(page) {
            out.text(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${pt(pw)} ${pt(ph)}] " +
                    "/Resources << /XObject << /Im0 ${page + 2} 0 R >> >> /Contents ${page + 1} 0 R >>",
            )
        }
        obj(page + 1) {
            out.text("<< /Length ${content.size} >>\nstream\n")
            out.write(content)
            out.text("endstream")
        }
        obj(page + 2) {
            val space = when (info.components) {
                1 -> "/DeviceGray"
                4 -> "/DeviceCMYK"
                else -> "/DeviceRGB"
            }
            out.text(
                "<< /Type /XObject /Subtype /Image /Width ${info.width} /Height ${info.height} /ColorSpace $space " +
                    "/BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.size} >>\nstream\n",
            )
            out.write(jpeg)
            out.text("\nendstream")
        }
    }
    val xref = out.size
    val count = offsets.size + 1
    out.text("xref\n0 $count\n0000000000 65535 f \n")
    for (o in offsets) out.text(o.toString().padStart(10, '0') + " 00000 n \n")
    out.text("trailer\n<< /Size $count /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n")
    val result = out.buf.copyOf(out.size)
    out.buf.fill(0)
    return result
}
