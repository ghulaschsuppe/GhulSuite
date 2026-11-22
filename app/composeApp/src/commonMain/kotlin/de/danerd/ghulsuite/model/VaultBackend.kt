// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.model

import kotlinx.coroutines.flow.StateFlow

/**
 * Platform-independent access to the vault (Rust core) and platform services.
 * Android implements it with USB/CTAP2; Windows will use the WebAuthn API.
 */
interface VaultBackend {
    /** True while the app waits for a security key to be plugged in (Android). */
    val waitingForKey: kotlinx.coroutines.flow.StateFlow<Boolean> get() = NOT_WAITING

    val unlocked: StateFlow<Boolean>
    val settings: Settings
    /** GhulCal. */
    val cal: CalBackend get() = NoCalBackend
    /** GhulDocs. */
    val docs: DocsBackend get() = NoDocsBackend

    // --- The working copy and the suite file
    /** Whether this device has a suite (its working copy, in the app's own storage). */
    fun exists(): Boolean
    /** Protection of the suite (for the unlock screen); null if there is no suite. */
    fun protection(): ProtectionUi?
    /** What a vault file needs to be opened. */
    suspend fun inspectFile(data: ByteArray): ContainerInfoUi

    // --- Setup
    suspend fun calibrateKdf(): KdfChoice
    suspend fun keyInfo(): KeyStatus
    suspend fun setKeyPin(newPin: String)
    fun generateKeyfile(): ByteArray
    /** Creates a new suite; returns the recovery code. */
    suspend fun create(setup: SetupUi, onTouchNeeded: () -> Unit): String
    /** Starts a suite from an existing vault file; returns a new recovery code, or null if the vault already has one. */
    suspend fun createFromVault(open: OpenVaultUi, onTouchNeeded: () -> Unit): String?

    // --- Unlock
    suspend fun unlock(password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit)
    /** Emergency: recovery code alone, sets a new master password. */
    suspend fun unlockWithRecoveryCode(code: String, newPassword: String)
    fun lock()
    /** Panic lock: locks and destroys every quick unlock (fingerprint/PIN). */
    fun panicLock()

    // Quick unlock after a full unlock
    fun biometricAvailable(): Boolean
    /** Which quick unlock methods can be used right now (not expired, not reset by a reboot). */
    fun quickUnlockReady(): QuickUnlockReady
    /** [password]: the master password (who finds the app unlocked must not give himself lasting access). */
    /**
     * Whether [password] is the master password of the open suite (M13, M14: asked before a step
     * that weakens the protection or shows a secret). Changes nothing.
     */
    suspend fun verifyPassword(password: String): Boolean = false
    /**
     * H6: fingerprint or app PIN unlock only with both factors of now ([proof]; right after
     * opening with the recovery code the new master password alone). [hours] is the validity,
     * fixed in the quick unlock file (0: no expiry); a key is touched through [onTouchNeeded].
     */
    suspend fun enableBiometricUnlock(proof: FactorProofUi, hours: Int, onTouchNeeded: () -> Unit = {})
    suspend fun enablePinUnlock(proof: FactorProofUi, pin: String, hours: Int, onTouchNeeded: () -> Unit = {})
    /**
     * H3: the open suite was opened with the recovery code (until it locks): a change of the
     * protection takes the new master password alone.
     */
    fun openedWithRecoveryCode(): Boolean = false
    /** The folder of this app's data on this computer (Windows; shown as the last way out of H5). */
    val dataFolder: String get() = ""
    /** M42: the rules of the core for master passwords and app PINs. */
    fun securityRules(): SecurityRulesUi = SecurityRulesUi()
    fun disableQuickUnlock()
    fun disableQuickUnlockMethod(bio: Boolean)
    suspend fun unlockWithBiometric()
    suspend fun unlockWithPin(pin: String)

    // --- Vaults
    suspend fun listVaults(): List<VaultUi>
    suspend fun createVault(kind: VaultKindUi, name: String): String
    suspend fun renameVault(id: String, name: String)
    /** Deletes the vault file from the folder. */
    suspend fun removeVault(id: String)

    // --- Areas: one file per area (GhulPass, GhulAuth, GhulCal)
    fun setupState(): SetupStateUi = if (exists()) SetupStateUi.COMPLETE else SetupStateUi.EMPTY
    /** Creates the area's file (when switched on later); an existing area keeps its file. */
    suspend fun addArea(kind: VaultKindUi, name: String): String = createVault(kind, name)
    /** Removes the area (its files are deleted). */
    suspend fun removeArea(kind: VaultKindUi) {}
    /** Options stored in the area's file (empty without the area). */
    suspend fun areaOptions(kind: VaultKindUi): Map<String, String> = emptyMap()
    suspend fun setAreaOptions(kind: VaultKindUi, options: Map<String, String>) {}
    /** Whether the current recovery code was confirmed by typing a group back. */
    suspend fun recoveryConfirmed(): Boolean = true
    /** Checks a typed group (0-based) of the recovery code; only a correct one confirms it. */
    suspend fun confirmRecoveryCode(group: Int, typed: String): Boolean = true

    /** r9 krypto M2: the whole code typed back (the core checks it with its checksum and marks it confirmed). */
    suspend fun confirmWholeRecoveryCode(typed: String): Boolean = true
    suspend fun exportVault(id: String): ByteArray
    /** Adds a vault file with the suite's protection; DIFFERENT_PROTECTION if it needs its own password. */
    suspend fun addVaultFile(data: ByteArray, name: String): String
    /** Takes over a vault with another protection: opened once with its own password, then switched to the suite's. */
    suspend fun adoptVault(
        data: ByteArray,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): String

    // --- GhulPass
    suspend fun listEntries(vaultId: String?): List<EntrySummaryUi>
    suspend fun getEntry(vaultId: String, id: String): EntryUi
    suspend fun saveEntry(entry: EntryUi): String
    suspend fun deleteEntry(vaultId: String, id: String)

    // --- GhulAuth
    suspend fun totpCodes(nowSeconds: Long): List<TotpCodeUi>
    suspend fun addTotp(vaultId: String, input: TotpInputUi)
    suspend fun parseOtpauth(uri: String): TotpInputUi
    suspend fun renameTotp(vaultId: String, id: String, issuer: String, account: String)
    suspend fun deleteTotp(vaultId: String, id: String)

    // --- Protection
    suspend fun slots(): List<SlotUi>
    /** Adds a security key; nothing else changes (other keys, key files, recovery code stay valid). */
    /**
     * Changes of the protection need both factors of now ([proof]: password plus key or key file);
     * a key of now is touched first, then the new one.
     */
    suspend fun addSecurityKey(proof: FactorProofUi, pin: String, label: String, onTouchNeeded: () -> Unit)
    /** Adds a key file as alternative second factor. */
    suspend fun addKeyfile(proof: FactorProofUi, keyfile: ByteArray, label: String, onTouchNeeded: () -> Unit)
    suspend fun newRecoveryCode(proof: FactorProofUi, label: String, onTouchNeeded: () -> Unit): String
    /** Removes a security key or key file (not the last one). */
    suspend fun removeSlot(proof: FactorProofUi, id: String, onTouchNeeded: () -> Unit)
    /** [proof]: the current password plus a key or key file of now. */
    suspend fun changePassword(proof: FactorProofUi, new: String, onTouchNeeded: () -> Unit)

    // --- Import and export
    /** What an import file contains (nothing is changed). */
    suspend fun previewImport(data: ByteArray, fileName: String): ImportPreviewUi
    suspend fun importData(
        data: ByteArray,
        fileName: String,
        passVault: String?,
        totpVault: String?,
        totpsFromLogins: Boolean,
        keep: KeepSkippedUi? = null,
    ): ImportResultUi
    /**
     * Unencrypted export of one vault (CSV or otpauth links); needs the master password.
     * [spreadsheetSafe] (M29): cells that a spreadsheet would run as a formula get an apostrophe
     * (other password managers would take it over into the password).
     */
    suspend fun exportPlain(vaultId: String, password: String, spreadsheetSafe: Boolean = false): ByteArray
    /** Some entries as unencrypted CSV (or otpauth links), only with the master password. */
    suspend fun exportEntries(vaultId: String, ids: List<String>, password: String, spreadsheetSafe: Boolean = false): ByteArray = throw UnsupportedOperationException()

    // --- Tools
    fun generatePassword(options: PasswordOptionsUi): String
    fun passwordScore(password: String): Int

    // --- GhulPass extras (defaults only for test fakes) -------------------------------------
    suspend fun entryExtras(vaultId: String, id: String): EntryExtrasUi = EntryExtrasUi(false, 0, emptyList(), emptyList())
    suspend fun setFavorite(vaultId: String, id: String, favorite: Boolean) {}
    suspend fun addAttachment(vaultId: String, id: String, name: String, data: ByteArray) {}
    suspend fun attachment(vaultId: String, id: String, name: String): ByteArray = ByteArray(0)
    suspend fun removeAttachment(vaultId: String, id: String, name: String) {}
    suspend fun duplicateEntry(vaultId: String, id: String, titleSuffix: String): String = id
    suspend fun moveEntry(vaultId: String, id: String, targetVaultId: String): String = id
    suspend fun securityReport(): SecurityReportUi = SecurityReportUi(100, 0, emptyList())
    fun entryTotp(secret: String, nowSeconds: Long): TotpNowUi? = null
    fun generatePassphrase(words: Int, separator: String, capitalize: Boolean, number: Boolean): String = ""
    /** GhulAuth: account as otpauth link (QR export). */
    suspend fun totpUri(vaultId: String, id: String, password: String): String = ""
    // --- The suite file (.ghlzip): opened into the working copy, saved from it. Read and written
    // as a stream (it can be over a gigabyte); [onProgress] gets bytes done and in all (0: not known).
    /** What a suite file needs to be opened (password plus key or key file); reads only its header. */
    suspend fun archiveProtection(file: SuiteFileRef): ProtectionUi = throw VaultException(VaultErrorKind.OTHER)
    /** Opens and checks a suite file; nothing is replaced before [archiveAccept]. */
    suspend fun archiveOpen(
        file: SuiteFileRef,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit = { _, _ -> },
    ): ArchiveCheckUi = throw VaultException(VaultErrorKind.OTHER)
    /** Emergency: opens a suite file with the recovery code; on accepting, [newPassword] protects it. */
    suspend fun archiveOpenWithCode(
        file: SuiteFileRef,
        code: String,
        newPassword: String,
        onProgress: (Long, Long) -> Unit = { _, _ -> },
    ): ArchiveCheckUi = throw VaultException(VaultErrorKind.OTHER)
    fun archiveCancel() {}
    /** Replaces the working copy with the checked file and opens it. */
    suspend fun archiveAccept() {}
    /**
     * H5: the checked file holds another suite than the one on this device: replacing it needs
     * the proof of the one here, both factors ([proof]) or its recovery code ([code]).
     */
    suspend fun archiveAcceptReplacing(proof: FactorProofUi?, code: String?, onTouchNeeded: () -> Unit = {}) {}
    /** The working copy has changes that are not in a file yet. */
    fun archiveUnsaved(): Boolean = false
    /** When the first change not in a file yet was made (ms since 1970); 0: nothing unsaved. Also while locked. */
    fun archiveUnsavedSince(): Long = 0
    /** When the state of the working copy was last saved into a suite file or came from one (ms); 0: not known. */
    fun archiveSavedAt(): Long = 0
    /** The file at the place the suite is saved to, compared with the working copy; null if it cannot be read. */
    suspend fun archiveTarget(file: SuiteFileRef): ArchiveTargetUi? = archiveTargetInfo(file)?.target
    /** [archiveTarget] with the device and time of the file there (when they can be read). */
    suspend fun archiveTargetInfo(file: SuiteFileRef): ArchiveTargetInfoUi? = ArchiveTargetInfoUi(ArchiveTargetUi.NEW)
    /**
     * Saves the working copy into [target]: built and written as a stream, read back and compared,
     * only then the working copy counts as saved. False (an error message is up to the caller) when
     * it could not be written completely; [created]: the file was just made for this ("Save as"),
     * a failed write removes it again.
     */
    suspend fun archiveSave(target: SuiteFileRef, device: String, created: Boolean, onProgress: (Long, Long) -> Unit = { _, _ -> }): Boolean = false
    /**
     * Deletes the suite on this device for good, to set up a new one in its place: it is locked,
     * then its quick unlock (with the platform's keys), calendar reminders, the remembered suite
     * file and this device's settings that name its vaults or entries go, and the core removes
     * every vault file, the documents of GhulDocs and the suite file. Suite files elsewhere are
     * not touched.
     *
     * H5: only with the proof of that suite (also while it is locked): both factors ([proof]) or
     * its recovery code ([code]).
     */
    suspend fun deleteSuite(proof: FactorProofUi?, code: String?, onTouchNeeded: () -> Unit = {})
    /** The suite file used last on this device (opened or saved), if any. */
    fun suiteFile(): SuiteFileRef? = null

    /**
     * r9 M-2: keeps the right to read and write [ref] across restarts (Android: a persisted URI
     * permission), once the user chose it as the place to save to. False: not possible.
     */
    fun keepAccess(ref: SuiteFileRef): Boolean = true
    fun setSuiteFile(ref: SuiteFileRef?) {}
    /**
     * Reads / writes a vault file (opened on its own) at a remembered place, as a whole; null / false
     * when it is not reachable (e.g. the stick is out) or too large for a vault file.
     */
    suspend fun readSuiteFile(ref: SuiteFileRef): ByteArray? = null
    suspend fun writeSuiteFile(ref: SuiteFileRef, data: ByteArray): Boolean = false
    /** Creates the file anew; false (and nothing changed) if there is one at that place. */
    suspend fun createSuiteFile(ref: SuiteFileRef, data: ByteArray): Boolean = false
    // --- One vault on its own, and picked entries handed on as a vault of their own
    /**
     * Opens one vault file on its own: only that vault is unlocked, the own suite stays locked.
     * Changes go back into [file] (see [saveSingle]); [closeSingle] or locking ends it.
     */
    suspend fun openSingle(file: SuiteFileRef, data: ByteArray, password: String, pin: String?, keyfile: ByteArray?, onTouchNeeded: () -> Unit): VaultKindUi =
        throw VaultException(VaultErrorKind.OTHER)
    /** The file of the vault open on its own; null while the own suite is in use. */
    fun singleFile(): SuiteFileRef? = null
    /** Writes the vault open on its own back into its file, as [mode] allows (see [SingleWrite]). */
    suspend fun saveSingle(mode: SingleWrite = SingleWrite.CHECKED): SingleSaveUi = SingleSaveUi.FAILED
    /** Writes the vault open on its own into another file, which it is written into from now on. */
    suspend fun saveSingleAs(ref: SuiteFileRef): Boolean = false
    /** Whether the own suite exists on this device (also while a vault is open on its own). */
    fun ownSuiteExists(): Boolean = exists()
    /** Grows when leftovers of vaults opened on their own may have changed. */
    val singleWritesChanged: kotlinx.coroutines.flow.StateFlow<Int> get() = kotlinx.coroutines.flow.MutableStateFlow(0)
    /** The vault open on its own as it is now (to add it to the own suite). */
    suspend fun singleVaultBytes(): ByteArray = throw VaultException(VaultErrorKind.OTHER)
    /** The vault open on its own is a GhulDocs vault (its file holds its documents too). */
    val singleIsDocs: Boolean get() = false
    /** Whether the app may write into this file (a vault opened on its own goes back into it). */
    suspend fun canWriteSuiteFile(ref: SuiteFileRef): Boolean = true

    /**
     * How saving to [ref] replaces the file there: SAFE (the new file is written next to it and
     * checked, then put in its place: there is always a complete file), FOLDER_NEEDED (Android:
     * safe once the folder is shared with the app), OVERWRITES (the place allows nothing else,
     * e.g. some cloud apps: the file there is written over; the working copy stays complete).
     */
    fun safeSave(ref: SuiteFileRef): SafeSaveUi = SafeSaveUi.SAFE

    /**
     * The suite file at [old] after the suite moved to another place: deleted where the app may
     * (never the current place); leftovers of a cut off save next to it go too, only if they are
     * of this suite. [OldFileDelete.where]: where it lies, for the user to delete it by hand.
     */
    suspend fun deleteOldSuiteFile(old: SuiteFileRef): OldFileDelete = OldFileDelete(OldFileOutcome.NOT_ALLOWED, old.name)

    /**
     * Whether [a] and [b] name the same file, by its identity, not by the text of the id
     * (Windows: case, 8.3 names, links; Android: two forms of a document's address). When this
     * cannot be told, true (the safe answer before a delete).
     */
    fun isSamePlace(a: SuiteFileRef, b: SuiteFileRef): Boolean = a.id == b.id

    /** Whether "Delete old file" may be offered for [old] (it holds only a state of this suite's own line). */
    suspend fun oldFileDeletable(old: SuiteFileRef): Boolean = false

    /** How often the suite was locked in this process (review N5: windows tied to an unlock end at once). */
    val lockCount: Long get() = 0L

    /**
     * The areas with changes not in the suite file yet: a sorted subset of "auth", "cal", "docs",
     * "pass", "suite" (protection, a removed vault, suite settings); empty: nothing; ["unknown"]:
     * marked by an older version, which areas is not known.
     */
    fun archiveUnsavedAreas(): List<String> = emptyList()
    /** Whether changes of an earlier opening still wait for [ref] (cheap: only their records). */
    suspend fun hasPendingSingleWrite(ref: SuiteFileRef): Boolean = false
    /** Vaults opened on their own whose last changes could not be written back into their file. */
    fun pendingSingleWrites(): List<PendingSingleUi> = emptyList()
    /** Tries again to write such changes into their file. */
    suspend fun retrySingleWrite(key: String, mode: SingleWrite = SingleWrite.CHECKED): SingleSaveUi = SingleSaveUi.FAILED
    /** Whether a file that is gone can be written anew at its place (not with Android's documents). */
    val canWriteFilesAnew: Boolean get() = true
    /** Writes such changes into another file instead. */
    suspend fun saveSingleWriteAs(key: String, ref: SuiteFileRef): Boolean = false
    /** Gives such changes up. */
    fun discardSingleWrite(key: String) {}
    /** Picked entries into a new vault file with a protection of its own (key file or security key). */
    suspend fun shareAsVault(share: ShareVaultUi, pin: String?, onTouchNeeded: () -> Unit): SharedVaultUi =
        throw VaultException(VaultErrorKind.OTHER)
    /** Last crash report of this device, if any. */
    fun crashReport(): String? = null
    /** The crash was handed on (copied, shared, saved): it is not shown again. */
    fun clearCrashReport() {}
    /** Breach check (k-anonymity); network. */
    suspend fun pwnedCount(password: String): Int = 0

    /**
     * Hands a file to other apps: Android's share sheet, Windows opens it in the program
     * registered for it. False if the platform cannot (then the caller offers to save it).
     */
    suspend fun shareToApps(name: String, mime: String, data: ByteArray): Boolean = false

    /**
     * The same for a file that is written piece by piece (a document of GhulDocs, up to 1 GB):
     * [write] puts the content into the sink it gets. The copy lives where [shareToApps] keeps
     * its copies (wiped when the app locks); a failed or cancelled [write] removes it at once and
     * hands nothing over.
     */
    suspend fun shareStreamToApps(name: String, mime: String, write: suspend (ByteSink) -> Unit): Boolean {
        val sink = MemorySink()
        write(sink)
        return shareToApps(name, mime, sink.bytes())
    }

    /**
     * Whether [shareToApps] refuses a file of this name because the system would run, install or
     * mount it instead of showing it (programs, scripts, shortcuts, packages; H4). Windows adds
     * the endings of PATHEXT.
     */
    fun refusesToOpen(name: String): Boolean = de.danerd.ghulsuite.ui.isRunnableFile(name)

    /** Whether copies for other apps land on this PC although the app runs portable from a stick (M16). */
    val openCopiesOnThisPc: Boolean get() = false

    /** Whether [shareToApps] lets the user pick another app to send the file with (Android). */
    val canShareToApps: Boolean get() = false
    /** Whether the system shows notifications of the app (Android 13+ asks once; reminders need them). */
    fun notificationsAllowed(): Boolean = true
    /** Opens the system settings of the app's notifications. */
    fun openNotificationSettings() {}

    /** Copies to the clipboard, marked as sensitive and cleared automatically. */
    fun copySensitive(label: String, text: String)

    // --- A GhulDocs package (a .ghldocs with the vault and its documents)
    /** What a `.ghlzip` or `.ghldocs` holds (reads only its first entry); null if it is neither. */
    suspend fun archiveInspect(file: SuiteFileRef): ArchiveInfoUi? = null
    /**
     * Opens a GhulDocs package, like [openSingle]: only that vault is unlocked, its
     * changes go back into [file] (as a stream, with its documents).
     */
    suspend fun openSingleDocs(
        file: SuiteFileRef,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
        onProgress: (Long, Long) -> Unit = { _, _ -> },
    ): VaultKindUi = throw VaultException(VaultErrorKind.OTHER)

    /** Opens the system print dialog with the emergency sheet (also "Save as PDF"). */
    fun printEmergencySheet(code: String, text: EmergencySheetText)

    val appVersion: String

    /** Updates of the app itself (GitHub releases). */
    val updater: AppUpdater get() = NoUpdater

    /** false where the system asks for the key PIN in its own dialog (Windows). */
    val keyPinInApp: Boolean get() = true

    /** Windows: no shake, quick tile or fingerprint unlock. */
    val isDesktop: Boolean get() = false
    val coreVersion: String
}

/** A folder chosen by the user (Android: SAF tree URI). */
data class FolderRef(val uri: String, val name: String)

/** A suite file the app may read and write again (Android: a document it keeps access to, Windows: a path). */
data class SuiteFileRef(val id: String, val name: String)

/** How deleting the old suite file went, and [where] it lies (for the user). */
data class OldFileDelete(val outcome: OldFileOutcome, val where: String) {
    val deleted: Boolean get() = outcome == OldFileOutcome.DELETED
}

enum class OldFileOutcome {
    DELETED,
    /** It is the suite file of now (by its identity or its content): never deleted. */
    SAME_FILE,
    /** It holds a state or protection this device does not have: kept. */
    DIFFERENT_STATE,
    /** The place does not let the app delete it. */
    NOT_ALLOWED,
    /** The stick or folder is not there now: try again later. */
    UNREACHABLE,
    /** Deleting failed (try again). */
    FAILED,
}

/**
 * Given as both numbers to the progress of [VaultBackend.archiveSave] (N3): the new file is in
 * place and checked, the file it replaced is now overwritten with zeros (after a change of the
 * protection; on a stick this can take minutes).
 */
const val WIPING_OLD_FILE = -2L

/**
 * N1: errors of a quick unlock that say nothing against the quick unlock itself (the storage:
 * an earlier opening of a suite file that could not be finished, a full disk). It stays set up
 * for the next try instead of being switched off.
 */
val QUICK_UNLOCK_KEPT_ON: Set<VaultErrorKind> = setOf(VaultErrorKind.STORAGE, VaultErrorKind.NO_SPACE, VaultErrorKind.LOCKED)

/** See [VaultBackend.safeSave]. */
/**
 * NOT_SHAREABLE (Android): the file lies in a folder Android never shares with an app ("Download",
 * the top of the storage, an SD card or a stick): sharing a folder cannot help, only saving it
 * once into a folder of its own ("Save as").
 */
enum class SafeSaveUi { SAFE, FOLDER_NEEDED, NOT_SHAREABLE, OVERWRITES }

/** How a suite file relates to what this device has. */
enum class ArchiveRelationUi { NO_SUITE, OTHER_SUITE, SAME, NEWER, OLDER, BRANCH }

/** The result of checking a suite file before it replaces the working copy. */
data class ArchiveCheckUi(
    val relation: ArchiveRelationUi,
    val unsavedChanges: Boolean,
    /** Opened with the recovery code: the seals could not be checked. */
    val unverified: Boolean,
    val savedAt: Long,
    val device: String,
    val vaults: Int,
)

/**
 * The file at the place the suite is saved to. OTHER_PROTECTION: this suite with a protection
 * changed elsewhere; overwriting would undo the change.
 */
enum class ArchiveTargetUi { NEW, SAME, CHANGED_ELSEWHERE, OTHER_SUITE, OTHER_PROTECTION }

/** M42: the rules of the core (the defaults are its values). */
data class SecurityRulesUi(val passwordMinLength: Int = 10, val passwordMinScore: Int = 3, val appPinMinLength: Int = 8)

/** Both factors of now, before a change of the protection: the master password plus the PIN of a key ([pin]) or a key file. */
class FactorProofUi(val password: String, val pin: String?, val keyfile: ByteArray?)

/** The file at the place a suite is saved to: how it relates, and who saved it when ([device] empty, [time] 0: not known). */
data class ArchiveTargetInfoUi(val target: ArchiveTargetUi, val device: String = "", val time: Long = 0)

/** A file picked by the user. */
class PickedFile(val name: String, val bytes: ByteArray)

/** A readable file chosen by the user (streamed, not loaded at once). */
interface ByteSource {
    val name: String
    /** Size in bytes, or -1 if unknown. */
    val size: Long
    /** Media type as the system names it (empty: not known, the app guesses it from the name). */
    val mime: String get() = ""
    /** Reads up to [length] bytes into [buffer] at [offset]; returns -1 at the end. */
    fun read(buffer: ByteArray, offset: Int, length: Int): Int
    fun close()
}

/** Collects what is written in memory (a document read for viewing, small files only). */
class MemorySink(private val max: Long = Long.MAX_VALUE) : ByteSink {
    private var buf = ByteArray(0)
    var size = 0
        private set

    override fun write(data: ByteArray) {
        if (size.toLong() + data.size > max) throw IllegalStateException("too large")
        if (size + data.size > buf.size) {
            val grown = ByteArray(maxOf(buf.size * 2, size + data.size, 64 * 1024))
            buf.copyInto(grown, 0, 0, size)
            buf.fill(0)
            buf = grown
        }
        data.copyInto(buf, size)
        size += data.size
    }

    override fun close() {}

    /** Forgets what was written (overwritten with zeros first). */
    override fun discard() {
        buf.fill(0)
        buf = ByteArray(0)
        size = 0
    }

    /** What was written, as a copy of its own; the buffer is wiped. */
    fun bytes(): ByteArray = buf.copyOf(size).also { discard() }
}

/** A writable file chosen by the user. */
interface ByteSink {
    fun write(data: ByteArray)
    fun close()
    /** Removes what was written (a cancelled or failed write leaves no half file). */
    fun discard() {}
    /** Free bytes where it is written, null where not known (H12: a full storage without an error code). */
    fun freeBytes(): Long? = null
}

/** The areas, in the order they are named everywhere: passwords, 2FA, documents, calendar. */
enum class VaultKindUi { PASS, TOTP, DOCS, CAL }

/** What the app finds in the suite folder. */
enum class SetupStateUi {
    EMPTY,
    COMPLETE,
    /** Vault files without the suite file: a setup that stopped; unlocking completes it. */
    INCOMPLETE,
}

data class VaultUi(
    val id: String,
    val kind: VaultKindUi,
    val name: String,
    val file: String,
    val available: Boolean,
    val entries: Int,
)

data class ProtectionUi(
    val securityKeys: Int,
    /** Key files that can be used instead of a security key. */
    val keyfiles: Int,
    val recoveryCodes: Int,
)

data class ContainerInfoUi(val kind: String, val protection: ProtectionUi)

data class NewVaultUi(val kind: VaultKindUi, val name: String)

data class SetupUi(
    val password: String,
    /** PIN of the security key; null = no security key (then [keyfile] is required). */
    val keyPin: String?,
    val keyLabel: String,
    /** Key file as second factor (instead of or in addition to the key). */
    val keyfile: ByteArray?,
    val keyfileLabel: String,
    val recoveryLabel: String,
    val kdf: KdfChoice,
    val vaults: List<NewVaultUi>,
)

data class OpenVaultUi(
    val file: ByteArray,
    val name: String,
    val password: String,
    val keyPin: String?,
    val keyfile: ByteArray?,
    val recoveryLabel: String,
)

/** What "hand on as a vault" needs besides a security key. */
class ShareVaultUi(
    val vaultId: String,
    val ids: List<String>,
    val name: String,
    /** M15: both factors of this suite. */
    val proof: FactorProofUi,
    val password: String,
    /** A new key file; null with a security key. */
    val keyfile: ByteArray?,
    val keyLabel: String,
    val keyfileLabel: String,
    val recoveryLabel: String,
)

/** Changes of a vault opened on its own that are not in its file yet ([key] names them). */
class PendingSingleUi(
    val key: String,
    val file: SuiteFileRef,
    /** The file was changed elsewhere since: writing would lose that. */
    val changedElsewhere: Boolean = false,
    /** The file there is a readable vault (giving up keeps that). */
    val fileIntact: Boolean = true,
    /** Its file is known (else only "save as" and giving up). */
    val targetKnown: Boolean = true,
    /** Its file is gone or cannot be read now (writing would create it anew). */
    val missing: Boolean = false,
    /** A GhulDocs vault: written back only while it is open, so only giving up is left. */
    val docs: Boolean = false,
)

/** How a vault opened on its own is written back into its file. */
enum class SingleWrite {
    /** Only when the file is as this vault left it. */
    CHECKED,
    /** Also when the file is gone (but not when it is back, changed elsewhere). */
    ANEW,
    /** Whatever the file holds now. */
    OVERWRITE,
}

/** How writing a vault opened on its own back into its file went. */
enum class SingleSaveUi {
    SAVED,
    /** Writing did not work (storage away?): try again later or save as. */
    FAILED,
    /** The file was changed elsewhere since: overwrite or save as. */
    CHANGED_ELSEWHERE,
    /** The file is gone or cannot be read: write it anew (overwrite) or save as. */
    MISSING,
}

/** A new vault file of picked entries and its recovery code. */
class SharedVaultUi(val kind: VaultKindUi, val data: ByteArray, val recoveryCode: String)

data class QuickUnlockReady(
    val biometric: Boolean,
    val pin: Boolean,
    /** Switched on, but from an older version: it must be set up again (with both factors). */
    val needsSetup: Boolean = false,
) {
    val any: Boolean get() = biometric || pin
}

/** Texts of the printable emergency sheet (localized by the UI). */
data class EmergencySheetText(
    val title: String,
    val intro: String,
    val codeLabel: String,
    val created: String,
    val steps: List<String>,
    val warning: String,
)

data class KdfChoice(val memoryKib: Int, val iterations: Int, val estimatedMs: Long)

data class KeyStatus(val product: String, val pinSet: Boolean, val supportsHmacSecret: Boolean, val minPinLength: Int)

data class EntrySummaryUi(
    val vaultId: String,
    val vaultName: String,
    val id: String,
    val type: String,
    val title: String,
    val subtitle: String,
    val favorite: Boolean = false,
    val hasTotp: Boolean = false,
)

enum class FieldType { TEXT, SECRET, URL, INT, BOOL, LIST }

/** A field; lists are represented one item per line. */
data class FieldUi(val name: String, val type: FieldType, val value: String)

data class EntryUi(
    val vaultId: String,
    val id: String?,
    val type: String,
    val fields: List<FieldUi>,
    val modified: Long = 0,
) {
    fun value(name: String): String = fields.firstOrNull { it.name == name }?.value.orEmpty()
}

data class TotpCodeUi(
    val vaultId: String,
    val vaultName: String,
    val id: String,
    val issuer: String,
    val account: String,
    val code: String,
    val nextCode: String,
    val period: Int,
    val remaining: Int,
    val favorite: Boolean = false,
    val modified: Long = 0,
)

data class PasswordHistoryUi(val password: String, val changed: Long)

data class AttachmentUi(val name: String, val size: Long)

data class EntryExtrasUi(val favorite: Boolean, val created: Long, val history: List<PasswordHistoryUi>, val attachments: List<AttachmentUi>)

enum class SecurityIssueKindUi { REUSED, WEAK, NO_SECOND_FACTOR }

data class SecurityIssueUi(val vaultId: String, val vaultName: String, val id: String, val title: String, val kind: SecurityIssueKindUi)

data class SecurityReportUi(val score: Int, val checked: Int, val issues: List<SecurityIssueUi>)

data class TotpNowUi(val code: String, val nextCode: String, val remaining: Int, val period: Int)

enum class OtpAlgorithmUi { SHA1, SHA256, SHA512, STEAM }

data class TotpInputUi(
    val issuer: String,
    val account: String,
    val secret: String,
    val algorithm: OtpAlgorithmUi = OtpAlgorithmUi.SHA1,
    val digits: Int = 6,
    val period: Int = 30,
)

enum class SlotType { SECURITY_KEY, KEYFILE, RECOVERY_CODE }

data class SlotUi(val id: String, val type: SlotType, val label: String, val created: Long)

data class PasswordOptionsUi(
    val length: Int = 20,
    val lowercase: Boolean = true,
    val uppercase: Boolean = true,
    val digits: Boolean = true,
    val symbols: Boolean = true,
    val avoidAmbiguous: Boolean = true,
)

data class ImportPreviewUi(
    val format: String,
    val entries: Int,
    val totps: Int,
    val totpsInLogins: Int,
    val skipped: Int,
    /** Which items cannot be converted, and why. */
    val skippedItems: List<SkippedImportUi> = emptyList(),
)

data class ImportResultUi(
    val entries: Int,
    val totps: Int,
    val duplicates: Int,
    val skipped: Int,
    val skippedItems: List<SkippedImportUi> = emptyList(),
    /** Skipped items kept as notes with their original data. */
    val kept: Int = 0,
)

enum class SkipReasonUi { HOTP, UNSUPPORTED, INVALID_CODE }

/** An item of an import file that could not be converted. */
data class SkippedImportUi(
    val name: String,
    /** Type in the source app (e.g. alias, hotp, sshKey). */
    val kind: String,
    val reason: SkipReasonUi,
    val detail: String,
    /** A 2FA code that stays in its imported login (only GhulAuth does not get it). */
    val inEntry: Boolean,
)

/** Keep what cannot be converted as notes in GhulPass (folder, field names and reasons in the app's language). */
data class KeepSkippedUi(
    val folder: String,
    val reasonLabel: String,
    val dataLabel: String,
    val reasonHotp: String,
    val reasonUnsupported: String,
    val reasonInvalid: String,
)

enum class VaultErrorKind {
    /** A vault opened on its own is still being written back: try again in a moment. */
    SINGLE_BUSY,
    /** Changes of an earlier opening of this file wait: write them first. */
    SINGLE_PENDING,
    /** A file from a newer GhulSuite: update the app. */
    UPDATE_REQUIRED,
    /** A file of an older development version (an older format): not read any more. */
    OLD_FORMAT,
    NO_KEY,
    PERMISSION_DENIED,
    NOT_A_FIDO_KEY,
    TRANSPORT,
    TIMEOUT,
    PIN_NOT_SET,
    PIN_INVALID,
    PIN_BLOCKED,
    PIN_AUTH_BLOCKED,
    PIN_POLICY,
    SECURITY_KEY_REQUIRED,
    KEYFILE_REQUIRED,
    WRONG_KEYFILE,
    DIFFERENT_PROTECTION,
    ALREADY_EXISTS,
    LAST_FACTOR,
    /** A security key needs a backup (a second key or a key file). */
    BACKUP_FACTOR_REQUIRED,
    IMPORT_ENCRYPTED,
    IMPORT_UNKNOWN,
    QUICK_UNAVAILABLE,
    QUICK_EXPIRED,
    /** The quick unlock file of an older version (validity not in the file): set it up again. */
    QUICK_OUTDATED,
    QUICK_INVALID,
    /** r13 N3: the unlock met a lock that came meanwhile (e.g. the screen went off): just once more. */
    TRY_AGAIN,
    /** Windows: another program holds the clipboard; nothing was copied (r9 dieb N6). */
    CLIPBOARD_BUSY,
    /** The device is locked: the app PIN's keys work only while it is unlocked (nothing used up). */
    DEVICE_LOCKED,
    BIOMETRIC_CANCELLED,
    /** Too many failed attempts: the system blocks biometrics for a while (or until the device PIN). */
    BIOMETRIC_LOCKOUT,
    BIOMETRIC_CHANGED,
    HMAC_UNSUPPORTED,
    KEY_NOT_REGISTERED,
    NOT_CONFIRMED,
    KEY_PROTOCOL,
    WRONG_PASSWORD,
    WRONG_SECOND_FACTOR,
    INVALID_RECOVERY_CODE,
    LOCKED,
    CORRUPTED,
    /** A file opens with the password, but its seal is wrong: it was changed without the password. */
    TAMPERED,
    /** A suite file with an older protection than this device saw: it is not opened. */
    PROTECTION_OUTDATED,
    STORAGE,
    INVALID_INPUT,
    NETWORK,
    CERT_UNTRUSTED,
    NOT_CONFIGURED,
    NOT_FOUND,
    /** GhulDocs holds as much as it may: [VaultException.detail] is "used|limit" in bytes. */
    DOCS_LIMIT,
    /** H5: the deletion of the suite began and stopped part way; deleting again removes the rest. */
    DELETE_INCOMPLETE,
    /** H12: not enough free space; detail `free|needed` (bytes). */
    NO_SPACE,
    OTHER,
}

class VaultException(val kind: VaultErrorKind, val detail: String = "", val retries: Int? = null) :
    Exception("$kind: $detail")

private val NOT_WAITING = kotlinx.coroutines.flow.MutableStateFlow(false)

/**
 * The auto-lock time in effect: right after opening with the recovery code at least 5 minutes
 * (H3: the password alone repairs the protection only until the lock; finding a new key takes a
 * while). Screen off and the instant lock still lock at once.
 */
/**
 * r9 dieb N4/N5: how long without any input GhulSuite stays open while it is in front (Android and
 * Windows alike): the automatic lock's time, at least 5 minutes (reading a long note must not lock),
 * at most 15 (a longer time for the background does not keep an unwatched app open).
 */
fun idleLockMillis(autoLockSeconds: Int): Long = (autoLockSeconds * 1000L).coerceIn(IDLE_MIN_MS, IDLE_MAX_MS)

const val IDLE_MIN_MS = 5 * 60 * 1000L

/**
 * r10 M3, r11 M2: whether the idle lock locks now after [idleMs] without input. Unsaved input in
 * an editor or a running action (e.g. waiting for the key) only defer it, up to
 * [idleLockLatestMillis]: an app left alone never stays open for good.
 */
fun idleLockDue(idleMs: Long, autoLockSeconds: Int, editorDirty: Boolean, busy: Boolean): Boolean =
    idleMs >= idleLockLatestMillis(autoLockSeconds) || (!editorDirty && !busy && idleMs >= idleLockMillis(autoLockSeconds))

/** The latest idle lock, also over unsaved input: twice the idle time, at least [IDLE_MAX_MS]. */
fun idleLockLatestMillis(autoLockSeconds: Int): Long = maxOf(IDLE_MAX_MS, 2 * idleLockMillis(autoLockSeconds))

/** How long before that latest lock the app warns that unsaved input will be lost. */
const val IDLE_WARN_MS = 60_000L

/** r11 M2: whether to warn now that the lock comes soon and takes the unsaved input with it. */
fun idleLockWarning(idleMs: Long, autoLockSeconds: Int, editorDirty: Boolean): Boolean =
    editorDirty && idleMs >= idleLockMillis(autoLockSeconds) && idleMs >= idleLockLatestMillis(autoLockSeconds) - IDLE_WARN_MS
const val IDLE_MAX_MS = 15 * 60 * 1000L

fun VaultBackend.autoLockSecondsNow(): Int =
    if (settings.autoLockSeconds < 300 && runCatching { openedWithRecoveryCode() }.getOrDefault(false)) 300 else settings.autoLockSeconds
