// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite session used by the app.
//!
//! **The folder is the suite:** every vault file (`*.ghlpss`, `*.ghltotp`) in the storage folder
//! belongs to it. All containers share the same password, the same factor slots (security keys
//! and key files, each wrapping the same factor secret) and the same recovery code, so one key
//! touch, or the recovery code alone, opens everything, and every file can also be opened on
//! its own. `suite.ghlsuite` only holds settings (last seen revisions, dismissed files, later mail
//! and storage accounts); it decides nothing about membership and is recreated if missing.
//! Vault files with another protection are offered and, once opened with their own password,
//! switched to the suite's protection.
//!
//! TOTP secrets never leave the core; GhulPass entries are returned to the app because it has to
//! show and fill them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use ciborium::value::Value;
use rand_core::{OsRng, RngCore};
use serde::Serialize;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::storage::VaultStorage;
use super::tools::{validate_totp, OtpAlgo, TotpInput};
use crate::ctap2::Authenticator;
use crate::ctaphid::CtapHid;
use crate::key_device::{CredRequest, KeyDevice};
use crate::vault::container::{Fido2Registration, KdfCost, SuiteKeys};
use crate::vault::{
    ContainerKind, Entry, FactorSecret, Field, Keyfile, LockedContainer, PasswordKey, RecoveryCode, SecondFactor, Slot,
    SlotMeta, UnlockedContainer,
};
use crate::{FidoListener, HidTransport, RP_ID, RP_NAME};

pub const SUITE_FILE: &str = "suite.ghlsuite";
const REVISIONS_KEY: &str = "revisions";
const IGNORED_KEY: &str = "ignored_files";
const NAME_KEY: &str = "name";
/// The factor secrets and recovery codes this device had before its last protection changes
/// (newest first, at most [`MAX_PREVIOUS`]): to read files on the server that another device
/// wrote before it learned of the change, instead of taking them for damaged. Never merged.
pub(super) const PREVIOUS_KEY: &str = "protection_before";
/// Practically no limit: 256 changes of the protection while another device is offline (64 bytes each).
const MAX_PREVIOUS: usize = 256;
/// Slot IDs (hex) of keys and key files removed on purpose (B2). Synchronised and united, so a
/// device that missed the removal (and changed its own protection meanwhile) drops them too.
pub(super) const REMOVED_SLOTS_KEY: &str = "removed_slots";
/// Whether this device has uploaded in its current protection since the last change (then the
/// protections from before no longer open server files, H1).
pub(super) const PREVIOUS_UPLOADED_KEY: &str = "protection_before_uploaded";
/// Fingerprints (SHA-256) of factor secrets and recovery codes replaced on purpose (a lost key
/// removed, a new code made). Synchronised; a protection that brings one back is never taken
/// over from the server (H1).
pub(super) const REVOKED_KEY: &str = "revoked_secrets";

#[derive(Serialize, serde::Deserialize, Default, Clone, PartialEq)]
pub(super) struct PreviousProtection {
    #[serde(with = "serde_bytes")]
    s: Vec<u8>,
    #[serde(with = "serde_bytes", default)]
    code: Vec<u8>,
}

/// Password keys from before a password change, each sealed with a key from the **current**
/// password key: a file still in the old password is checked with them (write tag), and the
/// recovery code alone (which opens this content) does not reveal them. Kept only until every
/// file is in the new protection and on the server.
pub(super) const PASSWORD_BEFORE_KEY: &str = "password_before";

#[derive(Serialize, serde::Deserialize, Default, Clone)]
struct SealedPasswordKey {
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    sealed: Vec<u8>,
}

fn password_before_key(current: &PasswordKey) -> Zeroizing<[u8; 32]> {
    crate::vault::keys::hkdf(current.expose(), &[b"ghul/v3/password-before"])
}

/// The password keys from before, opened with the current one.
fn previous_password_keys(suite: &UnlockedContainer, current: &PasswordKey) -> Vec<PasswordKey> {
    let key = password_before_key(current);
    let list: Vec<SealedPasswordKey> = setting(suite, PASSWORD_BEFORE_KEY);
    list.iter()
        .filter_map(|e| {
            let nonce: [u8; 24] = e.nonce.as_slice().try_into().ok()?;
            let plain = crate::vault::keys::open(&key, &nonce, &e.sealed, b"ghul/v3/password-before")?;
            let k: [u8; 32] = plain.as_slice().try_into().ok()?;
            Some(PasswordKey::from_bytes(k))
        })
        .collect()
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub(super) fn new_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    id
}

// --- exported types --------------------------------------------------------------------------

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultKind {
    Pass,
    Totp,
    /// GhulCal (`.ghlcal`).
    Cal,
    /// GhulDrive (`.ghldrv`).
    Drive,
    /// GhulMail (`.ghlmail`).
    Mail,
}

impl VaultKind {
    pub(super) fn container(self) -> ContainerKind {
        match self {
            VaultKind::Pass => ContainerKind::Pass,
            VaultKind::Totp => ContainerKind::Totp,
            VaultKind::Cal => ContainerKind::Cal,
            VaultKind::Drive => ContainerKind::Drive,
            VaultKind::Mail => ContainerKind::Mail,
        }
    }

    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "pass" => VaultKind::Pass,
            "cal" => VaultKind::Cal,
            "drive" => VaultKind::Drive,
            "mail" => VaultKind::Mail,
            _ => VaultKind::Totp,
        }
    }

    pub(super) fn from_container(k: ContainerKind) -> Option<Self> {
        match k {
            ContainerKind::Pass => Some(VaultKind::Pass),
            ContainerKind::Totp => Some(VaultKind::Totp),
            ContainerKind::Cal => Some(VaultKind::Cal),
            ContainerKind::Drive => Some(VaultKind::Drive),
            ContainerKind::Mail => Some(VaultKind::Mail),
            ContainerKind::Suite => None,
        }
    }
}

/// Field value as shown in the app. Binary fields are never exposed and are preserved on save.
#[derive(uniffi::Enum, Clone, PartialEq)]
pub enum FieldValue {
    Text { value: String },
    Secret { value: String },
    Url { value: String },
    Number { value: i64 },
    Flag { value: bool },
    Items { values: Vec<String> },
}

/// Secret values (passwords, card numbers, keys) never show up in debug output.
impl std::fmt::Debug for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldValue::Text { value } => f.debug_struct("Text").field("value", value).finish(),
            FieldValue::Secret { value } => write!(f, "Secret {{ {} characters }}", value.chars().count()),
            FieldValue::Url { value } => f.debug_struct("Url").field("value", value).finish(),
            FieldValue::Number { value } => f.debug_struct("Number").field("value", value).finish(),
            FieldValue::Flag { value } => f.debug_struct("Flag").field("value", value).finish(),
            FieldValue::Items { values } => f.debug_struct("Items").field("values", values).finish(),
        }
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FieldData {
    pub name: String,
    pub value: FieldValue,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct EntryData {
    /// Empty for a new entry.
    pub id: Vec<u8>,
    pub entry_type: String,
    pub fields: Vec<FieldData>,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub modified: i64,
    pub revision: u64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct EntrySummary {
    pub vault_id: String,
    pub vault_name: String,
    pub id: Vec<u8>,
    pub entry_type: String,
    pub title: String,
    pub subtitle: String,
    pub modified: i64,
    pub favorite: bool,
    /// A 2FA key is stored in the entry.
    pub has_totp: bool,
}

#[derive(uniffi::Record, Clone, PartialEq)]
pub struct TotpCode {
    pub vault_id: String,
    pub vault_name: String,
    pub id: Vec<u8>,
    pub issuer: String,
    pub account: String,
    pub code: String,
    pub next_code: String,
    pub period: u32,
    pub remaining: u32,
    pub digits: u32,
    pub favorite: bool,
    /// Unix milliseconds of the last change (for "newest first").
    pub modified: i64,
}

super::redact::redacted_debug!(TotpCode {
    vault_id,
    vault_name,
    id,
    issuer,
    account,
    period,
    remaining,
    digits,
    favorite,
    modified
});

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    SecurityKey,
    Keyfile,
    RecoveryCode,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SlotInfo {
    pub slot_id: Vec<u8>,
    pub kind: SlotKind,
    pub label: String,
    pub created: i64,
    /// A key or key file of an older app version: it gets the current protection when it is
    /// used to unlock once. Until then, removing another key cannot renew the factor secret.
    pub needs_update: bool,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct VaultInfo {
    pub id: String,
    pub kind: VaultKind,
    pub name: String,
    pub file: String,
    /// False if the file is damaged and cannot be opened.
    pub available: bool,
    pub entries: u32,
    /// The file is older than the last state this suite has seen (e.g. an old copy).
    pub outdated: bool,
}

/// Where a vault to be added comes from.
#[derive(uniffi::Enum, Clone, Debug)]
pub enum VaultSource {
    /// A file that already lies in the suite's folder (kept under its name).
    InFolder { file: String },
    /// File content, e.g. picked by the user; stored as a new file in the folder.
    Bytes { data: Vec<u8> },
}

/// A vault file in the folder with another protection (needs its own password once).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FoundVault {
    pub file: String,
    pub kind: VaultKind,
    /// What it needs: password plus key or key file.
    pub protection: ProtectionInfo,
}

/// What the unlock screen has to ask for.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ProtectionInfo {
    pub security_keys: u32,
    /// Key files that can be used instead of a security key.
    pub keyfiles: u32,
    pub recovery_codes: u32,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NewVault {
    pub kind: VaultKind,
    pub name: String,
}

#[derive(uniffi::Record, Clone)]
pub struct SetupOptions {
    pub password: String,
    /// Key file bytes (from `generate_keyfile`): second factor instead of (or in addition to) a key.
    pub keyfile: Option<Vec<u8>>,
    pub key_label: String,
    pub keyfile_label: String,
    pub recovery_label: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub vaults: Vec<NewVault>,
}

super::redact::redacted_debug!(SetupOptions {
    key_label,
    keyfile_label,
    recovery_label,
    memory_kib,
    iterations,
    vaults
});

#[derive(uniffi::Record, Clone, PartialEq)]
pub struct SetupResult {
    /// New recovery code to show and print once (11 groups of 5 characters); `None` if the
    /// opened vault already carried the suite's code.
    pub recovery_code: Option<String>,
}

super::redact::redacted_debug!(SetupResult {});

/// Generates a new key file (`.ghlkey`, 46 bytes) for the user to store, e.g. on a USB stick.
#[uniffi::export]
pub fn generate_keyfile() -> Vec<u8> {
    Keyfile::generate(&mut OsRng).encode().to_vec()
}

/// What an import file contains (nothing is changed yet).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ImportPreview {
    /// Recognised format, e.g. `Proton Pass`, `Bitwarden`, `Aegis`.
    pub format: String,
    /// GhulPass entries (logins, notes, cards, …).
    pub entries: u32,
    /// 2FA accounts of an authenticator export.
    pub totps: u32,
    /// 2FA codes stored inside logins (can also be copied to GhulAuth).
    pub totps_in_logins: u32,
    /// Items that cannot be converted (e.g. HOTP counters, unsupported types).
    pub skipped: u32,
    /// Which items, and why. Their original data can be kept as notes (`KeepSkipped`).
    pub skipped_items: Vec<SkippedImport>,
}

/// An item of an import file that could not be converted.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SkippedImport {
    pub name: String,
    /// Type in the source app, e.g. `alias`, `hotp`, `sshKey`.
    pub kind: String,
    pub reason: super::import::SkipReason,
    pub detail: String,
    /// A 2FA code that stays in its imported login (only GhulAuth does not get it).
    pub in_entry: bool,
}

/// Keep items that could not be converted as notes in GhulPass, with their original data, so
/// nothing of an import is lost. The texts come from the app (its language).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct KeepSkipped {
    /// Folder of the notes, e.g. "Import (not converted)".
    pub folder: String,
    /// Field names of the note.
    pub reason_label: String,
    pub data_label: String,
    /// Reason texts for `Hotp`, `Unsupported`, `InvalidCode`.
    pub reason_hotp: String,
    pub reason_unsupported: String,
    pub reason_invalid: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ImportResult {
    pub entries: u32,
    pub totps: u32,
    /// Already present in the target vault, not imported again.
    pub duplicates: u32,
    pub skipped: u32,
    pub skipped_items: Vec<SkippedImport>,
    /// Skipped items kept as notes with their original data.
    pub kept: u32,
}

fn skipped_items(list: &[super::import::Skipped]) -> Vec<SkippedImport> {
    list.iter()
        .map(|x| SkippedImport {
            name: x.name.clone(),
            kind: x.kind.clone(),
            reason: x.reason,
            detail: x.detail.clone(),
            in_entry: x.in_entry,
        })
        .collect()
}

/// Reads an import file and tells what it contains.
#[uniffi::export]
pub fn preview_import(data: Vec<u8>, file_name: String) -> Result<ImportPreview> {
    let data = Zeroizing::new(data);
    let p = super::import::parse(&data, &file_name)?;
    Ok(ImportPreview {
        format: p.format.clone(),
        entries: p.entries.len() as u32,
        totps: p.totps.iter().filter(|t| !t.from_login).count() as u32,
        totps_in_logins: p.totps.iter().filter(|t| t.from_login).count() as u32,
        skipped: p.skipped.len() as u32,
        skipped_items: skipped_items(&p.skipped),
    })
}

/// What makes two entries the same for the import: every field (H4: only title, user name,
/// address and password made two different notes or cards with the same title "the same", and
/// the second was dropped without a word). Title, user name and addresses ignore case and
/// surrounding spaces, so importing the same export again still adds nothing twice.
fn dedup_key(entry_type: &str, folder: Option<&str>, fields: &BTreeMap<String, Field>) -> Zeroizing<String> {
    // Built from secrets too: wiped when dropped.
    let mut key = Zeroizing::new(String::from(entry_type));
    // The same entry in two folders is kept twice (as in the export).
    if let Some(f) = folder.map(str::trim).filter(|f| !f.is_empty()) {
        key.push('\u{1f}');
        key.push_str("folder=");
        key.push_str(&f.to_lowercase());
    }
    for (name, value) in fields {
        let loose = matches!(name.as_str(), "title" | "username" | "urls");
        let norm = |s: &str| if loose { s.trim().to_lowercase() } else { s.trim().to_owned() };
        let text = Zeroizing::new(match value {
            Field::Text(s) | Field::Secret(s) | Field::Url(s) => norm(s),
            Field::List(l) => l.iter().map(|x| norm(x)).collect::<Vec<_>>().join("\u{1e}"),
            Field::Bytes(b) => crate::kdf::hex(b),
            Field::Int(i) => i.to_string(),
            Field::Bool(b) => b.to_string(),
        });
        if !text.is_empty() {
            key.push('\u{1f}');
            key.push_str(name);
            key.push('=');
            key.push_str(&text);
        }
    }
    key
}

/// Opens a vault that still has the protection from before a change: with an earlier factor
/// secret (same password parameters), else with the suite's code or an earlier one. Never with a
/// secret or code replaced on purpose (see [`REVOKED_KEY`]): whoever still knows one could have
/// made the file.
fn open_with_before(data: &[u8], suite: &UnlockedContainer, pk: &PasswordKey) -> Option<UnlockedContainer> {
    let before = previous_of(suite, pk);
    let revoked: Vec<String> = setting(suite, REVOKED_KEY);
    let allowed = |secret: &[u8]| !revoked.contains(&revoked_fingerprint(secret));
    // The current password key, and the earlier ones (a password change that stopped half way).
    let earlier = previous_password_keys(suite, pk);
    let pks: Vec<&PasswordKey> = std::iter::once(pk).chain(earlier.iter()).collect();
    for p in before.iter().filter(|p| allowed(p.s.expose())) {
        for k in &pks {
            if let Ok(c) = LockedContainer::parse(data).ok()?.unlock(k, &p.s) {
                return Some(c);
            }
        }
    }
    let codes = suite.recovery_code().into_iter().chain(
        before
            .iter()
            .filter_map(|p| p.code.as_ref())
            .filter(|c| allowed(c.expose()))
            .map(|c| RecoveryCode::from_bytes(*c.expose())),
    );
    for code in codes {
        // The code only gives the factor secret; the file must be written with the password (H0).
        for k in &pks {
            if let Ok(c) = LockedContainer::parse(data).ok()?.recover_with_password(&code, k) {
                return Some(c);
            }
        }
    }
    None
}

/// One protection from before.
pub(super) struct Previous {
    pub s: FactorSecret,
    pub code: Option<RecoveryCode>,
}

/// The protections from before, sealed with a key from the current password key: the recovery
/// code opens the suite file, and with earlier codes and factor secrets it would also open old
/// copies of every file.
fn previous_list(suite: &UnlockedContainer, pk: &PasswordKey) -> Vec<PreviousProtection> {
    let sealed: SealedPasswordKey = setting(suite, PREVIOUS_KEY);
    let Ok(nonce) = <[u8; 24]>::try_from(sealed.nonce.as_slice()) else { return Vec::new() };
    let key = crate::vault::keys::hkdf(pk.expose(), &[b"ghul/v3/protection-before"]);
    crate::vault::keys::open(&key, &nonce, &sealed.sealed, b"ghul/v3/protection-before")
        .and_then(|plain| ciborium::de::from_reader(&plain[..]).ok())
        .unwrap_or_default()
}

fn put_previous_list(suite: &mut UnlockedContainer, pk: &PasswordKey, list: &[PreviousProtection]) -> Result<()> {
    let mut cbor = Zeroizing::new(Vec::new());
    ciborium::ser::into_writer(&list, &mut *cbor).map_err(|e| GhulError::Corrupted { msg: e.to_string() })?;
    let key = crate::vault::keys::hkdf(pk.expose(), &[b"ghul/v3/protection-before"]);
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let sealed = crate::vault::keys::seal(&key, &nonce, &cbor, b"ghul/v3/protection-before");
    put_setting(suite, PREVIOUS_KEY, &SealedPasswordKey { nonce: nonce.to_vec(), sealed })
}

fn previous_of(suite: &UnlockedContainer, pk: &PasswordKey) -> Vec<Previous> {
    previous_list(suite, pk)
        .into_iter()
        .filter_map(|p| {
            let s: [u8; 32] = p.s.as_slice().try_into().ok()?;
            let code = <[u8; 32]>::try_from(p.code.as_slice()).ok().map(RecoveryCode::from_bytes);
            Some(Previous { s: FactorSecret::from_bytes(s), code })
        })
        .collect()
}

/// Which of `c`'s factor secret and recovery code this suite replaced on purpose.
pub(super) struct Revoked {
    pub secret: bool,
    pub code: bool,
}

pub(super) fn revoked_parts(o: &Open, c: &UnlockedContainer) -> Revoked {
    let list: Vec<String> = setting(&o.suite, REVOKED_KEY);
    Revoked {
        secret: list.contains(&revoked_fingerprint(c.factor_secret().expose())),
        code: c.recovery_code().is_some_and(|code| list.contains(&revoked_fingerprint(code.expose()))),
    }
}

fn revoked_fingerprint(secret: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(b"ghul/v1/revoked");
    h.update(secret);
    crate::kdf::hex(&h.finalize())
}

// --- files and names --------------------------------------------------------------------------

pub(super) fn is_vault_file(name: &str) -> bool {
    [".ghlpss", ".ghltotp", ".ghlcal", ".ghldrv", ".ghlmail"].iter().any(|e| name.ends_with(e))
}

/// F23: the file name of a vault says nothing about it: the first 16 hex digits of its random
/// container ID and the extension of its kind. Its name is kept encrypted inside.
pub(super) fn file_name(kind: VaultKind, id: &str) -> String {
    format!("{}.{}", &id[..id.len().min(16)], kind.container().extension())
}

/// The vault's display name, kept encrypted inside the vault; falls back to the file name.
pub(super) fn vault_name(c: &UnlockedContainer, file: &str) -> String {
    match c.payload.settings.get(NAME_KEY) {
        Some(Value::Text(t)) if !t.trim().is_empty() => t.clone(),
        _ => file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_owned(),
    }
}

fn set_vault_name(c: &mut UnlockedContainer, name: &str) {
    c.payload.settings.insert(NAME_KEY.into(), Value::Text(name.trim().to_owned()));
}

pub(super) fn setting<T: serde::de::DeserializeOwned + Default>(suite: &UnlockedContainer, key: &str) -> T {
    suite.payload.settings.get(key).and_then(|v| v.deserialized().ok()).unwrap_or_default()
}

pub(super) fn put_setting<T: Serialize>(suite: &mut UnlockedContainer, key: &str, value: &T) -> Result<()> {
    let v = Value::serialized(value).map_err(|e| GhulError::Corrupted { msg: e.to_string() })?;
    suite.payload.settings.insert(key.into(), v);
    Ok(())
}

fn ignored_files(suite: &UnlockedContainer) -> Vec<String> {
    setting(suite, IGNORED_KEY)
}

fn revisions(suite: &UnlockedContainer) -> BTreeMap<String, u64> {
    setting(suite, REVISIONS_KEY)
}

fn protection_of(l: &LockedContainer) -> ProtectionInfo {
    ProtectionInfo {
        security_keys: l.fido2_requests().len() as u32,
        keyfiles: l.keyfile_slots() as u32,
        recovery_codes: l.header().slots.iter().filter(|x| matches!(x, Slot::Recovery(_))).count() as u32,
    }
}

fn factor_slots(c: &UnlockedContainer) -> Vec<Slot> {
    c.slots().iter().filter(|s| s.provides_factor()).cloned().collect()
}

// --- session state ----------------------------------------------------------------------------

/// In-memory description of a vault file (nothing of this is stored outside the vault).
#[derive(Clone)]
pub(super) struct VaultRef {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) file: String,
}

pub(super) struct OpenVault {
    pub(super) reference: VaultRef,
    /// `None` if the file is damaged.
    pub(super) container: Option<UnlockedContainer>,
    /// The file's revision is lower than the one last seen.
    pub(super) stale: bool,
}

pub(super) struct Open {
    pub(super) password_key: PasswordKey,
    pub(super) suite: UnlockedContainer,
    pub(super) vaults: Vec<OpenVault>,
    /// Vault files with another protection.
    pub(super) found: Vec<FoundVault>,
}

impl Open {
    pub(super) fn vault_mut(&mut self, id: &str) -> Result<(&VaultRef, &mut UnlockedContainer)> {
        let v = self.vaults.iter_mut().find(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
        let c = v
            .container
            .as_mut()
            .ok_or_else(|| GhulError::Storage { msg: format!("{} is damaged", v.reference.file) })?;
        Ok((&v.reference, c))
    }

    pub(super) fn kdf(&self) -> crate::vault::KdfParams {
        self.suite.header().kdf.clone()
    }

    fn keys(&self) -> SuiteKeys {
        SuiteKeys::from_parts(
            self.suite.header().suite_id,
            self.kdf(),
            PasswordKey::from_bytes(*self.password_key.expose()),
            FactorSecret::from_bytes(*self.suite.factor_secret().expose()),
        )
    }

    /// All open containers: the suite first, then the vaults.
    pub(super) fn containers_mut(&mut self) -> Vec<(&mut UnlockedContainer, String)> {
        let mut out: Vec<(&mut UnlockedContainer, String)> = vec![(&mut self.suite, SUITE_FILE.to_owned())];
        for v in &mut self.vaults {
            if let Some(c) = v.container.as_mut() {
                out.push((c, v.reference.file.clone()));
            }
        }
        out
    }

    /// Records the vaults' current revisions as "last seen" (not for stale files).
    pub(super) fn record_revisions(&mut self) -> Result<()> {
        let mut map = revisions(&self.suite);
        for v in &self.vaults {
            if let (Some(c), false) = (&v.container, v.stale) {
                let r = map.entry(v.reference.id.clone()).or_insert(0);
                *r = (*r).max(c.payload.revision);
            }
        }
        put_setting(&mut self.suite, REVISIONS_KEY, &map)
    }

    /// A digest of everything that protects the open files (keys, slots, password parameters):
    /// tells whether a change of the protection that failed had already changed anything here.
    fn protection_digest(&self, with_settings: bool) -> [u8; 32] {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(self.password_key.expose());
        let all = std::iter::once(&self.suite).chain(self.vaults.iter().filter_map(|v| v.container.as_ref()));
        for c in all {
            h.update(c.vmk());
            h.update(c.factor_secret().expose());
            if let Some(code) = c.recovery_code() {
                h.update(code.expose());
            }
            h.update(format!("{:?}{:?}", c.slots(), c.header().kdf).as_bytes());
        }
        // And what the suite keeps about changes of the protection.
        for key in if !with_settings {
            &[][..]
        } else {
            &[REMOVED_SLOTS_KEY, REVOKED_KEY, PREVIOUS_KEY, super::sync::PROTECTION_CHANGED][..]
        } {
            h.update(format!("{:?}", self.suite.payload.settings.get(*key)).as_bytes());
        }
        h.finalize().into()
    }

    /// The current factor secret and recovery code (to see afterwards which ones were replaced).
    pub(super) fn current_secrets(&self) -> (Vec<u8>, Option<Vec<u8>>) {
        (self.suite.factor_secret().expose().to_vec(), self.suite.recovery_code().map(|c| c.expose().to_vec()))
    }

    /// Notes which of `before` (from [`Self::current_secrets`]) are replaced now: on purpose,
    /// never to come back (see [`REVOKED_KEY`]).
    pub(super) fn revoke_replaced(&mut self, before: (Vec<u8>, Option<Vec<u8>>)) -> Result<()> {
        let now = self.current_secrets();
        let mut list: Vec<String> = setting(&self.suite, REVOKED_KEY);
        let mut add = |secret: &[u8]| {
            let h = revoked_fingerprint(secret);
            if !list.contains(&h) {
                list.push(h);
            }
        };
        if before.0 != now.0 {
            add(&before.0);
        }
        if let Some(code) = &before.1 {
            if Some(code) != now.1.as_ref() {
                add(code);
            }
        }
        // In every file: an older suite file must not bring them back into the vaults (S1).
        for (c, _) in self.containers_mut() {
            put_setting(c, REVOKED_KEY, &list)?;
        }
        Ok(())
    }

    /// Keeps the current factor secret and recovery code before they change (see [`PREVIOUS_KEY`]).
    pub(super) fn remember_protection(&mut self) -> Result<()> {
        let mut list = previous_list(&self.suite, &self.password_key);
        let now = PreviousProtection {
            s: self.suite.factor_secret().expose().to_vec(),
            code: self.suite.recovery_code().map(|c| c.expose().to_vec()).unwrap_or_default(),
        };
        list.retain(|p| p != &now);
        list.insert(0, now);
        list.truncate(MAX_PREVIOUS);
        put_setting(&mut self.suite, PREVIOUS_UPLOADED_KEY, &false)?;
        put_previous_list(&mut self.suite, &self.password_key, &list)
    }

    /// The protections from before (factor secret, recovery code, password key), newest first.
    pub(super) fn previous_protection(&self) -> Vec<Previous> {
        previous_of(&self.suite, &self.password_key)
    }

    /// After a password change (`self.password_key` is already the new one): keeps `old` and
    /// the earlier ones, sealed with the new password key (see [`PASSWORD_BEFORE_KEY`]).
    pub(super) fn keep_password_key(&mut self, old: &PasswordKey) -> Result<()> {
        // The protections from before move to the new password key as well.
        let before = previous_list(&self.suite, old);
        put_previous_list(&mut self.suite, &self.password_key, &before)?;
        let mut keys = previous_password_keys(&self.suite, old);
        keys.insert(0, PasswordKey::from_bytes(*old.expose()));
        keys.truncate(MAX_PREVIOUS);
        let key = password_before_key(&self.password_key);
        let list: Vec<SealedPasswordKey> = keys
            .iter()
            .map(|k| {
                let mut nonce = [0u8; 24];
                OsRng.fill_bytes(&mut nonce);
                let sealed = crate::vault::keys::seal(&key, &nonce, k.expose(), b"ghul/v3/password-before");
                SealedPasswordKey { nonce: nonce.to_vec(), sealed }
            })
            .collect();
        put_setting(&mut self.suite, PASSWORD_BEFORE_KEY, &list)
    }

    /// The password keys from before (see [`PASSWORD_BEFORE_KEY`]).
    pub(super) fn previous_password_keys(&self) -> Vec<PasswordKey> {
        previous_password_keys(&self.suite, &self.password_key)
    }

    /// Forgets the password keys from before once every file is in the current protection and
    /// the server has it (or this device does not synchronise): only a change that stopped half
    /// way needs them. Returns whether anything changed.
    pub(super) fn forget_previous_password_keys(&mut self) -> Result<bool> {
        let uploaded = setting::<bool>(&self.suite, PREVIOUS_UPLOADED_KEY) || !super::sync::synchronises(self);
        // A Drive index still in the earlier password needs it too, until it is written again.
        let drive = super::drive::all_locations_current(self);
        if !uploaded || !drive || !self.suite.payload.settings.contains_key(PASSWORD_BEFORE_KEY) {
            return Ok(false);
        }
        self.suite.payload.settings.remove(PASSWORD_BEFORE_KEY);
        Ok(true)
    }

    /// B2: removes keys and key files that some device removed on purpose but that are still
    /// here (this device missed the removal). Renews the keys then. Returns whether anything changed.
    pub(super) fn drop_removed_slots(&mut self, now: i64) -> Result<bool> {
        let removed: Vec<String> = setting(&self.suite, REMOVED_SLOTS_KEY);
        let doomed: Vec<[u8; 16]> = self
            .suite
            .slots()
            .iter()
            .filter(|s| s.provides_factor() && removed.contains(&crate::kdf::hex(s.id())))
            .map(|s| *s.id())
            .collect();
        let mut dropped = false;
        for id in doomed {
            // The last key or key file stays (a second factor is always required).
            if self.suite.slots().iter().filter(|s| s.provides_factor()).count() <= 1 {
                break;
            }
            self.remember_protection()?;
            for (c, _) in self.containers_mut() {
                match c.remove_slot(&id) {
                    Ok(()) | Err(crate::vault::VaultError::SlotNotFound) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            dropped = true;
        }
        if dropped {
            self.renew_keys(true, now)?;
            super::sync::mark_protection_changed(self)?;
        }
        Ok(dropped)
    }

    /// B1: after a change of the protection, older copies of the files (backups, `.bak`, server
    /// versions) must not open what is written from now on. Every file gets a new container key;
    /// with `factor`, the suite also gets a new factor secret, wrapped for every remaining key
    /// and key file through their public keys. Returns whether the factor secret was renewed
    /// (not while a slot of format version 1 remains).
    pub(super) fn renew_keys(&mut self, factor: bool, now: i64) -> Result<bool> {
        let pk = PasswordKey::from_bytes(*self.password_key.expose());
        let mut renewed = false;
        if factor {
            if let Some((s, slots)) = self.suite.renewed_factor(&mut OsRng)? {
                self.suite.replace_factor(s, slots, &pk, &mut OsRng)?;
                renewed = true;
            }
        }
        // The vaults take over the suite's slots, factor secret and code ...
        self.harmonize(now)?;
        // ... and every file gets its new container key.
        for (c, _) in self.containers_mut() {
            c.renew_container_key(&pk, &mut OsRng)?;
        }
        Ok(renewed)
    }

    /// Brings vaults whose slots differ from the suite's (e.g. an old copy) in line with them.
    /// Returns whether anything changed.
    pub(super) fn harmonize(&mut self, now: i64) -> Result<bool> {
        let slots = factor_slots(&self.suite);
        let suite_id = self.suite.header().suite_id;
        let kdf = self.kdf();
        let code = self.suite.recovery_code();
        let labels: Vec<(String, i64)> = slots.iter().map(|s| slot_label(&self.suite, s)).collect();
        let code_label = self
            .suite
            .slots()
            .iter()
            .find(|s| matches!(s, Slot::Recovery(_)))
            .map(|s| slot_label(&self.suite, s).0)
            .unwrap_or_default();
        let s = *self.suite.factor_secret().expose();
        let removed: Vec<String> = setting(&self.suite, REMOVED_SLOTS_KEY);
        let mut changed = false;
        let mut want = slots.clone();
        want.sort_by_key(|x| *x.id());
        for v in &mut self.vaults {
            let Some(c) = v.container.as_mut() else { continue };
            // The same slots (also the same public-key wrapping) and the same factor secret.
            let mut have = factor_slots(c);
            have.sort_by_key(|x| *x.id());
            let same_secret = c.factor_secret().expose() == &s;
            let code_ok = match (&code, c.recovery_code()) {
                (Some(a), Some(b)) => a.expose() == b.expose(),
                (None, _) => true,
                _ => false,
            };
            if have == want && same_secret && c.header().kdf == kdf && c.header().suite_id == suite_id && code_ok {
                continue;
            }
            // S1: the suite file is older than this vault (it still has a code or key the vault
            // knows as replaced, e.g. an old copy put back): never write that into the vault.
            let revoked: Vec<String> = setting(c, REVOKED_KEY);
            if revoked.contains(&revoked_fingerprint(&s))
                || code.as_ref().is_some_and(|k| revoked.contains(&revoked_fingerprint(k.expose())))
            {
                // A key removed on purpose still goes (its removal may be newer than the vault).
                let mut gone: Vec<String> = setting(c, REMOVED_SLOTS_KEY);
                gone.extend(removed.iter().cloned());
                let ids: Vec<[u8; 16]> =
                    c.slots().iter().filter(|x| gone.contains(&crate::kdf::hex(x.id()))).map(|x| *x.id()).collect();
                for id in &ids {
                    match c.remove_slot(id) {
                        Ok(()) => changed = true,
                        // Never the last way in.
                        Err(crate::vault::VaultError::LastSlot) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                continue;
            }
            c.reprotect(
                suite_id,
                FactorSecret::from_bytes(s),
                slots.clone(),
                &PasswordKey::from_bytes(*self.password_key.expose()),
                kdf.clone(),
                &mut OsRng,
            )?;
            for (slot, (label, created)) in slots.iter().zip(&labels) {
                c.payload.slots.insert(
                    crate::kdf::hex(slot.id()),
                    SlotMeta { label: label.clone(), created: *created, extra: Default::default() },
                );
            }
            if let Some(code) = &code {
                c.set_recovery_code(code, &code_label, now, &mut OsRng)?;
            }
            // An old copy also had its old container key (B1, M1): whoever has that copy and
            // what opened it back then (an old code) must not open what is written from now on.
            c.renew_container_key(&PasswordKey::from_bytes(*self.password_key.expose()), &mut OsRng)?;
            changed = true;
        }
        Ok(changed)
    }
}

#[derive(uniffi::Object)]
pub struct GhulSuite {
    pub(super) storage: Arc<dyn VaultStorage>,
    /// Device-local directory for quick unlock (never inside a shared folder).
    pub(super) local_dir: PathBuf,
    open: Mutex<Option<Open>>,
    /// A lock was asked for while a call (e.g. a slow server) still held the open suite: the
    /// suite counts as locked at once and is dropped as soon as that call ends (F92).
    lock_pending: AtomicBool,
    /// Open server connection, Drive index and running uploads (dropped when locking).
    pub(super) net: Mutex<NetCache>,
    /// One synchronisation at a time: it no longer holds `open` while it waits for the server.
    /// Holds what a run merged here but could not upload, reported with the next run (F147).
    pub(super) syncing: Mutex<super::sync::SyncReport>,
    /// The time zone database of the device (see `set_time_zones`).
    pub(super) zones: Mutex<Option<Arc<dyn super::caltext::TimeZones>>>,
}

#[derive(Default)]
pub(super) struct NetCache {
    pub(super) conn: Option<(String, Arc<dyn crate::net::remote::RemoteFs>)>,
    pub(super) drive: Option<super::drive::DriveIndex>,
    pub(super) uploads: BTreeMap<String, super::drive::Upload>,
    pub(super) imap: BTreeMap<String, crate::net::imap::ImapSession>,
    /// The mail account settings that passed "check connection" last (a hash): only those may
    /// be saved.
    pub(super) mail_tested: Option<([u8; 32], Option<String>, Option<String>)>,
}

/// How containers of the folder are opened.
enum Opener<'a> {
    Factors { pk: &'a PasswordKey, s: &'a FactorSecret, suite_id: [u8; 16], kdf: &'a crate::vault::KdfParams },
    Code(&'a RecoveryCode),
}

/// Opens the key: CTAP2 over the given HID connection, or the system's WebAuthn API when the
/// app passed [`crate::key_device::platform_security_key`].
fn with_authenticator<T>(
    transport: &Arc<dyn HidTransport>,
    listener: Option<&Arc<dyn FidoListener>>,
    f: impl FnOnce(&dyn KeyDevice) -> Result<T>,
) -> Result<T> {
    if crate::key_device::is_platform(transport.as_ref()) {
        if let Some(l) = listener {
            l.on_status(crate::FidoStatus::TouchNeeded);
        }
        let device = crate::key_device::platform_device()?;
        return f(device.as_ref());
    }
    let hid = CtapHid::open(transport.as_ref(), listener.map(|l| l.as_ref() as &dyn FidoListener))?;
    let auth = Authenticator::new(hid)?;
    f(&auth)
}

/// Credential ID, PRF input and first hmac-secret output of a newly registered key.
type NewKey = (Vec<u8>, [u8; 32], Zeroizing<[u8; 32]>);

/// Registers a new credential and evaluates hmac-secret once (two touches).
fn register_key(auth: &dyn KeyDevice, pin: &str) -> Result<NewKey> {
    let cred = auth.make_credential(RP_ID, RP_NAME, pin)?;
    if !cred.hmac_secret {
        return Err(GhulError::HmacSecretUnsupported);
    }
    let prf_input = SuiteKeys::new_prf_input(&mut OsRng);
    let salt = crate::ctap2::prf_salt(&prf_input);
    let req = CredRequest { rp_id: RP_ID, credential_id: &cred.id, salt: &salt, prf_input: &prf_input };
    let out = auth.hmac_secret_any(std::slice::from_ref(&req), pin)?;
    if !out.user_verified {
        return Err(GhulError::KeyProtocol { msg: "key did not verify the PIN".into() });
    }
    Ok((cred.id, prf_input, out.output))
}

/// Asks the key for the factor secret of one of the container's FIDO2 slots.
fn key_factor(
    locked: &LockedContainer,
    transport: &Arc<dyn HidTransport>,
    listener: Option<&Arc<dyn FidoListener>>,
    pin: &str,
) -> Result<FactorSecret> {
    let requests = locked.fido2_requests();
    if requests.is_empty() {
        return Err(GhulError::InvalidInput { msg: "this vault has no security key".into() });
    }
    let list: Vec<CredRequest<'_>> = requests
        .iter()
        .map(|r| CredRequest {
            rp_id: &r.rp_id,
            credential_id: &r.credential_id,
            salt: &r.hmac_salt,
            prf_input: &r.prf_input,
        })
        .collect();
    with_authenticator(transport, listener, |auth| {
        loop {
            let out = match auth.hmac_secret_any(&list, pin) {
                Ok(out) => out,
                Err(crate::FidoError::NoCredentials) => return Err(GhulError::KeyNotRegistered),
                Err(e) => return Err(e.into()),
            };
            if !out.user_verified {
                return Err(GhulError::KeyProtocol { msg: "key did not verify the PIN".into() });
            }
            let req = &requests[out.index];
            match locked.factor_secret(&SecondFactor::Fido2 { slot_id: req.slot_id, hmac_output: &out.output }) {
                Ok(s) => return Ok(s),
                // The system API may have treated the salt differently: ask once more the other way.
                Err(_) if auth.switch_salt_mode() => continue,
                Err(e) => return Err(e.into()),
            }
        }
    })
}

pub(super) fn parse_keyfile(data: Option<Vec<u8>>) -> Result<Option<Keyfile>> {
    data.map(|d| Keyfile::parse(&Zeroizing::new(d)).map_err(GhulError::from)).transpose()
}

/// Gets the factor secret with a key file or a security key, whichever is given.
pub(super) fn second_factor(
    locked: &LockedContainer,
    transport: Option<&Arc<dyn HidTransport>>,
    listener: Option<&Arc<dyn FidoListener>>,
    pin: Option<&str>,
    keyfile: Option<&Keyfile>,
) -> Result<FactorSecret> {
    if let Some(kf) = keyfile {
        return Ok(locked.factor_secret(&SecondFactor::Keyfile(kf))?);
    }
    match (transport, pin) {
        (Some(t), Some(pin)) if !locked.fido2_requests().is_empty() => key_factor(locked, t, listener, pin),
        _ if locked.fido2_requests().is_empty() => Err(GhulError::KeyfileRequired),
        _ => Err(GhulError::SecurityKeyRequired),
    }
}

pub(super) fn text_field(e: &Entry, name: &str) -> String {
    match e.fields.get(name) {
        Some(Field::Text(s)) | Some(Field::Url(s)) => s.clone(),
        Some(Field::List(l)) => l.first().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

fn to_field_value(f: &Field) -> Option<FieldValue> {
    Some(match f {
        Field::Text(v) => FieldValue::Text { value: v.clone() },
        Field::Secret(v) => FieldValue::Secret { value: v.clone() },
        Field::Url(v) => FieldValue::Url { value: v.clone() },
        Field::Int(v) => FieldValue::Number { value: *v },
        Field::Bool(v) => FieldValue::Flag { value: *v },
        Field::List(v) => FieldValue::Items { values: v.clone() },
        Field::Bytes(_) => return None,
    })
}

fn from_field_value(v: FieldValue) -> Field {
    match v {
        FieldValue::Text { value } => Field::Text(value),
        FieldValue::Secret { value } => Field::Secret(value),
        FieldValue::Url { value } => Field::Url(value),
        FieldValue::Number { value } => Field::Int(value),
        FieldValue::Flag { value } => Field::Bool(value),
        FieldValue::Items { values } => Field::List(values),
    }
}

pub(super) fn summary(r: &VaultRef, e: &Entry) -> EntrySummary {
    let subtitle = [text_field(e, "username"), text_field(e, "urls"), text_field(e, "ssid"), text_field(e, "holder")]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    EntrySummary {
        vault_id: r.id.clone(),
        vault_name: r.name.clone(),
        id: e.id.clone(),
        entry_type: e.entry_type.clone(),
        title: text_field(e, "title"),
        subtitle,
        modified: e.modified,
        favorite: super::passx::is_favorite(e),
        has_totp: matches!(e.fields.get("totp"), Some(Field::Secret(s)) | Some(Field::Text(s)) if !s.is_empty()),
    }
}

fn apply(e: &mut Entry, data: EntryData) {
    e.fields = data
        .fields
        .into_iter()
        .filter(|f| !f.name.is_empty())
        .map(|f| (f.name, from_field_value(f.value)))
        .filter(|(_, v)| !matches!(v, Field::Text(s) | Field::Secret(s) | Field::Url(s) if s.is_empty()))
        .collect();
    e.tags = data.tags;
    e.folder = data.folder.filter(|f| !f.is_empty());
}

fn slot_label(c: &UnlockedContainer, s: &Slot) -> (String, i64) {
    c.payload.slots.get(&crate::kdf::hex(s.id())).map(|m| (m.label.clone(), m.created)).unwrap_or_default()
}

impl GhulSuite {
    fn guard(&self) -> MutexGuard<'_, Option<Open>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Unlocked: a lock asked for before is done with.
    fn set_open(&self, open: Open) {
        let mut guard = self.guard();
        self.lock_pending.store(false, Ordering::SeqCst);
        *guard = Some(open);
    }

    /// Drops the open suite (if a lock is still pending) and the network cache. With `wait`
    /// false it gives up at once if a call holds either (returns false; the call drops the
    /// suite when it ends, `finish_lock` the rest).
    fn drop_open(&self, wait: bool) -> bool {
        let mut guard = match self.open.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) if wait => self.guard(),
            Err(TryLockError::WouldBlock) => return false,
        };
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
        if guard.is_some() {
            // Unlocked again since: its connection and running uploads stay.
            return true;
        }
        // The connection, the Drive index and mail sessions go as well (same lock order as
        // the calls: open suite first, then the network cache).
        let mut net = match self.net.try_lock() {
            Ok(n) => n,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) if wait => self.net.lock().unwrap_or_else(|p| p.into_inner()),
            // The suite is dropped already; only the cache waits for the call using it.
            Err(TryLockError::WouldBlock) => return false,
        };
        *net = NetCache::default();
        true
    }

    pub(super) fn with_open<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let mut guard = self.guard();
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
        let result = self.run_open(&mut guard, f);
        // A lock asked for while this call ran drops the suite now, not at the next call.
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
        result
    }

    /// Runs a change of the protection. If it fails after it changed anything here, the open
    /// suite is dropped (locked): what is in memory would no longer match the files, and a later
    /// save would write a half-changed protection. Unlocking again reads what was written.
    pub(super) fn changing_protection<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        self.guarded(true, f)
    }

    /// Like [`Self::changing_protection`], but only keys, slots and password parameters count
    /// (the synchronisation merges settings about the protection in every run; an error after
    /// that alone does not lock the suite, the next run merges them again).
    pub(super) fn changing_keys<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        self.guarded(false, f)
    }

    fn guarded<T>(&self, with_settings: bool, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        self.with_open(|o| {
            let before = o.protection_digest(with_settings);
            let result = f(o);
            if result.is_err() && o.protection_digest(with_settings) != before {
                self.lock_pending.store(true, Ordering::SeqCst);
            }
            result
        })
    }

    fn run_open<T>(&self, guard: &mut Option<Open>, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let open = guard.as_mut().ok_or(GhulError::Locked)?;
        // Settings of older suites (or synced from a device with an older app) move into the area files.
        if super::areas::DRIVE_KEYS
            .iter()
            .chain(super::areas::MAIL_KEYS)
            .any(|k| open.suite.payload.settings.contains_key(*k))
        {
            self.migrate_areas(open)?;
        }
        f(open)
    }

    fn read_locked(&self, name: &str) -> Result<Option<LockedContainer>> {
        match self.storage.read(name.to_owned())? {
            None => Ok(None),
            Some(d) => Ok(Some(LockedContainer::parse(&d)?)),
        }
    }

    fn vault_files(&self) -> Result<Vec<String>> {
        let mut files: Vec<String> = self.storage.list()?.into_iter().filter(|f| is_vault_file(f)).collect();
        files.sort();
        Ok(files)
    }

    /// The suite file, or, if it is missing, any readable vault file of the folder. All
    /// containers carry the same slots, so each of them can be used to unlock.
    fn anchor(&self) -> Result<LockedContainer> {
        if let Some(l) = self.read_locked(SUITE_FILE)? {
            return Ok(l);
        }
        for f in self.vault_files()? {
            if let Ok(Some(l)) = self.read_locked(&f) {
                return Ok(l);
            }
        }
        Err(GhulError::NoVault)
    }

    /// Turns the unlocked anchor into the suite container (a vault anchor gets a new suite file).
    fn suite_from_anchor(&self, anchor: UnlockedContainer, pk: &PasswordKey) -> Result<UnlockedContainer> {
        if anchor.kind() == ContainerKind::Suite {
            return Ok(anchor);
        }
        self.suite_for_vault(&anchor, pk)
    }

    /// A new suite (settings) container with the same protection and recovery code as `anchor`.
    fn suite_for_vault(&self, anchor: &UnlockedContainer, pk: &PasswordKey) -> Result<UnlockedContainer> {
        let keys = SuiteKeys::from_parts(
            anchor.header().suite_id,
            anchor.header().kdf.clone(),
            PasswordKey::from_bytes(*pk.expose()),
            FactorSecret::from_bytes(*anchor.factor_secret().expose()),
        );
        let slots = factor_slots(anchor);
        let mut suite = keys.create_container(ContainerKind::Suite, slots.clone(), &mut OsRng)?;
        for s in &slots {
            let (label, created) = slot_label(anchor, s);
            suite.payload.slots.insert(crate::kdf::hex(s.id()), SlotMeta { label, created, extra: Default::default() });
        }
        if let Some(code) = anchor.recovery_code() {
            let label = anchor
                .slots()
                .iter()
                .find(|s| matches!(s, Slot::Recovery(_)))
                .map(|s| slot_label(anchor, s).0)
                .unwrap_or_default();
            suite.set_recovery_code(&code, &label, now_ms(), &mut OsRng)?;
        }
        super::setup::mark_unconfirmed(&mut suite)?;
        Ok(suite)
    }

    /// Opens every vault file of the folder. Files with another protection end up in `found`,
    /// damaged files are listed as unavailable. Copies of the same vault (e.g. conflict copies of
    /// a folder sync tool such as Syncthing or Nextcloud) are merged entry by entry into the
    /// newest one; their files are returned to be deleted once the merged vault is saved.
    fn scan(
        &self,
        suite: &UnlockedContainer,
        opener: &Opener<'_>,
    ) -> Result<(Vec<OpenVault>, Vec<FoundVault>, Vec<String>)> {
        let mut copies: Vec<String> = Vec::new();
        let seen = revisions(suite);
        let ignored = ignored_files(suite);
        let mut vaults: Vec<OpenVault> = Vec::new();
        let mut found = Vec::new();
        for file in self.vault_files()? {
            let kind_by_ext = if file.ends_with(".ghlpss") {
                VaultKind::Pass
            } else if file.ends_with(".ghlcal") {
                VaultKind::Cal
            } else if file.ends_with(".ghldrv") {
                VaultKind::Drive
            } else if file.ends_with(".ghlmail") {
                VaultKind::Mail
            } else {
                VaultKind::Totp
            };
            let damaged = |file: &str| OpenVault {
                reference: VaultRef {
                    id: format!("file:{file}"),
                    kind: kind_by_ext.container().name().into(),
                    name: file.rsplit_once('.').map_or(file, |(s, _)| s).to_owned(),
                    file: file.to_owned(),
                },
                container: None,
                stale: false,
            };
            let locked = match self.read_locked(&file) {
                Ok(Some(l)) => l,
                Ok(None) => continue,
                Err(_) => {
                    vaults.push(damaged(&file));
                    continue;
                }
            };
            let Some(kind) = VaultKind::from_container(locked.kind()) else { continue };
            let ours = match opener {
                Opener::Factors { suite_id, kdf, .. } => {
                    &locked.header().suite_id == suite_id && &locked.header().kdf == *kdf
                }
                Opener::Code(_) => locked.has_recovery(),
            };
            let protection = protection_of(&locked);
            let header_suite = locked.header().suite_id;
            let opened = match opener {
                Opener::Factors { pk, s, .. } if ours => locked.unlock(pk, s).ok(),
                Opener::Code(code) => locked.recover(code).ok(),
                _ => None,
            };
            // A vault of this suite still in the protection from before (a change of the
            // protection stopped after the suite file, M18/H6): opens with what the suite file
            // kept of it, and is brought in line after unlocking (`harmonize`).
            let (opened, ours) = match (opened, opener) {
                (None, Opener::Factors { pk, suite_id, kdf, .. }) if &header_suite == suite_id => {
                    let data = self.storage.read(file.clone()).ok().flatten();
                    match data.and_then(|d| open_with_before(&d, suite, pk)) {
                        Some(c) => (Some(c), true),
                        None => (None, ours),
                    }
                }
                (o, _) => (o, ours),
            };
            let c = match (opened, ours) {
                (Some(c), _) => c,
                (None, true) if matches!(opener, Opener::Factors { .. }) => {
                    vaults.push(damaged(&file));
                    continue;
                }
                _ => {
                    if !ignored.contains(&file) {
                        found.push(FoundVault { file, kind, protection });
                    }
                    continue;
                }
            };
            let id = crate::kdf::hex(&c.container_id());
            if let Some(existing) = vaults.iter_mut().find(|v| v.reference.id == id) {
                let Some(e) = existing.container.as_mut() else { continue };
                let mut c = c;
                // Both copies may hold changes of their own: merge, the newer copy carries on.
                let (winner, loser_file) = if c.payload.revision > e.payload.revision {
                    let (merged, _, _) = super::sync::merge_entries(&c.payload.entries, &e.payload.entries);
                    c.payload.entries = merged;
                    super::sync::merge_settings(&mut c.payload.settings, &e.payload.settings);
                    let old = std::mem::replace(&mut existing.reference.file, file.clone());
                    existing.reference.name = vault_name(&c, &file);
                    existing.stale = seen.get(&id).is_some_and(|r| c.payload.revision < *r);
                    (Some(c), old)
                } else {
                    let (merged, _, _) = super::sync::merge_entries(&e.payload.entries, &c.payload.entries);
                    e.payload.entries = merged;
                    super::sync::merge_settings(&mut e.payload.settings, &c.payload.settings);
                    (None, file.clone())
                };
                if let Some(w) = winner {
                    existing.container = Some(w);
                }
                copies.push(loser_file);
                continue;
            }
            let stale = seen.get(&id).is_some_and(|r| c.payload.revision < *r);
            vaults.push(OpenVault {
                reference: VaultRef { id, kind: kind.container().name().into(), name: vault_name(&c, &file), file },
                container: Some(c),
                stale,
            });
        }
        copies.retain(|f| !vaults.iter().any(|v| &v.reference.file == f));
        Ok((vaults, found, copies))
    }

    /// Saves vaults that were merged from copies and deletes the copies.
    fn remove_copies(&self, o: &mut Open, copies: Vec<String>) -> Result<()> {
        if copies.is_empty() {
            return Ok(());
        }
        self.save_all(o)?;
        for f in copies {
            self.storage.delete(f)?;
        }
        Ok(())
    }

    pub(super) fn save_container(&self, c: &mut UnlockedContainer, file: &str) -> Result<()> {
        let data = c.seal(&mut OsRng)?;
        Ok(self.storage.write(file.to_owned(), data)?)
    }

    /// Saves one vault and the suite (with its new revision as "last seen").
    pub(super) fn save_vault(&self, o: &mut Open, id: &str) -> Result<()> {
        let (r, c) = o.vault_mut(id)?;
        let file = r.file.clone();
        self.save_container(c, &file)?;
        o.record_revisions()?;
        self.save_container(&mut o.suite, SUITE_FILE)
    }

    /// Saves a change of the protection as one step (M18, H6): the suite file first, the vaults
    /// after it. Until the suite file is written nothing has changed on disk; if that fails, the
    /// suite is locked, so the changed state in memory is never saved later by an ordinary save
    /// (the old password and keys still open everything). After it, a vault that could not be
    /// written still has the old protection, which the suite file keeps (`protection_before`, the
    /// code): unlocking opens it with that and brings it in line.
    pub(super) fn save_protection(&self, o: &mut Open) -> Result<()> {
        let mut sealed = Vec::new();
        let result = (|| -> Result<()> {
            for v in &mut o.vaults {
                if let Some(c) = v.container.as_mut() {
                    sealed.push((v.reference.file.clone(), c.seal(&mut OsRng)?));
                }
            }
            o.record_revisions()?;
            self.save_container(&mut o.suite, SUITE_FILE)
        })();
        if let Err(e) = result {
            self.lock_pending.store(true, Ordering::SeqCst);
            return Err(e);
        }
        let mut all = true;
        for (file, data) in sealed {
            // Not undone: the next save writes it again, unlocking opens it meanwhile.
            all &= self.storage.write(file, data).is_ok();
        }
        // Every file is in the new protection: the old password key is no longer needed.
        if all && o.forget_previous_password_keys()? {
            self.save_container(&mut o.suite, SUITE_FILE)?;
        }
        Ok(())
    }

    /// Saves all vaults first and the suite last.
    pub(super) fn save_all(&self, o: &mut Open) -> Result<()> {
        for v in &mut o.vaults {
            if let Some(c) = v.container.as_mut() {
                let data = c.seal(&mut OsRng)?;
                self.storage.write(v.reference.file.clone(), data)?;
            }
        }
        o.forget_previous_password_keys()?;
        o.record_revisions()?;
        self.save_container(&mut o.suite, SUITE_FILE)
    }

    /// F23: renames vault files whose names come from older app versions (they contained the
    /// vault's name) to [`file_name`]. The bytes are copied as they are; the old file is deleted
    /// once the new one is written.
    fn neutral_file_names(&self, o: &mut Open) -> Result<()> {
        for v in &mut o.vaults {
            if v.container.is_none() {
                continue;
            }
            let want = file_name(VaultKind::from_name(&v.reference.kind), &v.reference.id);
            if v.reference.file == want {
                continue;
            }
            let Some(data) = self.storage.read(v.reference.file.clone())? else { continue };
            self.storage.write(want.clone(), data)?;
            self.storage.delete(v.reference.file.clone())?;
            v.reference.file = want;
        }
        Ok(())
    }

    /// Opens the folder's vaults with the suite's password and factor secret and keeps the session.
    fn finish_unlock(&self, password_key: PasswordKey, suite: UnlockedContainer, full: bool) -> Result<()> {
        let s = FactorSecret::from_bytes(*suite.factor_secret().expose());
        let kdf = suite.header().kdf.clone();
        let opener = Opener::Factors { pk: &password_key, s: &s, suite_id: suite.header().suite_id, kdf: &kdf };
        let (vaults, found, copies) = self.scan(&suite, &opener)?;
        let mut open = Open { password_key, suite, vaults, found };
        self.remove_copies(&mut open, copies)?;
        let suite_missing = self.storage.read(SUITE_FILE.to_owned())?.is_none();
        if open.harmonize(now_ms())? {
            self.save_all(&mut open)?;
        } else if suite_missing {
            open.record_revisions()?;
            self.save_container(&mut open.suite, SUITE_FILE)?;
        }
        if full {
            super::quick::refresh_stamps(&self.local_dir, open.password_key.expose(), now_ms());
        }
        // Files named after their vaults by older app versions get neutral names. Not being
        // able to rename now is no reason to refuse the unlock: it is tried again next time.
        let _ = self.neutral_file_names(&mut open);
        self.set_open(open);
        Ok(())
    }

    /// Creates a vault with the suite's protection and recovery code.
    pub(super) fn add_vault(&self, o: &mut Open, kind: VaultKind, name: &str) -> Result<String> {
        let slots = factor_slots(&o.suite);
        let mut c = o.keys().create_container(kind.container(), slots.clone(), &mut OsRng)?;
        for s in &slots {
            let (label, created) = slot_label(&o.suite, s);
            c.payload.slots.insert(crate::kdf::hex(s.id()), SlotMeta { label, created, extra: Default::default() });
        }
        if let Some(code) = o.suite.recovery_code() {
            let label = o
                .suite
                .slots()
                .iter()
                .find(|s| matches!(s, Slot::Recovery(_)))
                .map(|s| slot_label(&o.suite, s).0)
                .unwrap_or_default();
            c.set_recovery_code(&code, &label, now_ms(), &mut OsRng)?;
        }
        set_vault_name(&mut c, name);
        let id = crate::kdf::hex(&c.container_id());
        let file = file_name(kind, &id);
        let reference =
            VaultRef { id: id.clone(), kind: kind.container().name().into(), name: name.trim().to_owned(), file };
        o.vaults.push(OpenVault { reference, container: Some(c), stale: false });
        Ok(id)
    }

    /// Adds an opened vault (already with the suite's protection) and writes it to the folder.
    fn insert_vault(&self, o: &mut Open, mut c: UnlockedContainer, name: &str, file: Option<String>) -> Result<String> {
        let kind = VaultKind::from_container(c.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() })?;
        let id = crate::kdf::hex(&c.container_id());
        if o.vaults.iter().any(|v| v.reference.id == id) {
            return Err(GhulError::AlreadyExists);
        }
        if !name.trim().is_empty() {
            set_vault_name(&mut c, name);
        }
        let neutral = file_name(kind, &id);
        let name = vault_name(&c, file.as_deref().unwrap_or(&neutral));
        if let Some(f) = &file {
            o.found.retain(|x| &x.file != f);
        }
        let reference = VaultRef { id: id.clone(), kind: kind.container().name().into(), name, file: neutral.clone() };
        o.vaults.push(OpenVault { reference, container: Some(c), stale: false });
        self.save_vault(o, &id)?;
        // A file of the folder that was taken over keeps no name of its own (F23).
        if let Some(f) = file.filter(|f| f != &neutral) {
            self.storage.delete(f)?;
        }
        Ok(id)
    }

    /// Copies a new factor slot (created in the suite) into every vault and saves all.
    fn insert_everywhere(&self, o: &mut Open, slot: Slot, label: &str, now: i64) -> Result<()> {
        for v in &mut o.vaults {
            if let Some(c) = v.container.as_mut() {
                c.insert_slot(slot.clone(), label, now)?;
            }
        }
        super::sync::mark_protection_changed(o)?;
        self.save_protection(o)
    }

    fn source_bytes(&self, source: &VaultSource) -> Result<(Zeroizing<Vec<u8>>, Option<String>)> {
        match source {
            VaultSource::Bytes { data } => Ok((Zeroizing::new(data.clone()), None)),
            VaultSource::InFolder { file } => {
                let data = self.storage.read(file.clone())?.ok_or(GhulError::NotFound)?;
                Ok((Zeroizing::new(data), Some(file.clone())))
            }
        }
    }
}

#[uniffi::export]
impl GhulSuite {
    /// `storage`: the suite's folder. `local_dir`: private app directory for device-bound data
    /// (quick unlock).
    #[uniffi::constructor]
    pub fn new(storage: Arc<dyn VaultStorage>, local_dir: String) -> Arc<Self> {
        Arc::new(GhulSuite {
            storage,
            local_dir: PathBuf::from(local_dir),
            open: Mutex::new(None),
            lock_pending: AtomicBool::new(false),
            net: Mutex::new(NetCache::default()),
            syncing: Mutex::new(Default::default()),
            zones: Mutex::new(None),
        })
    }

    /// Whether the folder contains a suite (settings file or any vault file).
    pub fn exists(&self) -> Result<bool> {
        Ok(self.storage.read(SUITE_FILE.to_owned())?.is_some() || !self.vault_files()?.is_empty())
    }

    pub fn storage_name(&self) -> String {
        self.storage.describe()
    }

    /// "Change storage location": copies the settings file and every vault file to `target` and
    /// reads each copy back to compare it byte for byte. Refuses a target that already holds a
    /// suite (nothing is overwritten). The files here stay untouched; the caller switches to the
    /// new location only after this succeeded. If anything fails, the copies made so far are
    /// removed again. Returns the number of files copied.
    pub fn copy_to(&self, target: Arc<dyn VaultStorage>) -> Result<u32> {
        // No save may run while the files are copied.
        let _hold = self.guard();
        if target.read(SUITE_FILE.to_owned())?.is_some() || target.list()?.iter().any(|f| is_vault_file(f)) {
            return Err(GhulError::AlreadyExists);
        }
        let mut names = self.vault_files()?;
        if self.storage.read(SUITE_FILE.to_owned())?.is_some() {
            names.insert(0, SUITE_FILE.to_owned());
        }
        if names.is_empty() {
            return Err(GhulError::NoVault);
        }
        let mut done: Vec<String> = Vec::new();
        let result = (|| -> Result<()> {
            for name in &names {
                let data = self.storage.read(name.clone())?.ok_or(GhulError::NoVault)?;
                target.write(name.clone(), data.clone())?;
                done.push(name.clone());
                if target.read(name.clone())?.as_deref() != Some(&data[..]) {
                    return Err(GhulError::Storage { msg: format!("copy of {name} differs") });
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            for name in &done {
                let _ = target.delete(name.clone());
            }
            return Err(e);
        }
        Ok(names.len() as u32)
    }

    pub fn is_unlocked(&self) -> bool {
        if self.lock_pending.load(Ordering::SeqCst) {
            return false;
        }
        match self.open.try_lock() {
            Ok(g) => g.is_some(),
            Err(TryLockError::Poisoned(p)) => p.into_inner().is_some(),
            // A call works with the open suite right now (F92: never wait for it here).
            Err(TryLockError::WouldBlock) => true,
        }
    }

    /// Which factors the unlock screen has to ask for.
    pub fn protection(&self) -> Result<ProtectionInfo> {
        Ok(protection_of(&self.anchor()?))
    }

    /// Creates a new suite protected by the password plus a security key (`transport`/`pin`, two
    /// touches) and/or a key file (`options.keyfile`). At least one second factor is required.
    /// Every file gets the same recovery code.
    pub fn create(
        &self,
        options: SetupOptions,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
    ) -> Result<SetupResult> {
        let password = Zeroizing::new(options.password.clone());
        let pin = pin.map(Zeroizing::new);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        let keyfile = parse_keyfile(options.keyfile.clone())?;
        let cost = KdfCost { memory_kib: options.memory_kib, iterations: options.iterations, parallelism: 1 };
        let keys = SuiteKeys::create(&password, cost, &mut OsRng)?;
        let mut factor: Vec<(Slot, &str)> = Vec::new();
        match (&transport, &pin) {
            (Some(t), Some(pin)) => {
                let (cred_id, prf_input, output) = with_authenticator(t, listener.as_ref(), |a| register_key(a, pin))?;
                let slot = keys.fido2_slot(
                    &Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output },
                    &mut OsRng,
                );
                factor.push((slot, &options.key_label));
            }
            (None, None) => {}
            _ => return Err(GhulError::InvalidInput { msg: "key and PIN must be given together".into() }),
        }
        if let Some(kf) = &keyfile {
            factor.push((keys.keyfile_slot(kf, &mut OsRng), &options.keyfile_label));
        }
        if factor.is_empty() {
            return Err(GhulError::InvalidInput { msg: "a security key or key file is required".into() });
        }
        let now = now_ms();
        let slots: Vec<Slot> = factor.iter().map(|(s, _)| s.clone()).collect();
        let mut suite = keys.create_container(ContainerKind::Suite, slots, &mut OsRng)?;
        for (slot, label) in &factor {
            suite.payload.slots.insert(
                crate::kdf::hex(slot.id()),
                SlotMeta { label: label.to_string(), created: now, extra: Default::default() },
            );
        }
        let (_, code) = suite.add_recovery_slot(&options.recovery_label, now, &mut OsRng)?;
        super::setup::mark_unconfirmed(&mut suite)?;
        let mut open = Open {
            password_key: PasswordKey::from_bytes(*keys.password_key.expose()),
            suite,
            vaults: Vec::new(),
            found: Vec::new(),
        };
        for v in &options.vaults {
            self.add_vault(&mut open, v.kind, &v.name)?;
        }
        self.save_all(&mut open)?;
        self.set_open(open);
        Ok(SetupResult { recovery_code: Some(code.to_display().to_string()) })
    }

    /// Starts a suite in this (empty) folder from a single vault file, opened with its own
    /// password plus key or key file. The vault keeps its protection; if it carries no recovery
    /// code yet, a new one is created and returned.
    #[allow(clippy::too_many_arguments)]
    pub fn create_from_vault(
        &self,
        vault_file: Vec<u8>,
        vault_name: String,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
        recovery_label: String,
    ) -> Result<SetupResult> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        let locked = LockedContainer::parse(&vault_file)?;
        let kind = VaultKind::from_container(locked.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "this is a suite file, open its folder instead".into() })?;
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let mut vault = locked.unlock(&pk, &s)?;
        let mut new_code = None;
        if vault.recovery_code().is_none() {
            let (_, code) = vault.add_recovery_slot(&recovery_label, now_ms(), &mut OsRng)?;
            new_code = Some(code.to_display().to_string());
        }
        if !vault_name.trim().is_empty() {
            set_vault_name(&mut vault, &vault_name);
        }
        let id = crate::kdf::hex(&vault.container_id());
        let file = file_name(kind, &id);
        let suite = self.suite_for_vault(&vault, &pk)?;
        let name = self::vault_name(&vault, &file);
        let reference = VaultRef { id, kind: kind.container().name().into(), name, file };
        let mut open = Open {
            password_key: pk,
            suite,
            vaults: vec![OpenVault { reference, container: Some(vault), stale: false }],
            found: Vec::new(),
        };
        self.save_all(&mut open)?;
        self.set_open(open);
        Ok(SetupResult { recovery_code: new_code })
    }

    /// Unlocks with the master password plus a key file (if given) or a security key
    /// (`transport` + `pin`).
    pub fn unlock(
        &self,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
    ) -> Result<()> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let locked = self.anchor()?;
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let anchor = locked.unlock(&pk, &s)?;
        let suite = self.suite_from_anchor(anchor, &pk)?;
        self.finish_unlock(pk, suite, true)
    }

    /// Emergency: opens everything with the recovery code alone and sets a new master password.
    /// Keys, key files and the recovery code stay valid.
    pub fn unlock_with_recovery_code(&self, code: String, new_password: String) -> Result<()> {
        let code = Zeroizing::new(code);
        let new_password = Zeroizing::new(new_password);
        let code = RecoveryCode::parse(&code)?;
        let anchor = self.anchor()?.recover(&code)?;
        let mut kdf = anchor.header().kdf.clone();
        OsRng.fill_bytes(&mut kdf.salt);
        let pk = PasswordKey::derive(&new_password, &kdf)?;
        let suite = self.suite_from_anchor(anchor, &pk)?;
        let (vaults, found, copies) = self.scan(&suite, &Opener::Code(&code))?;
        let mut open = Open { password_key: PasswordKey::from_bytes(*pk.expose()), suite, vaults, found };
        open.suite.rekey(&pk, kdf.clone(), &mut OsRng)?;
        open.harmonize(now_ms())?;
        for (c, _) in open.containers_mut() {
            c.rekey(&pk, kdf.clone(), &mut OsRng)?;
        }
        open.renew_keys(false, now_ms())?;
        super::drive::renew_drive_key(&mut open)?;
        super::sync::mark_protection_changed(&mut open)?;
        self.save_all(&mut open)?;
        self.remove_copies(&mut open, copies)?;
        super::quick::delete_all(&self.local_dir);
        super::quick::refresh_stamps(&self.local_dir, open.password_key.expose(), now_ms());
        self.set_open(open);
        Ok(())
    }

    /// Locks immediately; all secrets are wiped from memory.
    /// Locks at once, also while another call (e.g. a slow or unreachable server) still works
    /// with the open suite: the caller (the app's main thread) never waits for it (F92). The
    /// suite counts as locked immediately; its keys and decrypted data are dropped right away
    /// or, if that call still holds them, as soon as it ends. Callers then run `finish_lock`
    /// on a background thread: it covers a call ending in the same moment and the network cache.
    pub fn lock(&self) {
        self.lock_pending.store(true, Ordering::SeqCst);
        // A checked mail connection (with its password's hash) is not kept past locking.
        self.net.lock().unwrap_or_else(|p| p.into_inner()).mail_tested = None;
        self.drop_open(false);
    }

    /// After `lock`, on a background thread: waits until a call still holding the open suite or
    /// the network cache has ended, and makes sure both are gone (unless unlocked again since).
    pub fn finish_lock(&self) {
        self.drop_open(true);
    }

    // --- quick unlock ------------------------------------------------------------------------

    /// Enables quick unlock under `name` (`bio` or `pin`); returns the quick key for the platform to protect.
    pub fn quick_unlock_enable(&self, name: String, pin: Option<String>) -> Result<Vec<u8>> {
        let pin = pin.map(Zeroizing::new);
        self.with_open(|o| {
            let qk = super::quick::create(
                &self.local_dir,
                &name,
                &o.suite.header().suite_id,
                o.password_key.expose(),
                o.suite.factor_secret().expose(),
                pin.as_deref().map(|p| p.as_str()),
                now_ms(),
                None,
            )?;
            Ok(qk.to_vec())
        })
    }

    pub fn quick_unlock_exists(&self, name: String) -> bool {
        super::quick::path(&self.local_dir, &name).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn unlock_quick(
        &self,
        name: String,
        quick_key: Vec<u8>,
        pin: Option<String>,
        max_age_seconds: u64,
    ) -> Result<()> {
        let quick_key = Zeroizing::new(quick_key);
        let pin = pin.map(Zeroizing::new);
        let locked = self.read_locked(SUITE_FILE)?.ok_or(GhulError::QuickUnlockUnavailable)?;
        let secret = super::quick::open_file(
            &self.local_dir,
            &name,
            &locked.header().suite_id,
            &quick_key,
            pin.as_deref().map(|p| p.as_str()),
            (max_age_seconds.min(i64::MAX as u64 / 1000) * 1000) as i64,
            now_ms(),
        )?;
        let pk = PasswordKey::from_bytes(*secret.password_key);
        // A quick unlock file of the first version (container key only) is no longer used.
        let opened = if secret.old_version {
            Err(crate::vault::VaultError::WrongPassword)
        } else {
            locked.unlock(&pk, &FactorSecret::from_bytes(*secret.secret))
        };
        let suite = match opened {
            Ok(s) => s,
            Err(_) => {
                super::quick::delete_all(&self.local_dir);
                return Err(GhulError::QuickUnlockUnavailable);
            }
        };
        self.finish_unlock(pk, suite, false)
    }

    pub fn quick_unlock_disable(&self, name: String) -> Result<()> {
        let p = super::quick::path(&self.local_dir, &name)?;
        if p.exists() {
            super::quick::wipe(&p)?;
        }
        super::quick::wipe_leftovers(&self.local_dir, &name);
        let _ = std::fs::remove_file(self.local_dir.join(format!("quick-{name}.stamp")));
        Ok(())
    }

    pub fn quick_unlock_disable_all(&self) {
        super::quick::delete_all(&self.local_dir);
    }

    // --- vaults ------------------------------------------------------------------------------

    pub fn list_vaults(&self) -> Result<Vec<VaultInfo>> {
        self.with_open(|o| {
            Ok(o.vaults
                .iter()
                .map(|v| VaultInfo {
                    id: v.reference.id.clone(),
                    kind: VaultKind::from_name(&v.reference.kind),
                    name: v.reference.name.clone(),
                    file: v.reference.file.clone(),
                    available: v.container.is_some(),
                    entries: v
                        .container
                        .as_ref()
                        .map(|c| c.payload.entries.iter().filter(|e| !e.deleted).count() as u32)
                        .unwrap_or(0),
                    outdated: v.stale,
                })
                .collect())
        })
    }

    pub fn create_vault(&self, kind: VaultKind, name: String) -> Result<String> {
        if name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        self.with_open(|o| {
            let id = self.add_vault(o, kind, &name)?;
            self.save_vault(o, &id)?;
            Ok(id)
        })
    }

    pub fn rename_vault(&self, id: String, name: String) -> Result<()> {
        if name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        self.with_open(|o| {
            let v = o.vaults.iter_mut().find(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
            let c = v.container.as_mut().ok_or(GhulError::NotFound)?;
            set_vault_name(c, &name);
            v.reference.name = name.trim().to_owned();
            self.save_vault(o, &id)
        })
    }

    /// Deletes a vault: its file is removed from the folder.
    pub fn remove_vault(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let pos = o.vaults.iter().position(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
            let v = o.vaults.remove(pos);
            self.storage.delete(v.reference.file.clone())?;
            // Deleted on purpose: the next synchronisation removes it on the server and the other
            // devices (and only such a mark does; a missing file is repaired instead).
            let mut deleted: Vec<String> = setting(&o.suite, super::sync::DELETED_VAULTS_KEY);
            if !deleted.contains(&v.reference.id) {
                deleted.push(v.reference.id.clone());
            }
            put_setting(&mut o.suite, super::sync::DELETED_VAULTS_KEY, &deleted)?;
            let mut map = revisions(&o.suite);
            map.remove(&v.reference.id);
            put_setting(&mut o.suite, REVISIONS_KEY, &map)?;
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    /// Current file of a vault (for export / backup).
    pub fn export_vault(&self, id: String) -> Result<Vec<u8>> {
        let file = self.with_open(|o| {
            Ok(o.vaults.iter().find(|v| v.reference.id == id).ok_or(GhulError::NotFound)?.reference.file.clone())
        })?;
        self.storage.read(file)?.ok_or(GhulError::NotFound)
    }

    pub fn export_suite(&self) -> Result<Vec<u8>> {
        self.storage.read(SUITE_FILE.to_owned())?.ok_or(GhulError::NoVault)
    }

    /// Reads the folder again: vaults of this suite that appeared meanwhile are opened right
    /// away, removed files disappear; vault files with another protection are returned (they need
    /// their password once).
    pub fn find_new_vaults(&self) -> Result<Vec<FoundVault>> {
        self.with_open(|o| {
            let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let kdf = o.kdf();
            let opener = Opener::Factors { pk: &o.password_key, s: &s, suite_id: o.suite.header().suite_id, kdf: &kdf };
            // Everything is saved immediately, so the folder is the truth: read it again.
            let (vaults, found, copies) = self.scan(&o.suite, &opener)?;
            o.vaults = vaults;
            o.found = found;
            self.remove_copies(o, copies)?;
            if o.harmonize(now_ms())? {
                self.save_all(o)?;
            }
            Ok(o.found.clone())
        })
    }

    /// Does not offer this file again.
    pub fn ignore_found_vault(&self, file: String) -> Result<()> {
        self.with_open(|o| {
            let mut list = ignored_files(&o.suite);
            if !list.contains(&file) {
                list.push(file.clone());
            }
            put_setting(&mut o.suite, IGNORED_KEY, &list)?;
            o.found.retain(|f| f.file != file);
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    /// Adds a vault file (e.g. picked by the user) that already has this suite's protection.
    /// Returns `DifferentProtection` if it needs its own password (then use `adopt_vault`).
    pub fn add_vault_file(&self, data: Vec<u8>, name: String) -> Result<String> {
        let data = Zeroizing::new(data);
        self.with_open(|o| {
            let locked = LockedContainer::parse(&data)?;
            if VaultKind::from_container(locked.kind()).is_none() {
                return Err(GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() });
            }
            if locked.header().suite_id != o.suite.header().suite_id || locked.header().kdf != o.kdf() {
                return Err(GhulError::DifferentProtection);
            }
            let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let mut c = locked.unlock(&o.password_key, &s).map_err(|_| GhulError::DifferentProtection)?;
            // The file may be an old copy: this suite's current code (not the one it had), and
            // a new container key, so its old protection opens nothing written from now on (M1).
            if let Some(code) = o.suite.recovery_code() {
                c.set_recovery_code(&code, "", now_ms(), &mut OsRng)?;
            }
            c.renew_container_key(&o.password_key, &mut OsRng)?;
            self.insert_vault(o, c, &name, None)
        })
    }

    /// Takes over a vault with another protection: it is opened once with its own password plus
    /// key or key file and then switched to this suite's password, keys and recovery code.
    #[allow(clippy::too_many_arguments)]
    pub fn adopt_vault(
        &self,
        source: VaultSource,
        name: String,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
    ) -> Result<String> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let (data, existing) = self.source_bytes(&source)?;
        let locked = LockedContainer::parse(&data)?;
        if VaultKind::from_container(locked.kind()).is_none() {
            return Err(GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() });
        }
        if !self.is_unlocked() {
            return Err(GhulError::Locked);
        }
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let mut c = locked.unlock(&pk, &s)?;
        self.with_open(|o| {
            let now = now_ms();
            let slots = factor_slots(&o.suite);
            c.reprotect(
                o.suite.header().suite_id,
                FactorSecret::from_bytes(*o.suite.factor_secret().expose()),
                slots.clone(),
                &o.password_key,
                o.kdf(),
                &mut OsRng,
            )?;
            for slot in &slots {
                let (label, created) = slot_label(&o.suite, slot);
                c.payload
                    .slots
                    .insert(crate::kdf::hex(slot.id()), SlotMeta { label, created, extra: Default::default() });
            }
            if let Some(code) = o.suite.recovery_code() {
                c.set_recovery_code(&code, "", now, &mut OsRng)?;
            }
            // The file it came from still opens with its old protection: new key for the content.
            c.renew_container_key(&o.password_key, &mut OsRng)?;
            self.insert_vault(o, c, &name, existing)
        })
    }

    /// Accepts the current (older) state of a vault file, e.g. a deliberately restored backup.
    pub fn accept_vault_state(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let v = o.vaults.iter_mut().find(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
            let rev = v.container.as_ref().map(|c| c.payload.revision).unwrap_or(0);
            v.stale = false;
            let mut map = revisions(&o.suite);
            map.insert(id.clone(), rev);
            put_setting(&mut o.suite, REVISIONS_KEY, &map)?;
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    // --- entries -----------------------------------------------------------------------------

    /// Entries of all vaults of `kind`, or of one vault.
    pub fn list_entries(&self, kind: VaultKind, vault_id: Option<String>) -> Result<Vec<EntrySummary>> {
        self.with_open(|o| {
            let kind_name = kind.container().name();
            let mut list: Vec<EntrySummary> = o
                .vaults
                .iter()
                .filter(|v| v.reference.kind == kind_name && vault_id.as_ref().map_or(true, |id| &v.reference.id == id))
                .filter_map(|v| v.container.as_ref().map(|c| (&v.reference, c)))
                .flat_map(|(r, c)| c.payload.entries.iter().filter(|e| !e.deleted).map(move |e| summary(r, e)))
                .collect();
            list.sort_by_key(|s| (s.title.to_lowercase(), s.vault_name.to_lowercase()));
            Ok(list)
        })
    }

    pub fn get_entry(&self, vault_id: String, id: Vec<u8>) -> Result<EntryData> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = c.payload.entries.iter().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)?;
            Ok(EntryData {
                id: e.id.clone(),
                entry_type: e.entry_type.clone(),
                fields: e
                    .fields
                    .iter()
                    .filter_map(|(k, v)| to_field_value(v).map(|value| FieldData { name: k.clone(), value }))
                    .collect(),
                tags: e.tags.clone(),
                folder: e.folder.clone(),
                modified: e.modified,
                revision: e.revision,
            })
        })
    }

    /// Creates (empty `id`) or updates a GhulPass entry and saves. Returns the entry ID.
    pub fn save_entry(&self, vault_id: String, entry: EntryData) -> Result<Vec<u8>> {
        if entry.entry_type.is_empty() || entry.entry_type == "totp" {
            return Err(GhulError::InvalidInput { msg: "invalid entry type".into() });
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            if r.kind != "pass" {
                return Err(GhulError::InvalidInput { msg: "not a GhulPass vault".into() });
            }
            let now = now_ms();
            let entries = &mut c.payload.entries;
            let id = if entry.id.is_empty() {
                let mut e = Entry::new(new_id(), &entry.entry_type, now);
                apply(&mut e, entry);
                e.extra.insert(super::passx::CREATED.into(), Value::Integer(now.into()));
                let id = e.id.clone();
                entries.push(e);
                id
            } else {
                let e = entries.iter_mut().find(|e| e.id == entry.id && !e.deleted).ok_or(GhulError::NotFound)?;
                let bytes: BTreeMap<String, Field> = e
                    .fields
                    .iter()
                    .filter(|(_, v)| matches!(v, Field::Bytes(_)))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                e.entry_type = entry.entry_type.clone();
                let new_password = entry
                    .fields
                    .iter()
                    .find(|f| f.name == "password")
                    .map(|f| match &f.value {
                        FieldValue::Secret { value } | FieldValue::Text { value } => value.clone(),
                        _ => String::new(),
                    })
                    .unwrap_or_default();
                super::passx::remember_password(e, &new_password, now);
                apply(e, entry);
                e.fields.extend(bytes);
                e.revision += 1;
                e.modified = now;
                e.id.clone()
            };
            self.save_vault(o, &vault_id)?;
            Ok(id)
        })
    }

    /// Deletes an entry (keeps a tombstone without content for later synchronisation).
    pub fn delete_entry(&self, vault_id: String, id: Vec<u8>) -> Result<()> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = c.payload.entries.iter_mut().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)?;
            e.make_tombstone(now_ms());
            self.save_vault(o, &vault_id)
        })
    }

    pub fn add_totp(&self, vault_id: String, input: TotpInput) -> Result<Vec<u8>> {
        let secret = validate_totp(&input)?;
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            if r.kind != "totp" {
                return Err(GhulError::InvalidInput { msg: "not a GhulAuth vault".into() });
            }
            // The same key twice (e.g. a QR code read again from an image) is not added (N9).
            let same =
                |e: &Entry| matches!(e.fields.get("secret"), Some(Field::Bytes(b)) if b.as_slice() == &secret[..]);
            if c.payload.entries.iter().any(|e| !e.deleted && same(e)) {
                return Err(GhulError::AlreadyExists);
            }
            let mut e = Entry::new(new_id(), "totp", now_ms());
            e.fields.insert("issuer".into(), Field::Text(input.issuer.trim().to_owned()));
            e.fields.insert("account".into(), Field::Text(input.account.trim().to_owned()));
            e.fields.insert("secret".into(), Field::Bytes(secret.to_vec()));
            e.fields.insert("algorithm".into(), Field::Text(input.algorithm.name().into()));
            e.fields.insert("digits".into(), Field::Int(input.digits as i64));
            e.fields.insert("period".into(), Field::Int(input.period as i64));
            let id = e.id.clone();
            c.payload.entries.push(e);
            self.save_vault(o, &vault_id)?;
            Ok(id)
        })
    }

    pub fn rename_totp(&self, vault_id: String, id: Vec<u8>, issuer: String, account: String) -> Result<()> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = c.payload.entries.iter_mut().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)?;
            e.fields.insert("issuer".into(), Field::Text(issuer.trim().to_owned()));
            e.fields.insert("account".into(), Field::Text(account.trim().to_owned()));
            e.revision += 1;
            e.modified = now_ms();
            self.save_vault(o, &vault_id)
        })
    }

    /// Current and next codes for all GhulAuth vaults at `unix_time` (seconds).
    pub fn totp_codes(&self, unix_time: u64) -> Result<Vec<TotpCode>> {
        self.with_open(|o| {
            let mut out = Vec::new();
            for v in o.vaults.iter().filter(|v| v.reference.kind == "totp") {
                let Some(c) = &v.container else { continue };
                for e in c.payload.entries.iter().filter(|e| !e.deleted && e.entry_type == "totp") {
                    let Some(Field::Bytes(secret)) = e.fields.get("secret") else { continue };
                    let int = |k: &str, d: i64| match e.fields.get(k) {
                        Some(Field::Int(x)) => *x,
                        _ => d,
                    };
                    let digits = int("digits", 6) as u32;
                    let period = int("period", 30).clamp(1, 3600) as u64;
                    let algo = OtpAlgo::from_name(&text_field(e, "algorithm")).unwrap_or(OtpAlgo::Sha1);
                    out.push(TotpCode {
                        vault_id: v.reference.id.clone(),
                        vault_name: v.reference.name.clone(),
                        id: e.id.clone(),
                        issuer: text_field(e, "issuer"),
                        account: text_field(e, "account"),
                        code: crate::totp::totp(secret, unix_time, period, digits, algo.into())?,
                        next_code: crate::totp::totp(secret, unix_time + period, period, digits, algo.into())?,
                        period: period as u32,
                        remaining: crate::totp::totp_remaining(unix_time, period) as u32,
                        digits,
                        favorite: super::passx::is_favorite(e),
                        modified: e.modified,
                    });
                }
            }
            out.sort_by_key(|c| (c.issuer.to_lowercase(), c.account.to_lowercase()));
            Ok(out)
        })
    }

    // --- import and export ---------------------------------------------------------------------

    /// Imports a file into the given vaults (GhulPass entries into `pass_vault`, 2FA accounts into
    /// `totp_vault`; `None` skips that part). With `totps_from_logins`, 2FA codes stored in logins
    /// are also added to GhulAuth. Items already present in the target vault are skipped. With
    /// `keep` (and a GhulPass vault), items that cannot be converted become notes with their
    /// original data.
    pub fn import_data(
        &self,
        data: Vec<u8>,
        file_name: String,
        pass_vault: Option<String>,
        totp_vault: Option<String>,
        totps_from_logins: bool,
        keep: Option<KeepSkipped>,
    ) -> Result<ImportResult> {
        let data = Zeroizing::new(data);
        let mut parsed = super::import::parse(&data, &file_name)?;
        self.with_open(|o| {
            let now = now_ms();
            let mut result =
                ImportResult { entries: 0, totps: 0, duplicates: 0, skipped: 0, skipped_items: Vec::new(), kept: 0 };
            if let Some(vid) = &pass_vault {
                let (r, c) = o.vault_mut(vid)?;
                if r.kind != "pass" {
                    return Err(GhulError::InvalidInput { msg: "not a GhulPass vault".into() });
                }
                let mut seen: Vec<Zeroizing<String>> = c
                    .payload
                    .entries
                    .iter()
                    .filter(|e| !e.deleted)
                    .map(|e| dedup_key(&e.entry_type, e.folder.as_deref(), &e.fields))
                    .collect();
                for d in &parsed.entries {
                    let key = dedup_key(&d.entry_type, d.folder.as_deref(), &d.fields);
                    if seen.contains(&key) {
                        result.duplicates += 1;
                        continue;
                    }
                    let mut e = Entry::new(new_id(), &d.entry_type, now);
                    e.fields = d.fields.clone();
                    e.folder = d.folder.clone();
                    c.payload.entries.push(e);
                    seen.push(key);
                    result.entries += 1;
                }
            }
            if let Some(vid) = &totp_vault {
                let (r, c) = o.vault_mut(vid)?;
                if r.kind != "totp" {
                    return Err(GhulError::InvalidInput { msg: "not a GhulAuth vault".into() });
                }
                let mut secrets: Vec<Zeroizing<Vec<u8>>> = c
                    .payload
                    .entries
                    .iter()
                    .filter(|e| !e.deleted)
                    .filter_map(|e| match e.fields.get("secret") {
                        Some(Field::Bytes(b)) => Some(Zeroizing::new(b.clone())),
                        _ => None,
                    })
                    .collect();
                for t in parsed.totps.iter().filter(|t| totps_from_logins || !t.from_login) {
                    let Some(secret) = super::import::totp_secret(&t.input) else {
                        let i = &t.input;
                        parsed.skipped.push(super::import::Skipped {
                            name: format!("{} {}", i.issuer, i.account).trim().to_owned(),
                            kind: "totp".into(),
                            reason: super::import::SkipReason::InvalidCode,
                            detail: String::new(),
                            raw: format!("{}\n{}\nSecret (Base32): {}", i.issuer, i.account, i.secret_base32),
                            in_entry: t.from_login,
                        });
                        continue;
                    };
                    if secrets.iter().any(|x| **x == *secret) {
                        result.duplicates += 1;
                        continue;
                    }
                    let input = &t.input;
                    let mut e = Entry::new(new_id(), "totp", now);
                    e.fields.insert("issuer".into(), Field::Text(input.issuer.trim().to_owned()));
                    e.fields.insert("account".into(), Field::Text(input.account.trim().to_owned()));
                    e.fields.insert("secret".into(), Field::Bytes(secret.to_vec()));
                    e.fields.insert("algorithm".into(), Field::Text(input.algorithm.name().into()));
                    e.fields.insert("digits".into(), Field::Int(input.digits as i64));
                    e.fields.insert("period".into(), Field::Int(input.period as i64));
                    c.payload.entries.push(e);
                    secrets.push(secret);
                    result.totps += 1;
                }
            }
            // Nothing is lost: what could not be converted becomes a note with its original data.
            if let (Some(k), Some(vid)) = (&keep, &pass_vault) {
                let (_, c) = o.vault_mut(vid)?;
                // Already kept by an earlier import of the same file: not again.
                let kept_before: Vec<String> = c
                    .payload
                    .entries
                    .iter()
                    .filter(|e| !e.deleted && e.entry_type == "note")
                    .filter_map(|e| match e.fields.get(&k.data_label) {
                        Some(Field::Secret(raw)) => Some(raw.clone()),
                        _ => None,
                    })
                    .collect();
                for x in parsed.skipped.iter().filter(|x| !x.in_entry && !x.raw.is_empty()) {
                    if kept_before.contains(&x.raw) {
                        result.duplicates += 1;
                        continue;
                    }
                    let reason = match x.reason {
                        super::import::SkipReason::Hotp => &k.reason_hotp,
                        super::import::SkipReason::Unsupported => &k.reason_unsupported,
                        super::import::SkipReason::InvalidCode => &k.reason_invalid,
                    };
                    let mut e = Entry::new(new_id(), "note", now);
                    let title =
                        if x.name.is_empty() { format!("{} ({})", parsed.format, x.kind) } else { x.name.clone() };
                    e.fields.insert("title".into(), Field::Text(title));
                    let why = if x.detail.is_empty() { reason.clone() } else { format!("{reason} ({})", x.detail) };
                    e.fields.insert(k.reason_label.clone(), Field::Text(format!("{} · {why}", parsed.format)));
                    e.fields.insert(k.data_label.clone(), Field::Secret(x.raw.clone()));
                    e.folder = (!k.folder.is_empty()).then(|| k.folder.clone());
                    c.payload.entries.push(e);
                    result.kept += 1;
                }
            }
            result.skipped = parsed.skipped.len() as u32;
            result.skipped_items = skipped_items(&parsed.skipped);
            for vid in [&pass_vault, &totp_vault].into_iter().flatten() {
                self.save_vault(o, vid)?;
            }
            Ok(result)
        })
    }

    /// Unencrypted export of one vault: GhulPass as CSV, GhulAuth as `otpauth://` links (one per
    /// line). Requires the master password, because the result is readable by anyone.
    pub fn export_plain(&self, vault_id: String, password: String) -> Result<Vec<u8>> {
        if !self.verify_password(password)? {
            return Err(GhulError::WrongPassword);
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            let text = if r.kind == "pass" {
                super::import::export_csv(&c.payload.entries)
            } else if r.kind == "cal" {
                let name = vault_name(c, &r.file);
                super::cal::export_vault_ics(c, &name)
            } else {
                super::import::export_otpauth(&c.payload.entries)
            };
            Ok(text.as_bytes().to_vec())
        })
    }

    /// Some entries of a vault as unencrypted CSV (same columns as the full export). Like the full
    /// export, only with the master password.
    pub fn export_entries(&self, vault_id: String, ids: Vec<Vec<u8>>, password: String) -> Result<Vec<u8>> {
        if !self.verify_password(password)? {
            return Err(GhulError::WrongPassword);
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            let picked: Vec<Entry> =
                c.payload.entries.iter().filter(|e| !e.deleted && ids.contains(&e.id)).cloned().collect();
            if picked.is_empty() {
                return Err(GhulError::NotFound);
            }
            let text = if r.kind == "pass" {
                super::import::export_csv(&picked)
            } else {
                super::import::export_otpauth(&picked)
            };
            Ok(text.as_bytes().to_vec())
        })
    }

    // --- protection ---------------------------------------------------------------------------

    pub fn slots(&self) -> Result<Vec<SlotInfo>> {
        self.with_open(|o| {
            Ok(o.suite
                .slots()
                .iter()
                .map(|s| {
                    let (label, created) = slot_label(&o.suite, s);
                    SlotInfo {
                        slot_id: s.id().to_vec(),
                        kind: match s {
                            Slot::Fido2(_) => SlotKind::SecurityKey,
                            Slot::Keyfile(_) => SlotKind::Keyfile,
                            Slot::Recovery(_) => SlotKind::RecoveryCode,
                        },
                        label,
                        created,
                        needs_update: s.is_legacy(),
                    }
                })
                .collect())
        })
    }

    /// Registers a security key for all files (two touches). Nothing else changes: other keys,
    /// key files and the recovery code stay valid.
    pub fn add_security_key(
        &self,
        transport: Arc<dyn HidTransport>,
        listener: Arc<dyn FidoListener>,
        pin: String,
        label: String,
    ) -> Result<()> {
        let pin = Zeroizing::new(pin);
        if !self.is_unlocked() {
            return Err(GhulError::Locked);
        }
        let (cred_id, prf_input, output) = with_authenticator(&transport, Some(&listener), |a| register_key(a, &pin))?;
        self.changing_protection(|o| {
            let reg = Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output };
            let now = now_ms();
            let slot = o.suite.add_fido2_slot(&reg, &label, now, &mut OsRng)?;
            self.insert_everywhere(o, slot, &label, now)
        })
    }

    /// Adds a key file (from `generate_keyfile`) as second factor for all files.
    pub fn add_keyfile(&self, keyfile: Vec<u8>, label: String) -> Result<()> {
        let keyfile = Keyfile::parse(&Zeroizing::new(keyfile))?;
        self.changing_protection(|o| {
            let id = keyfile.id();
            if o.suite.slots().iter().any(|s| matches!(s, Slot::Keyfile(k) if k.keyfile_id == id)) {
                return Err(GhulError::AlreadyExists);
            }
            let now = now_ms();
            let slot = o.suite.add_keyfile_slot(&keyfile, &label, now, &mut OsRng)?;
            self.insert_everywhere(o, slot, &label, now)
        })
    }

    /// Creates a new recovery code for all files; the previous code stops working.
    pub fn new_recovery_code(&self, label: String) -> Result<String> {
        self.changing_protection(|o| {
            o.remember_protection()?;
            let before = o.current_secrets();
            let code = RecoveryCode::generate(&mut OsRng);
            let now = now_ms();
            for (c, _) in o.containers_mut() {
                c.set_recovery_code(&code, &label, now, &mut OsRng)?;
            }
            // The old code with an old copy of a file must not open what is written now.
            o.renew_keys(false, now)?;
            super::drive::renew_drive_key(o)?;
            o.revoke_replaced(before)?;
            super::sync::mark_protection_changed(o)?;
            self.save_protection(o)?;
            Ok(code.to_display().to_string())
        })
    }

    /// Removes a security key or key file from all files. The last one cannot be removed (a
    /// second factor is always required). Recovery codes are replaced, not removed.
    pub fn remove_slot(&self, slot_id: Vec<u8>) -> Result<()> {
        let id: [u8; 16] =
            slot_id.as_slice().try_into().map_err(|_| GhulError::InvalidInput { msg: "invalid slot id".into() })?;
        self.changing_protection(|o| {
            let slot = o.suite.slots().iter().find(|s| *s.id() == id).cloned().ok_or(GhulError::NotFound)?;
            if !slot.provides_factor() {
                return Err(GhulError::InvalidInput { msg: "recovery codes are replaced, not removed".into() });
            }
            if o.suite.slots().iter().filter(|s| s.provides_factor()).count() <= 1 {
                return Err(GhulError::LastFactor);
            }
            o.remember_protection()?;
            let before = o.current_secrets();
            let mut removed: Vec<String> = setting(&o.suite, REMOVED_SLOTS_KEY);
            removed.push(crate::kdf::hex(&id));
            put_setting(&mut o.suite, REMOVED_SLOTS_KEY, &removed)?;
            for (c, _) in o.containers_mut() {
                match c.remove_slot(&id) {
                    Ok(()) | Err(crate::vault::VaultError::SlotNotFound) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            // The removed key with an old copy of a file must not open what is written now:
            // new factor secret for the remaining keys and key files, new container keys.
            o.renew_keys(true, now_ms())?;
            super::drive::renew_drive_key(o)?;
            o.revoke_replaced(before)?;
            super::sync::mark_protection_changed(o)?;
            self.save_protection(o)?;
            super::quick::delete_all(&self.local_dir);
            Ok(())
        })
    }

    /// Changes the master password (new salt) for all files. The current password must be given.
    pub fn change_password(&self, current: String, new_password: String) -> Result<()> {
        let current = Zeroizing::new(current);
        let new_password = Zeroizing::new(new_password);
        self.changing_protection(|o| {
            let kdf = o.kdf();
            let old_pk = PasswordKey::derive(&current, &kdf)?;
            if !o.suite.verify_password_key(&old_pk) {
                return Err(GhulError::WrongPassword);
            }
            let mut new_kdf = kdf;
            OsRng.fill_bytes(&mut new_kdf.salt);
            let new_pk = PasswordKey::derive(&new_password, &new_kdf)?;
            // A file that is not rewritten (the change stops half way) is still checked with
            // the old password key.
            o.remember_protection()?;
            for (c, _) in o.containers_mut() {
                c.rekey(&new_pk, new_kdf.clone(), &mut OsRng)?;
            }
            o.password_key = new_pk;
            o.keep_password_key(&old_pk)?;
            // The old password with an old copy of a file must not open what is written now.
            o.renew_keys(false, now_ms())?;
            super::drive::renew_drive_key(o)?;
            super::sync::mark_protection_changed(o)?;
            self.save_protection(o)?;
            super::quick::delete_all(&self.local_dir);
            Ok(())
        })
    }

    pub fn verify_password(&self, password: String) -> Result<bool> {
        let password = Zeroizing::new(password);
        self.with_open(|o| {
            let pk = PasswordKey::derive(&password, &o.kdf())?;
            Ok(o.suite.verify_password_key(&pk))
        })
    }
}

/// What a container file needs to be opened (shown before opening a single vault file).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ContainerInfo {
    /// `pass`, `totp` or `suite`
    pub kind: String,
    pub protection: ProtectionInfo,
}

#[uniffi::export]
pub fn inspect_container(data: Vec<u8>) -> Result<ContainerInfo> {
    let l = LockedContainer::parse(&data)?;
    Ok(ContainerInfo { kind: l.kind().name().to_owned(), protection: protection_of(&l) })
}
