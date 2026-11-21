// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import android.net.Uri
import androidx.compose.ui.graphics.asImageBitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.documentfile.provider.DocumentFile
import de.danerd.ghulsuite.ExternalActivity
import de.danerd.ghulsuite.model.FolderRef
import de.danerd.ghulsuite.model.PickedFile
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Vault files are small; anything bigger is certainly not one (protects against huge reads). */
private const val MAX_FILE_BYTES = 64L * 1024 * 1024

actual val filePickersAvailable: Boolean = true

@Composable
actual fun rememberOpenFile(onPicked: (PickedFile) -> Unit): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val file = withContext(Dispatchers.IO) {
                runCatching {
                    val doc = DocumentFile.fromSingleUri(context, uri)
                    if ((doc?.length() ?: 0) > MAX_FILE_BYTES) return@runCatching null
                    val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@runCatching null
                    PickedFile(doc?.name ?: uri.lastPathSegment ?: "file", bytes)
                }.getOrNull()
            }
            if (file != null) onPicked(file)
        }
    }
    return remember(launcher) {
        {
            ExternalActivity.started()
            launcher.launch(arrayOf("*/*"))
        }
    }
}

@Composable
actual fun rememberSaveFile(onSaved: (Boolean) -> Unit): (String, ByteArray) -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pending = remember { arrayOfNulls<ByteArray>(1) }
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri: Uri? ->
        ExternalActivity.finished()
        val data = pending[0]
        pending[0] = null
        if (uri == null || data == null) {
            onSaved(false)
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            val ok = withContext(Dispatchers.IO) {
                runCatching {
                    context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(data) } != null
                }.getOrDefault(false)
            }
            onSaved(ok)
        }
    }
    return remember(launcher) {
        { name, data ->
            pending[0] = data
            ExternalActivity.started()
            launcher.launch(name)
        }
    }
}

/** Keeps read and write access to a document across restarts (where the provider allows it). */
private fun keepAccess(context: android.content.Context, uri: Uri) {
    runCatching {
        context.contentResolver.takePersistableUriPermission(
            uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
    }
}

@Composable
actual fun rememberOpenSuiteFile(onPicked: (de.danerd.ghulsuite.model.SuiteFileRef) -> Unit): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // Nothing is read here: a suite file is read as a stream when it is opened, a vault file as
    // a whole (see readSuiteFile).
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val picked = withContext(Dispatchers.IO) {
                runCatching {
                    keepAccess(context, uri)
                    val name = DocumentFile.fromSingleUri(context, uri)?.name ?: uri.lastPathSegment ?: "GhulSuite.ghlzip"
                    de.danerd.ghulsuite.model.SuiteFileRef(uri.toString(), name)
                }.getOrNull()
            }
            if (picked != null) onPicked(picked)
        }
    }
    return remember(launcher) {
        {
            ExternalActivity.started()
            launcher.launch(arrayOf("*/*"))
        }
    }
}

@Composable
actual fun rememberCreateSuiteFile(onCreated: (de.danerd.ghulsuite.model.SuiteFileRef?) -> Unit): (String) -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) {
            onCreated(null)
            return@rememberLauncherForActivityResult
        }
        keepAccess(context, uri)
        val name = DocumentFile.fromSingleUri(context, uri)?.name ?: uri.lastPathSegment ?: "GhulSuite.ghlzip"
        onCreated(de.danerd.ghulsuite.model.SuiteFileRef(uri.toString(), name))
    }
    return remember(launcher) {
        { name ->
            ExternalActivity.started()
            launcher.launch(name)
        }
    }
}

/**
 * The folder of the suite file, shared once: then saving writes the new file next to the old one
 * and swaps them (see AndroidVaultBackend.safeWrite). Access is kept across restarts.
 */
@Composable
actual fun rememberGrantFolder(onGranted: () -> Unit): (near: String) -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri != null) keepAccess(context, uri)
        onGranted()
    }
    return remember(launcher) {
        { near ->
            ExternalActivity.started()
            launcher.launch(runCatching { Uri.parse(near) }.getOrNull()?.takeIf { near.isNotEmpty() })
        }
    }
}

@Composable
actual fun rememberPickFolder(onPicked: (FolderRef) -> Unit): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) return@rememberLauncherForActivityResult
        val name = DocumentFile.fromTreeUri(context, uri)?.name ?: uri.lastPathSegment ?: "?"
        onPicked(FolderRef(uri.toString(), name))
    }
    return remember(launcher) {
        {
            ExternalActivity.started()
            launcher.launch(null)
        }
    }
}

@Composable
actual fun rememberOpenStream(onPicked: (ByteSource) -> Unit): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) return@rememberLauncherForActivityResult
        val doc = DocumentFile.fromSingleUri(context, uri)
        val input = runCatching { context.contentResolver.openInputStream(uri) }.getOrNull() ?: return@rememberLauncherForActivityResult
        onPicked(object : ByteSource {
            override val name = doc?.name ?: uri.lastPathSegment ?: "file"
            override val size = doc?.length()?.takeIf { it > 0 } ?: -1L
            override fun read(buffer: ByteArray, offset: Int, length: Int) = input.read(buffer, offset, length)
            override fun close() = input.close()
        })
    }
    return remember(launcher) {
        {
            ExternalActivity.started()
            launcher.launch(arrayOf("*/*"))
        }
    }
}

/**
 * A file another app or the system picker hands over, opened only when it is read (several are
 * read one after the other).
 */
fun contentSource(context: android.content.Context, uri: Uri): ByteSource {
    var name: String? = null
    var size = -1L
    runCatching {
        context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME, android.provider.OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                name = c.getString(0)
                if (!c.isNull(1)) size = c.getLong(1)
            }
        }
    }
    val type = runCatching { context.contentResolver.getType(uri) }.getOrNull().orEmpty()
    return object : ByteSource {
        private var input: java.io.InputStream? = null
        override val name = name ?: uri.lastPathSegment?.substringAfterLast('/') ?: "file"
        override val size = size
        override val mime = type
        override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
            val i = input ?: (context.contentResolver.openInputStream(uri) ?: throw java.io.IOException("cannot open $name")).also { input = it }
            return i.read(buffer, offset, length)
        }
        override fun close() { input?.close(); input = null }
    }
}

@Composable
actual fun rememberOpenStreams(onPicked: (List<ByteSource>) -> Unit): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris: List<Uri> ->
        ExternalActivity.finished()
        if (uris.isNotEmpty()) onPicked(uris.map { contentSource(context.applicationContext, it) })
    }
    return remember(launcher) {
        {
            ExternalActivity.started()
            launcher.launch(arrayOf("*/*"))
        }
    }
}

@Composable
actual fun rememberCreateStream(onCreated: (ByteSink?) -> Unit): (String, String) -> Unit {
    val context = LocalContext.current
    val mime = remember { arrayOf("application/octet-stream") }
    val launcher = rememberLauncherForActivityResult(
        object : ActivityResultContracts.CreateDocument("application/octet-stream") {
            override fun createIntent(context: android.content.Context, input: String) =
                super.createIntent(context, input).setType(mime[0])
        },
    ) { uri: Uri? ->
        ExternalActivity.finished()
        // The application's context: a download may outlive this window.
        val resolver = context.applicationContext.contentResolver
        val out = uri?.let { runCatching { resolver.openOutputStream(it, "wt") }.getOrNull() }
        // Created but cannot be written: no empty file is left behind.
        if (uri != null && out == null) runCatching { android.provider.DocumentsContract.deleteDocument(resolver, uri) }
        onCreated(
            out?.let {
                object : ByteSink {
                    override fun write(data: ByteArray) = it.write(data)
                    override fun close() = it.close()
                    override fun discard() {
                        runCatching { android.provider.DocumentsContract.deleteDocument(resolver, uri) }
                    }
                }
            },
        )
    }
    return remember(launcher) {
        { name, type ->
            mime[0] = type.ifBlank { "application/octet-stream" }
            ExternalActivity.started()
            launcher.launch(name)
        }
    }
}

actual fun decodeImage(bytes: ByteArray, maxSide: Int): androidx.compose.ui.graphics.ImageBitmap? = runCatching {
    val source = android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(bytes))
    android.graphics.ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
        val side = maxOf(info.size.width, info.size.height)
        var sample = 1
        while (side / sample > maxSide) sample *= 2
        decoder.setTargetSampleSize(sample)
        // Drawn in software: a hardware bitmap could not be read back for the screen protection.
        decoder.allocator = android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE
    }.asImageBitmap()
}.getOrNull()

/**
 * A PDF in an anonymous file in memory (memfd, Android 11 and newer): PdfRenderer needs a file
 * it can seek in, and this one never reaches the storage. Older Android versions show no preview
 * (the app offers to open or export the document instead).
 */
actual fun openPdf(bytes: ByteArray): PdfView? {
    if (android.os.Build.VERSION.SDK_INT < 30) return null
    return runCatching {
        val fd = android.system.Os.memfd_create("ghuldocs", 0)
        try {
            var off = 0
            while (off < bytes.size) off += android.system.Os.write(fd, bytes, off, bytes.size - off)
            android.system.Os.lseek(fd, 0, android.system.OsConstants.SEEK_SET)
            val pfd = android.os.ParcelFileDescriptor.dup(fd)
            val renderer = try {
                android.graphics.pdf.PdfRenderer(pfd)
            } catch (e: Exception) {
                pfd.close()
                throw e
            }
            object : PdfView {
                override val pageCount: Int = renderer.pageCount

                override fun render(index: Int, widthPx: Int): androidx.compose.ui.graphics.ImageBitmap? = synchronized(renderer) {
                    runCatching {
                        renderer.openPage(index).use { page ->
                            val w = widthPx.coerceIn(100, 2400)
                            val h = (w.toLong() * page.height / page.width.coerceAtLeast(1)).toInt().coerceIn(1, 4 * w)
                            val bmp = android.graphics.Bitmap.createBitmap(w, h, android.graphics.Bitmap.Config.ARGB_8888)
                            bmp.eraseColor(android.graphics.Color.WHITE)
                            page.render(bmp, null, null, android.graphics.pdf.PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                            bmp.asImageBitmap()
                        }
                    }.getOrNull()
                }

                override fun close() = synchronized(renderer) {
                    runCatching { renderer.close() }
                    runCatching { pfd.close() }
                    Unit
                }
            }
        } finally {
            // The renderer holds its own copy of the descriptor.
            runCatching { android.system.Os.close(fd) }
        }
    }.getOrNull()
}
