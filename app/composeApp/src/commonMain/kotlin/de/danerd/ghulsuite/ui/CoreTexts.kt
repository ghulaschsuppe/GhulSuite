// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString

/**
 * U5: the core reports details of its errors in English (they also go into logs and tests). Every
 * detail a user can see is translated here; tools/check-texts.py fails if the core gains one
 * that is missing. Unknown details (e.g. messages of the operating system) are shown as they are.
 */
private val CORE_TEXTS: Map<String, StringResource> = mapOf(
    "too large to share" to Res.string.core_share_big,
    "name required" to Res.string.core_name_req,
    "a suite file cannot be added as a vault" to Res.string.core_suite_not_vault,
    "this is a suite file, open its folder instead" to Res.string.core_suite_file,
    "Drive block missing" to Res.string.core_drive_block_missing,
    "Drive block does not match" to Res.string.core_drive_block_mismatch,
    "Drive data damaged or from another key" to Res.string.core_drive_damaged,
    "key did not verify the PIN" to Res.string.core_key_pin_unverified,
    "hmac-secret output too short" to Res.string.core_key_bad_answer,
    "message cannot be read" to Res.string.core_msg_unreadable,
    "timeout" to Res.string.core_timeout,
    "image not available" to Res.string.core_image_na,
    "GhulDrive is being changed on another device, please try again" to Res.string.core_drive_busy,
    "the server module needs no folder" to Res.string.core_php_no_folder,
    "the server folder belongs to another suite" to Res.string.core_folder_other_suite,
    "the server module shares without setup" to Res.string.core_php_shares,
    "the module is uploaded with FTPS" to Res.string.core_php_ftps,
    "download limits need the server module" to Res.string.core_dl_limit_php,
    "not a GhulSuite backup" to Res.string.core_not_backup,
    "backup too large" to Res.string.core_backup_big,
    "the folder already contains a suite" to Res.string.core_folder_has_suite,
    "choose a folder outside the suite folder" to Res.string.core_folder_outside,
    "not a GhulPass vault" to Res.string.core_not_pass,
    "not a GhulAuth vault" to Res.string.core_not_auth,
    "not a calendar vault" to Res.string.core_not_cal,
    "target vault is of another kind" to Res.string.core_vault_other_kind,
    "this vault has no security key" to Res.string.core_vault_no_key,
    "invalid access key" to Res.string.core_access_key,
    "too many attachments" to Res.string.core_attach_many,
    "attachment too large (max 10 MB)" to Res.string.core_attach_big,
    "draft attachments larger than 10 MB" to Res.string.core_draft_attach,
    "too large to schedule (max 10 MB)" to Res.string.core_schedule_big,
    "too large (max 200 MB)" to Res.string.core_big_200,
    "chunk too large" to Res.string.core_chunk_big,
    "the key has no e-mail address" to Res.string.core_pgp_no_mail,
    "cannot decrypt" to Res.string.core_no_decrypt,
    "broken encrypted message" to Res.string.core_broken_enc,
    "no recipient" to Res.string.core_no_recipient,
    "invalid e-mail address" to Res.string.core_bad_email,
    "address required" to Res.string.core_address_req,
    "IMAP is only allowed with TLS (port 993)" to Res.string.core_imap_tls,
    "host and user required" to Res.string.core_host_user,
    "the address must start with https://" to Res.string.core_https_only,
    "GhulDrive's server is changed in GhulDrive" to Res.string.core_drive_server_here,
    "nothing to share" to Res.string.core_nothing_share,
    "no website or app" to Res.string.core_no_site,
    "no such group" to Res.string.core_no_group,
    "no password" to Res.string.core_no_password,
    "password required" to Res.string.core_password_req,
    "password or key required" to Res.string.core_password_or_key,
    "password must not be empty" to Res.string.core_password_empty,
    "password too short (at least 8 characters)" to Res.string.core_password_short,
    "select at least one character class" to Res.string.core_char_class,
    "length must be 8 to 128" to Res.string.core_length_range,
    "3 to 12 words" to Res.string.core_words_range,
    "secret is not valid Base32" to Res.string.core_base32,
    "period must be 1 to 3600 seconds" to Res.string.core_period,
    "digits must be 6 to 8" to Res.string.core_digits,
    "Steam codes have 5 characters" to Res.string.core_steam,
    "issuer or account is required" to Res.string.core_issuer,
    "recovery codes are replaced, not removed" to Res.string.core_code_replace,
    "a security key or key file is required" to Res.string.core_factor_req,
    "key and PIN must be given together" to Res.string.core_key_pin_pair,
    "a PIN is already set" to Res.string.core_pin_set,
    "PIN required" to Res.string.core_pin_req,
    "invalid time" to Res.string.core_bad_time,
    "invalid range" to Res.string.core_bad_range,
    "invalid slot id" to Res.string.core_bad_slot,
    "invalid quick unlock name" to Res.string.core_bad_quick,
    "invalid name" to Res.string.core_bad_name,
    "invalid label" to Res.string.core_bad_label,
    "invalid file name" to Res.string.core_bad_file,
    "invalid entry type" to Res.string.core_bad_entry,
    "cannot move a folder into itself" to Res.string.core_move_self,
    "only the trash and the spam folder can be emptied" to Res.string.core_empty_only_trash,
    "the server asks for password parameters far more expensive than this suite's; not used" to Res.string.core_kdf_expensive,
)

private val PIN_MIN = Regex("PIN must have at least (\\d+) characters")

/** The detail in the app's language, or null if it is not one of the core's known texts. */
suspend fun coreText(detail: String): String? {
    CORE_TEXTS[detail.trim()]?.let { return getString(it) }
    PIN_MIN.matchEntire(detail.trim())?.let { return getString(Res.string.core_pin_min, it.groupValues[1]) }
    return null
}
