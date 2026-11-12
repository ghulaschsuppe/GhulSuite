// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import de.danerd.ghulsuite.model.EmergencySheetText
import java.awt.BasicStroke
import java.awt.Color
import java.awt.Font
import java.awt.Graphics
import java.awt.Graphics2D
import java.awt.RenderingHints
import java.awt.font.LineBreakMeasurer
import java.awt.font.TextAttribute
import java.awt.print.PageFormat
import java.awt.print.Printable
import java.awt.print.PrinterJob
import java.text.AttributedString

/**
 * The emergency sheet on Windows: drawn straight into the system print dialog (which also offers
 * "Microsoft Print to PDF"). Like on Android, no file with the code is written by the app.
 */
object DesktopEmergencySheet {
    fun print(code: String, t: EmergencySheetText) {
        val job = PrinterJob.getPrinterJob()
        job.jobName = t.title
        job.setPrintable(Sheet(code, t))
        if (job.printDialog()) runCatching { job.print() }
    }

    private class Sheet(val code: String, val t: EmergencySheetText) : Printable {
        override fun print(g: Graphics, pf: PageFormat, page: Int): Int {
            if (page > 0) return Printable.NO_SUCH_PAGE
            val g2 = g as Graphics2D
            g2.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON)
            g2.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON)
            g2.color = Color.BLACK
            val x0 = pf.imageableX.toFloat() + 16f
            val width = pf.imageableWidth.toFloat() - 32f
            var y = pf.imageableY.toFloat() + 24f

            fun text(s: String, font: Font, w: Float = width, gap: Float = 8f, color: Color = Color.BLACK) {
                g2.color = color
                for (paragraph in s.split('\n')) {
                    if (paragraph.isBlank()) { y += font.size; continue }
                    val attr = AttributedString(paragraph).apply { addAttribute(TextAttribute.FONT, font) }
                    val measurer = LineBreakMeasurer(attr.iterator, g2.fontRenderContext)
                    while (measurer.position < paragraph.length) {
                        val layout = measurer.nextLayout(w)
                        y += layout.ascent
                        layout.draw(g2, x0, y)
                        y += layout.descent + layout.leading
                    }
                }
                y += gap
                g2.color = Color.BLACK
            }

            val title = Font(Font.SANS_SERIF, Font.BOLD, 20)
            val body = Font(Font.SANS_SERIF, Font.PLAIN, 10)
            val small = Font(Font.SANS_SERIF, Font.PLAIN, 8)
            val mono = Font(Font.MONOSPACED, Font.BOLD, 18)

            text(t.title, title, gap = 4f)
            text(t.created, small, gap = 12f, color = Color.DARK_GRAY)
            text(t.intro, body, gap = 16f)

            val qr = 140f
            val boxTop = y
            y += 12f
            val textWidth = width - qr - 24f
            text(t.codeLabel, small, textWidth, gap = 6f, color = Color.DARK_GRAY)
            for (row in code.split('-').chunked(3)) text(row.joinToString("  "), mono, textWidth, gap = 6f)
            drawQr(g2, code, x0 + width - qr - 8f, boxTop + 10f, qr)
            y = maxOf(y, boxTop + qr + 22f) + 6f
            g2.stroke = BasicStroke(1.1f)
            g2.drawRect((x0 - 8).toInt(), boxTop.toInt(), (width + 16).toInt(), (y - boxTop).toInt())
            y += 20f

            t.steps.forEachIndexed { i, s -> text("${i + 1}. $s", body, gap = 6f) }
            y += 10f
            text(t.warning, small, color = Color.DARK_GRAY)
            return Printable.PAGE_EXISTS
        }

        private fun drawQr(g: Graphics2D, text: String, x: Float, y: Float, size: Float) {
            val hints = mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M, EncodeHintType.MARGIN to 1)
            val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, hints)
            val cell = size / m.width
            g.color = Color.BLACK
            for (yy in 0 until m.height) for (xx in 0 until m.width) {
                if (m.get(xx, yy)) {
                    g.fill(java.awt.geom.Rectangle2D.Float(x + xx * cell, y + yy * cell, cell + 0.2f, cell + 0.2f))
                }
            }
        }
    }
}
