// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

/**
 * Network features of the open suite: own servers (SFTP), synchronisation, GhulDrive and GhulMail.
 * All calls need an unlocked suite and run on a background thread.
 */
interface NetBackend {
    // --- Servers
    suspend fun servers(): List<ServerUi>
    /**
     * Checks and stores a server. Without [hostKey] it returns the server's key fingerprint for the
     * user to compare (nothing is stored); with the confirmed fingerprint it returns the server id.
     */
    suspend fun saveServer(input: ServerInputUi, confirmed: String?): ServerSavedUi
    /**
     * "Check connection": logs in and checks that the folder can be written, stores nothing. On
     * first contact it returns the fingerprint to confirm (then check again with it).
     */
    suspend fun serverTest(input: ServerInputUi, confirmed: String?): ServerTestUi
    suspend fun removeServer(id: String)
    /**
     * Logs in once with the password in [input] and adds the suite's SSH key to the server's
     * `~/.ssh/authorized_keys`. The password is not stored. Without [hostKey] it returns the
     * server's key fingerprint to confirm; afterwards an empty string.
     */
    suspend fun installSshKey(input: ServerInputUi, hostKey: String?): String
    /** The suite's SSH public key for `authorized_keys` on the server. */
    suspend fun sshPublicKey(): String
    /** Folders below the login folder of a server (for choosing the website folder of the module). */
    suspend fun serverBrowse(serverId: String, path: String): List<String>
    /** Folders of a server as entered, before it is stored (folder picker). SFTP and FTPS. */
    suspend fun serverBrowseInput(input: ServerInputUi, confirmed: String?, path: String): ServerFoldersUi =
        ServerFoldersUi.Folders(emptyList())
    /**
     * Installs the GhulSuite server module (share links) over the server's own login into the
     * website folder [dir] and checks it at [url] (empty: the usual addresses are tried). Only
     * when every check passes is it kept for the server.
     */
    suspend fun serverModuleInstall(serverId: String, dir: String, url: String): ModuleInstalledUi =
        throw UnsupportedOperationException()
    /** Writes the module of this app's version over an older one; returns its version. */
    suspend fun serverModuleUpdate(serverId: String): Int = 0
    /** Removes the module from the server: share links of this server stop working. */
    suspend fun serverModuleRemove(serverId: String) {}

    // --- Sharing (always from GhulDrive's server, through its server module)
    /** Null: links can be made. "": GhulDrive is not set up. Else the server that needs the module. */
    suspend fun shareMissingModule(): String? = ""
    /** The share links that contain one of these GhulDrive items (asked before deleting them). */
    suspend fun shareLinksOf(ids: List<String>): List<ShareInfoUi> = emptyList()
    suspend fun shareDrive(ids: List<String>, name: String?, options: ShareOptionsUi): ShareInfoUi
    suspend fun shareBytes(name: String, data: ByteArray, options: ShareOptionsUi): ShareInfoUi
    suspend fun shareList(): List<ShareInfoUi>
    suspend fun shareDelete(id: String)
    /** Decrypts Drive files on the device and offers them to other apps (Android share sheet). */
    suspend fun shareDriveToApps(files: List<DriveItemUi>)
    /** Hands a file to other apps (share sheet); false if the platform has none (then save it). */
    suspend fun shareBytesToApps(name: String, mime: String, data: ByteArray): Boolean = false
    /** Decrypted content of Drive files as mail attachments (folders are skipped). */
    suspend fun driveFilesForMail(files: List<DriveItemUi>): List<OutgoingAttachmentUi>

    // --- Synchronisation
    suspend fun syncStatus(): SyncStatusUi
    suspend fun setSyncServer(id: String?)
    suspend fun syncNow(): SyncReportUi
    /** After the master password was changed on another device. */
    suspend fun adoptRemotePassword(password: String): SyncReportUi

    // --- GhulDrive
    suspend fun driveStatus(): DriveStatusUi
    suspend fun driveSetup(serverId: String)
    /** Where GhulDrive stores its files. */
    suspend fun driveStorage(): DriveStorageUi = DriveStorageUi(DriveModeUi.NONE, null, false)
    /** GhulDrive uses the server this device synchronises with ("wie Synchronisation"). */
    suspend fun driveUseSync() {}
    /** After "server state older than seen": takes the server's GhulDrive as it is. */
    suspend fun driveAcceptServerState() {}
    /** Checks a server for GhulDrive alone and stores it in the Drive file. */
    suspend fun driveSaveServer(input: ServerInputUi, confirmed: String?): ServerSavedUi = ServerSavedUi.Saved("")
    /** Before the sync server is removed or changed: GhulDrive keeps that server as its own. */
    suspend fun driveDetachFromSync() {}
    suspend fun driveList(folder: String?, refresh: Boolean): List<DriveItemUi>
    suspend fun driveSearch(query: String): List<DriveItemUi>
    suspend fun driveCreateFolder(parent: String?, name: String): String
    suspend fun driveRename(id: String, name: String)
    suspend fun driveMove(id: String, folder: String?)
    suspend fun driveDelete(id: String)

    // Drive extras (defaults for test fakes)
    suspend fun driveTrash(): List<DriveItemUi> = emptyList()
    suspend fun driveRestore(id: String) {}
    suspend fun driveDeleteForever(id: String) {}
    suspend fun driveEmptyTrash(days: Int): Int = 0
    suspend fun driveSetFavorite(id: String, favorite: Boolean) {}
    suspend fun driveFavorites(): List<DriveItemUi> = emptyList()
    suspend fun driveRecent(limit: Int): List<DriveItemUi> = emptyList()
    suspend fun driveOfflineFiles(): List<DriveItemUi> = emptyList()
    suspend fun driveSetOffline(id: String, offline: Boolean) {}
    suspend fun driveVersions(id: String): List<DriveVersionUi> = emptyList()
    suspend fun driveRestoreVersion(id: String, index: Int) {}
    suspend fun driveUsage(): DriveUsageUi? = null
    suspend fun driveZip(ids: List<String>, password: String): ByteArray = ByteArray(0)
    /** The device is on a network without data limit (Wi-Fi); desktops always. */
    fun unmeteredNetwork(): Boolean = true
    /**
     * Photo backup (Android): uploads photos added after [sinceSeconds] into "Fotos", oldest first;
     * [onFile] gets each one done with the time it was added and the time of the next photo
     * waiting (null: none), also when the run is being cancelled. Returns the mark to continue
     * from next time (see photoMark).
     */
    val photoBackupAvailable: Boolean get() = false
    suspend fun photoBackup(
        sinceSeconds: Long, wifiOnly: Boolean, onFile: (name: String, addedSeconds: Long, nextSeconds: Long?) -> Unit,
    ): Long = sinceSeconds
    /** Encrypts and uploads [source] block by block; [onProgress] gets the bytes done. */
    suspend fun driveUpload(folder: String?, source: ByteSource, onProgress: (Long) -> Unit): String
    /** Downloads and decrypts a file into [sink]. */
    suspend fun driveDownload(id: String, sink: ByteSink, onProgress: (Long) -> Unit)

    // --- GhulMail
    fun mailSuggest(email: String): MailServersUi
    /**
     * "Check connection" of an account as entered (stores nothing): both servers step by step.
     * Editing ([id] set) with an empty password checks with the stored one.
     */
    suspend fun mailTest(id: String?, input: MailAccountInputUi): MailTestUi
    /** Stores an account; only settings that just passed [mailTest] (or a change of names and signature only). */
    suspend fun mailSaveAccount(id: String?, input: MailAccountInputUi): String
    suspend fun mailAccounts(): List<MailAccountUi>
    /** Unread messages in the inbox of [accountId] (asks the server). */
    suspend fun mailUnseen(accountId: String): Int = 0
    suspend fun mailRemoveAccount(id: String)
    suspend fun mailFolders(accountId: String): List<MailFolderUi>
    suspend fun mailList(accountId: String, folder: String, offset: Int, limit: Int): MailPageUi
    suspend fun mailSearch(accountId: String, folder: String, query: String): List<MailSummaryUi>
    suspend fun mailGet(accountId: String, folder: String, uid: Long): MailMessageUi
    suspend fun mailAttachment(accountId: String, folder: String, uid: Long, index: Int): ByteArray
    suspend fun mailSetSeen(accountId: String, folder: String, uid: Long, seen: Boolean)
    suspend fun mailSetFlagged(accountId: String, folder: String, uid: Long, flagged: Boolean)
    suspend fun mailMove(accountId: String, folder: String, uid: Long, target: String)
    suspend fun mailDelete(accountId: String, folder: String, uid: Long)
    /** All UIDs of a folder ("select all mails in this folder"). */
    suspend fun mailFolderUids(accountId: String, folder: String): List<Long> = emptyList()
    suspend fun mailSetSeenMany(accountId: String, folder: String, uids: List<Long>, seen: Boolean) {}
    suspend fun mailSetFlaggedMany(accountId: String, folder: String, uids: List<Long>, flagged: Boolean) {}
    suspend fun mailMoveMany(accountId: String, folder: String, uids: List<Long>, target: String) {}
    /** To the trash; in the trash for good. */
    suspend fun mailDeleteMany(accountId: String, folder: String, uids: List<Long>) {}
    /** Deletes everything in the trash or spam folder for good; returns how many. */
    suspend fun mailEmptyFolder(accountId: String, folder: String): Int = 0
    suspend fun mailSend(mail: OutgoingMailUi)

    // GhulMail extras (defaults for test fakes)
    suspend fun mailFlagged(accountId: String, folder: String): List<MailSummaryUi> = emptyList()
    suspend fun mailSetLabel(accountId: String, folder: String, uid: Long, label: String, on: Boolean) {}
    suspend fun mailRaw(accountId: String, folder: String, uid: Long, decrypted: Boolean): ByteArray = ByteArray(0)

    // --- Mail archive in GhulDrive: each mail as an .eml file in Mail-Archiv/<account>/<year>
    /** The archive's folder in GhulDrive: the account's (with [accountId]) or the top one; null while nothing was archived. */
    suspend fun archiveFolder(accountId: String?): String? = null
    /** UIDs of the mails at the provider to archive: received before the day [beforeDays] (days since 1970-01-01), or all. Oldest first. */
    suspend fun archiveUids(accountId: String, folder: String, beforeDays: Long?): List<Long> = emptyList()
    /** Copies one mail into GhulDrive; it stays at the provider unchanged. Fails with NOT_CONFIGURED without GhulDrive. */
    suspend fun archiveToDrive(accountId: String, folder: String, uid: Long): MailArchivedUi = MailArchivedUi.SKIPPED
    /** Automatic archiving: mails older than this many days are copied (0 = off); the same on every device. */
    suspend fun archiveAutoDays(): Int = 0
    suspend fun archiveSetAutoDays(days: Int) {}
    /** For an automatic run: the mails of a folder old enough that came after the last automatic run. */
    suspend fun archiveAutoUids(accountId: String, folder: String): MailArchiveBatchUi = MailArchiveBatchUi(0, emptyList())
    /** After an automatic run: everything up to [lastUid] is in the archive. */
    suspend fun archiveAutoDone(accountId: String, folder: String, validity: Long, lastUid: Long) {}
    /** Reads an archived mail (.eml), decrypted with the account's key if it is encrypted. */
    suspend fun mailParseEml(accountId: String, data: ByteArray): MailMessageUi = throw UnsupportedOperationException()
    /** Attachment [index] of an archived mail (.eml). */
    suspend fun mailEmlAttachment(accountId: String, data: ByteArray, index: Int): ByteArray = ByteArray(0)
    suspend fun mailHeaders(accountId: String, folder: String, uid: Long): String = ""
    suspend fun mailSaveDraft(draft: MailDraftUi): String = draft.id
    suspend fun mailDrafts(): List<MailDraftUi> = emptyList()
    /** An image from the internet for an HTML mail (only on request; https/http, max 5 MB). */
    suspend fun mailFetchImage(url: String): ByteArray = throw UnsupportedOperationException()
    suspend fun mailDeleteDraft(id: String) {}
    suspend fun mailSchedule(mail: OutgoingMailUi, sendAt: Long): String = ""
    suspend fun mailScheduled(): List<ScheduledMailUi> = emptyList()
    suspend fun mailCancelScheduled(id: String) {}
    suspend fun mailSendDue(now: Long): Int = 0
    suspend fun mailContacts(query: String): List<MailContactUi> = emptyList()
    suspend fun setMailOffline(on: Boolean) {}
    fun mailOffline(): Boolean = false
    /** Puts bytes into GhulDrive (folder by name at the top level, created if missing). */
    suspend fun driveSaveBytes(folderName: String, name: String, data: ByteArray) {}

    // --- OpenPGP (automatic; the user only sees lock and seal)
    /** Whether each recipient can get encrypted mail (looks up unknown ones at their provider). */
    suspend fun mailRecipientKeys(recipients: List<String>): List<MailRecipientKeyUi>
    /** The account's public key, armored (to save or send by hand). */
    suspend fun mailPublicKey(accountId: String): String
    /** Imports a contact's public key (.asc); returns its addresses. */
    suspend fun mailImportKey(data: ByteArray): List<String>
    suspend fun mailKnownKeys(): List<MailKeyInfoUi>
    suspend fun mailForgetKey(email: String)
    /** The fingerprint matches what the contact sees: the key counts as confirmed. */
    suspend fun mailVerifyKey(email: String) {}

    /** Whether new-mail notifications are on for an account (needs a device-bound copy of the login). */
    fun mailNotificationsOn(accountId: String): Boolean

    /** Removes the new-mail notification of an account once its inbox or the mail was opened. */
    fun clearMailNotification(accountId: String) {}

    /**
     * After unlocking: new accounts notify by default, stored logins for notifications follow
     * changes from other devices, and the background check and instant service run.
     */
    suspend fun refreshMailNotifications() {}

    /** Whether Android lets mail arrive in deep sleep (battery optimisation off); null: not relevant. */
    fun mailBackgroundAllowed(): Boolean? = null

    /** Opens the system setting for mail in the background (battery optimisation). */
    fun openMailBackgroundSettings() {}
    suspend fun setMailNotifications(accountId: String, on: Boolean)

    /** What a new-mail notification shows (device setting): 0 only the count, 1 the sender, 2 sender and subject. */
    fun mailNotifyDetail(): Int = 0
    fun setMailNotifyDetail(level: Int) {}
}

/** A readable file chosen by the user (streamed, not loaded at once). */
interface ByteSource {
    val name: String
    /** Size in bytes, or -1 if unknown. */
    val size: Long
    /** Reads up to [length] bytes into [buffer] at [offset]; returns -1 at the end. */
    fun read(buffer: ByteArray, offset: Int, length: Int): Int
    fun close()
}

/** A writable file chosen by the user. */
interface ByteSink {
    fun write(data: ByteArray)
    fun close()
    /** Removes what was written (a cancelled or failed download leaves no half file). */
    fun discard() {}
}

enum class ServerKindUi { SFTP, FTPS }

sealed interface ServerTestUi {
    data class Confirm(val fingerprint: String) : ServerTestUi
    data object Ok : ServerTestUi
}

sealed interface ServerSavedUi {
    data class Confirm(val fingerprint: String) : ServerSavedUi
    data class Saved(val id: String) : ServerSavedUi
}

data class ServerInputUi(
    val kind: ServerKindUi = ServerKindUi.SFTP,
    val name: String,
    val host: String,
    val port: Int,
    val user: String,
    /** SFTP: null = log in with the suite's SSH key. FTPS: password. */
    val password: String?,
    val base: String,
)

sealed interface ServerFoldersUi {
    data class Confirm(val fingerprint: String) : ServerFoldersUi
    data class Folders(val folders: List<String>) : ServerFoldersUi
}

enum class DriveModeUi { NONE, SYNC, OWN, SUITE }

data class DriveStorageUi(val mode: DriveModeUi, val server: ServerUi?, val syncAvailable: Boolean)

/**
 * One step of installing the server module: `upload` (file written, detail = its path), `https`
 * (it answers, detail = the address), `php` (only when PHP does not run there; the file was
 * removed again), `folder` (it reads the GhulSuite folder; detail "skipped" = not checked).
 */
data class ModuleCheckUi(val step: String, val ok: Boolean, val detail: String)

/** Result of installing the server module: [url] null = it does not work yet (see [checks]). */
data class ModuleInstalledUi(val url: String?, val tried: List<String>, val checks: List<ModuleCheckUi>)

/** Share links always come from GhulDrive's server: 0 days = no expiry, 0 downloads = unlimited. */
data class ShareOptionsUi(val password: String?, val expiresDays: Int, val maxDownloads: Int)

data class ShareInfoUi(
    val id: String,
    val serverId: String,
    val name: String,
    val size: Long,
    val created: Long,
    val expires: Long,
    val password: Boolean,
    val maxDownloads: Int,
    /** -1 = the server cannot count. */
    val downloads: Long,
    val url: String,
)

data class ServerUi(
    val id: String,
    val name: String,
    val host: String,
    val port: Int,
    val user: String,
    val base: String,
    val usesKey: Boolean,
    val hostKey: String,
    /** Absolute folder on the server, e.g. /home/max/ghulsuite. */
    val location: String = "",
    val kind: ServerKindUi = ServerKindUi.SFTP,
    /** Address of the server module (share links); empty = not installed. */
    val moduleUrl: String = "",
    /** Version of the installed module (0 = none). */
    val moduleVersion: Int = 0,
    /** The version this app brings: an older module can be updated. */
    val moduleCurrent: Int = 0,
) {
    val hasModule: Boolean get() = moduleUrl.isNotEmpty()
    val moduleOutdated: Boolean get() = hasModule && moduleVersion < moduleCurrent
}

data class SyncStatusUi(val serverId: String?, val lastSync: Long)

data class SyncReportUi(val uploaded: Int, val downloaded: Int, val merged: Int, val deleted: Int, val skipped: Int)

data class DriveStatusUi(val serverId: String?, val files: Int, val bytes: Long)

data class DriveItemUi(
    val id: String,
    val parent: String?,
    val name: String,
    val isDir: Boolean,
    val size: Long,
    val modified: Long,
    val favorite: Boolean = false,
    val offline: Boolean = false,
    val trashed: Long = 0,
    val versions: Int = 0,
)

data class DriveVersionUi(val index: Int, val size: Long, val modified: Long)

data class DriveUsageUi(val images: Long, val videos: Long, val audio: Long, val documents: Long, val other: Long, val trash: Long, val versions: Long)

enum class MailSecurityUi { TLS, STARTTLS }

data class MailServersUi(
    val imapHost: String,
    val imapPort: Int,
    val imapSecurity: MailSecurityUi,
    val smtpHost: String,
    val smtpPort: Int,
    val smtpSecurity: MailSecurityUi,
)

data class MailAccountInputUi(
    val name: String,
    val displayName: String,
    val email: String,
    val username: String,
    /** Empty when editing = keep the stored password. */
    val password: String,
    val servers: MailServersUi,
    val imapCert: String? = null,
    val smtpCert: String? = null,
    val signature: String = "",
)

data class MailAccountUi(
    val id: String,
    val name: String,
    val displayName: String,
    val email: String,
    val username: String,
    val servers: MailServersUi,
    val signature: String,
    /** Fingerprint of the account's OpenPGP key, in groups of four. */
    val pgpFingerprint: String = "",
)

/** How a received mail was signed. */
enum class MailSignatureUi { NONE, VALID, UNKNOWN_KEY, INVALID }

data class MailRecipientKeyUi(val address: String, val hasKey: Boolean, val source: String)

/**
 * [verified]: the fingerprint was compared with the contact. [previous]: the key changed (the
 * fingerprint before). [offered]: a mail offered another key than the confirmed one.
 */
data class MailKeyInfoUi(
    val email: String,
    val fingerprint: String,
    val source: String,
    val updated: Long,
    val verified: Boolean = false,
    val previous: String = "",
    val offered: String = "",
)

sealed interface MailCheckUi {
    data object Ok : MailCheckUi
    data class ImapCertificate(val fingerprint: String) : MailCheckUi
    data class SmtpCertificate(val fingerprint: String) : MailCheckUi
}

/**
 * One step of checking a connection: imap_server, imap_login, smtp_server, smtp_login. [detail]:
 * empty when ok, [STEP_SKIPPED] when not reached, [STEP_CERTIFICATE] when a certificate is to be
 * confirmed, else the error.
 */
data class ConnStepUi(val step: String, val ok: Boolean, val detail: String)

const val STEP_SKIPPED = "skipped"
const val STEP_CERTIFICATE = "certificate"

/** Result of checking an account: every step, and a certificate to confirm (then check again). */
data class MailTestUi(val ok: Boolean, val steps: List<ConnStepUi>, val confirm: MailCheckUi? = null)

/** `role`: inbox, sent, trash, drafts, junk, archive or empty. */
data class MailFolderUi(val name: String, val role: String)

data class MailSummaryUi(
    val uid: Long,
    val from: String,
    val fromAddress: String,
    val subject: String,
    /** Unix seconds. */
    val date: Long,
    val seen: Boolean,
    val flagged: Boolean,
    val hasAttachments: Boolean,
    val size: Long,
    val preview: String = "",
    val encrypted: Boolean = false,
    val labels: List<String> = emptyList(),
    val threadKey: String = "",
)

data class MailDraftUi(
    val id: String,
    val accountId: String,
    val to: String,
    val cc: String,
    val bcc: String,
    val subject: String,
    val text: String,
    val inReplyTo: String,
    val updated: Long = 0,
    val attachments: List<OutgoingAttachmentUi> = emptyList(),
)

data class ScheduledMailUi(val id: String, val accountId: String, val to: List<String>, val subject: String, val sendAt: Long)

data class MailContactUi(val name: String, val email: String)

data class MailPageUi(val total: Int, val messages: List<MailSummaryUi>)

data class MailAttachmentUi(val index: Int, val name: String, val mime: String, val size: Long)

data class MailMessageUi(
    val uid: Long,
    val from: String,
    val fromAddress: String,
    val to: List<String>,
    val cc: List<String>,
    val replyTo: String,
    val subject: String,
    val date: Long,
    val text: String,
    val wasHtml: Boolean,
    val messageId: String,
    val attachments: List<MailAttachmentUi>,
    /** End-to-end encrypted (OpenPGP), decrypted on this device. */
    val encrypted: Boolean = false,
    val signature: MailSignatureUi = MailSignatureUi.NONE,
    /** Address whose key made the valid signature. */
    val signer: String = "",
    /** HTML mails as safe building blocks (empty: show [text]). */
    val blocks: List<MailBlockUi> = emptyList(),
    /** Images from the internet in the HTML (loaded only on request). */
    val remoteImages: Int = 0,
    /** Images inside the mail, by Content-ID. */
    val inlineImages: Map<String, ByteArray> = emptyMap(),
)

enum class MailBlockKindUi { PARAGRAPH, HEADING, LIST_ITEM, QUOTE, RULE, IMAGE }

data class MailSpanUi(val text: String, val bold: Boolean, val italic: Boolean, val link: String)

data class MailBlockUi(val kind: MailBlockKindUi, val level: Int, val spans: List<MailSpanUi>, val imageSrc: String, val imageAlt: String)

class OutgoingAttachmentUi(val name: String, val mime: String, val data: ByteArray)

data class OutgoingMailUi(
    val accountId: String,
    val to: List<String>,
    val cc: List<String>,
    val bcc: List<String>,
    val subject: String,
    val text: String,
    val inReplyTo: String,
    val attachments: List<OutgoingAttachmentUi>,
    /** OpenPGP for this mail; off sends a plain mail without signature and key. */
    val pgp: Boolean = true,
)

/** What archiving one mail did: stored, already in the archive, or no longer at the provider. */
enum class MailArchivedUi { SAVED, SKIPPED, GONE }

/** Mails of a folder for an automatic run: the folder's UIDVALIDITY and the UIDs, oldest first. */
data class MailArchiveBatchUi(val validity: Long, val uids: List<Long>)
