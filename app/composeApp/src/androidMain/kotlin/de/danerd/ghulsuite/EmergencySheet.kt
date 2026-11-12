// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.Activity
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.pdf.PdfDocument
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.PrintManager
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import de.danerd.ghulsuite.model.EmergencySheetText
import java.io.ByteArrayOutputStream
import java.io.FileOutputStream

/**
 * Builds the emergency sheet (A4 PDF) in memory and hands it to the system print dialog, which
 * also offers "Save as PDF". Nothing is written to app storage.
 */
object EmergencySheet {
    private const val W = 595 // A4 in PostScript points
    private const val H = 842
    private const val M = 56

    fun render(code: String, t: EmergencySheetText): ByteArray {
        val doc = PdfDocument()
        val page = doc.startPage(PdfDocument.PageInfo.Builder(W, H, 1).create())
        val c = page.canvas
        var y = M.toFloat()

        val title = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 22f; typeface = Typeface.DEFAULT_BOLD }
        val body = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 11f }
        val small = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 9f; color = Color.DKGRAY }
        val mono = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            textSize = 20f
            typeface = Typeface.create(Typeface.MONOSPACE, Typeface.BOLD)
            letterSpacing = 0.05f
        }

        fun text(s: String, p: TextPaint, width: Int = W - 2 * M, gap: Float = 8f) {
            val layout = StaticLayout.Builder.obtain(s, 0, s.length, p, width).setAlignment(Layout.Alignment.ALIGN_NORMAL).build()
            c.save()
            c.translate(M.toFloat(), y)
            layout.draw(c)
            c.restore()
            y += layout.height + gap
        }

        text(t.title, title, gap = 4f)
        text(t.created, small, gap = 14f)
        text(t.intro, body, gap = 18f)

        // Code box with QR code
        val boxTop = y
        val qrSize = 150
        val groups = code.split('-')
        val textWidth = W - 2 * M - qrSize - 24
        y += 14f
        text(t.codeLabel, small, textWidth, gap = 6f)
        for (row in groups.chunked(3)) text(row.joinToString("  "), mono, textWidth, gap = 6f)
        drawQr(c, code, W - M - qrSize - 10, boxTop.toInt() + 12, qrSize)
        y = maxOf(y, boxTop + qrSize + 24) + 8f
        c.drawRect(M.toFloat() - 8, boxTop, (W - M + 8).toFloat(), y, Paint().apply { style = Paint.Style.STROKE; strokeWidth = 1.2f })
        y += 22f

        t.steps.forEachIndexed { i, s -> text("${i + 1}. $s", body, gap = 6f) }
        y += 12f
        text(t.warning, small)

        doc.finishPage(page)
        val out = ByteArrayOutputStream()
        doc.writeTo(out)
        doc.close()
        return out.toByteArray()
    }

    private fun drawQr(c: Canvas, text: String, x: Int, y: Int, size: Int) {
        val hints = mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M, EncodeHintType.MARGIN to 1)
        val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, hints)
        val cell = size.toFloat() / m.width
        val black = Paint().apply { color = Color.BLACK }
        for (yy in 0 until m.height) for (xx in 0 until m.width) {
            if (m.get(xx, yy)) c.drawRect(x + xx * cell, y + yy * cell, x + (xx + 1) * cell, y + (yy + 1) * cell, black)
        }
    }

    fun print(activity: Activity, code: String, t: EmergencySheetText) {
        val pdf = render(code, t)
        val pm = activity.getSystemService(PrintManager::class.java) ?: return
        pm.print(t.title, object : PrintDocumentAdapter() {
            override fun onLayout(
                oldAttributes: PrintAttributes?, newAttributes: PrintAttributes?, cancellationSignal: CancellationSignal?,
                callback: LayoutResultCallback, extras: Bundle?,
            ) {
                if (cancellationSignal?.isCanceled == true) return callback.onLayoutCancelled()
                callback.onLayoutFinished(
                    PrintDocumentInfo.Builder("GhulSuite-Notfallblatt.pdf")
                        .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT).setPageCount(1).build(),
                    true,
                )
            }

            override fun onWrite(
                pages: Array<out PageRange>?, destination: ParcelFileDescriptor, cancellationSignal: CancellationSignal?,
                callback: WriteResultCallback,
            ) {
                try {
                    FileOutputStream(destination.fileDescriptor).use { it.write(pdf) }
                    callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
                } catch (e: Exception) {
                    callback.onWriteFailed(e.message)
                }
            }

            override fun onFinish() {
                pdf.fill(0)
            }
        }, PrintAttributes.Builder().setMediaSize(PrintAttributes.MediaSize.ISO_A4).build())
    }
}
