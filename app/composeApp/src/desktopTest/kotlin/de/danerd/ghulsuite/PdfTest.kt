// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.jpegPdf
import de.danerd.ghulsuite.ui.textPdf
import java.io.File
import kotlin.test.Test
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class PdfTest {
    @Test
    fun textAndPhotoPdfs() {
        val out = File("build/pdf-test").apply { mkdirs() }
        val text = textPdf("Grüße aus GhulMail", "Hallo Erika,\n\nÄpfel, Öl, Übermut \u2013 5 €.\n" + "Lange Zeile ".repeat(40))
        File(out, "text.pdf").writeBytes(text)
        assertTrue(String(text, Charsets.ISO_8859_1).startsWith("%PDF-1.4"))
        val img = java.awt.image.BufferedImage(300, 400, java.awt.image.BufferedImage.TYPE_INT_RGB)
        val bos = java.io.ByteArrayOutputStream()
        javax.imageio.ImageIO.write(img, "jpg", bos)
        val pdf = jpegPdf(bos.toByteArray())
        assertNotNull(pdf)
        File(out, "scan.pdf").writeBytes(pdf)
    }
}
