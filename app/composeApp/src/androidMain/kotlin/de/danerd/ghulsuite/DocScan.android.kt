// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.ImageFormat
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RectF
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.scan_camera_failed
import de.danerd.ghulsuite.resources.scan_permission
import de.danerd.ghulsuite.resources.scan_permission_allow
import de.danerd.ghulsuite.scan.Quad
import de.danerd.ghulsuite.scan.SCAN_PAGE_MAX_SIDE
import de.danerd.ghulsuite.scan.ScanBitmap
import de.danerd.ghulsuite.scan.ScanCamera
import de.danerd.ghulsuite.scan.ScanEngine
import de.danerd.ghulsuite.scan.ScanRendered
import de.danerd.ghulsuite.scan.ScanShot
import de.danerd.ghulsuite.scan.documentFilter
import de.danerd.ghulsuite.scan.jpegInfo
import de.danerd.ghulsuite.scan.pageTransform
import de.danerd.ghulsuite.scan.uprightSize
import java.io.OutputStream
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlin.coroutines.resumeWithException
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.suspendCancellableCoroutine
import org.jetbrains.compose.resources.stringResource

// Scanning documents on Android: the camera in the app (CameraX, not the system camera app, no
// Google Play services), pictures with android.graphics. Photos are taken into memory
// (OnImageCapturedCallback, never a file); every bitmap and byte array of the scan is cleared
// when it is no longer needed.

actual val docScanAvailable: Boolean = true

actual fun platformScanEngine(): ScanEngine? = AndroidScanEngine

private val main = Handler(Looper.getMainLooper())

/** Clears the pixels of a bitmap and frees it. */
private fun Bitmap.wipe() {
    runCatching { if (isMutable && !isRecycled) eraseColor(0) }
    runCatching { recycle() }
}

/** A bitmap on the screen: cleared at once when released, freed a moment later (a frame may still draw it). */
private class AndroidScanBitmap(val bitmap: Bitmap) : ScanBitmap {
    override val image: ImageBitmap = bitmap.asImageBitmap()
    override val width: Int get() = bitmap.width
    override val height: Int get() = bitmap.height
    private var released = false

    override fun release() {
        if (released) return
        released = true
        runCatching { if (bitmap.isMutable && !bitmap.isRecycled) bitmap.eraseColor(0) }
        main.postDelayed({ runCatching { bitmap.recycle() } }, 1000)
    }
}

/** JPEG bytes collected in memory; a buffer that grows leaves only zeros behind. */
private class WipingStream : OutputStream() {
    private var buf = ByteArray(256 * 1024)
    private var size = 0

    override fun write(b: Int) = write(byteArrayOf(b.toByte()), 0, 1)

    override fun write(b: ByteArray, off: Int, len: Int) {
        if (size + len > buf.size) {
            val grown = ByteArray(max(buf.size * 2, size + len))
            buf.copyInto(grown, 0, 0, size)
            buf.fill(0)
            buf = grown
        }
        b.copyInto(buf, size, off, off + len)
        size += len
    }

    /** What was written, as an array of its own; the buffer is cleared. */
    fun take(): ByteArray = buf.copyOf(size).also { buf.fill(0); size = 0 }
}

private fun Bitmap.jpeg(quality: Int): ByteArray {
    val out = WipingStream()
    compress(Bitmap.CompressFormat.JPEG, quality, out)
    return out.take()
}

object AndroidScanEngine : ScanEngine {
    @Composable
    override fun Camera(camera: ScanCamera, modifier: Modifier) {
        val context = LocalContext.current
        fun allowed() = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        var granted by remember { mutableStateOf(allowed()) }
        var asked by remember { mutableStateOf(false) }
        val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            granted = it
            asked = true
        }
        val owner = context.findActivity() as? LifecycleOwner ?: context as LifecycleOwner
        // Back from the system's settings: the permission may have been given there.
        DisposableEffect(owner) {
            val observer = LifecycleEventObserver { _, e -> if (e == Lifecycle.Event.ON_RESUME) granted = allowed() }
            owner.lifecycle.addObserver(observer)
            onDispose { owner.lifecycle.removeObserver(observer) }
        }
        LaunchedEffect(Unit) { if (!granted) launcher.launch(Manifest.permission.CAMERA) }
        androidx.compose.runtime.SideEffect { camera.denied = !granted && asked }
        if (!granted) {
            Column(modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally) {
                Text(stringResource(Res.string.scan_permission), color = Color.White, textAlign = TextAlign.Center)
                if (asked) {
                    Button(onClick = {
                        val activity = context.findActivity()
                        if (activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.CAMERA)) {
                            launcher.launch(Manifest.permission.CAMERA)
                        } else {
                            // Refused for good: only the system's page of the app can allow it.
                            runCatching {
                                context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null)))
                            }
                        }
                    }) { Text(stringResource(Res.string.scan_permission_allow)) }
                }
            }
            return
        }
        val view = remember {
            PreviewView(context).apply {
                // Drawn into the app's own window (TextureView): the screenshot protection covers it.
                implementationMode = PreviewView.ImplementationMode.COMPATIBLE
                scaleType = PreviewView.ScaleType.FIT_CENTER
            }
        }
        var bound by remember { mutableStateOf<androidx.camera.core.Camera?>(null) }
        Box(modifier, contentAlignment = Alignment.Center) {
            AndroidView({ view }, Modifier.matchParentSize())
            if (camera.failed) Text(stringResource(Res.string.scan_camera_failed), color = Color.White, modifier = Modifier.padding(24.dp))
        }
        DisposableEffect(owner) {
            val executor = Executors.newSingleThreadExecutor()
            val future = ProcessCameraProvider.getInstance(context)
            var provider: ProcessCameraProvider? = null
            var disposed = false
            future.addListener({
                if (disposed) return@addListener
                try {
                    val p = future.get()
                    provider = p
                    val aspect = AspectRatioStrategy.RATIO_4_3_FALLBACK_AUTO_STRATEGY
                    val preview = Preview.Builder()
                        .setResolutionSelector(ResolutionSelector.Builder().setAspectRatioStrategy(aspect).build())
                        .build().also { it.surfaceProvider = view.surfaceProvider }
                    // About 8 megapixels: enough for a page of 2500 pixels, small enough for many pages in memory.
                    val capture = ImageCapture.Builder()
                        .setCaptureMode(ImageCapture.CAPTURE_MODE_MAXIMIZE_QUALITY)
                        .setResolutionSelector(
                            ResolutionSelector.Builder().setAspectRatioStrategy(aspect)
                                .setResolutionStrategy(ResolutionStrategy(Size(3264, 2448), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER))
                                .build(),
                        )
                        .setJpegQuality(92)
                        .build()
                    p.unbindAll()
                    val cam = p.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, capture)
                    bound = cam
                    camera.hasTorch = cam.cameraInfo.hasFlashUnit()
                    camera.failed = false
                    camera.take = { capture.shoot(executor) }
                } catch (e: Exception) {
                    camera.take = null
                    camera.failed = true
                }
            }, ContextCompat.getMainExecutor(context))
            onDispose {
                disposed = true
                camera.take = null
                camera.torch = false
                bound = null
                runCatching { provider?.unbindAll() }
                executor.shutdown()
            }
        }
        LaunchedEffect(bound, camera.torch) { bound?.let { runCatching { it.cameraControl.enableTorch(camera.torch) } } }
    }

    override fun preview(shot: ScanShot, maxSide: Int): ScanBitmap {
        val (uw, uh) = uprightSize(shot.rotation, shot.width, shot.height)
        val f = min(1f, maxSide.toFloat() / max(uw, uh))
        val w = max(1, (uw * f).toInt())
        val h = max(1, (uh * f).toInt())
        val raw = decodeRaw(shot.jpeg, max(w, h))
        try {
            val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
            // The stored photo turned upright and made smaller in one step.
            val m = Matrix()
            m.postRotate(shot.rotation.toFloat())
            val r = RectF(0f, 0f, raw.width.toFloat(), raw.height.toFloat())
            m.mapRect(r)
            m.postTranslate(-r.left, -r.top)
            m.postScale(w / r.width(), h / r.height())
            Canvas(out).drawBitmap(raw, m, Paint(Paint.FILTER_BITMAP_FLAG or Paint.DITHER_FLAG))
            return AndroidScanBitmap(out)
        } finally {
            raw.wipe()
        }
    }

    override fun pixels(bitmap: ScanBitmap): IntArray {
        val b = (bitmap as AndroidScanBitmap).bitmap
        return IntArray(b.width * b.height).also { b.getPixels(it, 0, b.width, 0, 0, b.width, b.height) }
    }

    override fun render(shot: ScanShot, corners: Quad, turns: Int, filter: Boolean): ScanRendered {
        // At most 4096 pixels of the photo on its longer side (a 50 megapixel camera is made smaller first).
        val raw = decodeRaw(shot.jpeg, 4096)
        val page: Bitmap
        try {
            val t = pageTransform(corners, shot.rotation, raw.width, raw.height, turns, SCAN_PAGE_MAX_SIDE)
            page = Bitmap.createBitmap(t.width, t.height, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(page)
            canvas.drawColor(android.graphics.Color.WHITE)
            val m = Matrix().apply { setValues(t.matrix) }
            canvas.drawBitmap(raw, m, Paint(Paint.FILTER_BITMAP_FLAG or Paint.DITHER_FLAG or Paint.ANTI_ALIAS_FLAG))
        } finally {
            raw.wipe()
        }
        try {
            if (filter) {
                val px = IntArray(page.width * page.height)
                page.getPixels(px, 0, page.width, 0, 0, page.width, page.height)
                documentFilter(px, page.width, page.height)
                page.setPixels(px, 0, page.width, 0, 0, page.width, page.height)
                px.fill(0)
            }
            // About 200 to 600 KB a page; a busy page a little more compressed.
            var jpeg = page.jpeg(if (filter) 80 else 82)
            if (jpeg.size > 900_000) {
                jpeg.fill(0)
                jpeg = page.jpeg(68)
            }
            val thumb = scaled(page, 320)
            return ScanRendered(jpeg, page.width, page.height, AndroidScanBitmap(thumb))
        } finally {
            page.wipe()
        }
    }

    override fun decode(jpeg: ByteArray, maxSide: Int): ScanBitmap {
        val raw = decodeRaw(jpeg, maxSide)
        if (max(raw.width, raw.height) <= maxSide) return AndroidScanBitmap(raw)
        try {
            return AndroidScanBitmap(scaled(raw, maxSide))
        } finally {
            raw.wipe()
        }
    }

    override fun memoryLow(): Boolean {
        val rt = Runtime.getRuntime()
        val free = rt.maxMemory() - (rt.totalMemory() - rt.freeMemory())
        return free < 48L * 1024 * 1024
    }

    /** The JPEG decoded as it is stored (no turning), sampled down while its longer side is over twice [maxSide]. */
    private fun decodeRaw(jpeg: ByteArray, maxSide: Int): Bitmap {
        val info = jpegInfo(jpeg) ?: throw IllegalArgumentException("not a JPEG")
        var sample = 1
        while (max(info.width, info.height) / (sample * 2) >= maxSide) sample *= 2
        val opts = BitmapFactory.Options().apply {
            inSampleSize = sample
            inMutable = true
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        return BitmapFactory.decodeByteArray(jpeg, 0, jpeg.size, opts) ?: throw IllegalArgumentException("not decodable")
    }

    /** A smaller copy, its longer side [maxSide] (never larger than the original). */
    private fun scaled(src: Bitmap, maxSide: Int): Bitmap {
        val f = min(1f, maxSide.toFloat() / max(src.width, src.height))
        val w = max(1, (src.width * f).toInt())
        val h = max(1, (src.height * f).toInt())
        val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        val m = Matrix().apply { setScale(w.toFloat() / src.width, h.toFloat() / src.height) }
        Canvas(out).drawBitmap(src, m, Paint(Paint.FILTER_BITMAP_FLAG or Paint.DITHER_FLAG))
        return out
    }
}

private fun Context.findActivity(): Activity? {
    var c: Context? = this
    while (c is ContextWrapper) {
        if (c is Activity) return c
        c = c.baseContext
    }
    return null
}

/** Takes one photo into memory (never a file). A photo that arrives after the scan was left is wiped. */
private suspend fun ImageCapture.shoot(executor: Executor): ScanShot = suspendCancellableCoroutine { cont ->
    takePicture(executor, object : ImageCapture.OnImageCapturedCallback() {
        override fun onCaptureSuccess(image: ImageProxy) {
            val shot = try {
                toShot(image)
            } catch (e: Exception) {
                null
            } finally {
                image.close()
            }
            if (shot == null) {
                if (cont.isActive) cont.resumeWithException(IllegalStateException("no photo"))
                return
            }
            if (!cont.isActive) {
                shot.wipe()
                return
            }
            cont.resume(shot) { _, value, _ -> value.wipe() }
        }

        override fun onError(exception: ImageCaptureException) {
            if (cont.isActive) cont.resumeWithException(exception)
        }
    })
}

private fun toShot(image: ImageProxy): ScanShot? {
    if (image.format != ImageFormat.JPEG) return null
    val buffer = image.planes.firstOrNull()?.buffer ?: return null
    buffer.rewind()
    val bytes = ByteArray(buffer.remaining())
    buffer.get(bytes)
    // The camera's buffer too, where it can be written (it is reused for the next photo anyway).
    runCatching {
        if (!buffer.isReadOnly) {
            buffer.rewind()
            while (buffer.hasRemaining()) buffer.put(0)
        }
    }
    val info = jpegInfo(bytes) ?: run {
        bytes.fill(0)
        return null
    }
    return ScanShot(bytes, info.width, info.height, image.imageInfo.rotationDegrees)
}
