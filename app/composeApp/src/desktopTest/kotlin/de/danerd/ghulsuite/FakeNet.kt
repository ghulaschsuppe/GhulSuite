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
import de.danerd.ghulsuite.model.ModuleCheckUi
import de.danerd.ghulsuite.model.ModuleInstalledUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ServerSavedUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.model.ServerUi
import de.danerd.ghulsuite.model.SyncReportUi
import de.danerd.ghulsuite.model.SyncStatusUi

/** Sample data for screenshots of the network screens. */
class FakeNet(private val configured: Boolean = true, private val encryptedMail: Boolean = false, private val conversation: Boolean = false) : NetBackend {
    private val now = 1_762_000_000L
    /** The sync and GhulDrive server, with an older server module (it can be updated). */
    private val server = ServerUi(
        "s1", "Heimserver", "nas.example.org", 22, "max", "ghulsuite", true, "SHA256:abc", "/home/max/ghulsuite",
        ServerKindUi.SFTP, "https://nas.example.org/ghulsuite.php", 1, 2,
    )
    /** A web hosting over FTPS without the server module. */
    private val web = ServerUi(
        "s2", "Webspace", "example.org", 21, "web123", "ghulsuite", false, "SHA256:def", "/ghulsuite", ServerKindUi.FTPS, "", 0, 2,
    )
    /** Calls the screens made, for tests. */
    val calls: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf<String>())
    private val share = ShareInfoUi("a1", "s2", "Steuer 2025.pdf", 2_400_000, now * 1000, (now + 7 * 86_400) * 1000, true, 5, 1,
        "https://example.org/ghulsuite.php#a1b2c3.k3yk3yk3y")
    private val servers = MailServersUi("posteo.de", 993, MailSecurityUi.TLS, "posteo.de", 587, MailSecurityUi.STARTTLS)

    /** The next calls of servers() that fail (a core that is busy or gone). */
    var serversFailures = 0
    override suspend fun servers(): List<ServerUi> {
        if (serversFailures > 0) { serversFailures--; throw java.io.IOException("connection reset") }
        return if (configured) listOf(server, web) else emptyList()
    }
    override suspend fun saveServer(input: ServerInputUi, confirmed: String?): ServerSavedUi =
        if (confirmed == null) ServerSavedUi.Confirm("SHA256:7beJP+X7anXY96H6FlO+I7u8z2xP1gJsp7GiBGawac4") else ServerSavedUi.Saved("s3")
    override suspend fun removeServer(id: String) {}
    override suspend fun installSshKey(input: ServerInputUi, hostKey: String?) = hostKey?.let { "" } ?: "SHA256:fp"
    override suspend fun sshPublicKey() = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGhulSuiteBeispielSchluessel0123456789abcdef ghulsuite"
    /** How long listing a folder takes (to tap on before it is done). */
    var browseDelayMs = 0L
    @Volatile var slowBrowsesDone = 0
    override suspend fun serverBrowse(serverId: String, path: String): List<String> {
        // Like the real backend: a blocking call on another thread, which is not interrupted by a
        // cancellation; the cancellation arrives only when it is done.
        val wait = browseDelayMs
        if (wait > 0) kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { Thread.sleep(wait); slowBrowsesDone++ }
        return if (path.isEmpty()) listOf("ghulsuite", "public_html") else listOf("teilen", "blog")
    }
    /** The module answers at the usual address (else only at an address typed by the user). */
    var moduleAnswersWithoutAddress = false
    override suspend fun serverModuleInstall(serverId: String, dir: String, url: String): ModuleInstalledUi {
        calls += "moduleInstall:$serverId:$dir:$url"
        val tried = if (url.isEmpty()) listOf("https://example.org/ghulsuite.php", "https://www.example.org/ghulsuite.php") else listOf(url)
        val upload = ModuleCheckUi("upload", true, (if (dir.isEmpty()) "" else "$dir/") + "ghulsuite.php")
        if (url.isEmpty() && !moduleAnswersWithoutAddress) {
            return ModuleInstalledUi(null, tried, listOf(upload, ModuleCheckUi("https", false, tried[0]), ModuleCheckUi("folder", false, "skipped")))
        }
        val https = ModuleCheckUi("https", true, tried[0])
        return when (moduleFailure) {
            "no-write" -> ModuleInstalledUi(null, tried, listOf(upload, https, ModuleCheckUi("folder", false, "no-write")))
            "private" -> ModuleInstalledUi(null, tried, listOf(upload, https, ModuleCheckUi("folder", true, ""),
                ModuleCheckUi("private", false, "https://example.org/ghulsuite/shares/check-1")))
            else -> ModuleInstalledUi(tried[0], tried, listOf(upload, https, ModuleCheckUi("folder", true, ""), ModuleCheckUi("private", true, "")))
        }
    }
    /** How the module installation fails after HTTPS: "no-write", "private" or null (it works). */
    var moduleFailure: String? = null
    override suspend fun serverModuleUpdate(serverId: String): Int { calls += "moduleUpdate:$serverId"; return 2 }
    override suspend fun serverModuleRemove(serverId: String) { calls += "moduleRemove:$serverId" }
    /** What share links still need (null: nothing; "": GhulDrive; else the server for the module). */
    var missingModule: String? = if (configured) null else ""
    override suspend fun shareMissingModule() = missingModule
    /** The links containing GhulDrive items: "Steuer 2025.pdf" (f1) is shared. */
    override suspend fun shareLinksOf(ids: List<String>) = if (configured && "f1" in ids) listOf(share) else emptyList()
    override suspend fun shareDrive(ids: List<String>, name: String?, options: ShareOptionsUi): ShareInfoUi {
        calls += "shareDrive:${ids.joinToString(",")}:${name ?: ""}:${options.expiresDays}:${options.maxDownloads}:${options.password != null}"
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
    /** The sync server as set through the screens (the fake starts with s1 when configured). */
    var syncServer: String? = if (configured) "s1" else null
    /** While true, syncStatus() fails (a core that cannot answer). */
    var syncStatusFails = false
    override suspend fun syncStatus(): SyncStatusUi {
        if (syncStatusFails) throw java.io.IOException("timeout")
        return SyncStatusUi(syncServer, now * 1000)
    }
    override suspend fun setSyncServer(id: String?) {
        calls += "setSyncServer:$id"
        syncServer = id
    }
    override suspend fun syncNow() = SyncReportUi(1, 2, 0, 0, 0)
    override suspend fun adoptRemotePassword(password: String) = SyncReportUi(0, 0, 0, 0, 0)

    /** GhulDrive's storage; tests can give Drive a server of its own (not in the suite's list). */
    var driveOwnServer: ServerUi? = null
    override suspend fun driveStorage() = when {
        driveOwnServer != null -> de.danerd.ghulsuite.model.DriveStorageUi(de.danerd.ghulsuite.model.DriveModeUi.OWN, driveOwnServer, syncServer != null)
        configured && syncServer != null -> de.danerd.ghulsuite.model.DriveStorageUi(de.danerd.ghulsuite.model.DriveModeUi.SYNC, server, true)
        else -> de.danerd.ghulsuite.model.DriveStorageUi(de.danerd.ghulsuite.model.DriveModeUi.NONE, null, false)
    }
    /** Holds driveStatus() back (Drive's index still coming from a slow server) until released. */
    var driveStatusGate: java.util.concurrent.CountDownLatch? = null
    /** Files in GhulDrive (as counted by the core). */
    var driveFiles = 3
    /** GhulDrive is not set up (while the rest, e.g. mail, is). */
    var driveMissing = false
    override suspend fun driveStatus(): DriveStatusUi {
        driveStatusGate?.let { gate -> kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { gate.await(30, java.util.concurrent.TimeUnit.SECONDS) } }
        return DriveStatusUi(if (driveMissing) null else driveOwnServer?.id ?: if (configured) "s1" else null, driveFiles, 12_345_678)
    }
    override suspend fun driveSetup(serverId: String) {}
    /** How long listing a folder takes (a slow server); blocking, as the real core does. */
    var driveListDelayMs = 0L
    /** The server cannot be reached: fetching the index fails, the index already known stays. */
    var driveOffline = false
    override suspend fun driveList(folder: String?, refresh: Boolean): List<DriveItemUi> {
        archiveFiles[folder]?.let { return it.filter { f -> f.id !in driveTrashed } }
        if (driveOffline && refresh) throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.NETWORK, "Timed out.")
        if (folder != null && driveListDelayMs > 0) kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { Thread.sleep(driveListDelayMs) }
        return (if (folder == "d1") listOf(DriveItemUi("f5", "d1", "Strand.jpg", false, 3_100_000, now * 1000)) else listOf(
            DriveItemUi("d1", null, "Fotos", true, 0, now * 1000),
            DriveItemUi("d2", null, "Verträge", true, 0, now * 1000),
            DriveItemUi("f1", null, "Steuer 2025.pdf", false, 2_400_000, now * 1000),
            DriveItemUi("f2", null, "Notizen.txt", false, 1_234, now * 1000),
        )).filter { it.id !in driveTrashed }
    }
    /** Items moved to the trash (left out of the folders). */
    val driveTrashed: MutableSet<String> = java.util.Collections.synchronizedSet(mutableSetOf<String>())
    override suspend fun driveSearch(query: String) = driveList(null, false).filter { it.name.contains(query, true) }
    override suspend fun driveCreateFolder(parent: String?, name: String) = "d3"
    override suspend fun driveRename(id: String, name: String) {}
    override suspend fun driveMove(id: String, folder: String?) {}
    override suspend fun driveRestore(id: String) { calls += "driveRestore:$id"; driveTrashed -= id; if (id.startsWith("f")) driveFiles++ }
    /** Moving this item to the trash fails (the server went away in the middle). */
    var driveDeleteFails: String? = null
    override suspend fun driveDelete(id: String) {
        if (id == driveDeleteFails) throw java.io.IOException("connection reset")
        calls += "driveDelete:$id"; driveTrashed += id; if (id.startsWith("f")) driveFiles--
    }
    override suspend fun driveTrash() = listOf(DriveItemUi("f9", null, "Alter Entwurf.docx", false, 48_000, now * 1000, trashed = now * 1000))
    override suspend fun driveDeleteForever(id: String) { calls += "driveDeleteForever:$id" }
    override suspend fun driveEmptyTrash(days: Int): Int { if (days == 0) calls += "driveEmptyTrash"; return 0 }
    /** How long each upload takes (to see the bar); the names that fail. */
    var driveUploadMs = 0L
    var driveUploadFails: Set<String> = emptySet()
    override suspend fun driveUpload(folder: String?, source: ByteSource, onProgress: (Long) -> Unit): String {
        if (driveUploadMs > 0) kotlinx.coroutines.delay(driveUploadMs)
        if (source.name in driveUploadFails) throw java.io.IOException("connection reset")
        val buf = ByteArray(1024)
        var n = 0L
        while (true) { val r = source.read(buf, 0, buf.size); if (r < 0) break; n += r; onProgress(n) }
        calls += "driveUpload:${folder ?: ""}:${source.name}"
        driveFiles++
        return "f3"
    }
    /** Photos for the photo backup (name and the second it was added), and how long each takes. */
    var photos: List<Pair<String, Long>> = emptyList()
    var photoDelayMs = 0L
    override val photoBackupAvailable: Boolean get() = photos.isNotEmpty()
    /** Like the Android backend: oldest first, the mark of each photo also when cancelled during it. */
    override suspend fun photoBackup(sinceSeconds: Long, wifiOnly: Boolean, onFile: (String, Long, Long?) -> Unit): Long {
        val todo = photos.filter { it.second > sinceSeconds }.sortedBy { it.second }
        var mark = sinceSeconds
        for ((i, p) in todo.withIndex()) {
            if (photoDelayMs > 0) kotlinx.coroutines.delay(photoDelayMs)
            calls += "photo:${p.first}"
            val next = todo.getOrNull(i + 1)?.second
            mark = maxOf(mark, de.danerd.ghulsuite.ui.photoMark(p.second, next))
            kotlinx.coroutines.withContext(kotlinx.coroutines.NonCancellable) { onFile(p.first, p.second, next) }
        }
        return mark
    }

    override suspend fun driveDownload(id: String, sink: ByteSink, onProgress: (Long) -> Unit) {
        calls += "driveDownload:$id"
        archiveContent[id]?.let { sink.write(it.encodeToByteArray()); onProgress(it.length.toLong()) }
        sink.close()
    }

    /**
     * The mail archive in GhulDrive as the core lays it out: year folders below the account's
     * folder, .eml files named "date sender - subject (code).eml", plus a note and a folder of the
     * user's own, and one file that is no mail at all.
     */
    private val archiveFiles: Map<String?, List<DriveItemUi>> = mapOf(
        "archive-m1" to listOf(
            DriveItemUi("y2025", "archive-m1", "2025", true, 0, now * 1000),
            DriveItemUi("y2024", "archive-m1", "2024", true, 0, now * 1000),
            DriveItemUi("own", "archive-m1", "Steuern", true, 0, now * 1000),
        ),
        "y2025" to listOf(
            DriveItemUi("e1", "y2025", "2025-10-30 Finanzamt - Steuerbescheid 2024 (1a2b3c4d5e6f7a8b).eml", false, 3_200, now * 1000),
            DriveItemUi("e2", "y2025", "2025-03-01 Erika Musterfrau - Urlaub im Mai (0f1e2d3c4b5a6978).eml", false, 1_800, now * 1000),
            DriveItemUi("e3", "y2025", "kaputt.eml", false, 12, now * 1000),
            DriveItemUi("n1", "y2025", "Notizen zum Archiv.txt", false, 300, now * 1000),
        ),
        "y2024" to listOf(
            DriveItemUi("e4", "y2024", "2024-12-24 Stadtwerke - Ihre Jahresabrechnung (aa11bb22cc33dd44).eml", false, 9_000, now * 1000),
        ),
        "own" to emptyList(),
    )
    private val archiveContent = mapOf(
        "e1" to "From: Finanzamt <post@finanzamt.example>\r\nSubject: Steuerbescheid 2024\r\n\r\nAnbei Ihr Bescheid.",
        "e2" to "From: Erika <erika@example.org>\r\nSubject: Urlaub im Mai\r\n\r\nHallo!",
        "e3" to "broken",
        "e4" to "From: Stadtwerke <service@stadtwerke.example>\r\nSubject: Ihre Jahresabrechnung\r\n\r\nGuten Tag",
    )

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
        MailSummaryUi(3, "Erika Musterfrau", "erika@example.org", "Treffen am Freitag", now, false, false, false, 2_000,
            threadKey = if (conversation) "treffen" else ""),
        MailSummaryUi(2, "Stadtwerke", "service@stadtwerke.example", "Ihre Jahresabrechnung", now - 86_400, true, true, true, 120_000),
        MailSummaryUi(1, "Max Mustermann", "max@posteo.de", "Notiz an mich", now - 3 * 86_400, true, false, false, 900),
    ) + (if (conversation) {
        // An earlier mail of the same conversation: the list shows one row with "(2)".
        listOf(MailSummaryUi(5, "Erika Musterfrau", "erika@example.org", "Re: Treffen am Freitag", now - 600, true, false, false, 1_500, threadKey = "treffen"))
    } else emptyList()) + if (encryptedMail) {
        // Encrypted with the subject inside (outer subject "..."), as Thunderbird and Proton send it.
        listOf(MailSummaryUi(4, "Jana Beispiel", "jana@example.net", "...", now - 3_600, false, false, false, 5_000, encrypted = true))
    } else emptyList()
    override suspend fun mailList(accountId: String, folder: String, offset: Int, limit: Int) = MailPageUi(list.size, list)

    // --- Mail archive in GhulDrive: the mails of [list] copied as .eml files, kept in memory.
    /** "account/folder/uid" of every mail copied into GhulDrive. */
    val archivedToDrive: MutableSet<String> = java.util.Collections.synchronizedSet(mutableSetOf<String>())
    var archiveAutoDaysValue = 0
    /** Per "account/folder": UIDVALIDITY and the highest UID an automatic run handled. */
    val archiveAutoMarks = mutableMapOf<String, Pair<Long, Long>>()
    private fun archiveCandidates(beforeDays: Long?) =
        list.filter { beforeDays == null || it.date < beforeDays * 86_400 }.sortedBy { it.uid }.map { it.uid }
    /** The archive exists in GhulDrive already (else only after the first mail was archived). */
    var archiveExists = false
    /** How long copying one mail takes (to see the progress and to cancel). */
    var archiveDelayMs = 0L
    override suspend fun archiveFolder(accountId: String?): String? =
        if (archivedToDrive.isEmpty() && !archiveExists) null else if (accountId == null) "archive" else "archive-$accountId"
    override suspend fun archiveUids(accountId: String, folder: String, beforeDays: Long?): List<Long> {
        calls += "archiveUids:$accountId:$folder:${beforeDays ?: ""}"
        return archiveCandidates(beforeDays)
    }
    override suspend fun archiveToDrive(accountId: String, folder: String, uid: Long): de.danerd.ghulsuite.model.MailArchivedUi {
        if (driveMissing) throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.NOT_CONFIGURED, "")
        if (archiveDelayMs > 0) kotlinx.coroutines.delay(archiveDelayMs)
        calls += "archiveToDrive:$accountId:$folder:$uid"
        return when {
            list.none { it.uid == uid } -> de.danerd.ghulsuite.model.MailArchivedUi.GONE
            !archivedToDrive.add("$accountId/$folder/$uid") -> de.danerd.ghulsuite.model.MailArchivedUi.SKIPPED
            else -> de.danerd.ghulsuite.model.MailArchivedUi.SAVED
        }
    }
    /** How long asking for the automatic archive's days takes (two quick unlocks while it is asked). */
    var archiveAutoDaysDelayMs = 0L
    override suspend fun archiveAutoDays(): Int {
        if (archiveAutoDaysDelayMs > 0) kotlinx.coroutines.delay(archiveAutoDaysDelayMs)
        return archiveAutoDaysValue
    }
    override suspend fun archiveSetAutoDays(days: Int) { calls += "archiveSetAutoDays:$days"; archiveAutoDaysValue = days }
    override suspend fun archiveAutoUids(accountId: String, folder: String): de.danerd.ghulsuite.model.MailArchiveBatchUi {
        calls += "archiveAutoUids:$accountId:$folder"
        if (archiveAutoDaysValue == 0) return de.danerd.ghulsuite.model.MailArchiveBatchUi(0, emptyList())
        val after = archiveAutoMarks["$accountId/$folder"]?.takeIf { it.first == 1L }?.second ?: 0L
        val uids = archiveCandidates(now / 86_400 - archiveAutoDaysValue).filter { it > after }
        return de.danerd.ghulsuite.model.MailArchiveBatchUi(1, uids)
    }
    override suspend fun archiveAutoDone(accountId: String, folder: String, validity: Long, lastUid: Long) {
        calls += "archiveAutoDone:$accountId:$folder:$validity:$lastUid"
        archiveAutoMarks["$accountId/$folder"] = validity to lastUid
    }
    override suspend fun mailParseEml(accountId: String, data: ByteArray): MailMessageUi {
        calls += "mailParseEml:$accountId"
        val text = data.decodeToString()
        if (!text.contains("Subject:")) throw de.danerd.ghulsuite.model.VaultException(de.danerd.ghulsuite.model.VaultErrorKind.INVALID_INPUT, "not a mail")
        val subject = text.lineSequence().firstOrNull { it.startsWith("Subject:", true) }?.substringAfter(':')?.trim().orEmpty()
        return MailMessageUi(
            0, "Finanzamt", "post@finanzamt.example", listOf("Max Mustermann <max@posteo.de>"), emptyList(), "",
            subject, now - 400 * 86_400, text.substringAfter("\r\n\r\n", text), false, "bescheid@finanzamt.example",
            listOf(MailAttachmentUi(0, "Bescheid.pdf", "application/pdf", 280_000)),
        )
    }
    override suspend fun mailEmlAttachment(accountId: String, data: ByteArray, index: Int): ByteArray {
        calls += "mailEmlAttachment:$accountId:$index"
        return "%PDF-1.4".encodeToByteArray()
    }
    /** While true, the mail search fails (the server does not answer). */
    var mailSearchFails = false
    override suspend fun mailSearch(accountId: String, folder: String, query: String): List<MailSummaryUi> {
        calls += "mailSearch:$query"
        if (mailSearchFails) throw java.io.IOException("connection reset")
        return list.filter { it.subject.contains(query, true) || it.from.contains(query, true) }
    }
    override suspend fun mailGet(accountId: String, folder: String, uid: Long) = if (uid == 9L) htmlMail() else MailMessageUi(
        uid, "Stadtwerke", "service@stadtwerke.example", listOf("Max Mustermann <max@posteo.de>"), emptyList(), "",
        "Ihre Jahresabrechnung", now - 86_400,
        "Guten Tag,\n\nanbei erhalten Sie Ihre Jahresabrechnung.\n\nMit freundlichen Grüßen\nIhre Stadtwerke",
        true, "abc@stadtwerke.example", listOf(MailAttachmentUi(0, "Abrechnung.pdf", "application/pdf", 118_000)),
        encrypted = true, signature = de.danerd.ghulsuite.model.MailSignatureUi.VALID, signer = "service@stadtwerke.example",
    )
    private fun htmlMail(): MailMessageUi {
        fun span(t: String, b: Boolean = false, i: Boolean = false, l: String = "") = de.danerd.ghulsuite.model.MailSpanUi(t, b, i, l)
        fun block(k: de.danerd.ghulsuite.model.MailBlockKindUi, vararg s: de.danerd.ghulsuite.model.MailSpanUi, level: Int = 0, src: String = "", alt: String = "") =
            de.danerd.ghulsuite.model.MailBlockUi(k, level, s.toList(), src, alt)

        return MailMessageUi(
            9, "Radverein Musterstadt", "info@radverein.example", listOf("Max Mustermann <max@posteo.de>"), emptyList(), "",
            "Newsletter November", now - 3_600, "Newsletter November …", true, "nl@radverein.example", emptyList(),
            blocks = listOf(
                block(de.danerd.ghulsuite.model.MailBlockKindUi.HEADING, span("Herbsttour am Samstag"), level = 1),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.PARAGRAPH, span("Wir starten um "), span("10 Uhr", b = true), span(" am Marktplatz. Bitte denkt an "), span("Licht und Helm", i = true), span(".")),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.LIST_ITEM, span("Strecke: 42 km"), level = 1),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.LIST_ITEM, span("Einkehr im Gasthaus Linde"), level = 1),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.QUOTE, span("„Der Weg ist das Ziel.“"), level = 1),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.PARAGRAPH, span("Zur Anmeldung", l = "https://radverein.example/anmeldung"), span(" · "), span("Abmelden", l = "https://radverein.example/abmelden")),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.IMAGE, src = "https://radverein.example/banner.jpg", alt = "Tourkarte"),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.RULE),
                block(de.danerd.ghulsuite.model.MailBlockKindUi.PARAGRAPH, span("Radverein Musterstadt e. V.", i = true)),
            ),
            remoteImages = 1,
        )
    }

    override suspend fun mailAttachment(accountId: String, folder: String, uid: Long, index: Int) = ByteArray(0)
    override suspend fun mailSetSeen(accountId: String, folder: String, uid: Long, seen: Boolean) {}
    override suspend fun mailSetFlagged(accountId: String, folder: String, uid: Long, flagged: Boolean) {}
    override suspend fun mailMove(accountId: String, folder: String, uid: Long, target: String) {}
    override suspend fun mailDelete(accountId: String, folder: String, uid: Long) { calls += "mailDelete:$accountId:$folder:$uid" }
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

/** A file to upload in the tests: [size] bytes, [closed] notes when it was closed. */
fun fakeSource(name: String, size: Int = 3000, closed: MutableList<String>? = null) = object : ByteSource {
    var left = size
    override val name = name
    override val size = size.toLong()
    override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
        if (left <= 0) return -1
        val n = minOf(length, left); left -= n; return n
    }
    override fun close() { closed?.add(name) }
}
