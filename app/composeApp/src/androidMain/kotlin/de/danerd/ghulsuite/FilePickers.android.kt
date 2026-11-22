// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import android.net.Uri
import androidx.compose.ui.graphics.asImageBitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
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
actual fun rememberSaveFile(onSaved: (de.danerd.ghulsuite.ui.SaveFileResult) -> Unit): (String, ByteArray) -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pending = remember { arrayOfNulls<ByteArray>(1) }
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri: Uri? ->
        ExternalActivity.finished()
        val data = pending[0]
        pending[0] = null
        if (uri == null) {
            onSaved(SaveFileResult.CANCELLED)
            return@rememberLauncherForActivityResult
        }
        val resolver = context.applicationContext.contentResolver
        if (data == null) {
            // The app was recreated while the dialog was open: the content is gone, the document
            // just made stays no empty file, and it is no "cancelled" (r7 M-2).
            runCatching { android.provider.DocumentsContract.deleteDocument(resolver, uri) }
            onSaved(SaveFileResult.FAILED)
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            // M8: a failed write says so (a full storage with its own text).
            val result = withContext(Dispatchers.IO) {
                try {
                    // r9 krypto N6: read back before it counts (a cloud provider may take the
                    // bytes and fail later; a key file that is not really there locks out).
                    if (context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(data) } != null &&
                        context.contentResolver.openInputStream(uri)?.use { it.readBytes() }?.contentEquals(data) == true
                    ) {
                        de.danerd.ghulsuite.ui.SaveFileResult.SAVED
                    } else {
                        de.danerd.ghulsuite.ui.SaveFileResult.FAILED
                    }
                } catch (e: Exception) {
                    if (de.danerd.ghulsuite.isNoSpace(e)) de.danerd.ghulsuite.ui.SaveFileResult.NO_SPACE else de.danerd.ghulsuite.ui.SaveFileResult.FAILED
                }.also { r ->
                    // r7 M-1: the document made by the dialog is not left half written or empty
                    // (as with rememberCreateStream).
                    if (r != SaveFileResult.SAVED) runCatching { android.provider.DocumentsContract.deleteDocument(resolver, uri) }
                }
            }
            onSaved(result)
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
 * A safe place for the suite file: a folder kept for the app; saving there writes the new file
 * next to the old one and swaps them (see AndroidVaultBackend.safeWrite). Access is kept across
 * restarts; given back when the caller refuses the folder ([SafePlaceResult.Chosen.release]).
 */
@Composable
actual fun rememberChooseSafePlace(onResult: (SafePlaceResult) -> Unit): ((fileName: String) -> Unit)? {
    val context = LocalContext.current
    // Review N1: kept over a configuration change while the picker is open.
    var name by androidx.compose.runtime.saveable.rememberSaveable { androidx.compose.runtime.mutableStateOf("") }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri: Uri? ->
        ExternalActivity.finished()
        if (uri == null) {
            onResult(SafePlaceResult.Cancelled)
            return@rememberLauncherForActivityResult
        }
        val authority = "com.android.externalstorage.documents"
        val treeId = runCatching { android.provider.DocumentsContract.getTreeDocumentId(uri) }.getOrNull()
        when {
            // A folder of another app (a cloud app, Downloads): no safe swap there.
            uri.authority != authority || treeId == null -> onResult(SafePlaceResult.Refused(notShareable = false))
            // Download or the top of a storage: Android lets no app keep them (and the picker
            // mostly refuses them anyway).
            !de.danerd.ghulsuite.treeFolderAccepted(treeId) -> onResult(SafePlaceResult.Refused(notShareable = true))
            else -> {
                // Review N-a: a grant held before (for something else) is not given back later.
                val heldBefore = context.contentResolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission && it.isWritePermission }
                val kept = runCatching {
                    context.contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
                }.isSuccess
                if (kept && !heldBefore) de.danerd.ghulsuite.SafePlaceGrants.add(context, uri.toString())
                if (!kept) {
                    onResult(SafePlaceResult.Refused(notShareable = false))
                } else {
                    val fileName = name.ifEmpty { de.danerd.ghulsuite.ui.SUITE_FILE_NAME }
                    val docId = "$treeId/$fileName"
                    // Review N2: a file of that name, or what a cut off swap left (.alt, .neu),
                    // counts as there; so does a folder that cannot be asked.
                    fun there(id: String) = runCatching {
                        context.contentResolver.query(
                            android.provider.DocumentsContract.buildDocumentUriUsingTree(uri, id),
                            arrayOf(android.provider.DocumentsContract.Document.COLUMN_DOCUMENT_ID), null, null, null,
                        )?.use { it.moveToFirst() } == true
                    }.getOrDefault(true)
                    val exists = there(docId) || there("$docId.alt") || there("$docId.neu")
                    // Kept as the document's own address: the backend finds the shared folder for it.
                    val ref = de.danerd.ghulsuite.model.SuiteFileRef(android.provider.DocumentsContract.buildDocumentUri(authority, docId).toString(), fileName)
                    val release = {
                        if (!heldBefore) {
                            runCatching {
                                context.contentResolver.releasePersistableUriPermission(
                                    uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
                                )
                            }
                            de.danerd.ghulsuite.SafePlaceGrants.forget(context, uri.toString())
                        }
                        Unit
                    }
                    onResult(SafePlaceResult.Chosen(ref, exists, release))
                }
            }
        }
    }
    return remember(launcher) {
        { fileName ->
            name = fileName
            ExternalActivity.started()
            launcher.launch(null)
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
actual fun rememberCreateStream(onFailed: () -> Unit, onCreated: (ByteSink?) -> Unit): (String, String) -> Unit {
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
        if (uri != null && out == null) {
            runCatching { android.provider.DocumentsContract.deleteDocument(resolver, uri) }
            // r7 N-5: chosen but not writable is no "cancelled".
            onFailed()
        }
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
 * A PDF in an anonymous file in memory (memfd): PdfRenderer needs a file it can seek in, and this
 * one never reaches the storage.
 */
actual fun openPdf(bytes: ByteArray): PdfView? {
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
