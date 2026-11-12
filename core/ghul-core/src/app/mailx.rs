// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulMail extras: flagged view, labels, drafts and scheduled mail (both stored encrypted in the
//! suite and synchronised), address suggestions, the raw message (.eml, headers) and an encrypted
//! offline cache on this device.

use std::collections::BTreeMap;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::mail::{summary, MailPage, MailSummary, OutgoingAttachment, OutgoingMail};
use super::suite::{new_id, now_ms, put_setting, setting, GhulSuite, SUITE_FILE};
use super::sync::mark_deleted;
use crate::net::imap;

const DRAFTS_KEY: &str = "mail_drafts";
const OUTBOX_KEY: &str = "mail_outbox";
const CONTACTS_KEY: &str = "mail_contacts";
const CACHE_KEY: &str = "mail_cache_key";
const CACHE_MAGIC: &[u8; 8] = b"GHULMCH1";
/// Scheduled mail is kept in the (synchronised) suite settings: keep it small.
const MAX_SCHEDULED_BYTES: usize = 10 * 1024 * 1024;
const MAX_CONTACTS: usize = 500;

#[derive(uniffi::Record, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MailDraftData {
    /// Empty for a new draft.
    #[serde(default)]
    pub id: String,
    pub account_id: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub in_reply_to: String,
    /// Unix milliseconds.
    #[serde(default)]
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredAttachment {
    name: String,
    mime: String,
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredScheduled {
    id: String,
    account_id: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    text: String,
    in_reply_to: String,
    attachments: Vec<StoredAttachment>,
    /// Unix milliseconds.
    send_at: i64,
    #[serde(default)]
    updated: i64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ScheduledMail {
    pub id: String,
    pub account_id: String,
    pub to: Vec<String>,
    pub subject: String,
    pub send_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct StoredContact {
    email: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    last: i64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailContact {
    pub name: String,
    pub email: String,
}

/// "Erika <erika@example.org>" → ("Erika", "erika@example.org").
fn split_address(a: &str) -> (String, String) {
    let a = a.trim();
    match (a.rfind('<'), a.rfind('>')) {
        (Some(l), Some(r)) if l < r => (a[..l].trim().trim_matches('"').to_owned(), a[l + 1..r].trim().to_lowercase()),
        _ => (String::new(), a.to_lowercase()),
    }
}

/// IMAP keyword for a label: atom characters only.
fn keyword(label: &str) -> Result<String> {
    let k: String = label
        .trim()
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .collect();
    if k.is_empty() || k.len() > 40 {
        return Err(GhulError::InvalidInput { msg: "invalid label".into() });
    }
    Ok(k)
}

impl GhulSuite {
    pub(super) fn remember_contacts(&self, addresses: &[String]) -> Result<()> {
        self.with_open(|o| {
            let mut list: Vec<StoredContact> = setting(&o.suite, CONTACTS_KEY);
            let now = now_ms();
            for a in addresses {
                let (name, email) = split_address(a);
                if !email.contains('@') {
                    continue;
                }
                match list.iter_mut().find(|c| c.email == email) {
                    Some(c) => {
                        c.count += 1;
                        c.last = now;
                        if c.name.is_empty() {
                            c.name = name;
                        }
                    }
                    None => list.push(StoredContact { email, name, count: 1, last: now }),
                }
            }
            list.sort_by_key(|c| std::cmp::Reverse((c.count, c.last)));
            list.truncate(MAX_CONTACTS);
            put_setting(&mut o.suite, CONTACTS_KEY, &list)?;
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    fn cache_dir(&self) -> std::path::PathBuf {
        self.local_dir.join("mail-cache")
    }

    fn cache_enabled(&self) -> bool {
        self.cache_dir().join("enabled").exists()
    }

    fn cache_key(&self) -> Result<Zeroizing<[u8; 32]>> {
        self.with_open(|o| {
            let mut key: Vec<u8> = setting(&o.suite, CACHE_KEY);
            if key.len() != 32 {
                key = vec![0u8; 32];
                OsRng.fill_bytes(&mut key);
                put_setting(&mut o.suite, CACHE_KEY, &key)?;
                self.save_container(&mut o.suite, SUITE_FILE)?;
            }
            let mut k = Zeroizing::new([0u8; 32]);
            k.copy_from_slice(&key);
            Ok(k)
        })
    }

    fn cache_path(&self, kind: &str, parts: &[&str]) -> std::path::PathBuf {
        let mut h = Sha256::new();
        h.update(kind.as_bytes());
        for p in parts {
            h.update([0]);
            h.update(p.as_bytes());
        }
        self.cache_dir().join(crate::kdf::hex(&h.finalize()))
    }

    /// Stores data encrypted in the offline cache (if switched on).
    pub(super) fn cache_put(&self, kind: &str, parts: &[&str], data: &[u8]) {
        if !self.cache_enabled() {
            return;
        }
        let Ok(key) = self.cache_key() else { return };
        let path = self.cache_path(kind, parts);
        let _ = std::fs::write(path, super::drive::encrypt(CACHE_MAGIC, &key, data, kind.as_bytes()));
    }

    pub(super) fn cache_get(&self, kind: &str, parts: &[&str]) -> Option<Zeroizing<Vec<u8>>> {
        if !self.cache_enabled() {
            return None;
        }
        let key = self.cache_key().ok()?;
        let data = std::fs::read(self.cache_path(kind, parts)).ok()?;
        super::drive::decrypt(CACHE_MAGIC, &key, &data, kind.as_bytes()).ok()
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Flagged messages of a folder.
    pub fn mail_flagged(&self, account_id: String, folder: String) -> Result<Vec<MailSummary>> {
        let list = self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::flagged(s, &folder, 200).await })
        })?;
        Ok(list.iter().map(summary).collect())
    }

    /// Adds or removes a label (an IMAP keyword, visible in other programs that support them).
    pub fn mail_set_label(&self, account_id: String, folder: String, uid: u32, label: String, on: bool) -> Result<()> {
        let k = keyword(&label)?;
        self.with_imap(&account_id, |s| {
            let (folder, k) = (folder.clone(), k.clone());
            Box::pin(async move { imap::set_flag(s, &folder, uid, &k, on).await })
        })
    }

    /// The message as stored on the server (.eml). With `decrypted`, an encrypted message is
    /// returned decrypted (handle with care: it is then readable for anyone with the file).
    pub fn mail_raw(&self, account_id: String, folder: String, uid: u32, decrypted: bool) -> Result<Vec<u8>> {
        let raw = self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::fetch(s, &folder, uid).await })
        })?;
        if !decrypted {
            return Ok(raw);
        }
        let u = self.with_open(|o| super::mail::unwrap_raw(o, &account_id, &raw))?;
        Ok(u.to_vec())
    }

    /// Header lines of a message as received ("show original").
    pub fn mail_headers(&self, account_id: String, folder: String, uid: u32) -> Result<String> {
        let raw = self.mail_raw(account_id, folder, uid, false)?;
        let end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .or_else(|| raw.windows(2).position(|w| w == b"\n\n"))
            .unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
    }

    // --- drafts (encrypted in the suite, synchronised) ----------------------------------------

    pub fn mail_save_draft(&self, draft: MailDraftData) -> Result<String> {
        self.with_open(|o| {
            let mut list: Vec<MailDraftData> = setting(&o.suite, DRAFTS_KEY);
            let mut d = draft;
            if d.id.is_empty() {
                d.id = crate::kdf::hex(&new_id());
            }
            d.updated = now_ms();
            let id = d.id.clone();
            list.retain(|x| x.id != id);
            list.insert(0, d);
            list.truncate(100);
            put_setting(&mut o.suite, DRAFTS_KEY, &list)?;
            self.save_container(&mut o.suite, SUITE_FILE)?;
            Ok(id)
        })
    }

    pub fn mail_drafts(&self) -> Result<Vec<MailDraftData>> {
        self.with_open(|o| Ok(setting(&o.suite, DRAFTS_KEY)))
    }

    pub fn mail_delete_draft(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let mut list: Vec<MailDraftData> = setting(&o.suite, DRAFTS_KEY);
            list.retain(|x| x.id != id);
            put_setting(&mut o.suite, DRAFTS_KEY, &list)?;
            mark_deleted(&mut o.suite.payload.settings, DRAFTS_KEY, &id);
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    // --- send later -------------------------------------------------------------------------

    /// Keeps a mail to send at `send_at` (Unix ms). It is sent by the first device that is
    /// unlocked at or after that time.
    pub fn mail_schedule(&self, mail: OutgoingMail, send_at: i64) -> Result<String> {
        let size: usize = mail.attachments.iter().map(|a| a.data.len()).sum::<usize>() + mail.text.len();
        if size > MAX_SCHEDULED_BYTES {
            return Err(GhulError::InvalidInput { msg: "too large to schedule (max 10 MB)".into() });
        }
        self.with_open(|o| {
            let mut list: Vec<StoredScheduled> = setting(&o.suite, OUTBOX_KEY);
            let id = crate::kdf::hex(&new_id());
            list.push(StoredScheduled {
                id: id.clone(),
                account_id: mail.account_id,
                to: mail.to,
                cc: mail.cc,
                bcc: mail.bcc,
                subject: mail.subject,
                text: mail.text,
                in_reply_to: mail.in_reply_to,
                attachments: mail
                    .attachments
                    .into_iter()
                    .map(|a| StoredAttachment { name: a.name, mime: a.mime, data: a.data })
                    .collect(),
                send_at,
                updated: now_ms(),
            });
            put_setting(&mut o.suite, OUTBOX_KEY, &list)?;
            self.save_container(&mut o.suite, SUITE_FILE)?;
            Ok(id)
        })
    }

    pub fn mail_scheduled(&self) -> Result<Vec<ScheduledMail>> {
        self.with_open(|o| {
            let list: Vec<StoredScheduled> = setting(&o.suite, OUTBOX_KEY);
            Ok(list
                .into_iter()
                .map(|s| ScheduledMail {
                    id: s.id,
                    account_id: s.account_id,
                    to: s.to,
                    subject: s.subject,
                    send_at: s.send_at,
                })
                .collect())
        })
    }

    pub fn mail_cancel_scheduled(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let mut list: Vec<StoredScheduled> = setting(&o.suite, OUTBOX_KEY);
            list.retain(|x| x.id != id);
            put_setting(&mut o.suite, OUTBOX_KEY, &list)?;
            mark_deleted(&mut o.suite.payload.settings, OUTBOX_KEY, &id);
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    /// Sends scheduled mail that is due at `now` (Unix ms). Returns how many were sent.
    pub fn mail_send_due(&self, now: i64) -> Result<u32> {
        let due: Vec<StoredScheduled> = self.with_open(|o| {
            let list: Vec<StoredScheduled> = setting(&o.suite, OUTBOX_KEY);
            Ok(list.into_iter().filter(|s| s.send_at <= now).collect())
        })?;
        let mut sent = 0;
        for d in due {
            let id = d.id.clone();
            let mail = OutgoingMail {
                account_id: d.account_id,
                to: d.to,
                cc: d.cc,
                bcc: d.bcc,
                subject: d.subject,
                text: d.text,
                in_reply_to: d.in_reply_to,
                attachments: d
                    .attachments
                    .into_iter()
                    .map(|a| OutgoingAttachment { name: a.name, mime: a.mime, data: a.data })
                    .collect(),
            };
            // Removed first: a mail must never go out twice (another device may sync meanwhile).
            self.mail_cancel_scheduled(id)?;
            if let Err(e) = self.mail_send(mail.clone()) {
                // Back into the outbox, a minute later.
                self.mail_schedule(mail, now + 60_000)?;
                return Err(e);
            }
            sent += 1;
        }
        Ok(sent)
    }

    // --- address suggestions -----------------------------------------------------------------

    /// Known addresses matching `query` (name or address), most used first.
    pub fn mail_contacts(&self, query: String, limit: u32) -> Result<Vec<MailContact>> {
        let q = query.trim().to_lowercase();
        self.with_open(|o| {
            let list: Vec<StoredContact> = setting(&o.suite, CONTACTS_KEY);
            Ok(list
                .into_iter()
                .filter(|c| q.is_empty() || c.email.contains(&q) || c.name.to_lowercase().contains(&q))
                .take(limit as usize)
                .map(|c| MailContact { name: c.name, email: c.email })
                .collect())
        })
    }

    // --- offline cache -----------------------------------------------------------------------

    /// Keeps opened mail and lists encrypted on this device so they can be read offline.
    pub fn mail_set_offline(&self, on: bool) -> Result<()> {
        let dir = self.cache_dir();
        if on {
            std::fs::create_dir_all(&dir).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            std::fs::write(dir.join("enabled"), b"1").map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        } else {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(())
    }

    pub fn mail_offline(&self) -> bool {
        self.cache_enabled()
    }

    /// The cached list of a folder (offline), if there is one.
    pub fn mail_cached_list(&self, account_id: String, folder: String) -> Option<MailPage> {
        let data = self.cache_get("list", &[&account_id, &folder])?;
        let page: CachedPage = ciborium::de::from_reader(&data[..]).ok()?;
        Some(page.into())
    }
}

#[derive(Serialize, Deserialize)]
struct CachedSummary {
    uid: u32,
    from: String,
    from_address: String,
    subject: String,
    date: i64,
    seen: bool,
    flagged: bool,
    has_attachments: bool,
    size: u32,
    preview: String,
    encrypted: bool,
    labels: Vec<String>,
    thread_key: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct CachedPage {
    total: u32,
    messages: Vec<CachedSummary>,
}

impl From<&MailPage> for CachedPage {
    fn from(p: &MailPage) -> Self {
        CachedPage {
            total: p.total,
            messages: p
                .messages
                .iter()
                .map(|m| CachedSummary {
                    uid: m.uid,
                    from: m.from.clone(),
                    from_address: m.from_address.clone(),
                    subject: m.subject.clone(),
                    date: m.date,
                    seen: m.seen,
                    flagged: m.flagged,
                    has_attachments: m.has_attachments,
                    size: m.size,
                    preview: m.preview.clone(),
                    encrypted: m.encrypted,
                    labels: m.labels.clone(),
                    thread_key: m.thread_key.clone(),
                })
                .collect(),
        }
    }
}

impl From<CachedPage> for MailPage {
    fn from(p: CachedPage) -> Self {
        MailPage {
            total: p.total,
            messages: p
                .messages
                .into_iter()
                .map(|m| MailSummary {
                    uid: m.uid,
                    from: m.from,
                    from_address: m.from_address,
                    subject: m.subject,
                    date: m.date,
                    seen: m.seen,
                    flagged: m.flagged,
                    has_attachments: m.has_attachments,
                    size: m.size,
                    preview: m.preview,
                    encrypted: m.encrypted,
                    labels: m.labels,
                    thread_key: m.thread_key,
                })
                .collect(),
        }
    }
}

#[allow(dead_code)]
fn _unused(_: BTreeMap<String, String>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_labels_threads() {
        assert_eq!(split_address("Erika M. <Erika@Example.org>"), ("Erika M.".into(), "erika@example.org".into()));
        assert_eq!(split_address("max@posteo.de"), (String::new(), "max@posteo.de".into()));
        assert_eq!(keyword("Rechnungen 2026").unwrap(), "Rechnungen_2026");
        assert!(keyword("  ").is_err());
        assert!(keyword("a\"b)c(").unwrap() == "abc");
        assert_eq!(super::super::mail::thread_key("AW: Re: WG: Treffen am Freitag"), "treffen am freitag");
    }
}
