// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.ArchiveContent
import de.danerd.ghulsuite.core.ArchiveProgress
import de.danerd.ghulsuite.core.ArchiveSink
import de.danerd.ghulsuite.core.ArchiveSource
import de.danerd.ghulsuite.core.DocsItem
import de.danerd.ghulsuite.core.FidoListener
import de.danerd.ghulsuite.core.FidoStatus
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.ShareDocsOptions
import de.danerd.ghulsuite.core.StorageException
import de.danerd.ghulsuite.model.ArchiveContentUi
import de.danerd.ghulsuite.model.ArchiveInfoUi
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DocsBackend
import de.danerd.ghulsuite.model.DocsItemUi
import de.danerd.ghulsuite.model.DocsUsageUi
import de.danerd.ghulsuite.model.ShareVaultUi
import de.danerd.ghulsuite.model.SuiteFileRef
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

/** Runs a call of the core with the security key when [pin] is given (else without). */
interface KeyRunner {
    suspend fun <T> run(pin: String?, block: (HidTransport?) -> T): T
}

/** A file the app picked or another app handed over, for the core to read piece by piece. */
internal class ByteSourceInput(private val source: ByteSource, private val active: () -> Boolean) : ArchiveSource {
    override fun size(): ULong = source.size.coerceAtLeast(0).toULong()

    override fun read(max: UInt): ByteArray {
        // Cancelled: the core stops reading and removes what it wrote so far.
        if (!active()) throw StorageException.Io("cancelled")
        val buf = ByteArray(max.toInt().coerceIn(1, 1 shl 20))
        var n = 0
        try {
            while (n < buf.size) {
                val r = source.read(buf, n, buf.size - n)
                if (r < 0) break
                n += r
            }
        } catch (e: Exception) {
            throw StorageException.Io("the file cannot be read")
        }
        return if (n == buf.size) buf else buf.copyOf(n).also { buf.fill(0) }
    }
}

/** Where the core writes a document (or a vault file) to, piece by piece. */
internal class ByteSinkOutput(private val sink: ByteSink, private val active: () -> Boolean) : ArchiveSink {
    override fun write(data: ByteArray) {
        if (!active()) throw StorageException.Io("cancelled")
        try {
            sink.write(data)
        } catch (e: Exception) {
            throw StorageException.Io("the file cannot be written")
        } finally {
            // The core hands over a copy of its own: what was written does not stay in this one.
            data.fill(0)
        }
    }
}

internal class BytesProgress(private val report: (Long, Long) -> Unit) : ArchiveProgress {
    override fun progress(done: ULong, total: ULong) = report(done.toLong(), total.toLong())
}

/** A suite file or vault file for the core to read, piece by piece. */
internal class SuiteInputSource(private val input: SuiteInput) : ArchiveSource {
    override fun size(): ULong = input.size.coerceAtLeast(0).toULong()

    override fun read(max: UInt): ByteArray {
        val buf = ByteArray(max.toInt().coerceIn(1, 1 shl 20))
        var n = 0
        try {
            while (n < buf.size) {
                val r = input.stream.read(buf, n, buf.size - n)
                if (r < 0) break
                n += r
            }
        } catch (e: java.io.IOException) {
            throw StorageException.Io("the file cannot be read")
        }
        return if (n == buf.size) buf else buf.copyOf(n)
    }
}

internal fun DocsItem.toUi() = DocsItemUi(
    vaultId = vaultId, id = id, parent = parent, name = name, isFolder = isFolder, size = size.toLong(), mime = mime,
    created = created, modified = modified, favorite = favorite, trashed = trashed, path = path, items = items.toInt(),
)

/** GhulDocs on top of the Rust core, the same on Android and Windows. */
class CoreDocsBackend(
    private val suite: () -> GhulSuite,
    private val openInput: (SuiteFileRef) -> SuiteInput?,
    private val keys: KeyRunner,
) : DocsBackend {
    private suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { mapped(block) }

    /** Whether the calling coroutine still wants the result (else a transfer stops). */
    private suspend fun activity(): () -> Boolean {
        val job = currentCoroutineContext()[Job]
        return { job?.isActive != false }
    }

    /**
     * A transfer: the core stops at its next piece once the caller is cancelled and reports an
     * error of the source or sink; that is a cancellation, not a failure.
     */
    private suspend fun <T> transfer(block: () -> T): T = try {
        io(block)
    } catch (e: VaultException) {
        currentCoroutineContext().ensureActive()
        throw e
    }

    private class Touch(private val onTouchNeeded: () -> Unit) : FidoListener {
        override fun onStatus(status: FidoStatus) {
            if (status == FidoStatus.TOUCH_NEEDED) onTouchNeeded()
        }
    }

    override suspend fun list(vaultId: String, folder: String?) = io { suite().docsList(vaultId, folder).map { it.toUi() } }

    override suspend fun item(vaultId: String, id: String) = io { suite().docsItem(vaultId, id).toUi() }

    override suspend fun search(vaultId: String?, query: String) = io { suite().docsSearch(vaultId, query).map { it.toUi() } }

    override suspend fun favorites(vaultId: String?) = io { suite().docsFavorites(vaultId).map { it.toUi() } }

    override suspend fun trash(vaultId: String?) = io { suite().docsTrash(vaultId).map { it.toUi() } }

    override suspend fun createFolder(vaultId: String, folder: String?, name: String) =
        io { suite().docsCreateFolder(vaultId, folder, name).toUi() }

    override suspend fun rename(vaultId: String, id: String, name: String) = io { suite().docsRename(vaultId, id, name).toUi() }

    override suspend fun move(vaultId: String, id: String, folder: String?) = io { suite().docsMove(vaultId, id, folder).toUi() }

    override suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) = io { suite().docsSetFavorite(vaultId, id, favorite) }

    override suspend fun toTrash(vaultId: String, id: String) = io { suite().docsToTrash(vaultId, id) }

    override suspend fun restore(vaultId: String, id: String) = io { suite().docsRestore(vaultId, id).toUi() }

    override suspend fun deleteForever(vaultId: String, id: String) = io { suite().docsDeleteForever(vaultId, id) }

    override suspend fun emptyTrash(vaultId: String?) = io { suite().docsEmptyTrash(vaultId) }

    override suspend fun add(
        vaultId: String,
        folder: String?,
        name: String,
        mime: String,
        source: ByteSource,
        onProgress: (Long) -> Unit,
    ): DocsItemUi {
        val active = activity()
        val s = suite()
        return transfer { s.docsAddFile(vaultId, folder, name, mime, ByteSourceInput(source, active), BytesProgress { d, _ -> onProgress(d) }).toUi() }
    }

    override suspend fun read(vaultId: String, id: String, sink: ByteSink, onProgress: (Long) -> Unit) {
        val active = activity()
        val s = suite()
        transfer { s.docsReadFile(vaultId, id, ByteSinkOutput(sink, active), BytesProgress { d, _ -> onProgress(d) }) }
    }

    override suspend fun usage() = io {
        val u = suite().docsUsage()
        DocsUsageUi(u.used.toLong(), u.limit.toLong(), u.files.toInt())
    }

    override suspend fun verify(vaultId: String) = io { suite().docsVerify(vaultId).map { it.toUi() } }

    override suspend fun exportVault(vaultId: String, sink: ByteSink, onProgress: (Long, Long) -> Unit) {
        val active = activity()
        val s = suite()
        transfer { s.docsExportVault(vaultId, ByteSinkOutput(sink, active), BytesProgress(onProgress)) }
    }

    override suspend fun shareVault(
        share: ShareVaultUi,
        pin: String?,
        onTouchNeeded: () -> Unit,
        sink: ByteSink,
        onProgress: (Long, Long) -> Unit,
    ): String {
        val options = ShareDocsOptions(
            vaultId = share.vaultId,
            ids = share.ids,
            name = share.name,
            masterPassword = share.masterPassword,
            password = share.password,
            keyfile = share.keyfile,
            keyLabel = share.keyLabel,
            keyfileLabel = share.keyfileLabel,
            recoveryLabel = share.recoveryLabel,
        )
        val active = activity()
        val s = suite()
        return keys.run(pin) { t ->
            mapped { s.docsShareVault(options, t, t?.let { Touch(onTouchNeeded) }, pin, ByteSinkOutput(sink, active), BytesProgress(onProgress)).recoveryCode }
        }
    }

    override suspend fun adoptVault(
        file: SuiteFileRef,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): String {
        val s = suite()
        return keys.run(pin) { t ->
            val input = openInput(file) ?: throw VaultException(VaultErrorKind.STORAGE, "the file cannot be read")
            input.stream.use {
                mapped { s.docsAdoptVault(SuiteInputSource(input), name, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile, BytesProgress(onProgress)) }
            }
        }
    }

    override suspend fun replaceVault(
        file: SuiteFileRef,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): String {
        val s = suite()
        return keys.run(pin) { t ->
            val input = openInput(file) ?: throw VaultException(VaultErrorKind.STORAGE, "the file cannot be read")
            input.stream.use {
                mapped { s.docsReplaceVault(SuiteInputSource(input), name, password, t, t?.let { Touch(onTouchNeeded) }, pin, keyfile, BytesProgress(onProgress)) }
            }
        }
    }

    /** What a `.ghlzip` or `.ghldocs` holds; null if it is neither (or cannot be read). */
    suspend fun inspect(file: SuiteFileRef): ArchiveInfoUi? = withContext(Dispatchers.IO) {
        val input = openInput(file) ?: return@withContext null
        input.stream.use {
            runCatching { de.danerd.ghulsuite.core.archiveInspect(SuiteInputSource(input)) }.getOrNull()?.let {
                ArchiveInfoUi(
                    when (it.content) {
                        ArchiveContent.SUITE -> ArchiveContentUi.SUITE
                        ArchiveContent.DOCS_VAULT -> ArchiveContentUi.DOCS_VAULT
                    },
                    it.protection.toUi(),
                )
            }
        }
    }
}
