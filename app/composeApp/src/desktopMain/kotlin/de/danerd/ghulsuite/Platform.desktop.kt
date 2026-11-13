// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.compose.foundation.layout.Arrangement
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
import org.jetbrains.compose.resources.stringResource

@Composable
actual fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit) {}

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

actual val filePickersAvailable: Boolean = !GraphicsEnvironment.isHeadless()

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
actual fun rememberSaveFile(onSaved: (Boolean) -> Unit): (String, ByteArray) -> Unit = { name, data ->
    val f = chooseSave(name)
    onSaved(f != null && runCatching { f.writeBytes(data) }.isSuccess)
}

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
        val raf = RandomAccessFile(f, "r")
        onPicked(object : ByteSource {
            override val name: String = f.name
            override val size: Long = f.length()
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int = raf.read(buffer, offset, length)
            override fun close() = raf.close()
        })
    }
}

@Composable
actual fun rememberCreateStream(onCreated: (ByteSink?) -> Unit): (String, String) -> Unit = { name, _ ->
    val f = chooseSave(name)
    if (f == null) {
        onCreated(null)
    } else {
        val out = f.outputStream().buffered()
        onCreated(object : ByteSink {
            override fun write(data: ByteArray) = out.write(data)
            override fun close() = out.close()
        })
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

@androidx.compose.runtime.Composable
actual fun rememberPhotoPermission(onResult: (Boolean) -> Unit): () -> Unit = { onResult(false) }

actual val cameraPhotoAvailable: Boolean = false

@androidx.compose.runtime.Composable
actual fun rememberTakePhoto(onResult: (ByteArray?) -> Unit): () -> Unit = { onResult(null) }

actual fun deviceInfo(): String =
    "${System.getProperty("os.name")} ${System.getProperty("os.version")} (${System.getProperty("os.arch")}) · Java ${System.getProperty("java.version")}"

actual fun localNow(): String {
    val c = java.util.Calendar.getInstance()
    fun p(n: Int) = n.toString().padStart(2, '0')
    return "${c.get(java.util.Calendar.YEAR)}-${p(c.get(java.util.Calendar.MONTH) + 1)}-${p(c.get(java.util.Calendar.DAY_OF_MONTH))}" +
        "T${p(c.get(java.util.Calendar.HOUR_OF_DAY))}:${p(c.get(java.util.Calendar.MINUTE))}"
}

actual fun utcOffsetMinutes(): Int = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60000

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
