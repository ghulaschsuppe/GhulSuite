// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

/** A file or folder of a GhulDocs vault. Times are Unix ms. */
data class DocsItemUi(
    val vaultId: String,
    val id: String,
    /** The folder it is in (null: at the top of its vault). */
    val parent: String?,
    val name: String,
    val isFolder: Boolean,
    /** Bytes of content (0 for folders). */
    val size: Long,
    /** Media type, e.g. `application/pdf` (empty for folders). */
    val mime: String,
    val created: Long,
    val modified: Long,
    val favorite: Boolean,
    /** When it was moved to the trash (only on what was moved there itself). */
    val trashed: Long?,
    /** The folders it is in, from the top, joined with `/` (empty at the top). */
    val path: String,
    /** Folders: how many items are directly in it. */
    val items: Int,
)

/**
 * How much a GhulDocs vault holds ([used] of [limit], 1 GB; [files] documents; the trash
 * included) and all GhulDocs vaults of the suite together ([suiteUsed] of [suiteLimit], 3 GB;
 * [suiteFiles] of [maxFiles] documents).
 */
data class DocsUsageUi(
    val used: Long,
    val limit: Long,
    val files: Int,
    val suiteUsed: Long = used,
    val suiteLimit: Long = 3 * limit,
    val suiteFiles: Int = files,
    val maxFiles: Int = 20_000,
)

/** What a suite file (`.ghlzip`) or GhulDocs package (`.ghldocs`) holds. */
enum class ArchiveContentUi { SUITE, DOCS_VAULT }

data class ArchiveInfoUi(val content: ArchiveContentUi, val protection: ProtectionUi)

/**
 * GhulDocs: encrypted documents in folders (FORMAT.md section 11). Content goes in and out as a
 * stream, encrypted while it is read and decrypted while it is written: plaintext never reaches
 * the disk of the app. Cancelling the calling coroutine stops a transfer (nothing stays behind).
 */
interface DocsBackend {
    /** Folders first, then files, by name; nothing in the trash. [folder] null: the top. */
    suspend fun list(vaultId: String, folder: String?): List<DocsItemUi>
    suspend fun item(vaultId: String, id: String): DocsItemUi
    /** Names containing [query] in one vault or in all (null). */
    suspend fun search(vaultId: String?, query: String): List<DocsItemUi>
    suspend fun favorites(vaultId: String?): List<DocsItemUi>
    /** What was moved to the trash, the latest first. */
    suspend fun trash(vaultId: String?): List<DocsItemUi>
    suspend fun createFolder(vaultId: String, folder: String?, name: String): DocsItemUi
    suspend fun rename(vaultId: String, id: String, name: String): DocsItemUi
    suspend fun move(vaultId: String, id: String, folder: String?): DocsItemUi
    suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean)
    suspend fun toTrash(vaultId: String, id: String)
    suspend fun restore(vaultId: String, id: String): DocsItemUi
    suspend fun deleteForever(vaultId: String, id: String)
    suspend fun emptyTrash(vaultId: String?)

    /**
     * Adds a document from [source] into [folder] (a name that is taken gets a number). Too much
     * for GhulDocs: [VaultErrorKind.DOCS_LIMIT]. [onProgress] gets the bytes read.
     */
    suspend fun add(vaultId: String, folder: String?, name: String, mime: String, source: ByteSource, onProgress: (Long) -> Unit = {}): DocsItemUi

    /**
     * Writes the content of a document into [sink], decrypted while it is read; every part is
     * checked before it is written. A damaged document fails with [VaultErrorKind.CORRUPTED]: what
     * reached the sink until then is incomplete and the caller must throw it away.
     */
    suspend fun read(vaultId: String, id: String, sink: ByteSink, onProgress: (Long) -> Unit = {})

    /** How much the vault [vaultId] holds and all GhulDocs vaults together (null: the numbers of all vaults). */
    suspend fun usage(vaultId: String?): DocsUsageUi

    /** Checks every document of a vault; returns those that are damaged or missing. */
    suspend fun verify(vaultId: String): List<DocsItemUi>

    /** The whole vault with its documents as a GhulDocs package (`.ghldocs`, this suite's protection) into [sink]. */
    suspend fun exportVault(vaultId: String, sink: ByteSink, onProgress: (Long, Long) -> Unit = { _, _ -> })

    /**
     * Picked files and folders ([ShareVaultUi.ids], empty: all) as a vault of their own with a new
     * protection, into [sink] as a `.ghldocs`; returns the new vault's recovery code.
     */
    suspend fun shareVault(share: ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit, sink: ByteSink, onProgress: (Long, Long) -> Unit = { _, _ -> }): String

    /**
     * Adds a GhulDocs package (opened once with its own protection) as a GhulDocs vault of its
     * own named [name], with the suite's protection; the new vault's id.
     */
    suspend fun adoptVault(
        file: SuiteFileRef,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit = { _, _ -> },
    ): String
}

/** Without GhulDocs (test fakes of other areas). */
object NoDocsBackend : DocsBackend {
    private fun none(): Nothing = throw VaultException(VaultErrorKind.NOT_CONFIGURED)
    override suspend fun list(vaultId: String, folder: String?) = emptyList<DocsItemUi>()
    override suspend fun item(vaultId: String, id: String): DocsItemUi = none()
    override suspend fun search(vaultId: String?, query: String) = emptyList<DocsItemUi>()
    override suspend fun favorites(vaultId: String?) = emptyList<DocsItemUi>()
    override suspend fun trash(vaultId: String?) = emptyList<DocsItemUi>()
    override suspend fun createFolder(vaultId: String, folder: String?, name: String): DocsItemUi = none()
    override suspend fun rename(vaultId: String, id: String, name: String): DocsItemUi = none()
    override suspend fun move(vaultId: String, id: String, folder: String?): DocsItemUi = none()
    override suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) = none()
    override suspend fun toTrash(vaultId: String, id: String) = none()
    override suspend fun restore(vaultId: String, id: String): DocsItemUi = none()
    override suspend fun deleteForever(vaultId: String, id: String) = none()
    override suspend fun emptyTrash(vaultId: String?) = none()
    override suspend fun add(vaultId: String, folder: String?, name: String, mime: String, source: ByteSource, onProgress: (Long) -> Unit): DocsItemUi = none()
    override suspend fun read(vaultId: String, id: String, sink: ByteSink, onProgress: (Long) -> Unit) = none()
    override suspend fun usage(vaultId: String?) = DocsUsageUi(0, 1_000_000_000, 0)
    override suspend fun verify(vaultId: String) = emptyList<DocsItemUi>()
    override suspend fun exportVault(vaultId: String, sink: ByteSink, onProgress: (Long, Long) -> Unit) = none()
    override suspend fun shareVault(share: ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit, sink: ByteSink, onProgress: (Long, Long) -> Unit): String = none()
    override suspend fun adoptVault(
        file: SuiteFileRef, name: String, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit,
    ): String = none()
}
