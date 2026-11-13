// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite session used by the app.
//!
//! **The folder is the suite:** every vault file (`*.ghlpss`, `*.ghltotp`) in the storage folder
//! belongs to it. All containers share the same password, the same factor slots (security keys
//! and key files, each wrapping the same factor secret) and the same recovery code, so one key
//! touch – or the recovery code alone – opens everything, and every file can also be opened on
//! its own. `suite.ghlsuite` only holds settings (last seen revisions, dismissed files, later mail
//! and storage accounts); it decides nothing about membership and is recreated if missing.
//! Vault files with another protection are offered and, once opened with their own password,
//! switched to the suite's protection.
//!
//! TOTP secrets never leave the core; GhulPass entries are returned to the app because it has to
//! show and fill them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
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
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text { value: String },
    Secret { value: String },
    Url { value: String },
    Number { value: i64 },
    Flag { value: bool },
    Items { values: Vec<String> },
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

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
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

#[derive(uniffi::Record, Clone, Debug)]
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

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SetupResult {
    /// New recovery code to show and print once (11 groups of 5 characters); `None` if the
    /// opened vault already carried the suite's code.
    pub recovery_code: Option<String>,
}

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
    /// Folder of the notes, e.g. "Import – not converted".
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

/// A login that fits a website or app (for autofill).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct AutofillLogin {
    pub vault_id: String,
    pub id: Vec<u8>,
    pub title: String,
    pub username: String,
    pub password: String,
    /// May be filled directly (website with the same domain, or app linked with its signing
    /// certificate). Otherwise it is only a suggestion for the picker.
    pub direct: bool,
}

/// What happened to a login offered for saving by autofill.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutofillSaved {
    Created,
    /// An existing login with the same user name got the new password.
    Updated,
    /// Already stored like this.
    Unchanged,
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

fn dedup_key(entry_type: &str, fields: &BTreeMap<String, Field>) -> String {
    let get = |k: &str| match fields.get(k) {
        Some(Field::Text(s)) | Some(Field::Secret(s)) | Some(Field::Url(s)) => s.trim().to_lowercase(),
        Some(Field::List(l)) => l.first().map(|x| x.trim().to_lowercase()).unwrap_or_default(),
        _ => String::new(),
    };
    format!("{entry_type}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}", get("title"), get("username"), get("urls"), get("password"))
}

// --- files and names --------------------------------------------------------------------------

pub(super) fn is_vault_file(name: &str) -> bool {
    [".ghlpss", ".ghltotp", ".ghlcal", ".ghldrv", ".ghlmail"].iter().any(|e| name.ends_with(e))
}

pub(super) fn file_name(kind: VaultKind, name: &str, id: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "vault".to_owned() } else { slug.chars().take(24).collect() };
    let ext = kind.container().extension();
    format!("{slug}-{}.{ext}", &id[..8])
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

    /// Brings vaults whose slots differ from the suite's (e.g. an old copy) in line with them.
    /// Returns whether anything changed.
    pub(super) fn harmonize(&mut self, now: i64) -> Result<bool> {
        let slots = factor_slots(&self.suite);
        let ids: Vec<[u8; 16]> = slots.iter().map(|s| *s.id()).collect();
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
        let mut changed = false;
        for v in &mut self.vaults {
            let Some(c) = v.container.as_mut() else { continue };
            let mut vids: Vec<[u8; 16]> = factor_slots(c).iter().map(|x| *x.id()).collect();
            vids.sort();
            let mut want = ids.clone();
            want.sort();
            let code_ok = match (&code, c.recovery_code()) {
                (Some(a), Some(b)) => a.expose() == b.expose(),
                (None, _) => true,
                _ => false,
            };
            if vids == want && c.header().kdf == kdf && c.header().suite_id == suite_id && code_ok {
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
    /// Open server connection, Drive index and running uploads (dropped when locking).
    pub(super) net: Mutex<NetCache>,
}

#[derive(Default)]
pub(super) struct NetCache {
    pub(super) conn: Option<(String, Arc<dyn crate::net::remote::RemoteFs>)>,
    pub(super) drive: Option<super::drive::DriveIndex>,
    pub(super) uploads: BTreeMap<String, super::drive::Upload>,
    pub(super) imap: BTreeMap<String, crate::net::imap::ImapSession>,
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

fn parse_keyfile(data: Option<Vec<u8>>) -> Result<Option<Keyfile>> {
    data.map(|d| Keyfile::parse(&Zeroizing::new(d)).map_err(GhulError::from)).transpose()
}

/// Gets the factor secret with a key file or a security key, whichever is given.
fn second_factor(
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

    pub(super) fn with_open<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let mut guard = self.guard();
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

    /// The suite file, or – if it is missing – any readable vault file of the folder. All
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
    /// damaged files are listed as unavailable, copies of the same vault are skipped (the
    /// newest one wins).
    fn scan(&self, suite: &UnlockedContainer, opener: &Opener<'_>) -> Result<(Vec<OpenVault>, Vec<FoundVault>)> {
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
            let opened = match opener {
                Opener::Factors { pk, s, .. } if ours => locked.unlock(pk, s).ok(),
                Opener::Code(code) => locked.recover(code).ok(),
                _ => None,
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
                let newer = existing.container.as_ref().is_some_and(|e| c.payload.revision > e.payload.revision);
                if !newer {
                    continue;
                }
                existing.reference.file = file.clone();
                existing.reference.name = vault_name(&c, &file);
                existing.stale = seen.get(&id).is_some_and(|r| c.payload.revision < *r);
                existing.container = Some(c);
                continue;
            }
            let stale = seen.get(&id).is_some_and(|r| c.payload.revision < *r);
            vaults.push(OpenVault {
                reference: VaultRef { id, kind: kind.container().name().into(), name: vault_name(&c, &file), file },
                container: Some(c),
                stale,
            });
        }
        Ok((vaults, found))
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

    /// Saves all vaults first and the suite last.
    pub(super) fn save_all(&self, o: &mut Open) -> Result<()> {
        for v in &mut o.vaults {
            if let Some(c) = v.container.as_mut() {
                let data = c.seal(&mut OsRng)?;
                self.storage.write(v.reference.file.clone(), data)?;
            }
        }
        o.record_revisions()?;
        self.save_container(&mut o.suite, SUITE_FILE)
    }

    /// Opens the folder's vaults with the suite's password and factor secret and keeps the session.
    fn finish_unlock(&self, password_key: PasswordKey, suite: UnlockedContainer, full: bool) -> Result<()> {
        let s = FactorSecret::from_bytes(*suite.factor_secret().expose());
        let kdf = suite.header().kdf.clone();
        let opener = Opener::Factors { pk: &password_key, s: &s, suite_id: suite.header().suite_id, kdf: &kdf };
        let (vaults, found) = self.scan(&suite, &opener)?;
        let mut open = Open { password_key, suite, vaults, found };
        let suite_missing = self.storage.read(SUITE_FILE.to_owned())?.is_none();
        if open.harmonize(now_ms())? {
            self.save_all(&mut open)?;
        } else if suite_missing {
            open.record_revisions()?;
            self.save_container(&mut open.suite, SUITE_FILE)?;
        }
        if full {
            super::quick::refresh_stamps(&self.local_dir, open.suite.vmk(), now_ms());
        }
        *self.guard() = Some(open);
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
        let file = file_name(kind, name, &id);
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
        let file = file.unwrap_or_else(|| file_name(kind, name, &id));
        let name = vault_name(&c, &file);
        o.found.retain(|f| f.file != file);
        let reference = VaultRef { id: id.clone(), kind: kind.container().name().into(), name, file };
        o.vaults.push(OpenVault { reference, container: Some(c), stale: false });
        self.save_vault(o, &id)?;
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
        self.save_all(o)
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
            net: Mutex::new(NetCache::default()),
        })
    }

    /// Whether the folder contains a suite (settings file or any vault file).
    pub fn exists(&self) -> Result<bool> {
        Ok(self.storage.read(SUITE_FILE.to_owned())?.is_some() || !self.vault_files()?.is_empty())
    }

    pub fn storage_name(&self) -> String {
        self.storage.describe()
    }

    pub fn is_unlocked(&self) -> bool {
        self.guard().is_some()
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
        *self.guard() = Some(open);
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
        let file = file_name(kind, &vault_name, &id);
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
        *self.guard() = Some(open);
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
        let (vaults, found) = self.scan(&suite, &Opener::Code(&code))?;
        let mut open = Open { password_key: PasswordKey::from_bytes(*pk.expose()), suite, vaults, found };
        open.suite.rekey(&pk, kdf.clone(), &mut OsRng)?;
        open.harmonize(now_ms())?;
        for (c, _) in open.containers_mut() {
            c.rekey(&pk, kdf.clone(), &mut OsRng)?;
        }
        super::sync::mark_protection_changed(&mut open)?;
        self.save_all(&mut open)?;
        super::quick::delete_all(&self.local_dir);
        super::quick::refresh_stamps(&self.local_dir, open.suite.vmk(), now_ms());
        *self.guard() = Some(open);
        Ok(())
    }

    /// Locks immediately; all secrets are wiped from memory.
    pub fn lock(&self) {
        *self.guard() = None;
        *self.net.lock().unwrap_or_else(|p| p.into_inner()) = NetCache::default();
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
                o.suite.vmk(),
                pin.as_deref().map(|p| p.as_str()),
                now_ms(),
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
        let suite = match locked.unlock_with_vmk(&secret.vmk) {
            Ok(s) => s,
            Err(_) => {
                super::quick::delete_all(&self.local_dir);
                return Err(GhulError::QuickUnlockUnavailable);
            }
        };
        self.finish_unlock(PasswordKey::from_bytes(*secret.password_key), suite, false)
    }

    pub fn quick_unlock_disable(&self, name: String) -> Result<()> {
        let p = super::quick::path(&self.local_dir, &name)?;
        if p.exists() {
            super::quick::wipe(&p)?;
        }
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
            let (vaults, found) = self.scan(&o.suite, &opener)?;
            o.vaults = vaults;
            o.found = found;
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
            let c = locked.unlock(&o.password_key, &s).map_err(|_| GhulError::DifferentProtection)?;
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
            e.fields.clear();
            e.tags.clear();
            e.folder = None;
            e.extra.clear();
            e.deleted = true;
            e.revision += 1;
            e.modified = now_ms();
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

    // --- import, export, autofill ---------------------------------------------------------------

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
                let mut seen: Vec<String> = c
                    .payload
                    .entries
                    .iter()
                    .filter(|e| !e.deleted)
                    .map(|e| dedup_key(&e.entry_type, &e.fields))
                    .collect();
                for d in &parsed.entries {
                    let key = dedup_key(&d.entry_type, &d.fields);
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

    /// Logins that fit a website (`web_domain`) or an app (`package` with the SHA-256 of its
    /// signing certificate `cert`), for autofill. Without website and app, all logins are returned
    /// (for the picker), none of them as `direct`.
    pub fn autofill_logins(
        &self,
        web_domain: Option<String>,
        package: Option<String>,
        cert: Option<String>,
    ) -> Result<Vec<AutofillLogin>> {
        let everything = web_domain.is_none() && package.is_none();
        self.with_open(|o| {
            let mut out: Vec<(bool, AutofillLogin)> = Vec::new();
            for v in o.vaults.iter().filter(|v| v.reference.kind == "pass") {
                let Some(c) = &v.container else { continue };
                for e in c.payload.entries.iter().filter(|e| !e.deleted && e.entry_type == "login") {
                    let urls: Vec<String> = match e.fields.get("urls") {
                        Some(Field::List(l)) => l.clone(),
                        Some(Field::Text(t)) | Some(Field::Url(t)) => vec![t.clone()],
                        _ => Vec::new(),
                    };
                    let m =
                        super::autofill::match_entry(&urls, web_domain.as_deref(), package.as_deref(), cert.as_deref());
                    if m == super::autofill::Match::None && !everything {
                        continue;
                    }
                    out.push((
                        super::passx::is_favorite(e),
                        AutofillLogin {
                            vault_id: v.reference.id.clone(),
                            id: e.id.clone(),
                            title: text_field(e, "title"),
                            username: text_field(e, "username"),
                            password: match e.fields.get("password") {
                                Some(Field::Secret(s)) | Some(Field::Text(s)) => s.clone(),
                                _ => String::new(),
                            },
                            direct: m == super::autofill::Match::Direct,
                        },
                    ));
                }
            }
            // Favourites first ("pinned" for autofill), then alphabetically.
            out.sort_by_key(|(fav, l)| (!*fav, l.title.to_lowercase(), l.username.to_lowercase()));
            Ok(out.into_iter().map(|(_, l)| l).collect())
        })
    }

    /// Saves a login typed into a website or app (autofill save request): updates the password of
    /// a matching login with the same user name, or creates a new login in the first GhulPass vault.
    pub fn autofill_save(
        &self,
        web_domain: Option<String>,
        package: Option<String>,
        cert: Option<String>,
        username: String,
        password: String,
    ) -> Result<AutofillSaved> {
        let password = Zeroizing::new(password);
        if password.is_empty() {
            return Err(GhulError::InvalidInput { msg: "no password".into() });
        }
        let existing = self
            .autofill_logins(web_domain.clone(), package.clone(), cert.clone())?
            .into_iter()
            .find(|l| l.direct && l.username.trim().eq_ignore_ascii_case(username.trim()));
        self.with_open(|o| {
            let now = now_ms();
            if let Some(l) = existing {
                if l.password == *password {
                    return Ok(AutofillSaved::Unchanged);
                }
                let (_, c) = o.vault_mut(&l.vault_id)?;
                let e = c.payload.entries.iter_mut().find(|e| e.id == l.id).ok_or(GhulError::NotFound)?;
                e.fields.insert("password".into(), Field::Secret(password.to_string()));
                e.revision += 1;
                e.modified = now;
                self.save_vault(o, &l.vault_id)?;
                return Ok(AutofillSaved::Updated);
            }
            let vid = o
                .vaults
                .iter()
                .find(|v| v.reference.kind == "pass" && v.container.is_some())
                .map(|v| v.reference.id.clone())
                .ok_or(GhulError::NotFound)?;
            let host = web_domain.as_deref().and_then(super::autofill::host_of);
            let (title, url) = match (host, package) {
                (Some(h), _) => (super::autofill::base_domain(&h), format!("https://{h}")),
                (None, Some(p)) => {
                    let title = super::autofill::domain_of_package(&p).unwrap_or_else(|| p.clone());
                    (title, super::autofill::app_link(&p, cert.as_deref()))
                }
                (None, None) => return Err(GhulError::InvalidInput { msg: "no website or app".into() }),
            };
            let mut e = Entry::new(new_id(), "login", now);
            e.fields.insert("title".into(), Field::Text(title));
            if !username.trim().is_empty() {
                e.fields.insert("username".into(), Field::Text(username.trim().to_owned()));
            }
            e.fields.insert("password".into(), Field::Secret(password.to_string()));
            e.fields.insert("urls".into(), Field::List(vec![url]));
            let (_, c) = o.vault_mut(&vid)?;
            c.payload.entries.push(e);
            self.save_vault(o, &vid)?;
            Ok(AutofillSaved::Created)
        })
    }

    /// Links an app (package plus signing certificate) to a login, so that it is filled directly
    /// next time. Replaces older links of the same package.
    pub fn link_app_to_login(
        &self,
        vault_id: String,
        id: Vec<u8>,
        package: String,
        cert: Option<String>,
    ) -> Result<()> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
            let e = c.payload.entries.iter_mut().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)?;
            let link = super::autofill::app_link(&package, cert.as_deref());
            let mut urls = match e.fields.get("urls") {
                Some(Field::List(l)) => l.clone(),
                _ => Vec::new(),
            };
            if urls.contains(&link) {
                return Ok(());
            }
            let prefix = format!("androidapp://{}", package.trim());
            urls.retain(|u| u != &prefix && !u.starts_with(&format!("{prefix}?")));
            urls.push(link);
            e.fields.insert("urls".into(), Field::List(urls));
            e.revision += 1;
            e.modified = now_ms();
            self.save_vault(o, &vault_id)
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
        self.with_open(|o| {
            let reg = Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output };
            let now = now_ms();
            let slot = o.suite.add_fido2_slot(&reg, &label, now, &mut OsRng)?;
            self.insert_everywhere(o, slot, &label, now)
        })
    }

    /// Adds a key file (from `generate_keyfile`) as second factor for all files.
    pub fn add_keyfile(&self, keyfile: Vec<u8>, label: String) -> Result<()> {
        let keyfile = Keyfile::parse(&Zeroizing::new(keyfile))?;
        self.with_open(|o| {
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
        self.with_open(|o| {
            let code = RecoveryCode::generate(&mut OsRng);
            let now = now_ms();
            for (c, _) in o.containers_mut() {
                c.set_recovery_code(&code, &label, now, &mut OsRng)?;
            }
            super::sync::mark_protection_changed(o)?;
            self.save_all(o)?;
            Ok(code.to_display().to_string())
        })
    }

    /// Removes a security key or key file from all files. The last one cannot be removed (a
    /// second factor is always required). Recovery codes are replaced, not removed.
    pub fn remove_slot(&self, slot_id: Vec<u8>) -> Result<()> {
        let id: [u8; 16] =
            slot_id.as_slice().try_into().map_err(|_| GhulError::InvalidInput { msg: "invalid slot id".into() })?;
        self.with_open(|o| {
            let slot = o.suite.slots().iter().find(|s| *s.id() == id).cloned().ok_or(GhulError::NotFound)?;
            if !slot.provides_factor() {
                return Err(GhulError::InvalidInput { msg: "recovery codes are replaced, not removed".into() });
            }
            if o.suite.slots().iter().filter(|s| s.provides_factor()).count() <= 1 {
                return Err(GhulError::LastFactor);
            }
            for (c, _) in o.containers_mut() {
                match c.remove_slot(&id) {
                    Ok(()) | Err(crate::vault::VaultError::SlotNotFound) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            super::sync::mark_protection_changed(o)?;
            self.save_all(o)?;
            super::quick::delete_all(&self.local_dir);
            Ok(())
        })
    }

    /// Changes the master password (new salt) for all files. The current password must be given.
    pub fn change_password(&self, current: String, new_password: String) -> Result<()> {
        let current = Zeroizing::new(current);
        let new_password = Zeroizing::new(new_password);
        self.with_open(|o| {
            let kdf = o.kdf();
            let old_pk = PasswordKey::derive(&current, &kdf)?;
            if !o.suite.verify_password_key(&old_pk) {
                return Err(GhulError::WrongPassword);
            }
            let mut new_kdf = kdf;
            OsRng.fill_bytes(&mut new_kdf.salt);
            let new_pk = PasswordKey::derive(&new_password, &new_kdf)?;
            for (c, _) in o.containers_mut() {
                c.rekey(&new_pk, new_kdf.clone(), &mut OsRng)?;
            }
            o.password_key = new_pk;
            super::sync::mark_protection_changed(o)?;
            self.save_all(o)?;
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
