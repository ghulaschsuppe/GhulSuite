// SPDX-License-Identifier: AGPL-3.0-or-later
//! Archiving mails in GhulDrive: a copy of each mail as an `.eml` file, by account and year
//! (`Mail-Archiv/<account>/<year>/2024-05-03 Erika - Treffen (1a2b3c4d).eml`).
//!
//! Nothing happens at the provider: the mail stays there until the user deletes it there. The
//! copy is GhulDrive's: encrypted, on every device, with its trash. The mail archive is found by
//! the id of its folder (stored in the Mail file), so renaming or moving it keeps it working.
//! The short code at the end of a name comes from the Message-ID: a mail already in the year's
//! folder is skipped, even if the rest of its name was changed.

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::areas::{area_setting, put_area_setting};
use super::error::{GhulError, Result};
use super::mail::{account, message, unwrap_message, MailMessage};
use super::suite::{GhulSuite, VaultKind};
use crate::net::imap;

/// The id of the archive's top folder in GhulDrive (in the Mail file).
const ARCHIVE_KEY: &str = "drive_archive";
/// Name of the top folder when it is created.
const ARCHIVE_NAME: &str = "Mail-Archiv";
/// Longest file name (without the code and `.eml`); names stay readable on every system.
const NAME_CHARS: usize = 120;

/// Automatic archiving (in the Mail file).
const AUTO_KEY: &str = "archive_auto";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct AutoArchive {
    /// Mails older than this many days are copied; 0 = off.
    #[serde(default)]
    days: u32,
    /// Per "account/folder": UIDVALIDITY and the highest UID an automatic run handled.
    #[serde(default)]
    marks: std::collections::BTreeMap<String, (u32, u32)>,
}

/// Mails of a folder for an automatic run.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchiveBatch {
    /// The folder's UIDVALIDITY, for `mail_archive_auto_done`.
    pub validity: u32,
    /// Oldest first.
    pub uids: Vec<u32>,
}

/// What archiving one mail did.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailArchived {
    /// Stored in GhulDrive.
    Saved,
    /// Already in the archive (same Message-ID in the year's folder): nothing stored.
    Skipped,
    /// No longer at the provider (deleted or moved meanwhile): nothing stored.
    Gone,
}

/// Days since 1970-01-01 to `(year, month, day)`.
fn ymd(secs: i64) -> (i32, u32, u32) {
    let d = super::caldate::Date::from_days(secs.div_euclid(86_400));
    (d.y, d.m, d.d)
}

/// A part of a file name: no path or reserved characters, no control characters, single spaces.
fn clean(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| {
            let reserved = matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
            // Direction overrides could disguise the name ("…\u{202e}fdp.exe").
            let direction = matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
            if c.is_control() || reserved || direction {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut out = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = super::drive::trim_invisible(&out).trim_matches('.').to_owned();
    out = trimmed;
    out
}

/// Code of a mail from its Message-ID (or its content when it has none).
fn code_of(id: Option<&str>, raw: &[u8]) -> String {
    let digest = match id.map(str::trim).filter(|i| !i.is_empty()) {
        Some(i) => Sha256::digest(i.as_bytes()),
        None => Sha256::digest(raw),
    };
    crate::kdf::hex(&digest[..4])
}

/// File name, year and code of a mail: `2024-05-03 Erika Musterfrau - Treffen (1a2b3c4d).eml`.
pub(super) fn eml_name(raw: &[u8], received: i64, now: i64) -> (String, i32, String) {
    let parsed = mail_parser::MessageParser::default().parse_headers(raw);
    let date = parsed.as_ref().and_then(|m| m.date()).map(|d| d.to_timestamp()).filter(|t| *t > 0);
    let when = date.or((received > 0).then_some(received)).unwrap_or(now);
    let (y, m, d) = ymd(when);
    let from = parsed
        .as_ref()
        .and_then(|m| m.from())
        .and_then(|a| a.first())
        .map(|a| a.name().or(a.address()).unwrap_or_default().to_owned())
        .unwrap_or_default();
    let subject = parsed.as_ref().and_then(|m| m.subject()).unwrap_or_default().to_owned();
    let code = code_of(parsed.as_ref().and_then(|m| m.message_id()), raw);
    let mut words = format!("{y:04}-{m:02}-{d:02}");
    let (from, subject) = (clean(&from), clean(&subject));
    if !from.is_empty() {
        words.push(' ');
        words.push_str(&from);
    }
    if !subject.is_empty() {
        words.push_str(" - ");
        words.push_str(&subject);
    }
    let words: String = words.chars().take(NAME_CHARS).collect();
    (format!("{} ({code}).eml", words.trim_end()), y, code)
}

impl GhulSuite {
    /// The archive's folder in GhulDrive for an account (`None`: the top folder), if it exists.
    fn archive_root(&self) -> Result<Option<String>> {
        let id: String = self.with_open(|o| Ok(area_setting(o, VaultKind::Mail, ARCHIVE_KEY)))?;
        if id.is_empty() || !self.drive_folder_alive(&id)? {
            return Ok(None);
        }
        Ok(Some(id))
    }

    /// The archive's top folder, created (and remembered) when it is missing or in the trash.
    fn archive_root_or_new(&self) -> Result<String> {
        if let Some(id) = self.archive_root()? {
            return Ok(id);
        }
        let id = self.drive_create_folder(None, ARCHIVE_NAME.into())?;
        self.with_open(|o| {
            put_area_setting(o, VaultKind::Mail, ARCHIVE_KEY, &id)?;
            self.save_area(o, VaultKind::Mail)
        })?;
        Ok(id)
    }

    fn archive_candidates(&self, account_id: &str, folder: &str, before: Option<i64>) -> Result<(u32, Vec<u32>)> {
        let folder = folder.to_owned();
        self.with_imap(account_id, move |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::uids_before(s, &folder, before).await })
        })
    }

    /// Name of an account's folder in the archive.
    fn archive_account_name(&self, account_id: &str) -> Result<String> {
        let a = self.with_open(|o| account(o, account_id))?;
        let name = [a.name.as_str(), a.email.as_str()].iter().map(|n| clean(n)).find(|n| !n.is_empty());
        Ok(name.unwrap_or_else(|| "Mail".into()))
    }
}

#[uniffi::export]
impl GhulSuite {
    /// The mail archive's folder in GhulDrive: the account's (with `account_id`) or the top one.
    /// `None` while nothing was archived yet.
    pub fn mail_archive_folder(&self, account_id: Option<String>) -> Result<Option<String>> {
        let Some(root) = self.archive_root()? else { return Ok(None) };
        let Some(account_id) = account_id else { return Ok(Some(root)) };
        let name = self.archive_account_name(&account_id)?;
        let found = self.drive_list(Some(root), false)?.into_iter().find(|i| i.is_dir && i.name == name);
        Ok(found.map(|i| i.id))
    }

    /// The mails of a folder at the provider to archive: those received before the day
    /// `before_days` (days since 1970-01-01), or all. Oldest first.
    pub fn mail_archive_uids(&self, account_id: String, folder: String, before_days: Option<i64>) -> Result<Vec<u32>> {
        Ok(self.archive_candidates(&account_id, &folder, before_days)?.1)
    }

    /// Automatic archiving: mails older than this many days are copied (0 = off). The same on
    /// every device (stored in the Mail file).
    pub fn mail_archive_auto_days(&self) -> Result<u32> {
        self.with_open(|o| Ok(area_setting::<AutoArchive>(o, VaultKind::Mail, AUTO_KEY).days))
    }

    pub fn mail_archive_set_auto_days(&self, days: u32) -> Result<()> {
        self.with_open(|o| {
            let mut a: AutoArchive = area_setting(o, VaultKind::Mail, AUTO_KEY);
            a.days = days;
            put_area_setting(o, VaultKind::Mail, AUTO_KEY, &a)?;
            self.save_area(o, VaultKind::Mail)
        })
    }

    /// For automatic archiving: the mails of a folder older than the setting that came after the
    /// last automatic run (so a daily run does not fetch everything again). Empty when off.
    pub fn mail_archive_auto_uids(&self, account_id: String, folder: String) -> Result<MailArchiveBatch> {
        let days = self.mail_archive_auto_days()?;
        if days == 0 {
            return Ok(MailArchiveBatch { validity: 0, uids: Vec::new() });
        }
        let today = super::suite::now_ms() / 86_400_000;
        let (validity, uids) = self.archive_candidates(&account_id, &folder, Some(today - i64::from(days)))?;
        let key = format!("{account_id}/{folder}");
        let done =
            self.with_open(|o| Ok(area_setting::<AutoArchive>(o, VaultKind::Mail, AUTO_KEY).marks.get(&key).copied()))?;
        // Another UIDVALIDITY: the old mark names other mails now, start over (duplicates are skipped).
        let after = done.filter(|(v, _)| *v == validity).map_or(0, |(_, u)| u);
        Ok(MailArchiveBatch { validity, uids: uids.into_iter().filter(|u| *u > after).collect() })
    }

    /// After an automatic run: everything up to `last_uid` is in the archive.
    pub fn mail_archive_auto_done(
        &self,
        account_id: String,
        folder: String,
        validity: u32,
        last_uid: u32,
    ) -> Result<()> {
        self.with_open(|o| {
            let mut a: AutoArchive = area_setting(o, VaultKind::Mail, AUTO_KEY);
            let key = format!("{account_id}/{folder}");
            let keep = a.marks.get(&key).is_some_and(|(v, u)| *v == validity && *u >= last_uid);
            if !keep {
                a.marks.insert(key, (validity, last_uid));
                put_area_setting(o, VaultKind::Mail, AUTO_KEY, &a)?;
                self.save_area(o, VaultKind::Mail)?;
            }
            Ok(())
        })
    }

    /// Copies one mail as `.eml` into GhulDrive (`Mail-Archiv/<account>/<year>/`). The mail stays
    /// at the provider unchanged (also its "read" mark). A mail already archived is skipped.
    pub fn mail_archive_to_drive(&self, account_id: String, folder: String, uid: u32) -> Result<MailArchived> {
        if self.drive_status()?.server_id.is_none() {
            return Err(GhulError::NotConfigured);
        }
        let got = self.with_imap(&account_id, move |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::peek(s, &folder, uid).await })
        })?;
        let Some((raw, received)) = got else { return Ok(MailArchived::Gone) };
        let raw = Zeroizing::new(raw);
        let (name, year, code) = eml_name(&raw, received, super::suite::now_ms() / 1000);
        let root = self.archive_root_or_new()?;
        let acc = self.drive_folder_named(Some(&root), &self.archive_account_name(&account_id)?)?;
        let year_dir = self.drive_folder_named(Some(&acc), &format!("{year:04}"))?;
        let marker = format!("({code}).eml");
        if self.drive_list(Some(year_dir.clone()), false)?.iter().any(|i| !i.is_dir && i.name.ends_with(&marker)) {
            return Ok(MailArchived::Skipped);
        }
        self.drive_put_file(Some(year_dir), name, &raw)?;
        Ok(MailArchived::Saved)
    }

    /// Reads an archived mail (`.eml`): decrypted with the account's key if it is encrypted.
    pub fn mail_parse_eml(&self, account_id: String, data: Vec<u8>) -> Result<MailMessage> {
        let raw = Zeroizing::new(data);
        let u = self.with_open(|o| {
            let u = unwrap_message(o, &account_id, &raw)?;
            if u.changed {
                self.save_area(o, VaultKind::Mail)?;
            }
            Ok(u)
        })?;
        message(0, &raw, &u).map_err(|_| GhulError::InvalidInput { msg: "not a mail".into() })
    }

    /// Attachment `index` of an archived mail (`.eml`).
    pub fn mail_eml_attachment(&self, account_id: String, data: Vec<u8>, index: u32) -> Result<Vec<u8>> {
        let raw = Zeroizing::new(data);
        let u = self.with_open(|o| unwrap_message(o, &account_id, &raw))?;
        super::mail::attachment_of(&u, index)
    }
}

#[cfg(test)]
mod tests {
    use super::eml_name;

    #[test]
    fn names_carry_date_sender_subject_and_code() {
        let raw = b"From: Erika Musterfrau <erika@example.net>\r\nSubject: Treffen am Freitag\r\n\
                    Date: Sat, 3 May 2024 10:00:00 +0000\r\nMessage-ID: <a1@example.net>\r\n\r\nHallo\r\n";
        let (name, year, code) = eml_name(raw, 0, 0);
        assert_eq!(year, 2024);
        assert_eq!(code.len(), 8);
        assert_eq!(name, format!("2024-05-03 Erika Musterfrau - Treffen am Freitag ({code}).eml"));
        // The same Message-ID gives the same code (a second run skips it).
        let again = b"From: x@y\r\nSubject: anders\r\nMessage-ID: <a1@example.net>\r\n\r\n";
        assert_eq!(eml_name(again, 0, 0).2, code);
    }

    #[test]
    fn names_are_safe_and_short() {
        let raw = "From: \"A/B\\\\C:D*E?F\" <x@y>\r\nSubject: ../../etc/passwd \u{202e}fdp.exe | <x>\r\n\r\n";
        let (name, year, _) = eml_name(raw.as_bytes(), 1_700_000_000, 0);
        assert_eq!(year, 2023, "no Date: the server's receive time");
        assert!(!name.contains('/') && !name.contains('\\') && !name.contains(':') && !name.contains('|'));
        assert!(!name.chars().any(char::is_control));
        assert!(!name.contains('\u{202e}'), "no direction override");
        let long = format!("From: a@b\r\nSubject: {}\r\n\r\n", "x".repeat(1000));
        assert!(eml_name(long.as_bytes(), 0, 0).0.chars().count() < 150);
        // Without any header: the time given as now, no sender, no subject.
        let (bare, y, _) = eml_name(b"\r\n", 0, 86_400 * 366);
        assert_eq!(y, 1971);
        assert!(bare.starts_with("1971-01-02 ("), "{bare}");
    }
}
