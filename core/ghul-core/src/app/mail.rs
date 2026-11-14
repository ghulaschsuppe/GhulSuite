// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulMail: standard IMAP/SMTP accounts, kept in the suite's encrypted settings.
//!
//! - Only TLS or STARTTLS, never unencrypted. A self-signed certificate of an own server can be
//!   confirmed once by its fingerprint (pinning).
//! - Messages are shown as text (HTML converted, no browser engine, no remote images).
//! - Notifications while locked use `mail_poll` with a separate, device-bound copy of the
//!   login that the app keeps only if the user turns notifications on.

use mail_parser::MimeHeaders;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use base64::Engine;
use pgp::composed::{SignedPublicKey, SignedSecretKey};

use super::areas::{area_setting, mark_area_deleted, put_area_setting};
use super::error::{GhulError, Result};
use super::mailcrypt;
use super::suite::{new_id, GhulSuite, Open, VaultKind};
use crate::net::imap::{self, ImapConfig, ImapSession, Security};
use crate::net::smtp::{self, SmtpConfig};
use crate::net::{block_on, NetError, TIMEOUT};

const ACCOUNTS_KEY: &str = "mail_accounts";

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailSecurity {
    Tls,
    StartTls,
}

impl From<MailSecurity> for Security {
    fn from(s: MailSecurity) -> Self {
        match s {
            MailSecurity::Tls => Security::Tls,
            MailSecurity::StartTls => Security::StartTls,
        }
    }
}

/// Server settings of an account, as entered or suggested.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MailServers {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: MailSecurity,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: MailSecurity,
}

#[derive(uniffi::Record, Clone)]
pub struct MailAccountInput {
    /// Shown name of the account, e.g. "Privat".
    pub name: String,
    /// Sender name, e.g. "Max Mustermann".
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub servers: MailServers,
    /// Confirmed certificate fingerprints (self-signed own servers), else `None`.
    pub imap_cert: Option<String>,
    pub smtp_cert: Option<String>,
    pub signature: String,
}

super::redact::redacted_debug!(MailAccountInput {
    name,
    display_name,
    email,
    username,
    servers,
    imap_cert,
    smtp_cert,
    signature
});

#[derive(Serialize, Deserialize, Clone)]
struct StoredAccount {
    id: String,
    name: String,
    display_name: String,
    email: String,
    username: String,
    password: String,
    servers: MailServers,
    #[serde(default)]
    imap_cert: Option<String>,
    #[serde(default)]
    smtp_cert: Option<String>,
    #[serde(default)]
    signature: String,
    /// The account's OpenPGP secret key (armored, no passphrase: it lives in the encrypted suite).
    #[serde(default)]
    pgp_key: String,
    /// Last edit (Unix ms): the later edit wins when devices synchronise (B13).
    #[serde(default)]
    changed: i64,
}

impl Drop for StoredAccount {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.password);
        zeroize::Zeroize::zeroize(&mut self.pgp_key);
    }
}

/// A contact's public key, learned from a mail (Autocrypt), the provider (WKD) or imported.
#[derive(Serialize, Deserialize, Clone)]
struct ContactKey {
    /// Lower case.
    email: String,
    /// Binary public key, base64.
    key: String,
    fingerprint: String,
    /// `autocrypt`, `wkd` or `import`.
    source: String,
    /// Unix seconds of the last update.
    updated: i64,
    /// B12: for a key from Autocrypt, the mail it came with (SHA-256 of its Message-ID, else of
    /// the whole mail): that mail's own signature never counts as confirmed, also when reopened.
    #[serde(default)]
    origin: String,
    /// L7: the user compared the fingerprint with the contact. Only an import replaces it then.
    #[serde(default)]
    verified: bool,
    /// The fingerprint of the key this one replaced (a key change, shown as a warning until
    /// the new key is confirmed).
    #[serde(default)]
    previous: String,
    /// A confirmed key stays; a mail that offered another one leaves its fingerprint here.
    #[serde(default)]
    offered: String,
}

const KEYS_KEY: &str = "mail_keys";

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailAccountInfo {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub servers: MailServers,
    pub signature: String,
    /// Fingerprint of the account's OpenPGP key, in groups of four (empty until created).
    pub pgp_fingerprint: String,
}

/// How a received message was protected.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailSignatureStatus {
    /// Not signed.
    None,
    /// Signed by the sender, signature valid.
    Valid,
    /// Signed, but the sender's key is not known.
    UnknownKey,
    /// Signed, but the signature does not match (changed in transit or forged).
    Invalid,
}

/// Whether a recipient can get encrypted mail.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailRecipientKey {
    pub address: String,
    pub has_key: bool,
    /// `autocrypt`, `wkd`, `import`, `own` or empty.
    pub source: String,
}

/// A known key of a contact, for the settings.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailKeyInfo {
    pub email: String,
    pub fingerprint: String,
    pub source: String,
    pub updated: i64,
    /// The fingerprint was compared with the contact (`mail_verify_key`).
    pub verified: bool,
    /// Set after a key change: the fingerprint of the key before (warning until confirmed).
    pub previous: String,
    /// A mail offered another key than the confirmed one (warning): its fingerprint.
    pub offered: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailFolder {
    pub name: String,
    /// `inbox`, `sent`, `trash`, `drafts`, `junk`, `archive` or empty.
    pub role: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailSummary {
    pub uid: u32,
    pub from: String,
    pub from_address: String,
    pub subject: String,
    /// Unix time in seconds (0 if unknown).
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    pub size: u32,
    /// First words of the text ("" for encrypted mail: the server only has ciphertext).
    pub preview: String,
    pub encrypted: bool,
    pub labels: Vec<String>,
    /// Subject without "Re:", "AW:", "Fwd:" … (to group conversations).
    pub thread_key: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailPage {
    pub total: u32,
    pub messages: Vec<MailSummary>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailAttachment {
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailMessage {
    pub uid: u32,
    pub from: String,
    pub from_address: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub reply_to: String,
    pub subject: String,
    pub date: i64,
    /// Plain text (HTML messages are converted).
    pub text: String,
    pub was_html: bool,
    pub message_id: String,
    pub attachments: Vec<MailAttachment>,
    /// End-to-end encrypted (OpenPGP) and decrypted on this device.
    pub encrypted: bool,
    pub signature: MailSignatureStatus,
    /// Address whose key made the valid signature (empty otherwise).
    pub signer: String,
    /// HTML mails as safe building blocks (empty for plain-text mails); see `mailhtml`.
    pub blocks: Vec<super::mailhtml::MailBlock>,
    /// Images from the internet in the HTML (loaded only on request).
    pub remote_images: u32,
    /// Images inside the mail (`cid:`), at most 8 MB together.
    pub inline_images: Vec<MailInlineImage>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailInlineImage {
    /// Content-ID without angle brackets, as referenced by `cid:` in the HTML.
    pub cid: String,
    pub data: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OutgoingAttachment {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct OutgoingMail {
    pub account_id: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    /// Message-ID of the message answered (for threading), or empty.
    pub in_reply_to: String,
    pub attachments: Vec<OutgoingAttachment>,
    /// OpenPGP for this mail: signed (or encrypted when every recipient has a key), with the own
    /// key in the Autocrypt header. Off: a plain mail without signature, key or encryption.
    #[uniffi(default = true)]
    pub pgp: bool,
}

/// What a notification check needs; the app keeps it device-bound while locked.
#[derive(uniffi::Record, Clone)]
pub struct MailPollConfig {
    pub account_id: String,
    pub account_name: String,
    pub host: String,
    pub port: u16,
    pub security: MailSecurity,
    pub username: String,
    pub password: String,
    pub cert: Option<String>,
}

super::redact::redacted_debug!(MailPollConfig { account_id, account_name, host, port, security, username, cert });

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailPollResult {
    /// Next UID of the inbox; pass it as `since_uid` next time.
    pub next_uid: u32,
    pub unseen: u32,
    /// Messages that arrived since `since_uid`.
    pub new_messages: u32,
    pub newest_from: String,
    pub newest_subject: String,
    /// UID of the newest message in the inbox (0 if none is new), to open it from a notification.
    pub newest_uid: u32,
}

/// Result of checking a server: either fine, or a certificate fingerprint to confirm.
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum MailCheck {
    Ok,
    ImapCertificate { fingerprint: String },
    SmtpCertificate { fingerprint: String },
}

fn accounts(o: &Open) -> Vec<StoredAccount> {
    area_setting(o, VaultKind::Mail, ACCOUNTS_KEY)
}

fn account(o: &Open, id: &str) -> Result<StoredAccount> {
    accounts(o).into_iter().find(|a| a.id == id).ok_or(GhulError::NotFound)
}

fn imap_config(a: &StoredAccount) -> ImapConfig {
    ImapConfig {
        host: a.servers.imap_host.clone(),
        port: a.servers.imap_port,
        security: a.servers.imap_security.into(),
        user: a.username.clone(),
        password: Zeroizing::new(a.password.clone()),
        cert: a.imap_cert.clone(),
    }
}

fn smtp_config(a: &StoredAccount) -> SmtpConfig {
    SmtpConfig {
        host: a.servers.smtp_host.clone(),
        port: a.servers.smtp_port,
        security: a.servers.smtp_security.into(),
        user: a.username.clone(),
        password: Zeroizing::new(a.password.clone()),
        cert: a.smtp_cert.clone(),
    }
}

fn info(a: &StoredAccount) -> MailAccountInfo {
    MailAccountInfo {
        id: a.id.clone(),
        name: a.name.clone(),
        display_name: a.display_name.clone(),
        email: a.email.clone(),
        username: a.username.clone(),
        servers: a.servers.clone(),
        signature: a.signature.clone(),
        pgp_fingerprint: if a.pgp_key.is_empty() {
            String::new()
        } else {
            mailcrypt::secret_key(&a.pgp_key)
                .map(|k| mailcrypt::pretty_fingerprint(&mailcrypt::fingerprint_secret(&k)))
                .unwrap_or_default()
        },
    }
}

fn contact_keys(o: &Open) -> Vec<ContactKey> {
    area_setting(o, VaultKind::Mail, KEYS_KEY)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Bare address from "Name <a@b>" or "a@b", lower case.
fn bare(addr: &str) -> String {
    let a = addr.trim();
    match (a.rfind('<'), a.rfind('>')) {
        (Some(x), Some(y)) if y > x => a[x + 1..y].trim().to_lowercase(),
        _ => a.to_lowercase(),
    }
}

/// Public key known for an address (contacts, or one of the own accounts).
fn key_for(o: &Open, email: &str) -> Option<(SignedPublicKey, String)> {
    let email = bare(email);
    if let Some(c) = contact_keys(o).into_iter().find(|c| c.email == email) {
        let data = base64::engine::general_purpose::STANDARD.decode(&c.key).ok()?;
        return mailcrypt::parse_public(&data).ok().map(|k| (k, c.source.clone()));
    }
    accounts(o)
        .into_iter()
        .find(|a| a.email.to_lowercase() == email && !a.pgp_key.is_empty())
        .and_then(|a| mailcrypt::secret_key(&a.pgp_key).ok())
        .map(|k| (mailcrypt::public_of(&k), "own".to_owned()))
}

/// Stores a contact key. Keys from a mail (Autocrypt) never replace keys that were looked up
/// at the provider or imported by hand.
fn remember_key(o: &mut Open, email: &str, key: &SignedPublicKey, source: &str) -> Result<bool> {
    remember_key_seen(o, email, key, source, now_secs(), "")
}

/// Like [`remember_key`]; `seen` is when the key was seen (for Autocrypt: the mail's date). An
/// Autocrypt key replaces another Autocrypt key only from a newer mail.
pub(super) fn remember_key_seen(
    o: &mut Open,
    email: &str,
    key: &SignedPublicKey,
    source: &str,
    seen: i64,
    origin: &str,
) -> Result<bool> {
    let email = bare(email);
    let fingerprint = mailcrypt::fingerprint(key);
    let mut list = contact_keys(o);
    let mut previous = String::new();
    if let Some(c) = list.iter_mut().find(|c| c.email == email) {
        if c.fingerprint == fingerprint {
            return Ok(false);
        }
        // A confirmed key is only replaced by a deliberate import; another key from a mail or
        // the provider is remembered as a warning (L7).
        if c.verified && source != "import" {
            if c.offered != fingerprint {
                c.offered = fingerprint;
                put_area_setting(o, VaultKind::Mail, KEYS_KEY, &list)?;
                return Ok(true);
            }
            return Ok(false);
        }
        if source == "autocrypt" && (c.source != "autocrypt" || seen <= c.updated) {
            return Ok(false);
        }
        if source != "import" {
            previous = c.fingerprint.clone();
        }
    }
    list.retain(|c| c.email != email);
    list.push(ContactKey {
        email,
        key: base64::engine::general_purpose::STANDARD.encode(mailcrypt::public_bytes(key)?),
        fingerprint,
        source: source.to_owned(),
        updated: seen,
        origin: origin.to_owned(),
        verified: false,
        previous,
        offered: String::new(),
    });
    put_area_setting(o, VaultKind::Mail, KEYS_KEY, &list)?;
    Ok(true)
}

/// The account's key, created on first use for accounts from before OpenPGP.
fn ensure_key(o: &mut Open, account_id: &str) -> Result<SignedSecretKey> {
    ensure_key_changed(o, account_id).map(|(k, _)| k)
}

fn ensure_key_changed(o: &mut Open, account_id: &str) -> Result<(SignedSecretKey, bool)> {
    let mut list = accounts(o);
    let a = list.iter_mut().find(|a| a.id == account_id).ok_or(GhulError::NotFound)?;
    if a.pgp_key.is_empty() {
        let (armored, _) = mailcrypt::generate(&a.display_name, &a.email)?;
        a.pgp_key = armored.to_string();
        let key = mailcrypt::secret_key(&a.pgp_key)?;
        put_area_setting(o, VaultKind::Mail, ACCOUNTS_KEY, &list)?;
        return Ok((key, true));
    }
    Ok((mailcrypt::secret_key(&a.pgp_key)?, false))
}

/// One part of a multipart body, exactly as transmitted (RFC 3156: up to, not including, the
/// CRLF before the next delimiter).
fn mime_parts<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let delim = format!("--{boundary}");
    let d = delim.as_bytes();
    let mut starts = Vec::new();
    let mut i = 0;
    while i + d.len() <= body.len() {
        let at_line_start = i == 0 || body[i - 1] == b'\n';
        if at_line_start && &body[i..i + d.len()] == d {
            starts.push(i);
            i += d.len();
        } else {
            i += 1;
        }
    }
    let mut parts = Vec::new();
    for w in starts.windows(2) {
        let mut a = w[0] + d.len();
        // Skip the rest of the delimiter line.
        while a < body.len() && body[a] != b'\n' {
            a += 1;
        }
        a = (a + 1).min(body.len());
        let mut b = w[1];
        if b > a && body[b - 1] == b'\n' {
            b -= 1;
            if b > a && body[b - 1] == b'\r' {
                b -= 1;
            }
        }
        if b >= a {
            parts.push(&body[a..b]);
        }
    }
    parts
}

fn body_offset(raw: &[u8]) -> usize {
    raw.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n").map(|p| p + 2))
        .unwrap_or(raw.len())
}

fn to_crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 32);
    let mut prev = 0u8;
    for &b in data {
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        prev = b;
    }
    out
}

/// The readable content of a message: decrypted if needed, and how it was protected.
struct Unwrapped {
    raw: Zeroizing<Vec<u8>>,
    encrypted: bool,
    signature: MailSignatureStatus,
    signer: String,
    /// Subject from inside the encryption ("protected headers").
    subject: Option<String>,
    /// Settings changed (key learned or created): the suite must be saved.
    changed: bool,
}

/// Opens PGP/MIME (encrypted or signed) and learns the sender's key from Autocrypt.
/// The readable message (decrypted if it was encrypted).
pub(super) fn unwrap_raw(o: &mut Open, account_id: &str, raw: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    Ok(unwrap_message(o, account_id, raw)?.raw)
}

fn unwrap_message(o: &mut Open, account_id: &str, raw: &[u8]) -> Result<Unwrapped> {
    let (mut u, key_from_this_mail) = unwrap_message_inner(o, account_id, raw)?;
    if key_from_this_mail && u.signature == MailSignatureStatus::Valid {
        u.signature = MailSignatureStatus::UnknownKey;
    }
    Ok(u)
}

fn unwrap_message_inner(o: &mut Open, account_id: &str, raw: &[u8]) -> Result<(Unwrapped, bool)> {
    let m = mail_parser::MessageParser::default()
        .parse(raw)
        .ok_or_else(|| GhulError::Network { msg: "message cannot be read".into() })?;
    let from = m.from().and_then(|a| a.first()).and_then(|a| a.address()).unwrap_or_default().to_lowercase();
    // Autocrypt: the sender's key, if the header belongs to the From address. Never for an own
    // address (anyone can write "From: me"), and only from a mail newer than the key we have.
    let own = accounts(o).iter().any(|a| bare(&a.email) == bare(&from));
    let sent = m.date().map(|d| d.to_timestamp()).unwrap_or_else(now_secs);
    let mut learned = false;
    let origin = {
        use sha2::Digest;
        let id = m.message_id().map(|i| i.as_bytes().to_vec()).unwrap_or_else(|| raw.to_vec());
        crate::kdf::hex(&sha2::Sha256::digest(&id))
    };
    if let Some(h) = m.header_raw("Autocrypt") {
        if let Some((addr, key)) = mailcrypt::parse_autocrypt(&h.replace(['\r', '\n'], "")) {
            if addr == from && !own {
                learned = remember_key_seen(o, &addr, &key, "autocrypt", sent, &origin)?;
            }
        }
    }
    // A key that came with this very mail proves nothing about this mail: its signature counts
    // as "key not confirmed" (shown like an unknown key), not as valid. Also when the mail is
    // opened again later (B12): the key remembers which mail it came with.
    let key_from_this_mail = learned
        || contact_keys(o).iter().any(|c| c.email == bare(&from) && c.source == "autocrypt" && c.origin == origin);
    let signer_keys: Vec<SignedPublicKey> = key_for(o, &from).map(|(k, _)| vec![k]).unwrap_or_default();
    let signer_name = |fp: Option<String>| fp.map(|_| from.clone()).unwrap_or_default();
    let ct = m.content_type();
    let kind = ct.map(|c| (c.ctype().to_lowercase(), c.subtype().unwrap_or("").to_lowercase()));
    let boundary = ct.and_then(|c| c.attribute("boundary")).map(str::to_owned);
    let result = match (kind.as_ref().map(|(a, b)| (a.as_str(), b.as_str())), boundary) {
        (Some(("multipart", "encrypted")), Some(boundary)) => {
            let body = &raw[body_offset(raw)..];
            let parts = mime_parts(body, &boundary);
            let data = parts
                .get(1)
                .copied()
                .ok_or_else(|| GhulError::InvalidInput { msg: "broken encrypted message".into() })?;
            let (key, created) = ensure_key_changed(o, account_id)?;
            learned |= created;
            let opened = mailcrypt::decrypt(data, &key, &signer_keys)?;
            let inner = mail_parser::MessageParser::default().parse(&opened.plain[..]);
            let subject = inner.as_ref().and_then(|i| i.subject().map(str::to_owned));
            let status = match (&opened.signer, opened.signed) {
                (Some(_), _) => MailSignatureStatus::Valid,
                (None, true) if signer_keys.is_empty() => MailSignatureStatus::UnknownKey,
                (None, true) => MailSignatureStatus::Invalid,
                (None, false) => MailSignatureStatus::None,
            };
            Unwrapped {
                raw: opened.plain.clone(),
                encrypted: true,
                signature: status,
                signer: signer_name(opened.signer),
                subject,
                changed: false,
            }
        }
        (Some(("multipart", "signed")), Some(boundary)) => {
            let body = &raw[body_offset(raw)..];
            let parts = mime_parts(body, &boundary);
            let (status, signer) = match (parts.first(), parts.get(1)) {
                (Some(entity), Some(sig_part)) => {
                    let sig = &sig_part[body_offset(sig_part)..];
                    let entity = to_crlf(entity);
                    match mailcrypt::verify_detached(&entity, sig, &signer_keys) {
                        Some(fp) => (MailSignatureStatus::Valid, signer_name(Some(fp))),
                        None if signer_keys.is_empty() => (MailSignatureStatus::UnknownKey, String::new()),
                        None => (MailSignatureStatus::Invalid, String::new()),
                    }
                }
                _ => (MailSignatureStatus::Invalid, String::new()),
            };
            Unwrapped {
                raw: Zeroizing::new(raw.to_vec()),
                encrypted: false,
                signature: status,
                signer,
                subject: None,
                changed: false,
            }
        }
        _ => {
            // Inline OpenPGP in a plain text mail (some mobile apps send it so).
            let text = m.body_text(0).unwrap_or_default();
            if text.contains("-----BEGIN PGP MESSAGE-----") {
                let key = ensure_key(o, account_id)?;
                match mailcrypt::decrypt(text.as_bytes(), &key, &signer_keys) {
                    Ok(opened) => {
                        let status = if opened.signer.is_some() {
                            MailSignatureStatus::Valid
                        } else if opened.signed {
                            MailSignatureStatus::UnknownKey
                        } else {
                            MailSignatureStatus::None
                        };
                        let mut plain = b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec();
                        plain.extend_from_slice(&opened.plain);
                        Unwrapped {
                            raw: Zeroizing::new(plain),
                            encrypted: true,
                            signature: status,
                            signer: signer_name(opened.signer),
                            subject: None,
                            changed: false,
                        }
                    }
                    Err(_) => Unwrapped {
                        raw: Zeroizing::new(raw.to_vec()),
                        encrypted: false,
                        signature: MailSignatureStatus::None,
                        signer: String::new(),
                        subject: None,
                        changed: false,
                    },
                }
            } else {
                Unwrapped {
                    raw: Zeroizing::new(raw.to_vec()),
                    encrypted: false,
                    signature: MailSignatureStatus::None,
                    signer: String::new(),
                    subject: None,
                    changed: false,
                }
            }
        }
    };
    Ok((Unwrapped { changed: learned || result.changed, ..result }, key_from_this_mail))
}

/// Suggested servers for an e-mail address: known providers, otherwise `imap.`/`smtp.` + domain.
#[uniffi::export]
pub fn mail_suggest_servers(email: String) -> MailServers {
    let domain = email.rsplit_once('@').map(|(_, d)| d.trim().to_lowercase()).unwrap_or_default();
    let s = |ih: &str, ip: u16, is: MailSecurity, sh: &str, sp: u16, ss: MailSecurity| MailServers {
        imap_host: ih.to_owned(),
        imap_port: ip,
        imap_security: is,
        smtp_host: sh.to_owned(),
        smtp_port: sp,
        smtp_security: ss,
    };
    use MailSecurity::{StartTls as S, Tls as T};
    match domain.as_str() {
        "posteo.de" | "posteo.net" | "posteo.eu" => s("posteo.de", 993, T, "posteo.de", 465, T),
        "mailbox.org" => s("imap.mailbox.org", 993, T, "smtp.mailbox.org", 465, T),
        "gmx.de" | "gmx.net" | "gmx.at" | "gmx.ch" => s("imap.gmx.net", 993, T, "mail.gmx.net", 465, T),
        "web.de" => s("imap.web.de", 993, T, "smtp.web.de", 587, S),
        "gmail.com" | "googlemail.com" => s("imap.gmail.com", 993, T, "smtp.gmail.com", 465, T),
        "outlook.com" | "hotmail.com" | "live.com" | "outlook.de" | "hotmail.de" => {
            s("outlook.office365.com", 993, T, "smtp-mail.outlook.com", 587, S)
        }
        "t-online.de" => s("secureimap.t-online.de", 993, T, "securesmtp.t-online.de", 465, T),
        "icloud.com" | "me.com" | "mac.com" => s("imap.mail.me.com", 993, T, "smtp.mail.me.com", 587, S),
        "yahoo.com" | "yahoo.de" => s("imap.mail.yahoo.com", 993, T, "smtp.mail.yahoo.com", 465, T),
        "ionos.de" | "online.de" => s("imap.ionos.de", 993, T, "smtp.ionos.de", 465, T),
        "freenet.de" => s("mx.freenet.de", 993, T, "mx.freenet.de", 587, S),
        "tutanota.com" | "tuta.io" | "proton.me" | "protonmail.com" => {
            // No standard IMAP without the provider's own bridge; keep the guess editable.
            s(&format!("imap.{domain}"), 993, T, &format!("smtp.{domain}"), 587, S)
        }
        // Implicit TLS is preferred (RFC 8314); STARTTLS only where a provider offers nothing else.
        "" => s("", 993, T, "", 465, T),
        d => s(&format!("imap.{d}"), 993, T, &format!("smtp.{d}"), 465, T),
    }
}

/// Header fields of a raw header or message.
pub(super) fn summary(l: &imap::Listed) -> MailSummary {
    let parsed = mail_parser::MessageParser::default().parse_headers(&l.header);
    let (from, from_address) = parsed.as_ref().and_then(|m| m.from()).and_then(|a| a.first()).map_or_else(
        || (String::new(), String::new()),
        |a| {
            let addr = a.address().unwrap_or_default().to_owned();
            (a.name().map(str::to_owned).unwrap_or_else(|| addr.clone()), addr)
        },
    );
    let encrypted = parsed.as_ref().and_then(|m| m.content_type()).is_some_and(|ct| {
        ct.ctype().eq_ignore_ascii_case("multipart")
            && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("encrypted"))
    }) || l.text.windows(27).any(|w| w == b"-----BEGIN PGP MESSAGE-----");
    let has_attachments = parsed.as_ref().and_then(|m| m.content_type()).is_some_and(|ct| {
        ct.ctype().eq_ignore_ascii_case("multipart") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
    });
    MailSummary {
        uid: l.uid,
        from,
        from_address,
        subject: parsed.as_ref().and_then(|m| m.subject()).unwrap_or_default().to_owned(),
        date: parsed.as_ref().and_then(|m| m.date()).map(|d| d.to_timestamp()).unwrap_or(0),
        seen: l.seen,
        flagged: l.flagged,
        has_attachments,
        size: l.size,
        preview: preview(l, encrypted),
        encrypted,
        labels: l.labels.clone(),
        thread_key: thread_key(parsed.as_ref().and_then(|m| m.subject()).unwrap_or_default()),
    }
}

/// "Re: AW: Fwd: Treffen" → "treffen".
pub(crate) fn thread_key(subject: &str) -> String {
    let mut s = subject.trim();
    loop {
        let lower = s.to_lowercase();
        let cut = ["re:", "aw:", "fwd:", "fw:", "wg:", "antw:", "sv:", "vs:"]
            .iter()
            .find(|p| lower.starts_with(*p))
            .map(|p| p.len());
        match cut {
            Some(n) => s = s[n..].trim_start(),
            None => break,
        }
    }
    s.to_lowercase()
}

/// A short preview line from the start of the body.
fn preview(l: &imap::Listed, encrypted: bool) -> String {
    if encrypted || l.text.is_empty() {
        return String::new();
    }
    let mut raw = l.header.clone();
    raw.extend_from_slice(&l.text);
    let text = mail_parser::MessageParser::default()
        .parse(&raw[..])
        .and_then(|m| m.body_text(0).map(|t| t.into_owned()))
        .unwrap_or_default();
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('>') && !l.starts_with("-----BEGIN PGP"))
        .take(4)
        .collect();
    let mut out: String = lines.join(" ").chars().take(160).collect();
    if out.chars().count() == 160 {
        out.push('…');
    }
    out
}

fn addresses(a: Option<&mail_parser::Address<'_>>) -> Vec<String> {
    a.map(|a| {
        a.iter()
            .map(|x| match (x.name(), x.address()) {
                (Some(n), Some(ad)) => format!("{n} <{ad}>"),
                (None, Some(ad)) => ad.to_owned(),
                (Some(n), None) => n.to_owned(),
                (None, None) => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn message(uid: u32, raw: &[u8], u: &Unwrapped) -> Result<MailMessage> {
    let m = mail_parser::MessageParser::default()
        .parse(raw)
        .ok_or_else(|| GhulError::Network { msg: "message cannot be read".into() })?;
    // Content (text, attachments) from the decrypted part where there is one.
    let c = mail_parser::MessageParser::default().parse(&u.raw[..]).unwrap_or_else(|| m.clone());
    let first = m.from().and_then(|a| a.first());
    let from_address = first.and_then(|a| a.address()).unwrap_or_default().to_owned();
    let from = first.and_then(|a| a.name()).map(str::to_owned).unwrap_or_else(|| from_address.clone());
    let was_html = c.text_body.is_empty() && !c.html_body.is_empty();
    let html_part = c
        .html_body
        .first()
        .and_then(|i| c.parts.get(*i as usize))
        .is_some_and(|p| matches!(p.body, mail_parser::PartType::Html(_)));
    let html = if html_part { c.body_html(0).map(|h| super::mailhtml::parse(&h)) } else { None };
    let mut inline_images = Vec::new();
    let mut inline_total = 0usize;
    if html.is_some() {
        use mail_parser::MimeHeaders;
        for part in c.parts.iter() {
            let image = part.content_type().is_some_and(|t| t.ctype().eq_ignore_ascii_case("image"));
            if let (true, Some(cid)) = (image, part.content_id()) {
                let data = part.contents();
                if inline_total + data.len() <= 8 * 1024 * 1024 {
                    inline_total += data.len();
                    inline_images.push(MailInlineImage {
                        cid: cid.trim_matches(|c| c == '<' || c == '>').to_owned(),
                        data: data.to_vec(),
                    });
                }
            }
        }
    }
    let text = c.body_text(0).map(|t| t.replace("\r\n", "\n")).unwrap_or_default();
    let attachments = c
        .attachments()
        // The signature part of a signed message is not an attachment for the user.
        .filter(|a| {
            !a.content_type().is_some_and(|t| t.subtype().is_some_and(|s| s.eq_ignore_ascii_case("pgp-signature")))
        })
        .enumerate()
        .map(|(i, a)| MailAttachment {
            index: i as u32,
            name: a.attachment_name().unwrap_or("Anhang").to_owned(),
            mime: a
                .content_type()
                .map(|c| format!("{}/{}", c.ctype(), c.subtype().unwrap_or("octet-stream")))
                .unwrap_or_else(|| "application/octet-stream".into()),
            size: a.contents().len() as u64,
        })
        .collect();
    Ok(MailMessage {
        uid,
        from,
        from_address,
        to: addresses(m.to()),
        cc: addresses(m.cc()),
        reply_to: m.reply_to().and_then(|a| a.first()).and_then(|a| a.address()).unwrap_or_default().to_owned(),
        subject: u.subject.clone().unwrap_or_else(|| m.subject().unwrap_or_default().to_owned()),
        date: m.date().map(|d| d.to_timestamp()).unwrap_or(0),
        text,
        was_html,
        message_id: m.message_id().unwrap_or_default().to_owned(),
        attachments,
        encrypted: u.encrypted,
        signature: u.signature,
        signer: u.signer.clone(),
        remote_images: html.as_ref().map(|h| h.remote_images).unwrap_or(0),
        blocks: html.map(|h| h.blocks).unwrap_or_default(),
        inline_images,
    })
}

/// How an outgoing mail is protected.
pub(crate) enum Protection {
    /// Signed (PGP/MIME), with the sender's key in the Autocrypt header.
    Signed(SignedSecretKey),
    /// Encrypted for these recipients (and the sender), signed inside.
    Encrypted(SignedSecretKey, Vec<SignedPublicKey>),
    /// No OpenPGP at all (switched off for this mail).
    Plain,
}

/// RFC 2047 encoded word for a header value with non-ASCII characters.
fn encode_word(s: &str) -> String {
    if s.is_ascii() {
        s.to_owned()
    } else {
        format!("=?utf-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(s.as_bytes()))
    }
}

/// `Autocrypt:` header for lettre.
#[derive(Clone)]
struct AutocryptHeader(String);

impl lettre::message::header::Header for AutocryptHeader {
    fn name() -> lettre::message::header::HeaderName {
        lettre::message::header::HeaderName::new_from_ascii_str("Autocrypt")
    }
    fn parse(s: &str) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AutocryptHeader(s.to_owned()))
    }
    fn display(&self) -> lettre::message::header::HeaderValue {
        lettre::message::header::HeaderValue::new(Self::name(), self.0.clone())
    }
}

fn random_boundary() -> String {
    format!("ghul-{}", crate::kdf::hex(&new_id()))
}

/// One header line: no line breaks or other control characters (they could add headers, e.g.
/// a hidden Bcc), no surrounding spaces.
fn header_text(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_owned()
}

/// Values that end up in headers, made safe: subject, reference, attachment names and types.
fn header_safe(mail: &OutgoingMail) -> OutgoingMail {
    let mut m = mail.clone();
    m.subject = header_text(&mail.subject);
    // A message id has no spaces, brackets or control characters.
    m.in_reply_to = mail
        .in_reply_to
        .trim()
        .trim_matches(['<', '>'])
        .chars()
        .take_while(|c| !c.is_control() && !c.is_whitespace() && *c != '<' && *c != '>')
        .collect();
    for a in &mut m.attachments {
        a.name = header_text(&a.name).replace(['/', '\\'], "_");
        if a.name.is_empty() {
            a.name = "Anhang".into();
        }
        let simple = a.mime.split_once('/').is_some_and(|(t, st)| {
            let token =
                |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c));
            token(t) && token(st)
        });
        if !simple {
            a.mime = "application/octet-stream".into();
        }
    }
    m
}

#[cfg(test)]
pub(crate) fn build_for_tests(s: &GhulSuite, mail: &OutgoingMail) -> Result<(Vec<u8>, Vec<String>)> {
    let acc = s.with_open(|o| account(o, &mail.account_id))?;
    build(&acc, mail, &Protection::Plain)
}

fn build(account: &StoredAccount, mail: &OutgoingMail, protection: &Protection) -> Result<(Vec<u8>, Vec<String>)> {
    let mail = &header_safe(mail);
    use lettre::message::{header::ContentType, Attachment, Body, Mailbox, MultiPart, SinglePart};
    let bad = |e: &dyn std::fmt::Display| GhulError::InvalidInput { msg: e.to_string() };
    let from: Mailbox =
        format!("{} <{}>", header_text(&account.display_name).replace(['<', '>', '"'], ""), account.email)
            .parse()
            .or_else(|_| account.email.parse())
            .map_err(|e| bad(&e))?;
    let encrypted = matches!(protection, Protection::Encrypted(..));
    // Encrypted mails show "..." outside; the real subject travels inside.
    let outer_subject = if encrypted { "...".to_owned() } else { mail.subject.clone() };
    let mut b = lettre::Message::builder().from(from).subject(outer_subject);
    let mut rcpt = Vec::new();
    for (list, kind) in [(&mail.to, 0), (&mail.cc, 1), (&mail.bcc, 2)] {
        for addr in list.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
            let mb: Mailbox = addr.parse().map_err(|e| bad(&e))?;
            rcpt.push(mb.email.to_string());
            b = match kind {
                0 => b.to(mb),
                1 => b.cc(mb),
                _ => b.bcc(mb),
            };
        }
    }
    if rcpt.is_empty() {
        return Err(GhulError::InvalidInput { msg: "no recipient".into() });
    }
    if !mail.in_reply_to.trim().is_empty() {
        let id = format!("<{}>", mail.in_reply_to.trim().trim_matches(['<', '>']));
        b = b.in_reply_to(id.clone()).references(id);
    }
    // The content as its own MIME entity: text plus attachments.
    let mut text = mail.text.clone();
    if !account.signature.trim().is_empty() {
        text.push_str("\n\n-- \n");
        text.push_str(account.signature.trim());
    }
    let text_part = SinglePart::plain(text);
    let key = match protection {
        Protection::Signed(k) | Protection::Encrypted(k, _) => k,
        Protection::Plain => {
            // A normal mail: no signature part, no Autocrypt header.
            let msg = if mail.attachments.is_empty() {
                b.singlepart(text_part)
            } else {
                let mut mp = MultiPart::mixed().singlepart(text_part);
                for a in &mail.attachments {
                    let ct = ContentType::parse(&a.mime)
                        .unwrap_or(ContentType::parse("application/octet-stream").expect("valid"));
                    mp = mp.singlepart(Attachment::new(a.name.clone()).body(a.data.clone(), ct));
                }
                b.multipart(mp)
            }
            .map_err(|e| bad(&e))?;
            return Ok((msg.formatted(), rcpt));
        }
    };
    b = b.header(AutocryptHeader(mailcrypt::autocrypt_header(&account.email, &mailcrypt::public_of(key))?));
    let mut entity = if mail.attachments.is_empty() && !encrypted {
        text_part.formatted()
    } else {
        let mut mp = MultiPart::mixed().singlepart(text_part);
        for a in &mail.attachments {
            let ct =
                ContentType::parse(&a.mime).unwrap_or(ContentType::parse("application/octet-stream").expect("valid"));
            mp = mp.singlepart(Attachment::new(a.name.clone()).body(a.data.clone(), ct));
        }
        mp.formatted()
    };
    // lettre ends every entity with CRLF; in a multipart that CRLF belongs to the delimiter.
    if entity.ends_with(b"\r\n") {
        entity.truncate(entity.len() - 2);
    }
    let boundary = random_boundary();
    let (content_type, body) = match protection {
        Protection::Signed(key) => {
            let sig = mailcrypt::sign_detached(&entity, key)?;
            let mut body = b"This is an OpenPGP/MIME signed message (RFC 4880 and 3156)\r\n".to_vec();
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(&entity);
            body.extend_from_slice(
                format!(
                    "\r\n--{boundary}\r\nContent-Type: application/pgp-signature; name=\"signature.asc\"\r\n\
                 Content-Description: OpenPGP digital signature\r\n\
                 Content-Disposition: attachment; filename=\"signature.asc\"\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&to_crlf(sig.as_bytes()));
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            (
                format!("multipart/signed; boundary=\"{boundary}\"; micalg=pgp-sha256; protocol=\"application/pgp-signature\""),
                body,
            )
        }
        Protection::Encrypted(key, recipients) => {
            // Protected headers (the subject inside), see draft-autocrypt-lamps-protected-headers.
            let mut inner = format!("Subject: {}\r\n", encode_word(&mail.subject)).into_bytes();
            let marked = String::from_utf8_lossy(&entity).replacen(
                "Content-Type: multipart/mixed;",
                "Content-Type: multipart/mixed; protected-headers=\"v1\";",
                1,
            );
            inner.extend_from_slice(marked.as_bytes());
            let armored = mailcrypt::encrypt_signed(&inner, key, recipients)?;
            let mut body = b"This is an OpenPGP/MIME encrypted message (RFC 4880 and 3156)\r\n".to_vec();
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Type: application/pgp-encrypted\r\n\
                 Content-Description: PGP/MIME version identification\r\n\r\nVersion: 1\r\n\r\n\
                 --{boundary}\r\nContent-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
                 Content-Description: OpenPGP encrypted message\r\n\
                 Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&to_crlf(armored.as_bytes()));
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            (format!("multipart/encrypted; boundary=\"{boundary}\"; protocol=\"application/pgp-encrypted\""), body)
        }
        Protection::Plain => unreachable!("plain mails return above"),
    };
    let body = Body::new_with_encoding(body, lettre::message::header::ContentTransferEncoding::SevenBit)
        .map_err(|_| bad(&"message is not 7-bit clean"))?;
    let msg = b.header(ContentType::parse(&content_type).map_err(|e| bad(&e))?).body(body).map_err(|e| bad(&e))?;
    Ok((msg.formatted(), rcpt))
}

fn sent_folder(folders: &[imap::Folder]) -> Option<String> {
    folders
        .iter()
        .find(|f| f.role == "sent")
        .or_else(|| {
            folders.iter().find(|f| f.name.eq_ignore_ascii_case("Sent") || f.name.eq_ignore_ascii_case("Gesendet"))
        })
        .map(|f| f.name.clone())
}

fn trash_folder(folders: &[imap::Folder]) -> Option<String> {
    folders
        .iter()
        .find(|f| f.role == "trash")
        .or_else(|| {
            folders.iter().find(|f| f.name.eq_ignore_ascii_case("Trash") || f.name.eq_ignore_ascii_case("Papierkorb"))
        })
        .map(|f| f.name.clone())
}

/// Only encrypted connections: IMAP with TLS from the start (port 993), SMTP with TLS or a
/// STARTTLS that must succeed (some providers offer nothing else for sending).
fn require_secure(servers: &MailServers) -> Result<()> {
    if servers.imap_security != MailSecurity::Tls {
        return Err(GhulError::InvalidInput { msg: "IMAP is only allowed with TLS (port 993)".into() });
    }
    Ok(())
}

/// Looks for a key at the recipient's provider (Web Key Directory). Only https, short timeout.
fn wkd_lookup(email: &str) -> Option<SignedPublicKey> {
    let urls = mailcrypt::wkd_urls(email)?;
    for url in urls.iter() {
        let Ok(u) = crate::net::http::Url::parse(&wkd_override(url)) else { continue };
        let resp = block_on(crate::net::http::request("GET", &u, &[], b"", None), std::time::Duration::from_secs(6));
        if let Ok(r) = resp {
            if r.status == 200 {
                if let Ok(key) = mailcrypt::parse_public(&r.body) {
                    if mailcrypt::emails(&key).contains(&bare(email)) {
                        return Some(key);
                    }
                }
            }
        }
    }
    None
}

/// Tests point WKD at a local server.
#[cfg(test)]
fn wkd_override(url: &str) -> String {
    match std::env::var("GHUL_TEST_WKD") {
        Ok(base) => {
            let path = url.splitn(4, '/').nth(3).unwrap_or("");
            format!("{base}/{path}")
        }
        Err(_) => url.to_owned(),
    }
}

#[cfg(not(test))]
fn wkd_override(url: &str) -> String {
    url.to_owned()
}

fn check(e: NetError, imap: bool) -> Result<MailCheck> {
    match e {
        NetError::CertUntrusted(fingerprint) if imap => Ok(MailCheck::ImapCertificate { fingerprint }),
        NetError::CertUntrusted(fingerprint) => Ok(MailCheck::SmtpCertificate { fingerprint }),
        other => Err(other.into()),
    }
}

impl GhulSuite {
    /// Runs `f` on the account's IMAP session (reused; reconnects once if it broke).
    pub(super) fn with_imap<T: Send>(
        &self,
        account_id: &str,
        f: impl for<'a> Fn(
                &'a mut ImapSession,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::net::NetResult<T>> + Send + 'a>>
            + Send
            + Sync,
    ) -> Result<T> {
        let acc = self.with_open(|o| account(o, account_id))?;
        let cfg = imap_config(&acc);
        let cached = self.net.lock().unwrap_or_else(|p| p.into_inner()).imap.remove(account_id);
        let mut session = match cached {
            Some(s) => s,
            None => block_on(imap::connect(&cfg), TIMEOUT)?,
        };
        let r = block_on(f(&mut session), TIMEOUT * 4);
        let (r, session) = match r {
            Ok(v) => (Ok(v), Some(session)),
            Err(NetError::Protocol(_)) | Err(NetError::Connect(_)) | Err(NetError::Timeout) => {
                // The connection may have timed out on the server: once more, fresh.
                let mut fresh = block_on(imap::connect(&cfg), TIMEOUT)?;
                let r = block_on(f(&mut fresh), TIMEOUT * 4);
                let keep = r.is_ok().then_some(fresh);
                (r, keep)
            }
            Err(e) => (Err(e), None),
        };
        if let Some(s) = session {
            self.net.lock().unwrap_or_else(|p| p.into_inner()).imap.insert(account_id.to_owned(), s);
        }
        Ok(r?)
    }

    pub(super) fn mail_folders_raw(&self, account_id: &str) -> Result<Vec<imap::Folder>> {
        self.with_imap(account_id, |s| Box::pin(imap::folders(s)))
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Checks login and certificates of both servers. Untrusted certificates are returned with
    /// their fingerprint for the user to compare and confirm (then pass them in the input).
    pub fn mail_check_account(&self, input: MailAccountInput) -> Result<MailCheck> {
        let a = StoredAccount {
            id: String::new(),
            name: input.name.clone(),
            display_name: input.display_name.clone(),
            email: input.email.clone(),
            username: input.username.clone(),
            password: input.password.clone(),
            servers: input.servers.clone(),
            imap_cert: input.imap_cert.clone(),
            smtp_cert: input.smtp_cert.clone(),
            signature: String::new(),
            pgp_key: String::new(),
            changed: 0,
        };
        require_secure(&input.servers)?;
        let cfg = imap_config(&a);
        let login = async {
            let mut session = imap::connect(&cfg).await?;
            // Some servers close the connection right after BYE; that is fine.
            let _ = session.logout().await;
            Ok(())
        };
        match block_on(login, TIMEOUT) {
            Ok(()) => {}
            Err(e) => return check(e, true),
        }
        // SMTP: connect, TLS, login, without sending (an empty recipient list fails after AUTH).
        let scfg = smtp_config(&a);
        match block_on(smtp::login_only(&scfg), TIMEOUT) {
            Ok(()) => Ok(MailCheck::Ok),
            Err(e) => check(e, false),
        }
    }

    /// Stores a new account (`id` empty) or updates one; returns its id.
    pub fn mail_save_account(&self, id: Option<String>, input: MailAccountInput) -> Result<String> {
        if !input.email.contains('@') {
            return Err(GhulError::InvalidInput { msg: "invalid e-mail address".into() });
        }
        require_secure(&input.servers)?;
        self.with_open(|o| {
            self.ensure_area(o, VaultKind::Mail)?;
            let mut list = accounts(o);
            let id = id.unwrap_or_else(|| crate::kdf::hex(&new_id()[..8]));
            let mut password = input.password.clone();
            let mut pgp_key = String::new();
            if let Some(old) = list.iter().find(|a| a.id == id) {
                if password.is_empty() {
                    password = old.password.clone();
                }
                // The key stays when name or servers change; a new address gets a new key.
                if old.email.eq_ignore_ascii_case(input.email.trim()) {
                    pgp_key = old.pgp_key.clone();
                }
            }
            if pgp_key.is_empty() {
                pgp_key = mailcrypt::generate(input.display_name.trim(), input.email.trim())?.0.to_string();
            }
            list.retain(|a| a.id != id);
            list.push(StoredAccount {
                id: id.clone(),
                name: if input.name.trim().is_empty() {
                    input.email.trim().to_owned()
                } else {
                    input.name.trim().to_owned()
                },
                display_name: input.display_name.trim().to_owned(),
                email: input.email.trim().to_owned(),
                username: if input.username.trim().is_empty() {
                    input.email.trim().to_owned()
                } else {
                    input.username.trim().to_owned()
                },
                password,
                servers: input.servers.clone(),
                imap_cert: input.imap_cert.clone(),
                smtp_cert: input.smtp_cert.clone(),
                signature: input.signature.clone(),
                pgp_key,
                changed: super::suite::now_ms(),
            });
            put_area_setting(o, VaultKind::Mail, ACCOUNTS_KEY, &list)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).imap.remove(&id);
            self.save_area(o, VaultKind::Mail)?;
            Ok(id)
        })
    }

    pub fn mail_accounts(&self) -> Result<Vec<MailAccountInfo>> {
        self.with_open(|o| Ok(accounts(o).iter().map(info).collect()))
    }

    pub fn mail_remove_account(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let mut list = accounts(o);
            list.retain(|a| a.id != id);
            put_area_setting(o, VaultKind::Mail, ACCOUNTS_KEY, &list)?;
            mark_area_deleted(o, VaultKind::Mail, ACCOUNTS_KEY, &id)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).imap.remove(&id);
            self.save_area(o, VaultKind::Mail)
        })
    }

    pub fn mail_folders(&self, account_id: String) -> Result<Vec<MailFolder>> {
        let mut f: Vec<MailFolder> = self
            .mail_folders_raw(&account_id)?
            .into_iter()
            .map(|f| MailFolder { name: f.name, role: f.role })
            .collect();
        let rank = |r: &str| match r {
            "inbox" => 0,
            "drafts" => 1,
            "sent" => 2,
            "archive" => 3,
            "junk" => 4,
            "trash" => 5,
            _ => 6,
        };
        f.sort_by_key(|x| (rank(&x.role), x.name.to_lowercase()));
        Ok(f)
    }

    /// Newest messages of a folder, `limit` at a time.
    pub fn mail_list(&self, account_id: String, folder: String, offset: u32, limit: u32) -> Result<MailPage> {
        let got = self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::list(s, &folder, offset, limit).await })
        });
        let (total, list) = match got {
            Ok(x) => x,
            // Offline: the encrypted copy on this device, if "read offline" is on.
            Err(e @ GhulError::Network { .. }) if offset == 0 => {
                return self.mail_cached_list(account_id.clone(), folder.clone()).ok_or(e);
            }
            Err(e) => return Err(e),
        };
        let page = MailPage { total, messages: list.iter().map(summary).collect() };
        if offset == 0 {
            let mut buf = Vec::new();
            if ciborium::ser::into_writer(&super::mailx::CachedPage::from(&page), &mut buf).is_ok() {
                self.cache_put("list", &[&account_id, &folder], &buf);
            }
        }
        Ok(page)
    }

    pub fn mail_search(&self, account_id: String, folder: String, query: String) -> Result<Vec<MailSummary>> {
        let list = self.with_imap(&account_id, |s| {
            let (folder, query) = (folder.clone(), query.clone());
            Box::pin(async move { imap::search(s, &folder, &query, 100).await })
        })?;
        Ok(list.iter().map(summary).collect())
    }

    /// Reads a message and marks it as read.
    pub fn mail_get(&self, account_id: String, folder: String, uid: u32) -> Result<MailMessage> {
        let uid_s = uid.to_string();
        let got = self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move {
                let raw = imap::fetch(s, &folder, uid).await?;
                imap::set_flag(s, &folder, uid, "\\Seen", true).await?;
                Ok(raw)
            })
        });
        let raw = match got {
            // The cache keeps the message as received (still encrypted if it was).
            Ok(raw) => {
                self.cache_put("msg", &[&account_id, &folder, &uid_s], &raw);
                raw
            }
            Err(GhulError::Network { msg }) => match self.cache_get("msg", &[&account_id, &folder, &uid_s]) {
                Some(c) => c.to_vec(),
                None => return Err(GhulError::Network { msg }),
            },
            Err(e) => return Err(e),
        };
        let raw = Zeroizing::new(raw);
        let u = self.with_open(|o| {
            let u = unwrap_message(o, &account_id, &raw)?;
            if u.changed {
                self.save_area(o, VaultKind::Mail)?;
            }
            Ok(u)
        })?;
        message(uid, &raw, &u)
    }

    /// Content of attachment `index` of a message.
    pub fn mail_attachment(&self, account_id: String, folder: String, uid: u32, index: u32) -> Result<Vec<u8>> {
        let raw = self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::fetch(s, &folder, uid).await })
        })?;
        let u = self.with_open(|o| unwrap_message(o, &account_id, &raw))?;
        let m = mail_parser::MessageParser::default().parse(&u.raw[..]).ok_or(GhulError::NotFound)?;
        let a = m
            .attachments()
            .filter(|a| {
                !a.content_type().is_some_and(|t| t.subtype().is_some_and(|s| s.eq_ignore_ascii_case("pgp-signature")))
            })
            .nth(index as usize)
            .ok_or(GhulError::NotFound)?;
        Ok(a.contents().to_vec())
    }

    pub fn mail_set_seen(&self, account_id: String, folder: String, uid: u32, seen: bool) -> Result<()> {
        self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::set_flag(s, &folder, uid, "\\Seen", seen).await })
        })
    }

    pub fn mail_set_flagged(&self, account_id: String, folder: String, uid: u32, flagged: bool) -> Result<()> {
        self.with_imap(&account_id, |s| {
            let folder = folder.clone();
            Box::pin(async move { imap::set_flag(s, &folder, uid, "\\Flagged", flagged).await })
        })
    }

    pub fn mail_move(&self, account_id: String, folder: String, uid: u32, target: String) -> Result<()> {
        self.with_imap(&account_id, |s| {
            let (folder, target) = (folder.clone(), target.clone());
            Box::pin(async move { imap::move_to(s, &folder, uid, &target).await })
        })
    }

    /// Moves a message to the trash; in the trash it is deleted for good.
    pub fn mail_delete(&self, account_id: String, folder: String, uid: u32) -> Result<()> {
        let folders = self.mail_folders_raw(&account_id)?;
        let trash = trash_folder(&folders);
        self.with_imap(&account_id, |s| {
            let (folder, trash) = (folder.clone(), trash.clone());
            Box::pin(async move {
                match trash {
                    Some(t) if t != folder => imap::move_to(s, &folder, uid, &t).await,
                    _ => imap::delete(s, &folder, uid).await,
                }
            })
        })
    }

    /// Sends a message and stores a copy in "Sent".
    pub fn mail_send(&self, mail: OutgoingMail) -> Result<()> {
        let (acc, protection) = self.with_open(|o| {
            if !mail.pgp {
                return Ok((account(o, &mail.account_id)?, Protection::Plain));
            }
            let (key, created) = ensure_key_changed(o, &mail.account_id)?;
            if created {
                self.save_area(o, VaultKind::Mail)?;
            }
            let acc = account(o, &mail.account_id)?;
            // Encrypted when every recipient has a key; otherwise signed.
            let all: Vec<&String> =
                mail.to.iter().chain(&mail.cc).chain(&mail.bcc).filter(|a| !a.trim().is_empty()).collect();
            let keys: Option<Vec<SignedPublicKey>> = all.iter().map(|a| key_for(o, a).map(|(k, _)| k)).collect();
            let protection = match keys {
                Some(k) if !k.is_empty() => Protection::Encrypted(key, k),
                _ => Protection::Signed(key),
            };
            Ok((acc, protection))
        })?;
        let (bytes, rcpt) = build(&acc, &mail, &protection)?;
        block_on(smtp::send(&smtp_config(&acc), &acc.email, &rcpt, &bytes), TIMEOUT * 4)?;
        // Address suggestions learn from what is sent (never fails the sending).
        let _ = self.remember_contacts(&mail.to.iter().chain(&mail.cc).chain(&mail.bcc).cloned().collect::<Vec<_>>());
        if let Some(sent) = sent_folder(&self.mail_folders_raw(&mail.account_id)?) {
            // Many providers store sent mail themselves; a duplicate copy is harmless but avoided
            // for the known ones that do (Gmail).
            if !acc.servers.imap_host.ends_with("gmail.com") {
                self.with_imap(&mail.account_id, |s| {
                    let (sent, bytes) = (sent.clone(), bytes.clone());
                    Box::pin(async move { imap::append(s, &sent, &bytes, true).await })
                })?;
            }
        }
        Ok(())
    }

    /// Whether each recipient can get encrypted mail. Unknown addresses are looked up at their
    /// provider (Web Key Directory) once; keys found are remembered.
    pub fn mail_recipient_keys(&self, recipients: Vec<String>) -> Result<Vec<MailRecipientKey>> {
        let wanted: Vec<String> = recipients.iter().map(|r| bare(r)).filter(|r| r.contains('@')).collect();
        let missing: Vec<String> =
            self.with_open(|o| Ok(wanted.iter().filter(|r| key_for(o, r).is_none()).cloned().collect()))?;
        let found: Vec<(String, SignedPublicKey)> =
            missing.iter().filter_map(|r| wkd_lookup(r).map(|k| (r.clone(), k))).collect();
        self.with_open(|o| {
            if !found.is_empty() {
                for (r, k) in &found {
                    remember_key(o, r, k, "wkd")?;
                }
                self.save_area(o, VaultKind::Mail)?;
            }
            Ok(wanted
                .iter()
                .map(|r| match key_for(o, r) {
                    Some((_, source)) => MailRecipientKey { address: r.clone(), has_key: true, source },
                    None => MailRecipientKey { address: r.clone(), has_key: false, source: String::new() },
                })
                .collect())
        })
    }

    /// The account's public key (armored), e.g. to publish or send by hand.
    pub fn mail_public_key(&self, account_id: String) -> Result<String> {
        self.with_open(|o| {
            let (key, created) = ensure_key_changed(o, &account_id)?;
            if created {
                self.save_area(o, VaultKind::Mail)?;
            }
            mailcrypt::public_armored(&mailcrypt::public_of(&key))
        })
    }

    /// Imports public keys (armored or binary, e.g. a .asc file); returns their addresses.
    pub fn mail_import_key(&self, data: Vec<u8>) -> Result<Vec<String>> {
        let key = mailcrypt::parse_public(&data)?;
        let addrs = mailcrypt::emails(&key);
        if addrs.is_empty() {
            return Err(GhulError::InvalidInput { msg: "the key has no e-mail address".into() });
        }
        self.with_open(|o| {
            for a in &addrs {
                // An import is a deliberate choice: it replaces any key learned before.
                let mut list = contact_keys(o);
                list.retain(|c| &c.email != a);
                put_area_setting(o, VaultKind::Mail, KEYS_KEY, &list)?;
                remember_key(o, a, &key, "import")?;
            }
            self.save_area(o, VaultKind::Mail)?;
            Ok(addrs.clone())
        })
    }

    pub fn mail_known_keys(&self) -> Result<Vec<MailKeyInfo>> {
        self.with_open(|o| {
            let mut list: Vec<MailKeyInfo> = contact_keys(o)
                .into_iter()
                .map(|c| MailKeyInfo {
                    email: c.email.clone(),
                    fingerprint: mailcrypt::pretty_fingerprint(&c.fingerprint),
                    source: c.source.clone(),
                    updated: c.updated,
                    verified: c.verified,
                    previous: if c.previous.is_empty() {
                        String::new()
                    } else {
                        mailcrypt::pretty_fingerprint(&c.previous)
                    },
                    offered: if c.offered.is_empty() {
                        String::new()
                    } else {
                        mailcrypt::pretty_fingerprint(&c.offered)
                    },
                })
                .collect();
            list.sort_by(|a, b| a.email.cmp(&b.email));
            Ok(list)
        })
    }

    /// L7: the user compared the key's fingerprint with the contact (in person or on the phone)
    /// and it matches: the key counts as confirmed; warnings about a change are cleared.
    pub fn mail_verify_key(&self, email: String) -> Result<()> {
        self.with_open(|o| {
            let mut list = contact_keys(o);
            let c = list.iter_mut().find(|c| c.email == bare(&email)).ok_or(GhulError::NotFound)?;
            c.verified = true;
            c.previous.clear();
            c.offered.clear();
            put_area_setting(o, VaultKind::Mail, KEYS_KEY, &list)?;
            self.save_area(o, VaultKind::Mail)
        })
    }

    pub fn mail_forget_key(&self, email: String) -> Result<()> {
        self.with_open(|o| {
            let mut list = contact_keys(o);
            list.retain(|c| c.email != bare(&email));
            put_area_setting(o, VaultKind::Mail, KEYS_KEY, &list)?;
            mark_area_deleted(o, VaultKind::Mail, KEYS_KEY, &bare(&email))?;
            self.save_area(o, VaultKind::Mail)
        })
    }

    /// Login data for notifications while locked (the app stores it device-bound).
    pub fn mail_poll_config(&self, account_id: String) -> Result<MailPollConfig> {
        self.with_open(|o| {
            let a = account(o, &account_id)?;
            Ok(MailPollConfig {
                account_id: a.id.clone(),
                account_name: a.name.clone(),
                host: a.servers.imap_host.clone(),
                port: a.servers.imap_port,
                security: a.servers.imap_security,
                username: a.username.clone(),
                password: a.password.clone(),
                cert: a.imap_cert.clone(),
            })
        })
    }
}

fn poll_imap_config(config: &MailPollConfig) -> ImapConfig {
    ImapConfig {
        host: config.host.clone(),
        port: config.port,
        security: config.security.into(),
        user: config.username.clone(),
        password: Zeroizing::new(config.password.clone()),
        cert: config.cert.clone(),
    }
}

/// UIDNEXT, unread count and the newest message at or above `since_uid` (if it grew).
async fn poll_status(
    s: &mut imap::ImapSession,
    since_uid: u32,
) -> crate::net::NetResult<(u32, u32, Option<imap::Listed>)> {
    let (next_uid, unseen) = imap::status(s, "INBOX").await?;
    let newest = if since_uid > 0 && next_uid > since_uid { imap::newest(s, "INBOX", since_uid).await? } else { None };
    Ok((next_uid, unseen, newest))
}

fn poll_result(since_uid: u32, (next_uid, unseen, newest): (u32, u32, Option<imap::Listed>)) -> MailPollResult {
    let (from, subject) = newest.as_ref().map(summary).map(|m| (m.from, m.subject)).unwrap_or_default();
    let newest_uid = newest.as_ref().map_or(0, |m| m.uid);
    MailPollResult {
        next_uid,
        unseen,
        new_messages: if since_uid > 0 { next_uid.saturating_sub(since_uid) } else { 0 },
        newest_from: from,
        newest_subject: subject,
        newest_uid,
    }
}

/// Checks the inbox for new messages (works while GhulSuite is locked).
#[uniffi::export]
pub fn mail_poll(config: MailPollConfig, since_uid: u32) -> Result<MailPollResult> {
    let cfg = poll_imap_config(&config);
    let r = block_on(
        async move {
            let mut s = imap::connect(&cfg).await?;
            let r = poll_status(&mut s, since_uid).await?;
            let _ = s.logout().await;
            Ok(r)
        },
        TIMEOUT * 2,
    )?;
    Ok(poll_result(since_uid, r))
}

/// Like [`mail_poll`], but if nothing is new yet, waits up to `wait_secs` for the server to report
/// new mail (IMAP IDLE) and returns right after it arrives. For instant notifications.
#[uniffi::export]
pub fn mail_wait(config: MailPollConfig, since_uid: u32, wait_secs: u32) -> Result<MailPollResult> {
    let cfg = poll_imap_config(&config);
    let wait = std::time::Duration::from_secs(wait_secs.clamp(1, 25 * 60) as u64);
    let r = block_on(
        async move {
            let mut s = imap::connect(&cfg).await?;
            let mut r = poll_status(&mut s, since_uid).await?;
            if since_uid > 0 && r.0 <= since_uid {
                s = imap::idle_inbox(s, wait).await?;
                r = poll_status(&mut s, since_uid).await?;
            }
            let _ = s.logout().await;
            Ok(r)
        },
        TIMEOUT * 2 + wait,
    )?;
    Ok(poll_result(since_uid, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_of_plain_and_encrypted_mail() {
        let l = |header: &[u8], text: &[u8]| imap::Listed {
            uid: 1,
            seen: false,
            flagged: false,
            size: 1,
            header: header.to_vec(),
            text: text.to_vec(),
            labels: vec![],
        };
        let plain = summary(&l(
            b"From: a@b.c\r\nSubject: AW: Hallo\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n",
            b"Hi Max,\r\n\r\n> alt\r\npasst dir Freitag?\r\n",
        ));
        assert_eq!(plain.preview, "Hi Max, passt dir Freitag?");
        assert!(!plain.encrypted);
        assert_eq!(plain.thread_key, "hallo");
        let enc = summary(&l(
            b"From: a@b.c\r\nSubject: ...\r\nContent-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=x\r\n\r\n",
            b"--x\r\n...-----BEGIN PGP MESSAGE-----",
        ));
        assert!(enc.encrypted && enc.preview.is_empty());
    }

    fn account(key: &str) -> StoredAccount {
        StoredAccount {
            id: "a".into(),
            name: "Test".into(),
            display_name: "Alice Ä.".into(),
            email: "alice@example.org".into(),
            username: "alice".into(),
            password: String::new(),
            servers: mail_suggest_servers("alice@example.org".into()),
            imap_cert: None,
            smtp_cert: None,
            signature: "Viele Grüße\nAlice".into(),
            pgp_key: key.to_owned(),
            changed: 0,
        }
    }

    fn outgoing(attachments: Vec<OutgoingAttachment>) -> OutgoingMail {
        OutgoingMail {
            account_id: "a".into(),
            to: vec!["Bob <bob@example.org>".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Grüße \u{2013} Test".into(),
            text: "Zeile 1\nZeile mit Umlauten äöü\n.\nEnde".into(),
            in_reply_to: String::new(),
            attachments,
            pgp: true,
        }
    }

    /// Received mails can be anything: header, MIME structure, encodings, the preview line.
    #[test]
    fn received_mails_never_crash_the_reader() {
        let seeds = crate::fuzz_tests::seeds(&[
            "From: =?utf-8?q?M=C3=BCller?= <m@example.org>\r\nTo: a@example.org, \"B, C\" <b@example.org>\r\nSubject: =?utf-8?b?SGFsbG8g8J+Ygg==?=\r\nDate: Fri, 13 Nov 2026 10:00:00 +0100\r\nMessage-ID: <1@x>\r\nContent-Type: multipart/mixed; boundary=\"b1\"\r\n\r\n--b1\r\nContent-Type: text/plain; charset=iso-8859-1\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nGr=FC=DFe=\r\n weiter\r\n--b1\r\nContent-Type: text/html\r\n\r\n<p>Hallo</p>\r\n--b1\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\nContent-Disposition: attachment; filename=\"../../a.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--b1--\r\n",
            "From: x\r\nSubject: kurz\r\n\r\nText",
            "Content-Type: multipart/signed; boundary=s; protocol=\"application/pgp-signature\"\r\n\r\n--s\r\nContent-Type: text/plain\r\n\r\nsigned\r\n--s\r\nContent-Type: application/pgp-signature\r\n\r\n-----BEGIN PGP SIGNATURE-----\r\nabc\r\n-----END PGP SIGNATURE-----\r\n--s--\r\n",
        ]);
        crate::fuzz_tests::fuzz("mail", &seeds, 3000, |d| {
            let split = body_offset(d);
            let listed = imap::Listed {
                uid: 1,
                seen: false,
                flagged: false,
                size: d.len() as u32,
                header: d[..split.min(d.len())].to_vec(),
                text: d[split.min(d.len())..].to_vec(),
                labels: vec!["\u{202e}x".into()],
            };
            let _ = summary(&listed);
            let _ = preview(&listed, false);
            let _ = mime_parts(d, "b1");
            let _ = thread_key(&String::from_utf8_lossy(d));
            let u = Unwrapped {
                raw: Zeroizing::new(d.to_vec()),
                encrypted: false,
                signature: MailSignatureStatus::None,
                signer: String::new(),
                subject: None,
                changed: false,
            };
            let _ = message(1, d, &u);
        });
    }

    /// With OpenPGP off for a mail: a normal mail, no signature, no Autocrypt header, readable subject.
    #[test]
    fn mail_without_pgp_is_plain() {
        let (sk, _) = mailcrypt::generate("Alice", "alice@example.org").unwrap();
        for attachments in [
            vec![],
            vec![OutgoingAttachment { name: "a.txt".into(), mime: "text/plain".into(), data: b"hallo".to_vec() }],
        ] {
            let mut mail = outgoing(attachments.clone());
            mail.pgp = false;
            let (raw, rcpt) = build(&account(&sk), &mail, &Protection::Plain).unwrap();
            assert_eq!(rcpt, vec!["bob@example.org".to_string()]);
            let m = mail_parser::MessageParser::default().parse(&raw).unwrap();
            assert_eq!(m.subject(), Some("Grüße \u{2013} Test"));
            assert!(m.header_raw("Autocrypt").is_none());
            let text = String::from_utf8_lossy(&raw);
            assert!(!text.contains("pgp-signature") && !text.contains("BEGIN PGP"));
            assert!(m.body_text(0).unwrap().contains("Zeile mit Umlauten äöü"));
            assert!(m.body_text(0).unwrap().contains("Viele Grüße"));
            assert_eq!(m.attachment_count(), attachments.len());
        }
    }

    /// The signed bytes are exactly what a receiver extracts; any change breaks the signature.
    #[test]
    fn signed_mime_framing_and_tampering() {
        let (sk, fp) = mailcrypt::generate("Alice", "alice@example.org").unwrap();
        let key = mailcrypt::secret_key(&sk).unwrap();
        let public = mailcrypt::public_of(&key);
        for attachments in [
            vec![],
            vec![OutgoingAttachment {
                name: "a.bin".into(),
                mime: "application/octet-stream".into(),
                data: vec![1, 2, 3],
            }],
        ] {
            let (raw, rcpt) = build(&account(&sk), &outgoing(attachments), &Protection::Signed(key.clone())).unwrap();
            assert_eq!(rcpt, vec!["bob@example.org".to_string()]);
            let m = mail_parser::MessageParser::default().parse(&raw).unwrap();
            let ct = m.content_type().unwrap();
            assert_eq!((ct.ctype(), ct.subtype()), ("multipart", Some("signed")));
            let boundary = ct.attribute("boundary").unwrap().to_owned();
            assert_eq!(m.subject(), Some("Grüße \u{2013} Test"));
            assert!(m.header_raw("Autocrypt").unwrap().contains("addr=alice@example.org"));
            let body = &raw[body_offset(&raw)..];
            let parts = mime_parts(body, &boundary);
            assert_eq!(parts.len(), 2);
            let sig = &parts[1][body_offset(parts[1])..];
            assert_eq!(mailcrypt::verify_detached(parts[0], sig, std::slice::from_ref(&public)), Some(fp.clone()));
            // One changed character in the text: the signature no longer matches.
            let mut changed = parts[0].to_vec();
            let pos = changed.windows(5).position(|w| w == b"Zeile").unwrap();
            changed[pos] = b'z';
            assert_eq!(mailcrypt::verify_detached(&changed, sig, std::slice::from_ref(&public)), None);
            // The parsed text still reads normally.
            assert!(m.body_text(0).unwrap().contains("Zeile mit Umlauten äöü"));
        }
    }

    #[test]
    fn encrypted_mime_hides_subject_and_content() {
        let (sk, _) = mailcrypt::generate("Alice", "alice@example.org").unwrap();
        let (bob_sk, _) = mailcrypt::generate("Bob", "bob@example.org").unwrap();
        let key = mailcrypt::secret_key(&sk).unwrap();
        let bob = mailcrypt::secret_key(&bob_sk).unwrap();
        let (raw, _) = build(
            &account(&sk),
            &outgoing(vec![]),
            &Protection::Encrypted(key.clone(), vec![mailcrypt::public_of(&bob)]),
        )
        .unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("Subject: ...") && text.contains("multipart/encrypted"));
        assert!(!text.contains("Umlauten") && !text.contains("Gr=C3=BC") && !text.contains("R3LDvMOfZSDigJMgVGVzdA"));
        let m = mail_parser::MessageParser::default().parse(&raw).unwrap();
        let boundary = m.content_type().unwrap().attribute("boundary").unwrap().to_owned();
        let parts = mime_parts(&raw[body_offset(&raw)..], &boundary);
        let opened = mailcrypt::decrypt(parts[1], &bob, &[mailcrypt::public_of(&key)]).unwrap();
        assert!(opened.signer.is_some());
        let inner = mail_parser::MessageParser::default().parse(&opened.plain[..]).unwrap();
        assert_eq!(inner.subject(), Some("Grüße \u{2013} Test"));
        assert!(inner.body_text(0).unwrap().contains("Zeile mit Umlauten äöü"));
    }

    #[test]
    fn only_secure_mail_connections() {
        let mut s = mail_suggest_servers("max@example.org".into());
        assert_eq!((s.imap_security, s.smtp_port, s.smtp_security), (MailSecurity::Tls, 465, MailSecurity::Tls));
        assert!(require_secure(&s).is_ok());
        s.imap_security = MailSecurity::StartTls;
        assert!(require_secure(&s).is_err());
    }
}
