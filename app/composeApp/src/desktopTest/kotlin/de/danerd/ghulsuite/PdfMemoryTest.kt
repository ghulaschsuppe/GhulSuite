// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.ui.keepImagesInMemory
import de.danerd.ghulsuite.ui.openPdf
import java.awt.image.BufferedImage
import java.io.ByteArrayOutputStream
import java.nio.file.Files
import java.nio.file.StandardWatchEventKinds
import java.util.concurrent.TimeUnit
import javax.imageio.ImageIO
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import org.apache.pdfbox.pdmodel.PDDocument
import org.apache.pdfbox.pdmodel.PDPage
import org.apache.pdfbox.pdmodel.PDPageContentStream
import org.apache.pdfbox.pdmodel.graphics.image.JPEGFactory

/**
 * Showing a PDF on Windows puts nothing on the disk: a scanned document's pages are JPEG images,
 * which PDFBox decodes through ImageIO, whose cache would write them into the temp folder.
 */
class PdfMemoryTest {
    /** A PDF with one JPEG page (`/DCTDecode`, like a scan from Android). */
    private fun jpegPdf(): ByteArray {
        val img = BufferedImage(400, 300, BufferedImage.TYPE_INT_RGB)
        img.graphics.apply { color = java.awt.Color.WHITE; fillRect(20, 20, 200, 100); dispose() }
        val jpeg = ByteArrayOutputStream().also { ImageIO.write(img, "jpg", it) }.toByteArray()
        PDDocument().use { d ->
            val page = PDPage()
            d.addPage(page)
            val x = JPEGFactory.createFromByteArray(d, jpeg)
            PDPageContentStream(d, page).use { it.drawImage(x, 0f, 0f) }
            return ByteArrayOutputStream().also { d.save(it) }.toByteArray()
        }
    }

    @Test
    fun aJpegPageIsRenderedWithoutAnyFileInTheTempFolder() {
        val pdf = jpegPdf()
        val dir = Files.createTempDirectory("ghul-imageio")
        val tmpBefore = System.getProperty("java.io.tmpdir")
        val cacheBefore = ImageIO.getCacheDirectory()
        val ws = dir.fileSystem.newWatchService()
        try {
            System.setProperty("java.io.tmpdir", dir.toString())
            ImageIO.setCacheDirectory(dir.toFile())
            dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE)
            // What main does first.
            keepImagesInMemory()
            assertFalse(ImageIO.getUseCache())
            val view = assertNotNull(openPdf(pdf))
            try {
                assertEquals(1, view.pageCount)
                assertNotNull(view.render(0, 800))
            } finally {
                view.close()
            }
            val created = ws.poll(500, TimeUnit.MILLISECONDS)?.pollEvents()?.map { it.context().toString() }.orEmpty()
            assertEquals(emptyList(), created, "files created in the temp folder")
            assertEquals(emptyList(), dir.toFile().list().orEmpty().toList())
        } finally {
            ws.close()
            System.setProperty("java.io.tmpdir", tmpBefore)
            ImageIO.setCacheDirectory(cacheBefore)
            dir.toFile().deleteRecursively()
        }
    }
}
