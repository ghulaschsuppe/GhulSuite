// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite.ui

import de.danerd.ghulsuite.resources.Res
import de.danerd.ghulsuite.resources.*
import org.jetbrains.compose.resources.ResourceEnvironment
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getPluralString
import org.jetbrains.compose.resources.getString

/**
 * U5: the core reports details of its errors in English (they also go into logs and tests). Every
 * detail a user can see is translated here; tools/check-texts.py fails if the core gains one
 * that is missing. Unknown details (e.g. messages of the operating system) are shown as they are.
 */
private val CORE_TEXTS: Map<String, StringResource> = mapOf(
    "name required" to Res.string.core_name_req,
    "either a key file or a security key" to Res.string.core_single_one_factor,
    "no vault is open on its own" to Res.string.core_single_none,
    "only GhulPass and GhulAuth entries can be handed on" to Res.string.core_single_kinds,
    "password too short" to Res.string.core_pw_short,
    "not while a vault is open on its own" to Res.string.core_single_not_suite,
    "a vault of the suite cannot be read" to Res.string.core_vault_unreadable,
    "a suite file cannot be added as a vault" to Res.string.core_suite_not_vault,
    "this is a suite file, open its folder instead" to Res.string.core_suite_file,
    "key did not verify the PIN" to Res.string.core_key_pin_unverified,
    "hmac-secret output too short" to Res.string.core_key_bad_answer,
    "timeout" to Res.string.core_timeout,
    "server address not found" to Res.string.core_host_not_found,
    "connection refused" to Res.string.core_refused,
    "not a GhulSuite file" to Res.string.file_not_suite,
    // M10: the start of a suite file without its end (saving was interrupted).
    "the suite file is incomplete" to Res.string.file_incomplete,
    "the master password is too short" to Res.string.core_mpw_short,
    "the master password is too weak" to Res.string.core_mpw_weak,
    "replacing the suite on this device needs its proof" to Res.string.core_replace_proof,
    "the working copy changed since the file was checked" to Res.string.core_copy_changed,
    "documents are being added or copied, try again when that is done" to Res.string.core_docs_busy,
    "folders cannot be nested deeper" to Res.string.core_folders_deep,
    "a GhulDocs vault holds no more folders" to Res.string.core_folders_many,
    "the file holds too many events" to Res.string.core_ics_many,
    "quick unlock needs a validity" to Res.string.core_quick_validity,
    "only a security key over USB or NFC can be registered" to Res.string.core_only_usb_nfc,
    "the vault could not be removed: its file cannot be deleted right now" to Res.string.core_vault_not_removed,
    // The app PIN (M19). The last four mean the app handed over something wrong: set it up again.
    "the app PIN needs at least 8 digits" to Res.string.wz_pin_rule,
    "the app PIN has functions of its own" to Res.string.core_app_pin_internal,
    "a tag has 32 bytes" to Res.string.core_app_pin_internal,
    "no such slot" to Res.string.core_app_pin_internal,
    "not a challenge of the app PIN" to Res.string.core_app_pin_internal,
    "two keys of 32 bytes" to Res.string.core_app_pin_internal,
    // From the app (Windows, "Open with app", H4).
    "the copy could not be marked as downloaded" to Res.string.docs_open_no_mark,
    "the update needs PowerShell" to Res.string.upd_no_powershell,
    "an earlier opening of a suite file could not be finished" to Res.string.core_replace_unfinished,
    "an earlier save could not be finished: the files before it cannot be put back yet" to Res.string.core_save_unfinished,
    "the documents before the replace were not found" to Res.string.core_replace_aside_missing,
    // From the app's own file access (reading or writing a suite file as a stream).
    "the file cannot be read" to Res.string.file_unreadable,
    "the file cannot be written" to Res.string.file_not_written,
    "the suite file does not match its list" to Res.string.core_file_mismatch,
    "another state was built meanwhile" to Res.string.core_file_rebuilt,
    "not a GhulPass vault" to Res.string.core_not_pass,
    "not a GhulAuth vault" to Res.string.core_not_auth,
    "not a calendar vault" to Res.string.core_not_cal,
    "not a GhulDocs vault" to Res.string.core_not_docs,
    "a document is damaged or missing" to Res.string.core_doc_damaged,
    "a folder cannot go into itself" to Res.string.core_doc_folder_self,
    "a folder has no content" to Res.string.core_doc_folder_content,
    "invalid id" to Res.string.core_doc_bad_id,
    "invalid name" to Res.string.core_doc_bad_name,
    "documents cannot be stored in this folder" to Res.string.core_docs_no_place,
    "a GhulDocs vault goes with its documents" to Res.string.core_docs_with_documents,
    "the index of a GhulDocs vault is invalid" to Res.string.core_docs_index_bad,
    "this is a GhulDocs package, not a suite file" to Res.string.core_docs_package_not_suite,
    "this is a suite file, not a GhulDocs package" to Res.string.core_suite_not_docs_package,
    "a suite has at most 3 GhulDocs vaults" to Res.string.docs_vaults_full,
    "a suite file holds at most 3 GhulDocs vaults" to Res.string.core_docs_vaults_many,
    "target vault is of another kind" to Res.string.core_vault_other_kind,
    "this vault has no security key" to Res.string.core_vault_no_key,
    "too many attachments" to Res.string.core_attach_many,
    "attachment too large (max 10 MB)" to Res.string.core_attach_big,
    "the suite file would be too large" to Res.string.core_suite_too_large,
    "the vaults besides GhulDocs are full" to Res.string.core_other_full,
    // r9 (attackers' review): limits of the core.
    "the app PIN is too easy to guess: no repeated digits or blocks, no runs, no dates" to Res.string.pin_too_simple,
    "this vault is full: one vault file holds at most 128 MiB" to Res.string.core_vault_full,
    "the calendar file is larger than 16 MiB" to Res.string.core_ics_too_large,
    "too many columns or fields in this CSV file" to Res.string.core_csv_too_wide,
    "the documents before could not be put back, they were kept in a folder of their own" to Res.string.core_docs_kept_aside,
    "no such group" to Res.string.core_no_group,
    "no password" to Res.string.core_no_password,
    "password must not be empty" to Res.string.core_password_empty,
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
    "invalid label" to Res.string.core_bad_label,
    "invalid file name" to Res.string.core_bad_file,
    "invalid entry type" to Res.string.core_bad_entry,
    "invalid repository name" to Res.string.core_upd_repo,
    "the update server refused the request, please try again later" to Res.string.core_upd_refused,
    "update information unreadable" to Res.string.core_upd_unreadable,
    "the release has no signed checksums" to Res.string.core_upd_no_sums,
    "the signature of the update is not valid" to Res.string.core_upd_bad_sig,
    "the update file is not in the signed checksums" to Res.string.core_upd_not_listed,
    "update file size or checksum invalid" to Res.string.core_upd_bad_info,
    "the update file does not match its checksum" to Res.string.core_upd_mismatch,
    "the release has more than one file for this platform" to Res.string.core_upd_ambiguous,
    "update address not allowed" to Res.string.core_upd_host,
    "redirect without address" to Res.string.core_upd_redirect,
    "too many redirects" to Res.string.core_upd_redirects,
    "checksums of the update unreadable" to Res.string.core_upd_sums_unreadable,
    "this build has no update key" to Res.string.core_upd_no_key,
)

private val PIN_MIN = Regex("PIN must have at least (\\d+) characters")

/** The detail in the app's language, or null if it is not one of the core's known texts. */
suspend fun coreText(detail: String): String? {
    CORE_TEXTS[detail.trim()]?.let { return getString(it) }
    PIN_MIN.matchEntire(detail.trim())?.let { return getPluralString(Res.plurals.core_pin_min, it.groupValues[1].toIntOrNull() ?: 0, it.groupValues[1]) }
    return null
}

/** Like [coreText], in the language of the screen `env` belongs to (from a composable). */
suspend fun coreText(env: ResourceEnvironment, detail: String): String? {
    CORE_TEXTS[detail.trim()]?.let { return getString(env, it) }
    PIN_MIN.matchEntire(detail.trim())?.let { return getPluralString(env, Res.plurals.core_pin_min, it.groupValues[1].toIntOrNull() ?: 0, it.groupValues[1]) }
    return null
}
