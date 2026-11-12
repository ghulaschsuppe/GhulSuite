// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import androidx.documentfile.provider.DocumentFile
import de.danerd.ghulsuite.core.StorageException
import de.danerd.ghulsuite.core.VaultStorage

/**
 * A folder chosen by the user via the Storage Access Framework (e.g. "Documents/GhulSuite" or a
 * USB stick). Writes go to a temporary file which then replaces the target, so that an
 * interrupted write never leaves a half-written vault.
 */
class SafStorage(private val context: Context, private val treeUri: Uri) : VaultStorage {
    private val resolver = context.contentResolver

    private fun folder(): DocumentFile =
        DocumentFile.fromTreeUri(context, treeUri) ?: throw StorageException.Io("folder not accessible")

    private fun find(name: String): DocumentFile? = folder().findFile(name)

    override fun read(name: String): ByteArray? {
        val f = find(name) ?: return null
        return try {
            resolver.openInputStream(f.uri)?.use { it.readBytes() } ?: throw StorageException.Io("cannot read $name")
        } catch (e: StorageException) {
            throw e
        } catch (e: Exception) {
            throw StorageException.Io(e.message ?: "cannot read $name")
        }
    }

    override fun write(name: String, data: ByteArray) {
        try {
            val dir = folder()
            val tmpName = "$name.tmp"
            dir.findFile(tmpName)?.delete()
            val tmp = dir.createFile("application/octet-stream", tmpName) ?: throw StorageException.Io("cannot create $tmpName")
            resolver.openOutputStream(tmp.uri, "wt")?.use { it.write(data); it.flush() }
                ?: throw StorageException.Io("cannot write $tmpName")
            val existing = dir.findFile(name)
            if (existing == null) {
                if (!rename(tmp, name)) throw StorageException.Io("cannot rename $tmpName")
                return
            }
            // Keep the previous version as .bak, then move the new file into place.
            dir.findFile("$name.bak")?.delete()
            if (rename(existing, "$name.bak") && rename(tmp, name)) return
            // Provider without rename support: overwrite in place (not atomic, last resort).
            val target = dir.findFile(name) ?: dir.createFile("application/octet-stream", name)
                ?: throw StorageException.Io("cannot create $name")
            resolver.openOutputStream(target.uri, "wt")?.use { it.write(data); it.flush() }
                ?: throw StorageException.Io("cannot write $name")
            dir.findFile(tmpName)?.delete()
        } catch (e: StorageException) {
            throw e
        } catch (e: Exception) {
            throw StorageException.Io(e.message ?: "cannot write $name")
        }
    }

    private fun rename(file: DocumentFile, newName: String): Boolean = try {
        DocumentsContract.renameDocument(resolver, file.uri, newName) != null
    } catch (e: Exception) {
        false
    }

    override fun delete(name: String) {
        find(name)?.delete()
        find("$name.bak")?.delete()
    }

    override fun list(): List<String> =
        folder().listFiles().mapNotNull { it.name }.filter { !it.endsWith(".tmp") && !it.endsWith(".bak") }.sorted()

    override fun describe(): String = folder().name ?: treeUri.lastPathSegment.orEmpty()
}
