// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import android.net.Uri
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
        val out = uri?.let { runCatching { context.contentResolver.openOutputStream(it, "wt") }.getOrNull() }
        onCreated(
            out?.let {
                object : ByteSink {
                    override fun write(data: ByteArray) = it.write(data)
                    override fun close() = it.close()
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
