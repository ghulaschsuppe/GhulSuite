// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.DriveItem
import de.danerd.ghulsuite.core.GhulSuite
import de.danerd.ghulsuite.core.MailAccountInput
import de.danerd.ghulsuite.core.MailCheck
import de.danerd.ghulsuite.core.MailSecurity
import de.danerd.ghulsuite.core.MailServers
import de.danerd.ghulsuite.core.MailSummary
import de.danerd.ghulsuite.core.OutgoingAttachment
import de.danerd.ghulsuite.core.OutgoingMail
import de.danerd.ghulsuite.core.ServerInput
import de.danerd.ghulsuite.core.ServerKind
import de.danerd.ghulsuite.core.ServerSaved
import de.danerd.ghulsuite.core.PhpModule
import de.danerd.ghulsuite.core.ShareInfo
import de.danerd.ghulsuite.core.ShareOptions
import de.danerd.ghulsuite.core.phpModuleCreate as phpModuleCreateCore
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerSavedUi
import de.danerd.ghulsuite.model.PhpModuleUi
import de.danerd.ghulsuite.model.ShareCheckUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.model.OutgoingAttachmentUi
import de.danerd.ghulsuite.core.SyncReport
import de.danerd.ghulsuite.core.mailSuggestServers
import de.danerd.ghulsuite.model.ByteSink
import de.danerd.ghulsuite.model.ByteSource
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.DriveStatusUi
import de.danerd.ghulsuite.model.MailAccountInputUi
import de.danerd.ghulsuite.model.MailAccountUi
import de.danerd.ghulsuite.model.MailAttachmentUi
import de.danerd.ghulsuite.model.MailCheckUi
import de.danerd.ghulsuite.model.MailFolderUi
import de.danerd.ghulsuite.model.MailMessageUi
import de.danerd.ghulsuite.model.MailPageUi
import de.danerd.ghulsuite.model.MailSecurityUi
import de.danerd.ghulsuite.model.MailServersUi
import de.danerd.ghulsuite.model.MailSummaryUi
import de.danerd.ghulsuite.model.MailSignatureUi
import de.danerd.ghulsuite.model.MailRecipientKeyUi
import de.danerd.ghulsuite.model.MailKeyInfoUi
import de.danerd.ghulsuite.model.NetBackend
import de.danerd.ghulsuite.model.OutgoingMailUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.SyncReportUi
import de.danerd.ghulsuite.model.SyncStatusUi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

/** Size of the pieces handed to the core when uploading (the core encrypts 4 MiB blocks). */
private const val UPLOAD_CHUNK = 1024 * 1024

/**
 * Servers, sync, GhulDrive, sharing and GhulMail – the same on Android and Windows. Platforms add
 * handing files to other apps and mail notifications.
 */
abstract class CoreNetBackend(protected val suite: () -> GhulSuite) : NetBackend {
    protected suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { mapped(block) }

    /** A mail account was saved (password or servers may have changed). */
    protected open suspend fun onMailAccountSaved(id: String) {}

    /** A mail account was removed. */
    protected open fun onMailAccountRemoved(id: String) {}

    /** Writes a decrypted Drive file block by block (never the whole file in memory). */
    protected fun writeDriveFile(id: String, target: java.io.File) = mapped {
        val st = suite()
        target.outputStream().use { out ->
            for (part in 0 until st.driveFileBlocks(id).toInt()) out.write(st.driveDownloadBlock(id, part.toUInt()))
        }
    }

    // --- servers -------------------------------------------------------------------------------

    override suspend fun servers(): List<ServerUi> = io {
        suite().listServers().map {
            ServerUi(it.id, it.name, it.host, it.port.toInt(), it.user, it.base, it.usesKey, it.hostKey, it.location, it.kind.toUi(), it.shareUrl, it.shareDir)
        }
    }

    override suspend fun saveServer(input: ServerInputUi, confirmed: String?): ServerSavedUi = io {
        when (val r = suite().saveServer(input.toCore(), confirmed)) {
            is ServerSaved.Confirm -> ServerSavedUi.Confirm(r.fingerprint)
            is ServerSaved.Saved -> ServerSavedUi.Saved(r.id)
        }
    }

    override suspend fun serverBrowse(serverId: String, path: String): List<String> = io { suite().serverBrowse(serverId, path) }

    override fun phpModuleCreate(): PhpModuleUi = phpModuleCreateCore().let { PhpModuleUi(it.fileName, it.content, it.key) }

    override suspend fun phpModuleUpload(viaServerId: String, dir: String, module: PhpModuleUi) =
        io { suite().phpModuleUpload(viaServerId, dir, PhpModule(module.fileName, module.content, module.key)) }

    override suspend fun shareSetup(serverId: String, dir: String, publicUrl: String): List<ShareCheckUi> =
        io { suite().shareSetup(serverId, dir, publicUrl).map { ShareCheckUi(it.step, it.ok, it.detail) } }

    override suspend fun shareDrive(ids: List<String>, name: String?, options: ShareOptionsUi): ShareInfoUi =
        io { suite().shareDrive(ids, name, options.toCore()).toUi() }

    override suspend fun shareBytes(name: String, data: ByteArray, options: ShareOptionsUi): ShareInfoUi =
        io { suite().shareBytes(name, data, options.toCore()).toUi() }

    override suspend fun shareList(): List<ShareInfoUi> = io { suite().shareList().map { it.toUi() } }

    override suspend fun shareDelete(id: String) = io { suite().shareDelete(id) }

    private fun fileBytes(id: String): ByteArray {
        val s = suite()
        val out = java.io.ByteArrayOutputStream()
        for (part in 0 until s.driveFileBlocks(id).toInt()) out.write(s.driveDownloadBlock(id, part.toUInt()))
        return out.toByteArray()
    }

    override suspend fun driveFilesForMail(files: List<DriveItemUi>): List<OutgoingAttachmentUi> = io {
        files.filter { !it.isDir }.map { OutgoingAttachmentUi(it.name, mimeOf(it.name), fileBytes(it.id)) }
    }

    override suspend fun removeServer(id: String) = io { suite().removeServer(id) }

    override suspend fun installSshKey(input: ServerInputUi, hostKey: String?): String =
        io { suite().installSshKey(input.toCore(), hostKey) }

    override suspend fun sshPublicKey(): String = io { suite().sshPublicKey() }

    // --- sync ----------------------------------------------------------------------------------

    override suspend fun syncStatus(): SyncStatusUi = io {
        val s = suite().syncStatus()
        SyncStatusUi(s.serverId, s.lastSync)
    }

    override suspend fun setSyncServer(id: String?) = io { suite().setSyncServer(id) }

    override suspend fun syncNow(): SyncReportUi = io { suite().syncNow().toUi() }

    override suspend fun adoptRemotePassword(password: String): SyncReportUi = io { suite().adoptRemotePassword(password).toUi() }

    // --- drive ---------------------------------------------------------------------------------

    override suspend fun driveStatus(): DriveStatusUi = io {
        val s = suite().driveStatus()
        DriveStatusUi(s.serverId, s.files.toInt(), s.bytes.toLong())
    }

    override suspend fun driveSetup(serverId: String) = io { suite().driveSetup(serverId) }

    override suspend fun driveList(folder: String?, refresh: Boolean): List<DriveItemUi> =
        io { suite().driveList(folder, refresh).map { it.toUi() } }

    override suspend fun driveSearch(query: String): List<DriveItemUi> = io { suite().driveSearch(query).map { it.toUi() } }

    override suspend fun driveCreateFolder(parent: String?, name: String): String = io { suite().driveCreateFolder(parent, name) }

    override suspend fun driveRename(id: String, name: String) = io { suite().driveRename(id, name) }

    override suspend fun driveMove(id: String, folder: String?) = io { suite().driveMove(id, folder) }

    override suspend fun driveDelete(id: String) = io { suite().driveDelete(id) }

    override suspend fun driveTrash() = io { suite().driveTrash().map { it.toUi() } }
    override suspend fun driveRestore(id: String) = io { suite().driveRestore(id) }
    override suspend fun driveDeleteForever(id: String) = io { suite().driveDeleteForever(id) }
    override suspend fun driveEmptyTrash(days: Int): Int = io { suite().driveEmptyTrash(days.toUInt()).toInt() }
    override suspend fun driveSetFavorite(id: String, favorite: Boolean) = io { suite().driveSetFavorite(id, favorite) }
    override suspend fun driveFavorites() = io { suite().driveFavorites().map { it.toUi() } }
    override suspend fun driveRecent(limit: Int) = io { suite().driveRecent(limit.toUInt()).map { it.toUi() } }
    override suspend fun driveOfflineFiles() = io { suite().driveOfflineFiles().map { it.toUi() } }
    override suspend fun driveSetOffline(id: String, offline: Boolean) = io { suite().driveSetOffline(id, offline) }
    override suspend fun driveVersions(id: String) = io {
        suite().driveVersions(id).map { de.danerd.ghulsuite.model.DriveVersionUi(it.index.toInt(), it.size.toLong(), it.modified) }
    }
    override suspend fun driveRestoreVersion(id: String, index: Int) = io { suite().driveRestoreVersion(id, index.toUInt()) }
    override suspend fun driveUsage() = io {
        val u = suite().driveUsage()
        de.danerd.ghulsuite.model.DriveUsageUi(u.images.toLong(), u.videos.toLong(), u.audio.toLong(), u.documents.toLong(), u.other.toLong(), u.trash.toLong(), u.versions.toLong())
    }
    override suspend fun driveZip(ids: List<String>, password: String): ByteArray = io { suite().driveZip(ids, password) }

    override suspend fun driveUpload(folder: String?, source: ByteSource, onProgress: (Long) -> Unit): String =
        withContext(Dispatchers.IO) {
            val s = suite()
            val upload = mapped { s.driveUploadBegin(folder, source.name) }
            try {
                val buffer = ByteArray(UPLOAD_CHUNK)
                var done = 0L
                while (true) {
                    coroutineContext.ensureActive()
                    var filled = 0
                    while (filled < buffer.size) {
                        val n = source.read(buffer, filled, buffer.size - filled)
                        if (n < 0) break
                        filled += n
                    }
                    if (filled <= 0) break
                    mapped { s.driveUploadChunk(upload, buffer.copyOf(filled)) }
                    done += filled
                    withContext(Dispatchers.Main) { onProgress(done) }
                }
                mapped { s.driveUploadFinish(upload) }
            } catch (e: kotlinx.coroutines.CancellationException) {
                // Cancelled by the user: remove what was uploaded.
                runCatching { s.driveUploadCancel(upload) }
                throw e
            } catch (e: Throwable) {
                // Network trouble: blocks already on the server stay; uploading the same file
                // again skips them and continues where it stopped.
                throw e
            } finally {
                source.close()
            }
        }

    override suspend fun driveDownload(id: String, sink: ByteSink, onProgress: (Long) -> Unit) = withContext(Dispatchers.IO) {
        val s = suite()
        try {
            val parts = mapped { s.driveFileBlocks(id) }.toInt()
            var done = 0L
            for (part in 0 until parts) {
                coroutineContext.ensureActive()
                val data = mapped { s.driveDownloadBlock(id, part.toUInt()) }
                sink.write(data)
                done += data.size
                withContext(Dispatchers.Main) { onProgress(done) }
            }
        } finally {
            sink.close()
        }
    }

    // --- mail ----------------------------------------------------------------------------------

    override fun mailSuggest(email: String): MailServersUi = mailSuggestServers(email).toUi()

    override suspend fun mailCheck(input: MailAccountInputUi): MailCheckUi = io {
        when (val c = suite().mailCheckAccount(input.toCore())) {
            is MailCheck.Ok -> MailCheckUi.Ok
            is MailCheck.ImapCertificate -> MailCheckUi.ImapCertificate(c.fingerprint)
            is MailCheck.SmtpCertificate -> MailCheckUi.SmtpCertificate(c.fingerprint)
        }
    }

    override suspend fun mailSaveAccount(id: String?, input: MailAccountInputUi): String {
        val newId = io { suite().mailSaveAccount(id, input.toCore()) }
        onMailAccountSaved(newId)
        return newId
    }

    override suspend fun mailAccounts(): List<MailAccountUi> = io {
        suite().mailAccounts().map {
            MailAccountUi(it.id, it.name, it.displayName, it.email, it.username, it.servers.toUi(), it.signature, it.pgpFingerprint)
        }
    }

    override suspend fun mailRemoveAccount(id: String) {
        io { suite().mailRemoveAccount(id) }
        onMailAccountRemoved(id)
    }

    override suspend fun mailFolders(accountId: String): List<MailFolderUi> =
        io { suite().mailFolders(accountId).map { MailFolderUi(it.name, it.role) } }

    override suspend fun mailList(accountId: String, folder: String, offset: Int, limit: Int): MailPageUi = io {
        val p = suite().mailList(accountId, folder, offset.toUInt(), limit.toUInt())
        MailPageUi(p.total.toInt(), p.messages.map { it.toUi() })
    }

    override suspend fun mailSearch(accountId: String, folder: String, query: String): List<MailSummaryUi> =
        io { suite().mailSearch(accountId, folder, query).map { it.toUi() } }

    override suspend fun mailGet(accountId: String, folder: String, uid: Long): MailMessageUi = io {
        val m = suite().mailGet(accountId, folder, uid.toUInt())
        MailMessageUi(
            uid = m.uid.toLong(), from = m.from, fromAddress = m.fromAddress, to = m.to, cc = m.cc, replyTo = m.replyTo,
            subject = m.subject, date = m.date, text = m.text, wasHtml = m.wasHtml, messageId = m.messageId,
            attachments = m.attachments.map { MailAttachmentUi(it.index.toInt(), it.name, it.mime, it.size.toLong()) },
            encrypted = m.encrypted,
            signature = when (m.signature) {
                de.danerd.ghulsuite.core.MailSignatureStatus.NONE -> MailSignatureUi.NONE
                de.danerd.ghulsuite.core.MailSignatureStatus.VALID -> MailSignatureUi.VALID
                de.danerd.ghulsuite.core.MailSignatureStatus.UNKNOWN_KEY -> MailSignatureUi.UNKNOWN_KEY
                de.danerd.ghulsuite.core.MailSignatureStatus.INVALID -> MailSignatureUi.INVALID
            },
            signer = m.signer,
            blocks = m.blocks.map { b ->
                de.danerd.ghulsuite.model.MailBlockUi(
                    de.danerd.ghulsuite.model.MailBlockKindUi.valueOf(b.kind.name),
                    b.level.toInt(),
                    b.spans.map { de.danerd.ghulsuite.model.MailSpanUi(it.text, it.bold, it.italic, it.link) },
                    b.imageSrc, b.imageAlt,
                )
            },
            remoteImages = m.remoteImages.toInt(),
            inlineImages = m.inlineImages.associate { it.cid to it.data },
        )
    }

    override suspend fun mailFetchImage(url: String): ByteArray = io { de.danerd.ghulsuite.core.mailFetchImage(url) }

    override suspend fun mailAttachment(accountId: String, folder: String, uid: Long, index: Int): ByteArray =
        io { suite().mailAttachment(accountId, folder, uid.toUInt(), index.toUInt()) }

    override suspend fun mailSetSeen(accountId: String, folder: String, uid: Long, seen: Boolean) =
        io { suite().mailSetSeen(accountId, folder, uid.toUInt(), seen) }

    override suspend fun mailSetFlagged(accountId: String, folder: String, uid: Long, flagged: Boolean) =
        io { suite().mailSetFlagged(accountId, folder, uid.toUInt(), flagged) }

    override suspend fun mailMove(accountId: String, folder: String, uid: Long, target: String) =
        io { suite().mailMove(accountId, folder, uid.toUInt(), target) }

    override suspend fun mailDelete(accountId: String, folder: String, uid: Long) =
        io { suite().mailDelete(accountId, folder, uid.toUInt()) }

    override suspend fun mailSend(mail: OutgoingMailUi) = io {
        suite().mailSend(
            OutgoingMail(
                accountId = mail.accountId, to = mail.to, cc = mail.cc, bcc = mail.bcc, subject = mail.subject,
                text = mail.text, inReplyTo = mail.inReplyTo,
                attachments = mail.attachments.map { OutgoingAttachment(it.name, it.mime, it.data) },
            ),
        )
    }

    private fun OutgoingMailUi.toCore() = OutgoingMail(
        accountId = accountId, to = to, cc = cc, bcc = bcc, subject = subject, text = text, inReplyTo = inReplyTo,
        attachments = attachments.map { OutgoingAttachment(it.name, it.mime, it.data) },
    )

    override suspend fun mailFlagged(accountId: String, folder: String) = io { suite().mailFlagged(accountId, folder).map { it.toUi() } }
    override suspend fun mailSetLabel(accountId: String, folder: String, uid: Long, label: String, on: Boolean) =
        io { suite().mailSetLabel(accountId, folder, uid.toUInt(), label, on) }
    override suspend fun mailRaw(accountId: String, folder: String, uid: Long, decrypted: Boolean): ByteArray =
        io { suite().mailRaw(accountId, folder, uid.toUInt(), decrypted) }
    override suspend fun mailHeaders(accountId: String, folder: String, uid: Long): String = io { suite().mailHeaders(accountId, folder, uid.toUInt()) }
    override suspend fun mailSaveDraft(draft: de.danerd.ghulsuite.model.MailDraftUi): String = io {
        suite().mailSaveDraft(
            de.danerd.ghulsuite.core.MailDraftData(
                draft.id, draft.accountId, draft.to, draft.cc, draft.bcc, draft.subject, draft.text, draft.inReplyTo, 0,
                draft.attachments.map { OutgoingAttachment(it.name, it.mime, it.data) },
            ),
        )
    }
    override suspend fun mailDrafts() = io {
        suite().mailDrafts().map {
            de.danerd.ghulsuite.model.MailDraftUi(
                it.id, it.accountId, it.to, it.cc, it.bcc, it.subject, it.text, it.inReplyTo, it.updated,
                it.attachments.map { a -> de.danerd.ghulsuite.model.OutgoingAttachmentUi(a.name, a.mime, a.data) },
            )
        }
    }
    override suspend fun mailDeleteDraft(id: String) = io { suite().mailDeleteDraft(id) }
    override suspend fun mailSchedule(mail: OutgoingMailUi, sendAt: Long): String = io { suite().mailSchedule(mail.toCore(), sendAt) }
    override suspend fun mailScheduled() = io {
        suite().mailScheduled().map { de.danerd.ghulsuite.model.ScheduledMailUi(it.id, it.accountId, it.to, it.subject, it.sendAt) }
    }
    override suspend fun mailCancelScheduled(id: String) = io { suite().mailCancelScheduled(id) }
    override suspend fun mailSendDue(now: Long): Int = io { suite().mailSendDue(now).toInt() }
    override suspend fun mailContacts(query: String) = io { suite().mailContacts(query, 6u).map { de.danerd.ghulsuite.model.MailContactUi(it.name, it.email) } }
    override suspend fun setMailOffline(on: Boolean) = io { suite().mailSetOffline(on) }
    override fun mailOffline(): Boolean = runCatching { suite().mailOffline() }.getOrDefault(false)
    override suspend fun driveSaveBytes(folderName: String, name: String, data: ByteArray) {
        val folder = driveList(null, true).firstOrNull { it.isDir && it.name == folderName }?.id ?: driveCreateFolder(null, folderName)
        var pos = 0
        val source = object : ByteSource {
            override val name = name
            override val size = data.size.toLong()
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                if (pos >= data.size) return -1
                val n = minOf(length, data.size - pos)
                data.copyInto(buffer, offset, pos, pos + n)
                pos += n
                return n
            }
            override fun close() {}
        }
        driveUpload(folder, source) {}
    }

    override suspend fun mailRecipientKeys(recipients: List<String>): List<MailRecipientKeyUi> =
        io { suite().mailRecipientKeys(recipients).map { MailRecipientKeyUi(it.address, it.hasKey, it.source) } }

    override suspend fun mailPublicKey(accountId: String): String = io { suite().mailPublicKey(accountId) }

    override suspend fun mailImportKey(data: ByteArray): List<String> = io { suite().mailImportKey(data) }

    override suspend fun mailKnownKeys(): List<MailKeyInfoUi> =
        io { suite().mailKnownKeys().map { MailKeyInfoUi(it.email, it.fingerprint, it.source, it.updated) } }

    override suspend fun mailForgetKey(email: String) = io { suite().mailForgetKey(email) }
}
