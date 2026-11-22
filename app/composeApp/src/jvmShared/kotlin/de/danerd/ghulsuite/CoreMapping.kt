// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared by Android and Windows: conversions between the core (UniFFI) and the UI model.
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.FidoException
import de.danerd.ghulsuite.core.FieldData
import de.danerd.ghulsuite.core.FieldValue
import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.ProtectionInfo
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.model.FieldType
import de.danerd.ghulsuite.model.FieldUi
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi

private const val HEX = "0123456789abcdef"

fun ByteArray.toHex(): String = buildString(size * 2) {
    for (b in this@toHex) {
        val v = b.toInt() and 0xff
        append(HEX[v shr 4]); append(HEX[v and 0x0f])
    }
}

fun String.hexToBytes(): ByteArray = ByteArray(length / 2) { i -> substring(2 * i, 2 * i + 2).toInt(16).toByte() }

internal fun VaultKindUi.toCore() = when (this) {
    VaultKindUi.PASS -> VaultKind.PASS
    VaultKindUi.TOTP -> VaultKind.TOTP
    VaultKindUi.DOCS -> VaultKind.DOCS
    VaultKindUi.CAL -> VaultKind.CAL
}

internal fun VaultKind.toUi() = when (this) {
    VaultKind.PASS -> VaultKindUi.PASS
    VaultKind.TOTP -> VaultKindUi.TOTP
    VaultKind.CAL -> VaultKindUi.CAL
    VaultKind.DOCS -> VaultKindUi.DOCS
}

internal fun ProtectionInfo.toUi() = ProtectionUi(securityKeys.toInt(), keyfiles.toInt(), recoveryCodes.toInt())

internal fun FieldData.toUi(): FieldUi = when (val v = value) {
    is FieldValue.Text -> FieldUi(name, FieldType.TEXT, v.value)
    is FieldValue.Secret -> FieldUi(name, FieldType.SECRET, v.value)
    is FieldValue.Url -> FieldUi(name, FieldType.URL, v.value)
    is FieldValue.Number -> FieldUi(name, FieldType.INT, v.value.toString())
    is FieldValue.Flag -> FieldUi(name, FieldType.BOOL, v.value.toString())
    is FieldValue.Items -> FieldUi(name, FieldType.LIST, v.values.joinToString("\n"))
}

internal fun FieldUi.toCore(): FieldData = FieldData(
    name,
    when (type) {
        FieldType.TEXT -> FieldValue.Text(value)
        FieldType.SECRET -> FieldValue.Secret(value)
        FieldType.URL -> FieldValue.Url(value)
        FieldType.INT -> FieldValue.Number(value.toLongOrNull() ?: 0)
        FieldType.BOOL -> FieldValue.Flag(value.toBoolean())
        FieldType.LIST -> FieldValue.Items(value.lines().filter { it.isNotBlank() })
    },
)

/** Runs [block] and converts core errors into [VaultException]. */
internal inline fun <T> mapped(block: () -> T): T = try {
    block()
} catch (e: GhulException) {
    throw e.toVault()
} catch (e: FidoException) {
    throw e.toVault()
}

internal fun GhulException.toVault(): VaultException {
    val d = message.orEmpty()
    return when (this) {
        is GhulException.Transport -> VaultException(VaultErrorKind.TRANSPORT, msg)
        is GhulException.Timeout -> VaultException(VaultErrorKind.TIMEOUT, d)
        is GhulException.PinNotSet -> VaultException(VaultErrorKind.PIN_NOT_SET, d)
        is GhulException.PinInvalid -> VaultException(VaultErrorKind.PIN_INVALID, d, retries.toInt())
        is GhulException.PinBlocked -> VaultException(VaultErrorKind.PIN_BLOCKED, d)
        is GhulException.PinAuthBlocked -> VaultException(VaultErrorKind.PIN_AUTH_BLOCKED, d)
        is GhulException.PinPolicyViolation -> VaultException(VaultErrorKind.PIN_POLICY, d)
        is GhulException.HmacSecretUnsupported -> VaultException(VaultErrorKind.HMAC_UNSUPPORTED, d)
        is GhulException.KeyNotRegistered -> VaultException(VaultErrorKind.KEY_NOT_REGISTERED, d)
        is GhulException.NotConfirmed -> VaultException(VaultErrorKind.NOT_CONFIRMED, d)
        is GhulException.KeyProtocol -> VaultException(VaultErrorKind.KEY_PROTOCOL, msg)
        is GhulException.WrongPassword -> VaultException(VaultErrorKind.WRONG_PASSWORD, d)
        is GhulException.WrongSecondFactor -> VaultException(VaultErrorKind.WRONG_SECOND_FACTOR, d)
        is GhulException.InvalidRecoveryCode -> VaultException(VaultErrorKind.INVALID_RECOVERY_CODE, d)
        is GhulException.Locked -> VaultException(VaultErrorKind.LOCKED, d)
        is GhulException.Corrupted -> VaultException(VaultErrorKind.CORRUPTED, msg)
        is GhulException.Tampered -> VaultException(VaultErrorKind.TAMPERED, d)
        is GhulException.ProtectionOutdated -> VaultException(VaultErrorKind.PROTECTION_OUTDATED, d)
        is GhulException.Storage -> VaultException(VaultErrorKind.STORAGE, msg)
        is GhulException.InvalidInput -> VaultException(VaultErrorKind.INVALID_INPUT, msg)
        is GhulException.SecurityKeyRequired -> VaultException(VaultErrorKind.SECURITY_KEY_REQUIRED, d)
        is GhulException.KeyfileRequired -> VaultException(VaultErrorKind.KEYFILE_REQUIRED, d)
        is GhulException.WrongKeyfile -> VaultException(VaultErrorKind.WRONG_KEYFILE, d)
        is GhulException.DifferentProtection -> VaultException(VaultErrorKind.DIFFERENT_PROTECTION, d)
        is GhulException.AlreadyExists -> VaultException(VaultErrorKind.ALREADY_EXISTS, d)
        is GhulException.LastFactor -> VaultException(VaultErrorKind.LAST_FACTOR, d)
        is GhulException.BackupFactorRequired -> VaultException(VaultErrorKind.BACKUP_FACTOR_REQUIRED, d)
        is GhulException.ImportEncrypted -> VaultException(VaultErrorKind.IMPORT_ENCRYPTED, d)
        is GhulException.ImportUnknownFormat -> VaultException(VaultErrorKind.IMPORT_UNKNOWN, d)
        is GhulException.NoVault -> VaultException(VaultErrorKind.STORAGE, d)
        is GhulException.QuickUnlockInvalid -> VaultException(VaultErrorKind.QUICK_INVALID, d)
        is GhulException.QuickUnlockExpired -> VaultException(VaultErrorKind.QUICK_EXPIRED, d)
        is GhulException.QuickUnlockOutdated -> VaultException(VaultErrorKind.QUICK_OUTDATED, d)
        is GhulException.QuickUnlockUnavailable -> VaultException(VaultErrorKind.QUICK_UNAVAILABLE, d)
        is GhulException.Network -> VaultException(VaultErrorKind.NETWORK, msg)
        is GhulException.CertUntrusted -> VaultException(VaultErrorKind.CERT_UNTRUSTED, fingerprint)
        is GhulException.NotConfigured -> VaultException(VaultErrorKind.NOT_CONFIGURED, d)
        is GhulException.NotFound -> VaultException(VaultErrorKind.NOT_FOUND, d)
        is GhulException.UpdateRequired -> VaultException(VaultErrorKind.UPDATE_REQUIRED, d)
        is GhulException.OldFormat -> VaultException(VaultErrorKind.OLD_FORMAT, d)
        is GhulException.DocsLimit -> VaultException(VaultErrorKind.DOCS_LIMIT, "$used|$limit|$needed|$suite")
        is GhulException.DeleteIncomplete -> VaultException(VaultErrorKind.DELETE_INCOMPLETE, msg)
        is GhulException.NoSpace -> VaultException(VaultErrorKind.NO_SPACE, "$free|$needed")
    }
}

internal fun FidoException.toVault(): VaultException {
    val d = message.orEmpty()
    return when (this) {
        is FidoException.Transport -> VaultException(VaultErrorKind.TRANSPORT, msg)
        is FidoException.Timeout -> VaultException(VaultErrorKind.TIMEOUT, d)
        is FidoException.PinNotSet -> VaultException(VaultErrorKind.PIN_NOT_SET, d)
        is FidoException.PinInvalid -> VaultException(VaultErrorKind.PIN_INVALID, d, retries.toInt())
        is FidoException.PinBlocked -> VaultException(VaultErrorKind.PIN_BLOCKED, d)
        is FidoException.PinAuthBlocked -> VaultException(VaultErrorKind.PIN_AUTH_BLOCKED, d)
        is FidoException.PinPolicyViolation -> VaultException(VaultErrorKind.PIN_POLICY, d)
        is FidoException.HmacSecretUnsupported -> VaultException(VaultErrorKind.HMAC_UNSUPPORTED, d)
        is FidoException.NoCredentials -> VaultException(VaultErrorKind.KEY_NOT_REGISTERED, d)
        is FidoException.NotConfirmed -> VaultException(VaultErrorKind.NOT_CONFIRMED, d)
        is FidoException.InvalidInput -> VaultException(VaultErrorKind.INVALID_INPUT, msg)
        else -> VaultException(VaultErrorKind.KEY_PROTOCOL, d)
    }
}

/**
 * A file name that stays inside the share folder: no path separators, control characters or
 * characters Windows does not allow (":" would write an alternate data stream instead of the
 * file), no trailing dots or spaces (Windows drops them, "a.exe." would be "a.exe"), not "." or
 * "..", and unique among the files shared together.
 */
private val WINDOWS_DEVICES = setOf("CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$") +
    (0..9).flatMap { listOf("COM$it", "LPT$it") } + listOf("COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³")

internal fun safeFileName(name: String, used: MutableSet<String>): String {
    val clean = name.map { if (it in "/\\<>:\"|?*" || it.code < 32) '_' else it }.joinToString("").trim().trimEnd('.', ' ')
    // Shortened, the ending stays: cutting "aaa…a.exe.txt" must not leave "aaa…a.exe".
    val dot = clean.lastIndexOf('.').takeIf { it > 0 && clean.length - it <= 16 } ?: clean.length
    var base = if (clean.length <= 120) clean else clean.substring(0, 120 - (clean.length - dot)).trimEnd('.', ' ') + clean.substring(dot)
    if (base.isEmpty() || base.all { it == '.' }) base = "file"
    // r9 N-D5: a name Windows takes for a device ("CON", "NUL.txt", "COM1.jpg", also with spaces
    // before the dot) would write to the device instead of a file.
    if (base.substringBefore('.').trimEnd(' ').uppercase() in WINDOWS_DEVICES) base = "_$base"
    var candidate = base
    var n = 2
    while (!used.add(candidate.lowercase())) {
        val dot = base.lastIndexOf('.').takeIf { it > 0 } ?: base.length
        candidate = base.substring(0, dot) + " ($n)" + base.substring(dot)
        n++
    }
    return candidate
}
