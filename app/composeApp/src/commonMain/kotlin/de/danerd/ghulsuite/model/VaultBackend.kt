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
    /** Servers, synchronisation, GhulDrive and GhulMail. */
    val net: NetBackend

    // --- Storage location and suite
    /** Whether a suite exists in the configured storage location. */
    fun exists(): Boolean
    /** Name of the storage location (app storage or folder name). */
    fun storageName(): String
    /** Uses the app's private storage (`null`) or a folder chosen by the user. */
    fun useStorage(folder: FolderRef?)
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

    /** SSH public key of this device for fetching a suite from a server (created once). */
    fun deviceSshKey(): String
    /**
     * New device: downloads the suite from a server into the empty storage location. Logs in with
     * the password in [input] or this device's SSH key. Without [hostKey] it returns the server's
     * key fingerprint to confirm; afterwards an empty string. Then unlock as usual.
     */
    suspend fun fetchFromServer(input: ServerInputUi, hostKey: String?): String

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
    suspend fun enableBiometricUnlock()
    suspend fun enablePinUnlock(pin: String)
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
    suspend fun exportVault(id: String): ByteArray
    /** Adds a vault file with the suite's protection; DIFFERENT_PROTECTION if it needs its own password. */
    suspend fun addVaultFile(data: ByteArray, name: String): String
    /** Takes over a vault with another protection: opened once with its own password, then switched to the suite's. */
    suspend fun adoptVault(
        source: VaultSourceUi,
        name: String,
        password: String,
        pin: String?,
        keyfile: ByteArray?,
        onTouchNeeded: () -> Unit,
    ): String
    /** Reads the folder again; returns vault files with another protection. */
    suspend fun findNewVaults(): List<FoundVaultUi>
    suspend fun ignoreFoundVault(file: String)
    /** Accepts an older state of a vault file (e.g. a restored backup). */
    suspend fun acceptVaultState(id: String)

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
    suspend fun addSecurityKey(pin: String, label: String, onTouchNeeded: () -> Unit)
    /** Adds a key file as alternative second factor. */
    suspend fun addKeyfile(keyfile: ByteArray, label: String)
    suspend fun newRecoveryCode(label: String): String
    /** Removes a security key or key file (not the last one). */
    suspend fun removeSlot(id: String)
    suspend fun changePassword(current: String, new: String)

    // --- Import and export
    /** What an import file contains (nothing is changed). */
    suspend fun previewImport(data: ByteArray, fileName: String): ImportPreviewUi
    suspend fun importData(
        data: ByteArray,
        fileName: String,
        passVault: String?,
        totpVault: String?,
        totpsFromLogins: Boolean,
    ): ImportResultUi
    /** Unencrypted export of one vault (CSV or otpauth links); needs the master password. */
    suspend fun exportPlain(vaultId: String, password: String): ByteArray
    /** Some entries as unencrypted CSV (or otpauth links), only with the master password. */
    suspend fun exportEntries(vaultId: String, ids: List<String>, password: String): ByteArray = throw UnsupportedOperationException()

    // --- Autofill (Android)
    fun autofillStatus(): AutofillStatus
    fun openAutofillSettings()

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
    /** GhulAuth: account as otpauth link (QR export); clock difference to an own web server. */
    suspend fun totpUri(vaultId: String, id: String): String = ""
    suspend fun clockOffset(nowSeconds: Long): Long? = null
    /** Complete encrypted backup of the suite folder (ZIP) and restoring it into an empty folder. */
    suspend fun suiteBackup(): ByteArray = ByteArray(0)
    suspend fun restoreBackup(data: ByteArray): Int = 0
    /** Writes a file into a folder the user chose once (automatic backup); keeps the newest [keep]. */
    suspend fun writeToFolder(folder: FolderRef, name: String, data: ByteArray, keep: Int) {}
    /** Last crash report of this device, if any. */
    fun crashReport(): String? = null
    /** Breach check (k-anonymity); network. */
    suspend fun pwnedCount(password: String): Int = 0

    /** Copies to the clipboard, marked as sensitive and cleared automatically. */
    fun copySensitive(label: String, text: String)

    /** Opens the system print dialog with the emergency sheet (also "Save as PDF"). */
    fun printEmergencySheet(code: String, text: EmergencySheetText)

    val appVersion: String

    /** false where the system asks for the key PIN in its own dialog (Windows). */
    val keyPinInApp: Boolean get() = true

    /** Windows: no shake, quick tile, fingerprint unlock or Android autofill. */
    val isDesktop: Boolean get() = false
    val coreVersion: String
}

/** A folder chosen by the user (Android: SAF tree URI). */
data class FolderRef(val uri: String, val name: String)

/** A file picked by the user. */
class PickedFile(val name: String, val bytes: ByteArray)

enum class VaultKindUi { PASS, TOTP }

data class VaultUi(
    val id: String,
    val kind: VaultKindUi,
    val name: String,
    val file: String,
    val available: Boolean,
    val entries: Int,
    /** The file is older than the last state the suite has seen. */
    val outdated: Boolean = false,
)

sealed interface VaultSourceUi {
    data class InFolder(val file: String) : VaultSourceUi
    class Bytes(val data: ByteArray) : VaultSourceUi
}

data class FoundVaultUi(val file: String, val kind: VaultKindUi, val protection: ProtectionUi)

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

data class QuickUnlockReady(val biometric: Boolean, val pin: Boolean) {
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

enum class OtpAlgorithmUi { SHA1, SHA256, SHA512 }

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

data class ImportPreviewUi(val format: String, val entries: Int, val totps: Int, val totpsInLogins: Int, val skipped: Int)

data class ImportResultUi(val entries: Int, val totps: Int, val duplicates: Int, val skipped: Int)

enum class AutofillStatus { UNSUPPORTED, OFF, ON }

enum class VaultErrorKind {
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
    IMPORT_ENCRYPTED,
    IMPORT_UNKNOWN,
    QUICK_UNAVAILABLE,
    QUICK_EXPIRED,
    QUICK_INVALID,
    BIOMETRIC_CANCELLED,
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
    STORAGE,
    INVALID_INPUT,
    NETWORK,
    HOST_KEY_UNKNOWN,
    HOST_KEY_CHANGED,
    CERT_UNTRUSTED,
    SERVER_AUTH,
    PASSWORD_CHANGED_ELSEWHERE,
    NOT_CONFIGURED,
    NOT_FOUND,
    OTHER,
}

class VaultException(val kind: VaultErrorKind, val detail: String = "", val retries: Int? = null) :
    Exception("$kind: $detail")

private val NOT_WAITING = kotlinx.coroutines.flow.MutableStateFlow(false)
