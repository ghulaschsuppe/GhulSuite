// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import java.text.DateFormat
import java.util.Date

@Composable
actual fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit) = BackHandler(enabled, onBack)

actual fun nowSeconds(): Long = System.currentTimeMillis() / 1000

actual fun formatDate(millis: Long): String = DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(millis))
actual fun formatTime(millis: Long): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(millis))

actual fun qrModules(text: String): List<BooleanArray> {
    val hints = mapOf(com.google.zxing.EncodeHintType.MARGIN to 0, com.google.zxing.EncodeHintType.ERROR_CORRECTION to com.google.zxing.qrcode.decoder.ErrorCorrectionLevel.M)
    val m = com.google.zxing.qrcode.QRCodeWriter().encode(text, com.google.zxing.BarcodeFormat.QR_CODE, 0, 0, hints)
    return (0 until m.height).map { y -> BooleanArray(m.width) { x -> m.get(x, y) } }
}

actual fun decodeQrImage(bytes: ByteArray): String? = runCatching {
    val bmp = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
    val px = IntArray(bmp.width * bmp.height)
    bmp.getPixels(px, 0, bmp.width, 0, 0, bmp.width, bmp.height)
    val bitmap = com.google.zxing.BinaryBitmap(com.google.zxing.common.HybridBinarizer(com.google.zxing.RGBLuminanceSource(bmp.width, bmp.height, px)))
    com.google.zxing.MultiFormatReader().decode(bitmap, mapOf(com.google.zxing.DecodeHintType.TRY_HARDER to true)).text
}.getOrNull()

@androidx.compose.runtime.Composable
actual fun rememberPhotoPermission(onResult: (Boolean) -> Unit): () -> Unit {
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) { onResult(it) }
    val perm = if (android.os.Build.VERSION.SDK_INT >= 33) android.Manifest.permission.READ_MEDIA_IMAGES else android.Manifest.permission.READ_EXTERNAL_STORAGE
    return { de.danerd.ghulsuite.ExternalActivity.started(); launcher.launch(perm) }
}

actual val cameraPhotoAvailable: Boolean = true

@androidx.compose.runtime.Composable
actual fun rememberTakePhoto(onResult: (ByteArray?) -> Unit): () -> Unit {
    val context = androidx.compose.ui.platform.LocalContext.current
    val file = androidx.compose.runtime.remember { java.io.File(context.cacheDir, "scan/scan.jpg") }
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.TakePicture()) { ok ->
        val data = if (ok && file.exists()) runCatching { file.readBytes() }.getOrNull() else null
        file.delete()
        onResult(data)
    }
    return {
        file.parentFile?.mkdirs()
        file.delete()
        val uri = androidx.core.content.FileProvider.getUriForFile(context, context.packageName + ".files", file)
        de.danerd.ghulsuite.ExternalActivity.started()
        launcher.launch(uri)
    }
}

actual fun deviceInfo(): String =
    "Android ${android.os.Build.VERSION.RELEASE} (API ${android.os.Build.VERSION.SDK_INT}) · " +
        "${android.os.Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${android.os.Build.MODEL}"

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
    val activity = de.danerd.ghulsuite.MainActivity.current?.get() ?: return
    if (android.os.Build.VERSION.SDK_INT >= 33) {
        // The system keeps the choice (also visible under Settings › Apps › GhulSuite › Language).
        activity.getSystemService(android.app.LocaleManager::class.java)?.applicationLocales =
            if (tag.isEmpty()) android.os.LocaleList.getEmptyLocaleList() else android.os.LocaleList.forLanguageTags(tag)
    } else {
        activity.recreate() // MainActivity.attachBaseContext applies the stored language
    }
}
