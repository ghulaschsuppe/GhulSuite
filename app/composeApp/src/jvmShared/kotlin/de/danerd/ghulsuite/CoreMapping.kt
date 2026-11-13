// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared by Android and Windows: conversions between the core (UniFFI) and the UI model.
package de.danerd.ghulsuite

import de.danerd.ghulsuite.core.DriveItem
import de.danerd.ghulsuite.core.FidoException
import de.danerd.ghulsuite.core.FieldData
import de.danerd.ghulsuite.core.FieldValue
import de.danerd.ghulsuite.core.GhulException
import de.danerd.ghulsuite.core.MailAccountInput
import de.danerd.ghulsuite.core.MailSecurity
import de.danerd.ghulsuite.core.MailServers
import de.danerd.ghulsuite.core.MailSummary
import de.danerd.ghulsuite.core.ProtectionInfo
import de.danerd.ghulsuite.core.ServerInput
import de.danerd.ghulsuite.core.ServerKind
import de.danerd.ghulsuite.core.ShareInfo
import de.danerd.ghulsuite.core.ShareOptions
import de.danerd.ghulsuite.core.SyncReport
import de.danerd.ghulsuite.core.VaultKind
import de.danerd.ghulsuite.core.VaultSource
import de.danerd.ghulsuite.model.DriveItemUi
import de.danerd.ghulsuite.model.FieldType
import de.danerd.ghulsuite.model.FieldUi
import de.danerd.ghulsuite.model.MailAccountInputUi
import de.danerd.ghulsuite.model.MailSecurityUi
import de.danerd.ghulsuite.model.MailServersUi
import de.danerd.ghulsuite.model.MailSummaryUi
import de.danerd.ghulsuite.model.ProtectionUi
import de.danerd.ghulsuite.model.ServerInputUi
import de.danerd.ghulsuite.model.ServerKindUi
import de.danerd.ghulsuite.model.ShareInfoUi
import de.danerd.ghulsuite.model.ShareOptionsUi
import de.danerd.ghulsuite.model.SyncReportUi
import de.danerd.ghulsuite.model.VaultErrorKind
import de.danerd.ghulsuite.model.VaultException
import de.danerd.ghulsuite.model.VaultKindUi
import de.danerd.ghulsuite.model.VaultSourceUi

private const val HEX = "0123456789abcdef"

fun ByteArray.toHex(): String = buildString(size * 2) {
    for (b in this@toHex) {
        val v = b.toInt() and 0xff
        append(HEX[v shr 4]); append(HEX[v and 0x0f])
    }
}

fun String.hexToBytes(): ByteArray = ByteArray(length / 2) { i -> substring(2 * i, 2 * i + 2).toInt(16).toByte() }

internal fun VaultSourceUi.toCore(): VaultSource = when (this) {
    is VaultSourceUi.InFolder -> VaultSource.InFolder(file)
    is VaultSourceUi.Bytes -> VaultSource.Bytes(data)
}

internal fun VaultKindUi.toCore() = when (this) {
    VaultKindUi.PASS -> VaultKind.PASS
    VaultKindUi.TOTP -> VaultKind.TOTP
    VaultKindUi.CAL -> VaultKind.CAL
}

internal fun VaultKind.toUi() = when (this) {
    VaultKind.PASS -> VaultKindUi.PASS
    VaultKind.TOTP -> VaultKindUi.TOTP
    VaultKind.CAL -> VaultKindUi.CAL
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
        is GhulException.Storage -> VaultException(VaultErrorKind.STORAGE, msg)
        is GhulException.InvalidInput -> VaultException(VaultErrorKind.INVALID_INPUT, msg)
        is GhulException.SecurityKeyRequired -> VaultException(VaultErrorKind.SECURITY_KEY_REQUIRED, d)
        is GhulException.KeyfileRequired -> VaultException(VaultErrorKind.KEYFILE_REQUIRED, d)
        is GhulException.WrongKeyfile -> VaultException(VaultErrorKind.WRONG_KEYFILE, d)
        is GhulException.DifferentProtection -> VaultException(VaultErrorKind.DIFFERENT_PROTECTION, d)
        is GhulException.AlreadyExists -> VaultException(VaultErrorKind.ALREADY_EXISTS, d)
        is GhulException.LastFactor -> VaultException(VaultErrorKind.LAST_FACTOR, d)
        is GhulException.ImportEncrypted -> VaultException(VaultErrorKind.IMPORT_ENCRYPTED, d)
        is GhulException.ImportUnknownFormat -> VaultException(VaultErrorKind.IMPORT_UNKNOWN, d)
        is GhulException.NoVault -> VaultException(VaultErrorKind.STORAGE, d)
        is GhulException.QuickUnlockInvalid -> VaultException(VaultErrorKind.QUICK_INVALID, d)
        is GhulException.QuickUnlockExpired -> VaultException(VaultErrorKind.QUICK_EXPIRED, d)
        is GhulException.QuickUnlockUnavailable -> VaultException(VaultErrorKind.QUICK_UNAVAILABLE, d)
        is GhulException.Network -> VaultException(VaultErrorKind.NETWORK, msg)
        is GhulException.HostKeyUnknown -> VaultException(VaultErrorKind.HOST_KEY_UNKNOWN, fingerprint)
        is GhulException.HostKeyChanged -> VaultException(VaultErrorKind.HOST_KEY_CHANGED, fingerprint)
        is GhulException.CertUntrusted -> VaultException(VaultErrorKind.CERT_UNTRUSTED, fingerprint)
        is GhulException.ServerAuthFailed -> VaultException(VaultErrorKind.SERVER_AUTH, d)
        is GhulException.PasswordChangedElsewhere -> VaultException(VaultErrorKind.PASSWORD_CHANGED_ELSEWHERE, d)
        is GhulException.NotConfigured -> VaultException(VaultErrorKind.NOT_CONFIGURED, d)
        is GhulException.NotFound -> VaultException(VaultErrorKind.NOT_FOUND, d)
        else -> VaultException(VaultErrorKind.OTHER, d)
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

fun ServerInputUi.toCore() = ServerInput(kind.toCore(), name, host, port.toUShort(), user, password, base, url)

internal fun ServerKindUi.toCore() = when (this) {
    ServerKindUi.SFTP -> ServerKind.SFTP
    ServerKindUi.FTPS -> ServerKind.FTPS
    ServerKindUi.PHP -> ServerKind.PHP
}

internal fun ServerKind.toUi() = when (this) {
    ServerKind.SFTP -> ServerKindUi.SFTP
    ServerKind.FTPS -> ServerKindUi.FTPS
    ServerKind.PHP -> ServerKindUi.PHP
}

internal fun ShareOptionsUi.toCore() = ShareOptions(serverId, password, expiresDays.toUInt(), maxDownloads.toUInt())

internal fun ShareInfo.toUi() =
    ShareInfoUi(id, serverId, name, size.toLong(), created, expires, password, maxDownloads.toInt(), downloads, url)

fun mimeOf(name: String): String = when (name.substringAfterLast('.', "").lowercase()) {
    "pdf" -> "application/pdf"
    "jpg", "jpeg" -> "image/jpeg"
    "png" -> "image/png"
    "gif" -> "image/gif"
    "txt" -> "text/plain"
    "zip" -> "application/zip"
    "mp4" -> "video/mp4"
    "mp3" -> "audio/mpeg"
    else -> "application/octet-stream"
}

internal fun SyncReport.toUi() =
    SyncReportUi(uploaded.toInt(), downloaded.toInt(), merged.toInt(), deleted.toInt(), skipped.toInt())

internal fun DriveItem.toUi() = DriveItemUi(id, parent, name, isDir, size.toLong(), modified, favorite, offline, trashed, versions.toInt())

internal fun MailSecurity.toUi() = if (this == MailSecurity.TLS) MailSecurityUi.TLS else MailSecurityUi.STARTTLS

internal fun MailSecurityUi.toCore() = if (this == MailSecurityUi.TLS) MailSecurity.TLS else MailSecurity.START_TLS

internal fun MailServers.toUi() =
    MailServersUi(imapHost, imapPort.toInt(), imapSecurity.toUi(), smtpHost, smtpPort.toInt(), smtpSecurity.toUi())

internal fun MailServersUi.toCore() = MailServers(
    imapHost.trim(), imapPort.toUShort(), imapSecurity.toCore(), smtpHost.trim(), smtpPort.toUShort(), smtpSecurity.toCore(),
)

internal fun MailAccountInputUi.toCore() =
    MailAccountInput(name, displayName, email.trim(), username.trim(), password, servers.toCore(), imapCert, smtpCert, signature)

internal fun MailSummary.toUi() =
    MailSummaryUi(uid.toLong(), from, fromAddress, subject, date, seen, flagged, hasAttachments, size.toLong(), preview, encrypted, labels, threadKey)

/**
 * A file name that stays inside the share folder: no path separators or control characters,
 * not "." or "..", and unique among the files shared together.
 */
internal fun safeFileName(name: String, used: MutableSet<String>): String {
    var base = name.map { if (it == '/' || it == '\\' || it.code < 32) '_' else it }.joinToString("").trim().take(120)
    if (base.isEmpty() || base.all { it == '.' }) base = "file"
    var candidate = base
    var n = 2
    while (!used.add(candidate.lowercase())) {
        val dot = base.lastIndexOf('.').takeIf { it > 0 } ?: base.length
        candidate = base.substring(0, dot) + " ($n)" + base.substring(dot)
        n++
    }
    return candidate
}
