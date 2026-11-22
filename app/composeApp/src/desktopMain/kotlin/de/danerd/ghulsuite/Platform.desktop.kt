// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.graphics.toComposeImageBitmap
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.multi.qrcode.QRCodeMultiReader
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import java.awt.FileDialog
import java.awt.Frame
import java.awt.GraphicsEnvironment
import java.awt.Rectangle
import java.awt.Robot
import java.awt.Toolkit
import java.awt.datatransfer.DataFlavor
import java.awt.image.BufferedImage
import java.io.File
import java.io.RandomAccessFile
import java.text.DateFormat
import java.util.Date
import javax.swing.JFileChooser
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import org.jetbrains.compose.resources.stringResource

/**
 * "Back" on Windows: Esc, Alt+Left and the back button of the mouse do what the back key does on
 * Android (bug: they did nothing). As with Android, the handler registered last that is switched
 * on gets it: an open page before the list, a selection before the page.
 */
object DesktopBack {
    class Handler(var enabled: Boolean, var onBack: () -> Unit)

    private val handlers = mutableListOf<Handler>()

    fun add(h: Handler) { handlers += h }
    fun remove(h: Handler) { handlers -= h }

    /** Goes back one step; false when there is nothing to go back to. */
    fun dispatch(): Boolean {
        val h = handlers.lastOrNull { it.enabled } ?: return false
        h.onBack()
        return true
    }

    /** Esc or Alt+Left, pressed. */
    fun isBackKey(e: androidx.compose.ui.input.key.KeyEvent): Boolean =
        e.type == androidx.compose.ui.input.key.KeyEventType.KeyDown && isBackKey(e.key, e.isAltPressed)

    private var held = false

    /**
     * The window's key handler: goes back once per press (a held key repeats "pressed", which
     * must not go back several steps).
     */
    fun onKey(e: androidx.compose.ui.input.key.KeyEvent): Boolean = onKey(e.key, e.isAltPressed, e.type)

    fun onKey(key: androidx.compose.ui.input.key.Key, alt: Boolean, type: androidx.compose.ui.input.key.KeyEventType): Boolean {
        if (!isBackKey(key, alt)) return false
        return when (type) {
            androidx.compose.ui.input.key.KeyEventType.KeyUp -> { held = false; false }
            androidx.compose.ui.input.key.KeyEventType.KeyDown -> if (held) true else { held = true; dispatch() }
            else -> false
        }
    }

    fun isBackKey(key: androidx.compose.ui.input.key.Key, alt: Boolean): Boolean =
        key == androidx.compose.ui.input.key.Key.Escape || (alt && key == androidx.compose.ui.input.key.Key.DirectionLeft)

    /** The back button of a mouse (button 4 in Java on Windows). */
    fun isBackButton(e: java.awt.event.MouseEvent): Boolean = isBackButton(e.id, e.button)

    fun isBackButton(id: Int, button: Int): Boolean = id == java.awt.event.MouseEvent.MOUSE_RELEASED && button == 4
}

actual fun androidx.compose.ui.Modifier.testTagsAsIds(): androidx.compose.ui.Modifier = this

@Composable
actual fun GuardWindow() {}

@Composable
actual fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit) {
    val handler = androidx.compose.runtime.remember { DesktopBack.Handler(enabled, onBack) }
    androidx.compose.runtime.SideEffect {
        handler.enabled = enabled
        handler.onBack = onBack
    }
    androidx.compose.runtime.DisposableEffect(handler) {
        DesktopBack.add(handler)
        onDispose { DesktopBack.remove(handler) }
    }
}

actual fun nowSeconds(): Long = System.currentTimeMillis() / 1000

actual fun formatDate(millis: Long): String = DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(millis))

actual fun formatTime(millis: Long): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(millis))

// --- QR codes: there is rarely a camera, so the screen (or a copied image) is scanned. --------

actual val qrScannerAvailable: Boolean = !GraphicsEnvironment.isHeadless()

private fun decodeQr(image: BufferedImage): String? = runCatching {
    val pixels = image.getRGB(0, 0, image.width, image.height, null, 0, image.width)
    val bitmap = BinaryBitmap(HybridBinarizer(RGBLuminanceSource(image.width, image.height, pixels)))
    QRCodeMultiReader().decodeMultiple(bitmap, mapOf(DecodeHintType.TRY_HARDER to true))
        .map { it.text }
        // Prefer what GhulAuth understands.
        .sortedByDescending { it.startsWith("otpauth") }
        .firstOrNull()
}.getOrNull()

/** Captures every screen and looks for a QR code (e.g. a 2FA code shown in the browser). */
private fun scanScreens(): String? {
    val robot = Robot()
    for (device in GraphicsEnvironment.getLocalGraphicsEnvironment().screenDevices) {
        val bounds: Rectangle = device.defaultConfiguration.bounds
        val shot = robot.createScreenCapture(bounds)
        try {
            decodeQr(shot)?.let { return it }
        } finally {
            shot.flush()
        }
    }
    return null
}

private fun scanClipboardImage(): String? = runCatching {
    val clip = Toolkit.getDefaultToolkit().systemClipboard
    if (!clip.isDataFlavorAvailable(DataFlavor.imageFlavor)) return null
    val img = clip.getData(DataFlavor.imageFlavor) as java.awt.Image
    val buffered = BufferedImage(img.getWidth(null), img.getHeight(null), BufferedImage.TYPE_INT_RGB)
    buffered.createGraphics().apply { drawImage(img, 0, 0, null); dispose() }
    decodeQr(buffered)
}.getOrNull()

@Composable
actual fun QrScanner(onResult: (String) -> Unit) {
    var notFound by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Hint(stringResource(Res.string.qr_desktop_hint))
        Button(onClick = {
            val r = scanScreens()
            notFound = r == null
            r?.let(onResult)
        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.qr_scan_screen)) }
        OutlinedButton(onClick = {
            val r = scanClipboardImage()
            notFound = r == null
            r?.let(onResult)
        }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(Res.string.qr_scan_clipboard)) }
        if (notFound) ErrorText(stringResource(Res.string.qr_not_found))
    }
}

// --- files: the native Windows dialogs ------------------------------------------------------

// Screenshots for the docs are made without a display: "ghul.pickers" shows the buttons anyway.
actual val filePickersAvailable: Boolean
    get() = System.getProperty("ghul.pickers")?.toBoolean() ?: !GraphicsEnvironment.isHeadless()

private fun chooseOpen(): File? {
    val d = FileDialog(null as Frame?, "", FileDialog.LOAD)
    d.isVisible = true
    return d.file?.let { File(d.directory, it) }
}

private fun chooseSave(name: String): File? {
    val d = FileDialog(null as Frame?, "", FileDialog.SAVE)
    d.file = name
    d.isVisible = true
    return d.file?.let { File(d.directory, it) }
}

@Composable
actual fun rememberOpenFile(onPicked: (PickedFile) -> Unit): () -> Unit = {
    chooseOpen()?.let { f -> runCatching { onPicked(PickedFile(f.name, f.readBytes())) } }
}

@Composable
actual fun rememberSaveFile(onSaved: (SaveFileResult) -> Unit): (String, ByteArray) -> Unit = { name, data ->
    onSaved(saveChosenFile(chooseSave(name), data))
}

/** M8: writes [data] to the file chosen ([f], null: the dialog was closed) and says how it went. */
internal fun saveChosenFile(f: File?, data: ByteArray): SaveFileResult {
    f ?: return SaveFileResult.CANCELLED
    return try {
        writeReplacing(f, data)
        // r9 krypto N6: read back before it counts.
        if (f.readBytes().contentEquals(data)) SaveFileResult.SAVED else SaveFileResult.FAILED
    } catch (e: Exception) {
        val free = { f.absoluteFile.parentFile?.takeIf { it.isDirectory }?.usableSpace }
        if (de.danerd.ghulsuite.isNoSpace(e, free)) SaveFileResult.NO_SPACE else SaveFileResult.FAILED
    }
}

@Composable
actual fun rememberOpenSuiteFile(onPicked: (de.danerd.ghulsuite.model.SuiteFileRef) -> Unit): () -> Unit = {
    chooseOpen()?.let { de.danerd.ghulsuite.SuiteFiles.resolve(it) }?.takeIf { it.isFile }?.let { f -> onPicked(de.danerd.ghulsuite.model.SuiteFileRef(f.absolutePath, f.name)) }
}

@Composable
actual fun rememberCreateSuiteFile(onCreated: (de.danerd.ghulsuite.model.SuiteFileRef?) -> Unit): (String) -> Unit = { name ->
    val f = chooseSave(name)
    onCreated(f?.let { de.danerd.ghulsuite.model.SuiteFileRef(it.absolutePath, it.name) })
}

/** Windows writes by path (a file next to it, then moved into place): nothing to share. */
@Composable
actual fun rememberGrantFolder(onGranted: (picked: Boolean) -> Unit): (near: String) -> Unit = remember { { _: String -> onGranted(true) } }

@Composable
actual fun rememberPickFolder(onPicked: (FolderRef) -> Unit): () -> Unit = {
    val chooser = JFileChooser().apply { fileSelectionMode = JFileChooser.DIRECTORIES_ONLY }
    if (chooser.showOpenDialog(null) == JFileChooser.APPROVE_OPTION) {
        chooser.selectedFile?.let { onPicked(FolderRef(it.absolutePath, it.name)) }
    }
}

@Composable
actual fun rememberOpenStream(onPicked: (ByteSource) -> Unit): () -> Unit = {
    chooseOpen()?.let { f ->
        // Locked by another program or not readable: nothing is handed on (no crash in the click).
        val raf = runCatching { RandomAccessFile(f, "r") }.getOrNull() ?: return@let
        onPicked(object : ByteSource {
            override val name: String = f.name
            override val size: Long = f.length()
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int = raf.read(buffer, offset, length)
            override fun close() = raf.close()
        })
    }
}

@Composable
actual fun rememberOpenStreams(onPicked: (List<ByteSource>) -> Unit): () -> Unit = {
    val d = FileDialog(null as Frame?, "", FileDialog.LOAD)
    d.isMultipleMode = true
    d.isVisible = true
    val files = d.files.orEmpty().filter { it.isFile }
    if (files.isNotEmpty()) {
        onPicked(files.map { f ->
            object : ByteSource {
                private var raf: RandomAccessFile? = null
                override val name: String = f.name
                override val size: Long = f.length()
                override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
                    (raf ?: RandomAccessFile(f, "r").also { raf = it }).read(buffer, offset, length)
                override fun close() { raf?.close(); raf = null }
            }
        })
    }
}

@Composable
actual fun rememberCreateStream(onFailed: () -> Unit, onCreated: (ByteSink?) -> Unit): (String, String) -> Unit = { name, _ ->
    val f = chooseSave(name)
    if (f == null) {
        onCreated(null)
    } else {
        // A read-only stick or a file locked by another program: "not created", no crash.
        val out = runCatching { f.outputStream().buffered() }.getOrNull()
        if (out == null) {
            // r7 N-5: chosen but not writable is no "cancelled".
            onFailed()
            onCreated(null)
        } else {
            onCreated(object : ByteSink {
                override fun write(data: ByteArray) = out.write(data)
                override fun close() = out.close()
                override fun discard() { f.delete() }
                override fun freeBytes(): Long? = f.absoluteFile.parentFile?.usableSpace?.takeIf { it > 0 }
            })
        }
    }
}

actual fun qrModules(text: String): List<BooleanArray> {
    val hints = mapOf(com.google.zxing.EncodeHintType.MARGIN to 0, com.google.zxing.EncodeHintType.ERROR_CORRECTION to com.google.zxing.qrcode.decoder.ErrorCorrectionLevel.M)
    val m = com.google.zxing.qrcode.QRCodeWriter().encode(text, com.google.zxing.BarcodeFormat.QR_CODE, 0, 0, hints)
    return (0 until m.height).map { y -> BooleanArray(m.width) { x -> m.get(x, y) } }
}

actual fun decodeQrImage(bytes: ByteArray): String? = runCatching {
    val img = javax.imageio.ImageIO.read(java.io.ByteArrayInputStream(bytes)) ?: return null
    val px = img.getRGB(0, 0, img.width, img.height, null, 0, img.width)
    val bitmap = com.google.zxing.BinaryBitmap(com.google.zxing.common.HybridBinarizer(com.google.zxing.RGBLuminanceSource(img.width, img.height, px)))
    com.google.zxing.MultiFormatReader().decode(bitmap, mapOf(com.google.zxing.DecodeHintType.TRY_HARDER to true)).text
}.getOrNull()

actual fun deviceInfo(): String =
    "${System.getProperty("os.name")} ${System.getProperty("os.version")} (${System.getProperty("os.arch")}) · Java ${System.getProperty("java.version")}"

actual fun localNow(): String {
    val c = java.util.Calendar.getInstance()
    fun p(n: Int) = n.toString().padStart(2, '0')
    return "${c.get(java.util.Calendar.YEAR)}-${p(c.get(java.util.Calendar.MONTH) + 1)}-${p(c.get(java.util.Calendar.DAY_OF_MONTH))}" +
        "T${p(c.get(java.util.Calendar.HOUR_OF_DAY))}:${p(c.get(java.util.Calendar.MINUTE))}"
}

actual fun epochMillisOf(local: String): Long {
    val c = java.util.Calendar.getInstance()
    c.clear()
    c.set(local.substring(0, 4).toInt(), local.substring(5, 7).toInt() - 1, local.substring(8, 10).toInt(),
        local.substring(11, 13).toInt(), local.substring(14, 16).toInt(), 0)
    return c.timeInMillis
}

actual fun applyAppLanguage(tag: String) {
    // Takes effect at the next start (Main.kt applies the stored language before the window opens).
}

actual fun decodeImage(bytes: ByteArray, maxSide: Int): androidx.compose.ui.graphics.ImageBitmap? = runCatching {
    val image = org.jetbrains.skia.Image.makeFromEncoded(bytes)
    val side = maxOf(image.width, image.height)
    // The decoded copy of the file is released at once (not when the garbage collector comes).
    if (side <= maxSide) return@runCatching image.toComposeImageBitmap().also { image.close() }
    // Scaled down in memory: a photo of a phone camera is far larger than any window.
    val scale = maxSide.toFloat() / side
    val w = (image.width * scale).toInt().coerceAtLeast(1)
    val h = (image.height * scale).toInt().coerceAtLeast(1)
    val surface = org.jetbrains.skia.Surface.makeRasterN32Premul(w, h)
    surface.canvas.drawImageRect(image, org.jetbrains.skia.Rect.makeWH(w.toFloat(), h.toFloat()))
    surface.makeImageSnapshot().toComposeImageBitmap().also { image.close() }
}.getOrNull()

/**
 * Image data stays in memory: ImageIO caches the streams it decodes in a file in the temp folder
 * by default (PDFBox decodes JPEG pages, e.g. of a scanned document, through ImageIO). Called
 * first in `main`, before anything can use ImageIO, and again before a PDF is shown.
 */
fun keepImagesInMemory() {
    javax.imageio.ImageIO.setUseCache(false)
}

/**
 * A PDF read from the bytes with Apache PDFBox, everything in memory (no scratch file; the
 * stream cache is memory only, and so is ImageIO's, see [keepImagesInMemory]).
 */
actual fun openPdf(bytes: ByteArray): PdfView? = runCatching {
    keepImagesInMemory()
    // PDFBox keeps a list of the system's fonts: in the portable version next to the data, not on the PC.
    if (System.getProperty("pdfbox.fontcache") == null && System.getProperty("jpackage.app-path") != null) {
        System.setProperty("pdfbox.fontcache", de.danerd.ghulsuite.DesktopPaths.dataDir.absolutePath)
    }
    val doc = org.apache.pdfbox.Loader.loadPDF(
        org.apache.pdfbox.io.RandomAccessReadBuffer(bytes), "", null, null,
        org.apache.pdfbox.io.IOUtils.createMemoryOnlyStreamCache(),
    )
    val renderer = org.apache.pdfbox.rendering.PDFRenderer(doc)
    object : PdfView {
        override val pageCount: Int = doc.numberOfPages

        override fun render(index: Int, widthPx: Int): androidx.compose.ui.graphics.ImageBitmap? = synchronized(doc) {
            runCatching {
                val box = doc.getPage(index).cropBox
                val w = widthPx.coerceIn(100, 2400)
                val scale = w / box.width.coerceAtLeast(1f)
                val img = renderer.renderImage(index, scale, org.apache.pdfbox.rendering.ImageType.RGB)
                img.toComposeImageBitmap()
            }.getOrNull()
        }

        override fun close() {
            synchronized(doc) { runCatching { doc.close() } }
        }
    }
}.getOrNull()

/** Windows: no scanning (files are added instead). */
actual val docScanAvailable: Boolean = false

actual fun platformScanEngine(): de.danerd.ghulsuite.scan.ScanEngine? = null

/** Writes [data] next to [f] first and moves it over [f] in one step: an existing file stays whole if writing fails. */
private fun writeReplacing(f: File, data: ByteArray) {
    val tmp = File(f.absoluteFile.parentFile, f.name + ".ghltmp")
    try {
        java.io.FileOutputStream(tmp).use { out ->
            out.write(data)
            out.fd.sync()
        }
        try {
            java.nio.file.Files.move(tmp.toPath(), f.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE)
        } catch (e: java.nio.file.AtomicMoveNotSupportedException) {
            java.nio.file.Files.move(tmp.toPath(), f.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
        }
    } finally {
        tmp.delete()
    }
}
