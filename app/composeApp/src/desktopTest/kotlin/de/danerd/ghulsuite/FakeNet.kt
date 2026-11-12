// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

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
import de.danerd.ghulsuite.model.NetBackend
import de.danerd.ghulsuite.model.OutgoingMailUi
import de.danerd.ghulsuite.model.OutgoingAttachmentUi
import de.danerd.ghulsuite.model.PhpModuleUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerSavedUi
import de.danerd.ghulsuite.model.ShareCheckUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.SyncReportUi
import de.danerd.ghulsuite.model.SyncStatusUi

/** Sample data for screenshots of the network screens. */
class FakeNet(private val configured: Boolean = true) : NetBackend {
    private val now = 1_762_000_000L
    private val server = ServerUi(
        "s1", "Heimserver", "nas.example.org", 22, "max", "ghulsuite", true, "SHA256:abc", "/home/max/ghulsuite",
        ServerKindUi.SFTP, "https://example.org/teilen", "public_html/teilen",
    )
    private val php = ServerUi(
        "s2", "Webspace", "", 0, "", "", false, "", "https://example.org/ghulsuite.php", ServerKindUi.PHP, "https://example.org/ghulsuite.php", "",
    )
    /** Calls the screens made, for tests. */
    val calls = mutableListOf<String>()
    private val share = ShareInfoUi("a1", "s2", "Steuer 2025.pdf", 2_400_000, now * 1000, (now + 7 * 86_400) * 1000, true, 5, 1,
        "https://example.org/ghulsuite.php#a1b2c3.k3yk3yk3y")
    private val servers = MailServersUi("posteo.de", 993, MailSecurityUi.TLS, "posteo.de", 587, MailSecurityUi.STARTTLS)

    override suspend fun servers() = if (configured) listOf(server, php) else emptyList()
    override suspend fun saveServer(input: ServerInputUi, confirmed: String?): ServerSavedUi =
        if (confirmed == null) ServerSavedUi.Confirm("SHA256:7beJP+X7anXY96H6FlO+I7u8z2xP1gJsp7GiBGawac4") else ServerSavedUi.Saved("s3")
    override suspend fun removeServer(id: String) {}
    override suspend fun installSshKey(input: ServerInputUi, hostKey: String?) = hostKey?.let { "" } ?: "SHA256:fp"
    override suspend fun sshPublicKey() = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGhulSuiteBeispielSchluessel0123456789abcdef ghulsuite"
    override suspend fun serverBrowse(serverId: String, path: String) =
        if (path.isEmpty()) listOf("ghulsuite", "public_html") else listOf("teilen", "blog")
    override fun phpModuleCreate() = PhpModuleUi("ghulsuite-1a2b3c4d.php", "<?php // module", "00".repeat(32))
    override suspend fun phpModuleUpload(viaServerId: String, dir: String, module: PhpModuleUi) { calls += "phpUpload:$viaServerId:$dir" }
    override suspend fun shareSetup(serverId: String, dir: String, publicUrl: String) = listOf(
        ShareCheckUi("write", true, ""), ShareCheckUi("https", true, ""), ShareCheckUi("listing", true, ""), ShareCheckUi("page", true, ""),
    )
    override suspend fun shareDrive(ids: List<String>, name: String?, options: ShareOptionsUi): ShareInfoUi {
        calls += "shareDrive:${ids.joinToString(",")}:${options.serverId}:${options.expiresDays}:${options.maxDownloads}:${options.password != null}"
        return share
    }
    override suspend fun shareBytes(name: String, data: ByteArray, options: ShareOptionsUi): ShareInfoUi {
        calls += "shareBytes:$name"
        return share.copy(name = name)
    }
    override suspend fun shareList() = if (configured) listOf(share, share.copy(id = "a2", serverId = "s1", name = "Fotos.zip", password = false, maxDownloads = 0, downloads = -1)) else emptyList()
    override suspend fun shareDelete(id: String) { calls += "shareDelete:$id" }
    override suspend fun shareDriveToApps(files: List<DriveItemUi>) { calls += "toApps:${files.size}" }
    override suspend fun driveFilesForMail(files: List<DriveItemUi>) =
        files.filter { !it.isDir }.map { OutgoingAttachmentUi(it.name, "application/octet-stream", ByteArray(3)) }
    override suspend fun syncStatus() = SyncStatusUi(if (configured) "s1" else null, now * 1000)
    override suspend fun setSyncServer(id: String?) {}
    override suspend fun syncNow() = SyncReportUi(1, 2, 0, 0, 0)
    override suspend fun adoptRemotePassword(password: String) = SyncReportUi(0, 0, 0, 0, 0)

    override suspend fun driveStatus() = DriveStatusUi(if (configured) "s1" else null, 3, 12_345_678)
    override suspend fun driveSetup(serverId: String) {}
    override suspend fun driveList(folder: String?, refresh: Boolean) = listOf(
        DriveItemUi("d1", null, "Fotos", true, 0, now * 1000),
        DriveItemUi("d2", null, "Verträge", true, 0, now * 1000),
        DriveItemUi("f1", null, "Steuer 2025.pdf", false, 2_400_000, now * 1000),
        DriveItemUi("f2", null, "Notizen.txt", false, 1_234, now * 1000),
    )
    override suspend fun driveSearch(query: String) = driveList(null, false).filter { it.name.contains(query, true) }
    override suspend fun driveCreateFolder(parent: String?, name: String) = "d3"
    override suspend fun driveRename(id: String, name: String) {}
    override suspend fun driveMove(id: String, folder: String?) {}
    override suspend fun driveDelete(id: String) {}
    override suspend fun driveUpload(folder: String?, source: ByteSource, onProgress: (Long) -> Unit) = "f3"
    override suspend fun driveDownload(id: String, sink: ByteSink, onProgress: (Long) -> Unit) {}

    override fun mailSuggest(email: String) = servers
    override suspend fun mailCheck(input: MailAccountInputUi) = MailCheckUi.Ok
    override suspend fun mailSaveAccount(id: String?, input: MailAccountInputUi) = "m1"
    override suspend fun mailAccounts() =
        if (configured) listOf(MailAccountUi("m1", "Privat", "Max Mustermann", "max@posteo.de", "max@posteo.de", servers, "Viele Grüße\nMax", "1A2B 3C4D 5E6F 7081 92A3 B4C5 D6E7 F809 1A2B 3C4D")) else emptyList()
    override suspend fun mailRemoveAccount(id: String) {}
    override suspend fun mailFolders(accountId: String) = listOf(
        MailFolderUi("INBOX", "inbox"), MailFolderUi("Drafts", "drafts"), MailFolderUi("Sent", "sent"), MailFolderUi("Trash", "trash"),
    )
    private val list = listOf(
        MailSummaryUi(3, "Erika Musterfrau", "erika@example.org", "Treffen am Freitag", now, false, false, false, 2_000),
        MailSummaryUi(2, "Stadtwerke", "service@stadtwerke.example", "Ihre Jahresabrechnung", now - 86_400, true, true, true, 120_000),
        MailSummaryUi(1, "Max Mustermann", "max@posteo.de", "Notiz an mich", now - 3 * 86_400, true, false, false, 900),
    )
    override suspend fun mailList(accountId: String, folder: String, offset: Int, limit: Int) = MailPageUi(3, list)
    override suspend fun mailSearch(accountId: String, folder: String, query: String) = list
    override suspend fun mailGet(accountId: String, folder: String, uid: Long) = MailMessageUi(
        uid, "Stadtwerke", "service@stadtwerke.example", listOf("Max Mustermann <max@posteo.de>"), emptyList(), "",
        "Ihre Jahresabrechnung", now - 86_400,
        "Guten Tag,\n\nanbei erhalten Sie Ihre Jahresabrechnung.\n\nMit freundlichen Grüßen\nIhre Stadtwerke",
        true, "abc@stadtwerke.example", listOf(MailAttachmentUi(0, "Abrechnung.pdf", "application/pdf", 118_000)),
        encrypted = true, signature = de.danerd.ghulsuite.model.MailSignatureUi.VALID, signer = "service@stadtwerke.example",
    )
    override suspend fun mailAttachment(accountId: String, folder: String, uid: Long, index: Int) = ByteArray(0)
    override suspend fun mailSetSeen(accountId: String, folder: String, uid: Long, seen: Boolean) {}
    override suspend fun mailSetFlagged(accountId: String, folder: String, uid: Long, flagged: Boolean) {}
    override suspend fun mailMove(accountId: String, folder: String, uid: Long, target: String) {}
    override suspend fun mailDelete(accountId: String, folder: String, uid: Long) {}
    override suspend fun mailSend(mail: OutgoingMailUi) {}
    override suspend fun mailRecipientKeys(recipients: List<String>) =
        recipients.map { de.danerd.ghulsuite.model.MailRecipientKeyUi(it, it.contains("erika"), if (it.contains("erika")) "autocrypt" else "") }
    override suspend fun mailPublicKey(accountId: String) = "-----BEGIN PGP PUBLIC KEY BLOCK-----"
    override suspend fun mailImportKey(data: ByteArray) = listOf("erika@example.org")
    override suspend fun mailKnownKeys() = listOf(
        de.danerd.ghulsuite.model.MailKeyInfoUi("erika@example.org", "3F2A 9C11 07B4 E8D2 5A61  0C9E 7D44 21AB 96F0 3E85", "autocrypt", now),
    )
    override suspend fun mailForgetKey(email: String) {}
    override fun mailNotificationsOn(accountId: String) = true
    override suspend fun setMailNotifications(accountId: String, on: Boolean) {}
}
