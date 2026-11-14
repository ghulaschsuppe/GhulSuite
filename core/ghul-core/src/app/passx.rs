// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulPass extras: favourites, password history, attachments, duplicate and move, security
//! check, a 2FA code stored in a login, passphrases and the optional breach check.
//!
//! Everything is stored inside the encrypted entry (`extra` keys and `attachment:` fields), so it
//! is synchronised and merged like the rest of the entry. Older app versions keep it untouched.

use std::collections::{BTreeMap, HashMap};

use ciborium::value::Value;
use sha1::Sha1;
use sha2::Digest;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::suite::{new_id, now_ms, text_field, GhulSuite};
use super::tools::{parse_otpauth_uri, password_strength, OtpAlgo};
use crate::vault::{Entry, Field};

pub(crate) const FAVORITE: &str = "favorite";
pub(crate) const CREATED: &str = "created";
pub(crate) const HISTORY: &str = "history";
pub(crate) const ATTACHMENT_PREFIX: &str = "attachment:";
/// Old passwords kept per entry.
const MAX_HISTORY: usize = 10;
/// An attachment travels inside the vault file: keep them small.
pub const MAX_ATTACHMENT: usize = 10 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 20;

#[derive(uniffi::Record, Clone, PartialEq)]
pub struct PasswordHistoryItem {
    pub password: String,
    /// Unix milliseconds: until when this password was in use.
    pub changed: i64,
}

super::redact::redacted_debug!(PasswordHistoryItem { changed });

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct AttachmentInfo {
    pub name: String,
    pub size: u64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct EntryExtras {
    pub favorite: bool,
    /// Unix milliseconds, 0 if unknown (entries from older versions).
    pub created: i64,
    /// Newest first.
    pub history: Vec<PasswordHistoryItem>,
    pub attachments: Vec<AttachmentInfo>,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityIssueKind {
    /// The same password is used in another entry.
    Reused,
    /// Easy to guess (zxcvbn score below 3).
    Weak,
    /// A website login without a one-time code (neither in the entry nor in GhulAuth).
    NoSecondFactor,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SecurityIssue {
    pub vault_id: String,
    pub vault_name: String,
    pub id: Vec<u8>,
    pub title: String,
    pub kind: SecurityIssueKind,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SecurityReport {
    /// 0 to 100; 100 = nothing to do.
    pub score: u8,
    /// Logins with a password that were checked.
    pub checked: u32,
    pub issues: Vec<SecurityIssue>,
}

#[derive(uniffi::Record, Clone, PartialEq)]
pub struct TotpNow {
    pub code: String,
    pub next_code: String,
    pub remaining: u32,
    pub period: u32,
}

super::redact::redacted_debug!(TotpNow { remaining, period });

pub(crate) fn is_favorite(e: &Entry) -> bool {
    matches!(e.extra.get(FAVORITE), Some(Value::Bool(true)))
}

fn secret(e: &Entry, name: &str) -> String {
    match e.fields.get(name) {
        Some(Field::Secret(s)) | Some(Field::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

fn history(e: &Entry) -> Vec<PasswordHistoryItem> {
    let Some(Value::Array(items)) = e.extra.get(HISTORY) else { return Vec::new() };
    items
        .iter()
        .filter_map(|v| {
            let Value::Map(m) = v else { return None };
            let get = |k: &str| m.iter().find(|(key, _)| key.as_text() == Some(k)).map(|(_, v)| v);
            let password = get("p")?.as_text()?.to_owned();
            let changed = get("t").and_then(|v| v.as_integer()).and_then(|i| i64::try_from(i).ok()).unwrap_or(0);
            Some(PasswordHistoryItem { password, changed })
        })
        .collect()
}

/// Called before an entry is changed: remembers the old password if it changes.
pub(crate) fn remember_password(e: &mut Entry, new_password: &str, now: i64) {
    let old = secret(e, "password");
    if old.is_empty() || old == new_password {
        return;
    }
    let mut items = history(e);
    items.insert(0, PasswordHistoryItem { password: old, changed: now });
    items.truncate(MAX_HISTORY);
    let values = items
        .into_iter()
        .map(|i| {
            Value::Map(vec![
                (Value::Text("p".into()), Value::Text(i.password)),
                (Value::Text("t".into()), Value::Integer(i.changed.into())),
            ])
        })
        .collect();
    e.extra.insert(HISTORY.into(), Value::Array(values));
}

fn attachments(e: &Entry) -> Vec<AttachmentInfo> {
    e.fields
        .iter()
        .filter_map(|(k, v)| match (k.strip_prefix(ATTACHMENT_PREFIX), v) {
            (Some(name), Field::Bytes(b)) => Some(AttachmentInfo { name: name.to_owned(), size: b.len() as u64 }),
            _ => None,
        })
        .collect()
}

fn clean_name(name: &str) -> Result<String> {
    let n: String = name.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\')).collect();
    let n = n.trim().to_owned();
    if n.is_empty() || n == "." || n == ".." || n.len() > 200 {
        return Err(GhulError::InvalidInput { msg: "invalid file name".into() });
    }
    Ok(n)
}

/// Lower-case host of a URL ("https://www.Example.org/x" → "example.org").
fn host(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#', ':']).next().unwrap_or("").to_lowercase();
    host.strip_prefix("www.").map(str::to_owned).unwrap_or(host)
}

fn urls(e: &Entry) -> Vec<String> {
    match e.fields.get("urls") {
        Some(Field::List(l)) => l.clone(),
        Some(Field::Url(u)) | Some(Field::Text(u)) => vec![u.clone()],
        _ => Vec::new(),
    }
}

fn find_entry<'a>(entries: &'a mut [Entry], id: &[u8]) -> Result<&'a mut Entry> {
    entries.iter_mut().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)
}

#[uniffi::export]
impl GhulSuite {
    /// Favourite flag, creation time, password history and attachments of an entry.
    pub fn entry_extras(&self, vault_id: String, id: Vec<u8>) -> Result<EntryExtras> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = find_entry(&mut c.payload.entries, &id)?;
            let created = match e.extra.get(CREATED) {
                Some(Value::Integer(i)) => i64::try_from(*i).unwrap_or(0),
                _ => 0,
            };
            Ok(EntryExtras { favorite: is_favorite(e), created, history: history(e), attachments: attachments(e) })
        })
    }

    /// Marks an entry (GhulPass or GhulAuth) as favourite.
    pub fn set_entry_favorite(&self, vault_id: String, id: Vec<u8>, favorite: bool) -> Result<()> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = find_entry(&mut c.payload.entries, &id)?;
            if favorite {
                e.extra.insert(FAVORITE.into(), Value::Bool(true));
            } else {
                e.extra.remove(FAVORITE);
            }
            e.revision += 1;
            e.modified = now_ms();
            self.save_vault(o, &vault_id)
        })
    }

    /// Adds a file to an entry (stored encrypted in the vault; at most 10 MB).
    pub fn add_entry_attachment(&self, vault_id: String, id: Vec<u8>, name: String, data: Vec<u8>) -> Result<()> {
        let data = Zeroizing::new(data);
        if data.len() > MAX_ATTACHMENT {
            return Err(GhulError::InvalidInput { msg: "attachment too large (max 10 MB)".into() });
        }
        let name = clean_name(&name)?;
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = find_entry(&mut c.payload.entries, &id)?;
            if attachments(e).len() >= MAX_ATTACHMENTS {
                return Err(GhulError::InvalidInput { msg: "too many attachments".into() });
            }
            // Same name: keep both ("name (2).ext").
            let mut unique = name.clone();
            let mut n = 2;
            while e.fields.contains_key(&format!("{ATTACHMENT_PREFIX}{unique}")) {
                unique = match name.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                    None => format!("{name} ({n})"),
                };
                n += 1;
            }
            e.fields.insert(format!("{ATTACHMENT_PREFIX}{unique}"), Field::Bytes(data.to_vec()));
            e.revision += 1;
            e.modified = now_ms();
            self.save_vault(o, &vault_id)
        })
    }

    pub fn entry_attachment(&self, vault_id: String, id: Vec<u8>, name: String) -> Result<Vec<u8>> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = find_entry(&mut c.payload.entries, &id)?;
            match e.fields.get(&format!("{ATTACHMENT_PREFIX}{name}")) {
                Some(Field::Bytes(b)) => Ok(b.clone()),
                _ => Err(GhulError::NotFound),
            }
        })
    }

    pub fn remove_entry_attachment(&self, vault_id: String, id: Vec<u8>, name: String) -> Result<()> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = find_entry(&mut c.payload.entries, &id)?;
            e.fields.remove(&format!("{ATTACHMENT_PREFIX}{name}")).ok_or(GhulError::NotFound)?;
            e.revision += 1;
            e.modified = now_ms();
            self.save_vault(o, &vault_id)
        })
    }

    /// Copies an entry within its vault; `title_suffix` is appended to the title (e.g. " (Kopie)").
    pub fn duplicate_entry(&self, vault_id: String, id: Vec<u8>, title_suffix: String) -> Result<Vec<u8>> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let src = find_entry(&mut c.payload.entries, &id)?.clone();
            let now = now_ms();
            let mut e = Entry::new(new_id(), &src.entry_type, now);
            e.fields = src.fields;
            e.tags = src.tags;
            e.folder = src.folder;
            let title = text_field(&e, "title");
            e.fields.insert("title".into(), Field::Text(format!("{title}{title_suffix}")));
            e.extra.insert(CREATED.into(), Value::Integer(now.into()));
            let new = e.id.clone();
            c.payload.entries.push(e);
            self.save_vault(o, &vault_id)?;
            Ok(new)
        })
    }

    /// Moves an entry into another vault of the same kind. Returns the new ID.
    pub fn move_entry(&self, vault_id: String, id: Vec<u8>, target_vault_id: String) -> Result<Vec<u8>> {
        if vault_id == target_vault_id {
            return Ok(id);
        }
        self.with_open(|o| {
            let (src_ref, c) = o.vault_mut(&vault_id)?;
            let kind = src_ref.kind.clone();
            let src = find_entry(&mut c.payload.entries, &id)?.clone();
            let (dst_ref, dst) = o.vault_mut(&target_vault_id)?;
            if dst_ref.kind != kind {
                return Err(GhulError::InvalidInput { msg: "target vault is of another kind".into() });
            }
            let now = now_ms();
            let mut e = Entry::new(new_id(), &src.entry_type, now);
            e.fields = src.fields.clone();
            e.tags = src.tags.clone();
            e.folder = src.folder.clone();
            e.extra = src.extra.clone();
            let new = e.id.clone();
            dst.payload.entries.push(e);
            self.save_vault(o, &target_vault_id)?;
            // Only after the copy is safely stored: delete the original (tombstone).
            let (_, c) = o.vault_mut(&vault_id)?;
            let old = find_entry(&mut c.payload.entries, &id)?;
            old.make_tombstone(now);
            self.save_vault(o, &vault_id)?;
            Ok(new)
        })
    }

    /// Checks all logins on this device: reused and weak passwords, websites without 2FA.
    pub fn security_report(&self) -> Result<SecurityReport> {
        self.with_open(|o| {
            // Issuers and accounts known to GhulAuth.
            let mut otp_names: Vec<String> = Vec::new();
            for v in o.vaults.iter().filter(|v| v.reference.kind == "totp") {
                let Some(c) = &v.container else { continue };
                for e in c.payload.entries.iter().filter(|e| !e.deleted) {
                    otp_names.push(text_field(e, "issuer").to_lowercase());
                }
            }
            otp_names.retain(|n| n.len() >= 3);
            let mut logins = Vec::new();
            for v in o.vaults.iter().filter(|v| v.reference.kind == "pass") {
                let Some(c) = &v.container else { continue };
                // Wi-Fi passwords count for weak and reused passwords too (N6).
                for e in c
                    .payload
                    .entries
                    .iter()
                    .filter(|e| !e.deleted && (e.entry_type == "login" || e.entry_type == "wifi"))
                {
                    logins.push((&v.reference, e));
                }
            }
            // Counted by hash, so no second copy of the passwords is kept.
            let key = |pw: &str| -> [u8; 32] { sha2::Sha256::digest(pw.as_bytes()).into() };
            let mut uses: HashMap<[u8; 32], u32> = HashMap::new();
            for (_, e) in &logins {
                let pw = Zeroizing::new(secret(e, "password"));
                if !pw.is_empty() {
                    *uses.entry(key(&pw)).or_default() += 1;
                }
            }
            let mut issues = Vec::new();
            let mut checked = 0u32;
            for (r, e) in &logins {
                let pw = Zeroizing::new(secret(e, "password"));
                let title = text_field(e, "title");
                let issue = |kind| SecurityIssue {
                    vault_id: r.id.clone(),
                    vault_name: r.name.clone(),
                    id: e.id.clone(),
                    title: title.clone(),
                    kind,
                };
                if !pw.is_empty() {
                    checked += 1;
                    if uses.get(&key(&pw)).copied().unwrap_or(0) > 1 {
                        issues.push(issue(SecurityIssueKind::Reused));
                    }
                    if password_strength(pw.to_string()).score < 3 {
                        issues.push(issue(SecurityIssueKind::Weak));
                    }
                }
                let hosts: Vec<String> = urls(e).iter().map(|u| host(u)).filter(|h| !h.is_empty()).collect();
                let has_code = !secret(e, "totp").is_empty();
                // Every login without a code of its own or a matching GhulAuth account, with or
                // without a website (N6).
                if e.entry_type == "login" && !has_code {
                    let t = title.to_lowercase();
                    let known = otp_names.iter().any(|n| {
                        t.contains(n.as_str())
                            || hosts
                                .iter()
                                .any(|h| h.contains(n.as_str()) || n.contains(h.split('.').next().unwrap_or("")))
                    });
                    if !known {
                        issues.push(issue(SecurityIssueKind::NoSecondFactor));
                    }
                }
            }
            let bad: std::collections::BTreeSet<&Vec<u8>> =
                issues.iter().filter(|i| i.kind != SecurityIssueKind::NoSecondFactor).map(|i| &i.id).collect();
            let no2fa = issues.iter().filter(|i| i.kind == SecurityIssueKind::NoSecondFactor).count();
            let total = logins.len().max(1) as f64;
            let score = 100.0 - 80.0 * bad.len() as f64 / total - 20.0 * no2fa as f64 / total;
            Ok(SecurityReport { score: score.clamp(0.0, 100.0).round() as u8, checked, issues })
        })
    }
}

/// The current one-time code for a 2FA secret stored in a login (otpauth:// link or Base32 key).
#[uniffi::export]
pub fn entry_totp(secret: String, unix_time: u64) -> Option<TotpNow> {
    let secret = secret.trim();
    if secret.is_empty() {
        return None;
    }
    let (key, period, digits, algo) = if secret.starts_with("otpauth://") {
        let mut t = parse_otpauth_uri(secret.to_owned()).ok()?;
        t.issuer = "-".into();
        (super::tools::validate_totp(&t).ok()?, t.period as u64, t.digits, t.algorithm)
    } else {
        let input = super::tools::TotpInput {
            issuer: "-".into(),
            account: String::new(),
            secret_base32: secret.to_owned(),
            algorithm: OtpAlgo::Sha1,
            digits: 6,
            period: 30,
        };
        (super::tools::validate_totp(&input).ok()?, 30, 6, OtpAlgo::Sha1)
    };
    let period = period.clamp(1, 3600);
    Some(TotpNow {
        code: crate::totp::totp(&key, unix_time, period, digits, algo.into()).ok()?,
        next_code: crate::totp::totp(&key, unix_time + period, period, digits, algo.into()).ok()?,
        remaining: crate::totp::totp_remaining(unix_time, period) as u32,
        period: period as u32,
    })
}

const WORDS: &str = include_str!("eff_large_wordlist.txt");

/// Random passphrase from the EFF large word list (7776 words, about 12.9 bits per word), e.g.
/// "Lantern-Compass-Meadow-Coffee-42".
#[uniffi::export]
pub fn generate_passphrase(words: u32, separator: String, capitalize: bool, with_number: bool) -> Result<String> {
    if !(3..=12).contains(&words) {
        return Err(GhulError::InvalidInput { msg: "3 to 12 words".into() });
    }
    let list: Vec<&str> = WORDS.lines().filter(|l| !l.is_empty()).collect();
    let mut parts: Vec<String> = (0..words)
        .map(|_| {
            let w = list[super::tools::uniform(list.len() as u32) as usize];
            if capitalize {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                w.to_owned()
            }
        })
        .collect();
    if with_number {
        parts.push((10 + super::tools::uniform(90)).to_string());
    }
    Ok(parts.join(&separator))
}

/// How often a password appears in known data breaches ("Have I Been Pwned", k-anonymity): only
/// the first 5 characters of its SHA-1 hash leave the device; the answer is padded.
#[uniffi::export]
pub fn pwned_count(password: String) -> Result<u32> {
    let password = Zeroizing::new(password);
    let hash = hex_upper(&Sha1::digest(password.as_bytes()));
    let (prefix, suffix) = hash.split_at(5);
    let url = crate::net::http::Url::parse(&format!("https://api.pwnedpasswords.com/range/{prefix}"))?;
    let resp = crate::net::block_on(
        async move { crate::net::http::request("GET", &url, &[("Add-Padding", "true".into())], &[], None).await },
        crate::net::TIMEOUT,
    )?;
    if resp.status != 200 {
        return Err(GhulError::Network { msg: format!("HTTP {}", resp.status) });
    }
    Ok(parse_range(&String::from_utf8_lossy(&resp.body), suffix))
}

fn hex_upper(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn parse_range(body: &str, suffix: &str) -> u32 {
    body.lines()
        .filter_map(|l| l.trim().split_once(':'))
        .find(|(s, _)| s.eq_ignore_ascii_case(suffix))
        .and_then(|(_, n)| n.trim().parse().ok())
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _unused(_: BTreeMap<String, Value>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breach_answer_is_parsed() {
        // SHA-1 of "password" = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
        let hash = hex_upper(&Sha1::digest(b"password"));
        assert_eq!(&hash[..5], "5BAA6");
        let body = "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\nFFFF:2\r\n";
        assert_eq!(parse_range(body, &hash[5..]), 9659365);
        assert_eq!(parse_range(body, "ABCDEF"), 0);
    }

    #[test]
    fn passphrases() {
        // Space as separator: a few list words contain a hyphen ("t-shirt", "yo-yo").
        let p = generate_passphrase(4, " ".into(), true, true).unwrap();
        let parts: Vec<&str> = p.split(' ').collect();
        assert_eq!(parts.len(), 5);
        assert!(parts[..4].iter().all(|w| w.chars().next().unwrap().is_uppercase()));
        assert!(parts[4].parse::<u32>().unwrap() >= 10);
        assert!(generate_passphrase(2, "-".into(), false, false).is_err());
        assert_eq!(WORDS.lines().count(), 7776);
        assert!(WORDS.lines().all(|w| !w.contains(' ')));
    }

    #[test]
    fn codes_from_a_login() {
        // RFC 6238 test key "12345678901234567890" in Base32.
        let key = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let now = entry_totp(key.into(), 59).unwrap();
        assert_eq!(now.code, "287082");
        assert_eq!(now.remaining, 1);
        let uri = format!("otpauth://totp/X:me?secret={key}&digits=8");
        assert_eq!(entry_totp(uri, 59).unwrap().code, "94287082");
        assert!(entry_totp("nonsense!".into(), 59).is_none());
    }

    #[test]
    fn hosts() {
        assert_eq!(host("https://www.Example.org/login?x"), "example.org");
        assert_eq!(host("codeberg.org"), "codeberg.org");
    }
}
