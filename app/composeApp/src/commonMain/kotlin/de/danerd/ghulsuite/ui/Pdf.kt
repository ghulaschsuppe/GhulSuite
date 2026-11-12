// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

/**
 * A plain text as a small PDF (A4, Helvetica, Latin-1). Enough for saving a mail; no external
 * library and the same on every platform.
 */
fun textPdf(title: String, body: String): ByteArray {
    val width = 595
    val height = 842
    val margin = 56
    val size = 10.5
    val leading = 14.0
    val maxChars = 92
    fun wrap(line: String): List<String> {
        if (line.length <= maxChars) return listOf(line)
        val out = mutableListOf<String>()
        var rest = line
        while (rest.length > maxChars) {
            val cut = rest.lastIndexOf(' ', maxChars).takeIf { it > 20 } ?: maxChars
            out += rest.substring(0, cut)
            rest = rest.substring(cut).trimStart()
        }
        out += rest
        return out
    }
    val lines = (listOf(title, "") + body.replace("\r", "").split('\n')).flatMap(::wrap)
    val perPage = ((height - 2 * margin) / leading).toInt()
    val pages = lines.chunked(perPage).ifEmpty { listOf(emptyList()) }

    fun esc(s: String): String = buildString {
        for (c in s) {
            when {
                c == '(' || c == ')' || c == '\\' -> { append('\\'); append(c) }
                c.code in 32..126 -> append(c)
                c.code in 160..255 -> append("\\" + c.code.toString(8).padStart(3, '0'))
                c == '\t' -> append("    ")
                c == '€' -> append("\\200")
                c == '–' || c == '—' -> append('-')
                c == '„' || c == '“' || c == '”' -> append('"')
                c == '‚' || c == '‘' || c == '’' -> append('\'')
                c == '…' -> append("...")
                else -> append('?')
            }
        }
    }

    val objects = mutableListOf<String>()
    // 1 catalog, 2 pages, 3 font, then per page: page + content.
    objects += "<< /Type /Catalog /Pages 2 0 R >>"
    val kids = pages.indices.joinToString(" ") { "${4 + it * 2} 0 R" }
    objects += "<< /Type /Pages /Kids [$kids] /Count ${pages.size} >>"
    objects += "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
    pages.forEachIndexed { i, page ->
        val content = buildString {
            append("BT /F1 $size Tf $leading TL $margin ${height - margin} Td\n")
            page.forEachIndexed { n, l ->
                if (i == 0 && n == 0) append("/F1 14 Tf (${esc(l)}) Tj /F1 $size Tf T*\n") else append("(${esc(l)}) Tj T*\n")
            }
            append("ET")
        }
        objects += "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 $width $height] /Resources << /Font << /F1 3 0 R >> >> /Contents ${5 + i * 2} 0 R >>"
        objects += "<< /Length ${content.encodeToByteArray().size} >>\nstream\n$content\nendstream"
    }
    val out = StringBuilder("%PDF-1.4\n")
    val offsets = mutableListOf<Int>()
    objects.forEachIndexed { i, o ->
        offsets += out.toString().encodeToByteArray().size
        out.append("${i + 1} 0 obj\n$o\nendobj\n")
    }
    val xref = out.toString().encodeToByteArray().size
    out.append("xref\n0 ${objects.size + 1}\n0000000000 65535 f \n")
    for (o in offsets) out.append(o.toString().padStart(10, '0') + " 00000 n \n")
    out.append("trailer\n<< /Size ${objects.size + 1} /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n")
    return out.toString().encodeToByteArray()
}

/** Width and height of a JPEG (from its SOF marker), or null. */
fun jpegSize(b: ByteArray): Pair<Int, Int>? {
    var i = 2
    while (i + 9 < b.size) {
        if (b[i].toInt() and 0xFF != 0xFF) return null
        val marker = b[i + 1].toInt() and 0xFF
        val len = ((b[i + 2].toInt() and 0xFF) shl 8) or (b[i + 3].toInt() and 0xFF)
        if (marker in 0xC0..0xC3) {
            val h = ((b[i + 5].toInt() and 0xFF) shl 8) or (b[i + 6].toInt() and 0xFF)
            val w = ((b[i + 7].toInt() and 0xFF) shl 8) or (b[i + 8].toInt() and 0xFF)
            return w to h
        }
        i += 2 + len
    }
    return null
}

/** A photographed page (JPEG) as a one-page A4 PDF, scaled to fit. */
fun jpegPdf(jpeg: ByteArray): ByteArray? {
    val (w, h) = jpegSize(jpeg) ?: return null
    val pw = 595.0
    val ph = 842.0
    val scale = minOf((pw - 40) / w, (ph - 40) / h)
    val dw = w * scale
    val dh = h * scale
    val x = (pw - dw) / 2
    val y = (ph - dh) / 2
    fun n(v: Double) = ((v * 100).toLong() / 100.0).toString()
    val content = "q ${n(dw)} 0 0 ${n(dh)} ${n(x)} ${n(y)} cm /Im1 Do Q"
    val parts = mutableListOf<ByteArray>()
    val offsets = mutableListOf<Int>()
    var size = 0
    fun add(bytes: ByteArray) { parts += bytes; size += bytes.size }
    fun obj(n: Int, head: String, stream: ByteArray? = null) {
        offsets += size
        if (stream == null) {
            add("$n 0 obj\n$head\nendobj\n".encodeToByteArray())
        } else {
            add("$n 0 obj\n$head\nstream\n".encodeToByteArray()); add(stream); add("\nendstream\nendobj\n".encodeToByteArray())
        }
    }
    add("%PDF-1.4\n".encodeToByteArray())
    obj(1, "<< /Type /Catalog /Pages 2 0 R >>")
    obj(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
    obj(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /XObject << /Im1 4 0 R >> >> /Contents 5 0 R >>")
    obj(4, "<< /Type /XObject /Subtype /Image /Width $w /Height $h /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.size} >>", jpeg)
    val c = content.encodeToByteArray()
    obj(5, "<< /Length ${c.size} >>", c)
    val xref = size
    val tail = StringBuilder("xref\n0 6\n0000000000 65535 f \n")
    for (o in offsets) tail.append(o.toString().padStart(10, '0') + " 00000 n \n")
    tail.append("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n$xref\n%%EOF\n")
    add(tail.toString().encodeToByteArray())
    val out = ByteArray(size)
    var p = 0
    for (part in parts) { part.copyInto(out, p); p += part.size }
    return out
}
