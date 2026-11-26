// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite session used by the app.
//!
//! The suite's files live in one folder on the device (its working copy): `suite.ghlsuite` and
//! one vault file per area or vault (`*.ghlpss`, `*.ghltotp`, ...). Only the app writes there;
//! nothing is matched or merged with other devices. All containers share the same password, the
//! same factor slots (security keys and key files, each wrapping the same factor secret) and the
//! same recovery code, so one key touch, or the recovery code alone, opens everything, and every
//! file can also be opened on its own. Vault files with another protection are ignored.
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
use super::tools::{check_master_password, validate_totp, OtpAlgo, TotpInput};
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
const NAME_KEY: &str = "name";
/// Grows with every change of the protection (password, keys, key files, recovery code).
pub(super) const EPOCH_KEY: &str = "protection_epoch";

pub(super) fn now_ms() -> i64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    #[cfg(test)]
    return now + NOW_SHIFT_MS.with(std::cell::Cell::get);
    #[cfg(not(test))]
    now
}

#[cfg(test)]
thread_local! {
    /// Tests: the clock of the core moved on by this many milliseconds (in this thread).
    pub(crate) static NOW_SHIFT_MS: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
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
    /// GhulDocs (`.ghldoc` and its content files, see `docs`).
    Docs,
}

impl VaultKind {
    pub(super) fn container(self) -> ContainerKind {
        match self {
            VaultKind::Pass => ContainerKind::Pass,
            VaultKind::Totp => ContainerKind::Totp,
            VaultKind::Cal => ContainerKind::Cal,
            VaultKind::Docs => ContainerKind::Docs,
        }
    }

    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "pass" => VaultKind::Pass,
            "cal" => VaultKind::Cal,
            "docs" => VaultKind::Docs,
            _ => VaultKind::Totp,
        }
    }

    pub(super) fn from_container(k: ContainerKind) -> Option<Self> {
        match k {
            ContainerKind::Pass => Some(VaultKind::Pass),
            ContainerKind::Totp => Some(VaultKind::Totp),
            ContainerKind::Cal => Some(VaultKind::Cal),
            ContainerKind::Docs => Some(VaultKind::Docs),
            ContainerKind::Suite | ContainerKind::Export => None,
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
    /// Tags and folder. `get_entry` always gives them; for `save_entry`, `None` keeps those the
    /// entry has (H8: an edit that does not show them must not drop them; a new entry gets
    /// none).
    pub placement: Option<EntryPlacement>,
    pub modified: i64,
    pub revision: u64,
}

/// The tags and the folder of a GhulPass entry.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Default)]
pub struct EntryPlacement {
    pub tags: Vec<String>,
    /// `None`: in no folder.
    pub folder: Option<String>,
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

/// What makes two imported entries the same: every field (two cards "Visa" with different
/// numbers, or two notes without a title, are two entries). Title, user name and addresses
/// ignore case and spaces around them; everything else must match exactly.
pub(super) fn dedup_key(entry_type: &str, fields: &BTreeMap<String, Field>) -> String {
    let mut key = entry_type.to_owned();
    for (name, f) in fields {
        let loose = matches!(name.as_str(), "title" | "username" | "urls");
        let value = match f {
            Field::Text(s) | Field::Secret(s) | Field::Url(s) if loose => s.trim().to_lowercase(),
            Field::List(l) if loose => l.iter().map(|x| x.trim().to_lowercase()).collect::<Vec<_>>().join("\u{1e}"),
            Field::Text(s) | Field::Secret(s) | Field::Url(s) => s.clone(),
            other => format!("{other:?}"),
        };
        if value.is_empty() {
            continue;
        }
        key.push('\u{1f}');
        key.push_str(name);
        key.push('\u{1d}');
        key.push_str(&value);
    }
    key
}

// --- files and names --------------------------------------------------------------------------

pub(super) fn is_vault_file(name: &str) -> bool {
    [".ghlpss", ".ghltotp", ".ghlcal", ".ghldoc"].iter().any(|e| name.ends_with(e))
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

pub(super) fn set_vault_name(c: &mut UnlockedContainer, name: &str) {
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

pub(super) fn protection_of(l: &LockedContainer) -> ProtectionInfo {
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
}

pub(super) struct Open {
    pub(super) password_key: PasswordKey,
    pub(super) suite: UnlockedContainer,
    pub(super) vaults: Vec<OpenVault>,
    /// Opened with the recovery code (H3): until the next lock, a change of the protection needs
    /// the (new) master password alone, so a user who lost every key and key file can add new
    /// ones, remove the lost ones and get a new recovery code.
    pub(super) recovered: bool,
    /// r12 M1: the count of saves of all containers (`protection_changes`) this state goes with:
    /// taken when the call that opened it began, advanced by its own saves.
    pub(super) changes: u64,
}

/// When a call that opens the suite began (see `lock_gen_now`): the lock generation (H10) and
/// the count of saves of all containers (r12 M1).
#[derive(Clone, Copy, Debug)]
pub(super) struct Gen {
    pub(super) lock: u64,
    pub(super) changes: u64,
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

    /// A vault whose entries the generic entry functions may change: GhulPass, GhulAuth (and
    /// calendars), never GhulDocs, whose index goes with its content files and has functions of
    /// its own (`docs`).
    pub(super) fn entry_vault_mut(&mut self, id: &str) -> Result<(&VaultRef, &mut UnlockedContainer)> {
        let (r, c) = self.vault_mut(id)?;
        refuse_docs_file(VaultKind::from_name(&r.kind))?;
        Ok((r, c))
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

    /// A new container of `kind` with the suite's protection and recovery code.
    pub(super) fn new_container(&self, kind: ContainerKind) -> Result<UnlockedContainer> {
        let slots = factor_slots(&self.suite);
        let mut c = self.keys().create_container(kind, slots.clone(), &mut OsRng)?;
        for s in &slots {
            let (label, created) = slot_label(&self.suite, s);
            c.payload.slots.insert(crate::kdf::hex(s.id()), SlotMeta { label, created, extra: Default::default() });
        }
        if let Some(code) = self.suite.recovery_code() {
            let label = self
                .suite
                .slots()
                .iter()
                .find(|s| matches!(s, Slot::Recovery(_)))
                .map(|s| slot_label(&self.suite, s).0)
                .unwrap_or_default();
            c.set_recovery_code(&code, &label, now_ms(), &mut OsRng)?;
        }
        Ok(c)
    }

    /// The protection epoch of the open suite.
    pub(super) fn epoch(&self) -> u64 {
        setting(&self.suite, EPOCH_KEY)
    }

    /// Notes a change of the protection: the epoch grows (see [`EPOCH_KEY`]). Use
    /// [`GhulSuite::commit_protection`], which also tells the device.
    fn bump_epoch(&mut self) -> Result<()> {
        let epoch: u64 = setting(&self.suite, EPOCH_KEY);
        put_setting(&mut self.suite, EPOCH_KEY, &(epoch + 1))
    }

    /// B1: after a change of the protection, older copies of the files (backups, `.bak`, copies
    /// of the suite file) must not open what is written from now on. Every file gets a new container key;
    /// with `factor`, the suite also gets a new factor secret, wrapped for every remaining key
    /// and key file through their public keys.
    pub(super) fn renew_keys(&mut self, factor: bool, now: i64) -> Result<()> {
        let pk = PasswordKey::from_bytes(*self.password_key.expose());
        if factor {
            let (s, slots) = self.suite.renewed_factor(&mut OsRng)?;
            self.suite.replace_factor(s, slots, &pk, &mut OsRng)?;
        }
        // The vaults take over the suite's slots, factor secret and code ...
        self.harmonize(now)?;
        // ... and every file gets its new container key.
        for (c, _) in self.containers_mut() {
            c.renew_container_key(&pk, &mut OsRng)?;
        }
        Ok(())
    }

    /// Brings vaults whose slots differ from the suite's in line with them.
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

/// The first step of an attempt with the app PIN (see `GhulSuite::quick_pin_challenge`).
#[derive(uniffi::Record, Clone)]
pub struct QuickPinChallenge {
    /// The generation of the PIN's file: the app's Keystore aliases name it.
    pub generation: u32,
    /// What the Keystore key of the slot signs with HMAC-SHA256 (53 bytes: 21 of the label, 32 of `P`; secret like the PIN).
    pub message: Vec<u8>,
}

super::redact::redacted_debug!(QuickPinChallenge { generation });

/// Two keys of 32 bytes for the app PIN's slots.
fn slot_keys_of(keys: Vec<Vec<u8>>) -> Result<Zeroizing<[[u8; 32]; super::quick::PIN_SLOTS]>> {
    let bad = || GhulError::InvalidInput { msg: "two keys of 32 bytes".into() };
    if keys.len() != super::quick::PIN_SLOTS {
        return Err(bad());
    }
    let mut out = Zeroizing::new([[0u8; 32]; super::quick::PIN_SLOTS]);
    for (o, k) in out.iter_mut().zip(keys.iter()) {
        *o = k.as_slice().try_into().map_err(|_| bad())?;
    }
    let mut keys = keys;
    keys.iter_mut().for_each(zeroize::Zeroize::zeroize);
    Ok(out)
}

/// Proof of both factors before a change of the protection: the master password plus a key
/// file or the PIN of a security key (touched through the transport given with the call).
#[derive(uniffi::Record, Clone)]
pub struct FactorProof {
    pub password: String,
    pub pin: Option<String>,
    pub keyfile: Option<Vec<u8>>,
}

#[derive(uniffi::Object)]
pub struct GhulSuite {
    pub(super) storage: Arc<dyn VaultStorage>,
    /// Device-local directory for quick unlock (never inside a shared folder).
    pub(super) local_dir: PathBuf,
    open: Mutex<Option<Open>>,
    /// A lock was asked for while a call (e.g. a slow import) still held the open suite: the
    /// suite counts as locked at once and is dropped as soon as that call ends (F92).
    lock_pending: AtomicBool,
    /// Grows with every `lock` (H10): an unlock or `archive_accept` that started before a lock
    /// does not open the suite afterwards (see `set_open`).
    lock_gen: std::sync::atomic::AtomicU64,
    /// Read-modify-write of the device's notes (`archive-seen.json`) one at a time (M5).
    pub(super) seen_lock: Mutex<()>,
    /// One replace of the working copy (or its recovery) at a time (see `finish_earlier_replace`).
    pub(super) replace_lock: Mutex<()>,
    /// r11 M1: one journaled save of all containers (or its putting back) at a time (see
    /// `write_journaled`). Taken after `replace_lock` where both are held.
    /// It assumes one `GhulSuite` per local folder and storage (the app makes exactly one; a
    /// second object on the same folders would not see this lock).
    pub(super) save_lock: Mutex<()>,
    /// r12 M1: grows with every save of all containers (`write_journaled`) and when the suite
    /// is deleted, under `save_lock` before the first file is written. A call that read the
    /// files before does not open (`set_open`) or write (`write_journaled`) what it read.
    pub(super) protection_changes: std::sync::atomic::AtomicU64,
    /// The time zone database of the device (see `set_time_zones`).
    pub(super) zones: Mutex<Option<Arc<dyn super::caltext::TimeZones>>>,
    /// A checked `.ghlzip` waiting to replace the working copy (see `archive_accept`).
    pub(super) pending: Mutex<Option<super::archive::Pending>>,
    /// Grows with every write of a container (see `archive_saved`, `single_saved`).
    pub(super) changes: std::sync::atomic::AtomicU64,
    /// Grows with every write or removal of a vault file (see `content_change`).
    pub(super) content_changes: std::sync::atomic::AtomicU64,
    /// Grows with every lock and cancel: a file checked meanwhile is not kept (see `archive_open`).
    pub(super) pending_gen: std::sync::atomic::AtomicU64,
    /// The last `archive_accept` got as far as removing this device's data of the old state.
    pub(super) wiped: AtomicBool,
    /// How much content a GhulDocs vault may hold ([`super::docs::DOCS_LIMIT`]; smaller only
    /// in tests; all GhulDocs vaults together [`super::docs::suite_limit`] of it).
    pub(super) docs_limit: std::sync::atomic::AtomicU64,
    /// Content files being copied into a `.ghlzip` right now, and content files deleted
    /// meanwhile (deleted only when the last copy ended, see `docs::ContentCopy`).
    pub(super) content_copies: Mutex<super::docs::Copies>,
    /// The state the last `archive_build` sealed, until `archive_saved` notes it.
    pub(super) built: Mutex<Option<super::archive::BuiltState>>,
    /// Marking and clearing the unsaved mark one at a time (a change while a save is noted is
    /// never left unmarked).
    pub(super) unsaved_lock: Mutex<()>,
    /// Documents being added right now (their content files are not in an index yet).
    pub(super) docs_adds: std::sync::atomic::AtomicUsize,
    /// Both factors were shown a moment ago (`confirm_factors`, or the suite was just created):
    /// the next change of the protection needs the password alone, once (see `require_factors`).
    pub(super) factors_confirmed: Mutex<Option<std::time::Instant>>,
}

/// How containers of the folder are opened.
enum Opener<'a> {
    Factors { pk: &'a PasswordKey, s: &'a FactorSecret, suite_id: [u8; 16], kdf: &'a crate::vault::KdfParams },
    Code(&'a RecoveryCode),
}

/// Opens the key: CTAP2 over the given HID connection, or the system's WebAuthn API when the
/// app passed [`crate::key_device::platform_security_key`].
pub(super) fn with_authenticator<T>(
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
/// `exclude`: the credentials of the keys a suite has already (the same key twice is no backup).
pub(super) fn register_key(auth: &dyn KeyDevice, pin: &str, exclude: &[Vec<u8>]) -> Result<NewKey> {
    let cred = auth.make_credential(RP_ID, RP_NAME, pin, exclude)?;
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

/// Asks the key for the factor secret of one of the container's FIDO2 slots. r9 N3: only slots
/// of GhulSuite's own RP ID are asked for: a vault file from elsewhere naming another one (a
/// website) never makes the key answer for it with the user's PIN.
pub(super) fn key_factor(
    locked: &LockedContainer,
    transport: &Arc<dyn HidTransport>,
    listener: Option<&Arc<dyn FidoListener>>,
    pin: &str,
) -> Result<FactorSecret> {
    let all = locked.fido2_requests();
    if all.is_empty() {
        return Err(GhulError::InvalidInput { msg: "this vault has no security key".into() });
    }
    let requests: Vec<_> = all.into_iter().filter(|r| r.rp_id == RP_ID).collect();
    if requests.is_empty() {
        return Err(GhulError::KeyNotRegistered);
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

/// The message when the file of a vault to be removed cannot be deleted (another program holds
/// it): nothing was removed (translated by the app).
pub const VAULT_NOT_REMOVED: &str = "the vault could not be removed: its file cannot be deleted right now";

/// The message when a suite has as many GhulDocs vaults as it may (translated by the app).
pub(super) const DOCS_VAULTS_FULL: &str = "a suite has at most 3 GhulDocs vaults";

/// A suite holds at most [`super::docs::DOCS_SUITE_VAULTS`] GhulDocs vaults (each up to 1 GB, the
/// suite file stays below 4 GiB): one more is refused wherever a vault comes in.
pub(super) fn refuse_docs_vault_over_limit(o: &Open, kind: VaultKind) -> Result<()> {
    let name = VaultKind::Docs.container().name();
    let have = o.vaults.iter().filter(|v| v.reference.kind == name).count() as u64;
    if kind == VaultKind::Docs && have >= super::docs::DOCS_SUITE_VAULTS {
        return Err(GhulError::InvalidInput { msg: DOCS_VAULTS_FULL.into() });
    }
    Ok(())
}

/// A GhulDocs vault is more than its vault file: it goes with its content files (`docs`).
pub(super) fn refuse_docs_file(kind: VaultKind) -> Result<()> {
    if kind == VaultKind::Docs {
        return Err(GhulError::InvalidInput { msg: "a GhulDocs vault goes with its documents".into() });
    }
    Ok(())
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
    if let Some(p) = data.placement {
        e.tags = p.tags;
        e.folder = p.folder.filter(|f| !f.is_empty());
    }
}

pub(super) fn slot_label(c: &UnlockedContainer, s: &Slot) -> (String, i64) {
    c.payload.slots.get(&crate::kdf::hex(s.id())).map(|m| (m.label.clone(), m.created)).unwrap_or_default()
}

#[cfg(test)]
thread_local! {
    /// Tests: runs once at the next point where a call that started before would open the suite
    /// or keep a confirmation (`set_open`, `confirm_factors`), e.g. a lock from another thread.
    pub(crate) static LOCK_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    /// Tests: runs once when `open_folder` begins (after the anchor was read and opened), e.g.
    /// a deletion from another thread (r13 N1).
    pub(crate) static FOLDER_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    /// Tests: added to the age of a confirmation of both factors (the two minutes of
    /// `confirm_factors`).
    pub(crate) static CLOCK_SHIFT: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

#[cfg(test)]
pub(super) fn run_lock_hook() {
    if let Some(f) = LOCK_HOOK.with(|h| h.borrow_mut().take()) {
        f();
    }
}

/// How long ago a confirmation of both factors was given.
fn ticket_age(t: std::time::Instant) -> std::time::Duration {
    #[cfg(test)]
    return t.elapsed() + CLOCK_SHIFT.with(std::cell::Cell::get);
    #[cfg(not(test))]
    t.elapsed()
}

/// In the device's own folder: older copies of containers are still to be removed (see
/// `purge_old_copies`).
pub(super) const PURGE_PENDING: &str = "purge-pending";

/// r10 N5: the journal of a save of all containers (local folder) and the copies of the files
/// before it (in the storage, `<file>.prev`).
const SAVE_JOURNAL: &str = "save-journal";
const PREV_SUFFIX: &str = ".prev";
/// The journal once its save is written or put back: nothing is put back any more.
const SAVE_DONE: &str = "done";

/// r11 N2: the message when the files before an unfinished save cannot be put back (unlocking
/// waits until they can: the storage is full or a file is held by another program).
pub const SAVE_UNFINISHED: &str = "an earlier save could not be finished: the files before it cannot be put back yet";

/// A copy of a save (`<file>.prev`) or a leftover of writing one (`.prev.tmp`, `.prev.bak`).
pub(super) fn is_prev_copy(name: &str) -> bool {
    let name = name.trim_start_matches('.');
    name.ends_with(PREV_SUFFIX) || name.ends_with(".prev.tmp") || name.ends_with(".prev.bak")
}

fn prev_name(file: &str) -> String {
    format!("{file}{PREV_SUFFIX}")
}

#[cfg(test)]
thread_local! {
    /// Tests: writing a container fails from this one on (0-based), as if the storage went away.
    pub(crate) static WRITES_BEFORE_FAILURE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    /// Tests: after such a failure nothing is put back at once (as if the app stopped there).
    pub(crate) static STOP_AFTER_FAILED_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// H-D1: what is added does not fit into the vault's file any more.
pub const VAULT_FULL: &str = "this vault is full: one vault file holds at most 128 MiB";

#[cfg(test)]
thread_local! {
    /// Tests: no older copy can be removed (as if another program held them open).
    pub(crate) static PURGE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How long a confirmation of both factors (`confirm_factors`) holds.
pub(super) const TICKET_VALIDITY: std::time::Duration = std::time::Duration::from_secs(120);

impl GhulSuite {
    /// Locks without counting a new lock generation: the suite counts as locked at once, its
    /// keys go (see `lock`). `archive_accept` uses it for its own lock, so a lock from outside
    /// during the call still wins.
    pub(super) fn lock_now(&self) {
        self.lock_pending.store(true, Ordering::SeqCst);
        *self.factors_confirmed.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // A checked suite file holds its keys: not past locking either.
        self.set_pending(None);
        self.drop_open(false);
    }

    /// The open suite. A call that panicked while it held it (the core unwinds, M28) may have
    /// left it half changed (a change of the protection, for one): it is dropped then, the suite
    /// is locked, and the next unlock reads what is on the disk.
    fn guard(&self) -> MutexGuard<'_, Option<Open>> {
        self.open.lock().unwrap_or_else(|p| self.poisoned(p.into_inner()))
    }

    fn poisoned<'a>(&self, mut g: MutexGuard<'a, Option<Open>>) -> MutexGuard<'a, Option<Open>> {
        *g = None;
        self.open.clear_poison();
        g
    }

    /// The lock generation now: taken when an unlock (or `archive_accept`) starts, given to
    /// [`Self::set_open`] at its end.
    pub(super) fn lock_gen_now(&self) -> Gen {
        Gen { lock: self.lock_gen.load(Ordering::SeqCst), changes: self.protection_changes.load(Ordering::SeqCst) }
    }

    /// Unlocked: a lock asked for before the call started (`gen`) is done with. A lock asked for
    /// after it started wins (H10): the suite stays locked and the call ends with `Locked`.
    pub(super) fn set_open(&self, open: Open, gen: Gen) -> Result<()> {
        #[cfg(test)]
        run_lock_hook();
        let mut guard = self.guard();
        if self.lock_gen.load(Ordering::SeqCst) != gen.lock {
            return Err(GhulError::Locked);
        }
        // r12 M1: the files were read before a save of all of them (a change of the protection,
        // or the suite deleted) wrote anew: what was opened is the state before. The app
        // unlocks again.
        if self.protection_changes.load(Ordering::SeqCst) != open.changes {
            return Err(GhulError::Locked);
        }
        *guard = Some(open);
        self.lock_pending.store(false, Ordering::SeqCst);
        // `lock` counts first and marks the pending lock after: a lock counted by now either
        // is seen here, or its mark comes after the one cleared above and drops the suite.
        if self.lock_gen.load(Ordering::SeqCst) != gen.lock {
            *guard = None;
            return Err(GhulError::Locked);
        }
        Ok(())
    }

    /// Drops the open suite (if a lock is still pending). With `wait` false it gives up at once
    /// if a call holds it (returns false; the call drops the suite when it ends).
    fn drop_open(&self, wait: bool) -> bool {
        let mut guard = match self.open.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(p)) => self.poisoned(p.into_inner()),
            Err(TryLockError::WouldBlock) if wait => self.guard(),
            Err(TryLockError::WouldBlock) => return false,
        };
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
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

    fn run_open<T>(&self, guard: &mut Option<Open>, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let open = guard.as_mut().ok_or(GhulError::Locked)?;
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
    pub(super) fn anchor(&self) -> Result<LockedContainer> {
        self.anchor_with(false)
    }

    /// [`Self::anchor`]; with `past_damaged`, a suite file that cannot be read (cut off,
    /// damaged) is passed over as well: the vault files carry the same protection. Only for the
    /// proof before deleting or replacing (round 6, M4): unlocking with a vault as anchor would
    /// make a new suite file without the protection epoch.
    pub(super) fn anchor_with(&self, past_damaged: bool) -> Result<LockedContainer> {
        match self.read_locked(SUITE_FILE) {
            Ok(Some(l)) => return Ok(l),
            Ok(None) => {}
            Err(GhulError::Corrupted { .. }) if past_damaged => {}
            Err(e) => return Err(e),
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
    pub(super) fn suite_for_vault(&self, anchor: &UnlockedContainer, pk: &PasswordKey) -> Result<UnlockedContainer> {
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

    /// Opens every vault file of the folder. Files with another protection are left alone;
    /// damaged files of this suite are listed as unavailable.
    fn scan(&self, opener: &Opener<'_>) -> Result<Vec<OpenVault>> {
        let mut vaults: Vec<OpenVault> = Vec::new();
        for file in self.vault_files()? {
            let kind_by_ext = if file.ends_with(".ghlpss") {
                VaultKind::Pass
            } else if file.ends_with(".ghlcal") {
                VaultKind::Cal
            } else if file.ends_with(".ghldoc") {
                VaultKind::Docs
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
            let opened = match opener {
                Opener::Factors { pk, s, suite_id, kdf } => {
                    if &locked.header().suite_id != suite_id {
                        continue;
                    }
                    // A vault of this suite with another protection (a change of the password
                    // that was saved halfway): unavailable, not gone. Saving the suite file is
                    // refused then, and the recovery code still opens it.
                    if &locked.header().kdf != *kdf {
                        vaults.push(damaged(&file));
                        continue;
                    }
                    locked.unlock(pk, s).ok()
                }
                Opener::Code(code) => {
                    if !locked.has_recovery() {
                        continue;
                    }
                    match locked.recover(code) {
                        Ok(c) => Some(c),
                        Err(_) => continue,
                    }
                }
            };
            let Some(c) = opened else {
                vaults.push(damaged(&file));
                continue;
            };
            let id = crate::kdf::hex(&c.container_id());
            if vaults.iter().any(|v| v.reference.id == id) {
                // A second file of the same vault (only the app writes here): the first counts.
                continue;
            }
            vaults.push(OpenVault {
                reference: VaultRef { id, kind: kind.container().name().into(), name: vault_name(&c, &file), file },
                container: Some(c),
            });
        }
        Ok(vaults)
    }

    pub(super) fn save_container(&self, c: &mut UnlockedContainer, file: &str) -> Result<()> {
        self.save_container_as(c, file, &[super::archive::unsaved_area_of(file)])
    }

    /// [`Self::save_container`], noting `areas` as unsaved (none: the suite file written along
    /// with a vault, its area is the vault's).
    fn save_container_as(&self, c: &mut UnlockedContainer, file: &str, areas: &[&str]) -> Result<()> {
        let data = c.seal(&mut OsRng)?;
        self.mark_unsaved(areas)?;
        self.storage.write(file.to_owned(), data)?;
        Ok(())
    }

    /// Saves one vault and the suite.
    pub(super) fn save_vault(&self, o: &mut Open, id: &str) -> Result<()> {
        let (r, c) = o.vault_mut(id)?;
        let file = r.file.clone();
        self.note_content_change();
        self.save_container(c, &file)?;
        self.save_container_as(&mut o.suite, SUITE_FILE, &[])
    }

    /// Saves all vaults first and the suite last. H-D1: every container is sealed before the
    /// first file is written, so a container that cannot be sealed (too large) changes nothing
    /// on the disk: a change of the protection never lands half (some vaults with the new
    /// protection, the suite with the one before).
    pub(super) fn save_all(&self, o: &mut Open) -> Result<()> {
        let mut sealed = Vec::with_capacity(o.vaults.len() + 1);
        for v in &mut o.vaults {
            if let Some(c) = v.container.as_mut() {
                sealed.push((v.reference.file.clone(), c.seal(&mut OsRng)?));
            }
        }
        sealed.push((SUITE_FILE.to_owned(), o.suite.seal(&mut OsRng)?));
        self.write_journaled(sealed, &mut o.changes)
    }

    /// r10 N5: writes all containers so that a failure or a stop in the middle never leaves a
    /// change of the protection half on the disk. Each file there is first copied to
    /// `<file>.prev`, then a journal names them (`keep`) and the files this save makes new
    /// (`new`); only then the new files are written, and the journal is marked done. A failed
    /// write puts the copies back at once, a stop does so at the next unlock
    /// ([`Self::finish_earlier_save`]). Without files before (a new suite) it writes directly.
    /// r11 M1: one at a time with the putting back (`save_lock`).
    /// r12 M1: `changes` is the count the caller's state goes with; if another save of all
    /// containers (or a deletion) came since, nothing is written (`Locked`). Otherwise the count
    /// grows before the first file is written, and `changes` with it.
    fn write_journaled(&self, sealed: Vec<(String, Vec<u8>)>, changes: &mut u64) -> Result<()> {
        let _one = self.save_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.protection_changes.load(Ordering::SeqCst) != *changes {
            return Err(GhulError::Locked);
        }
        *changes = self.protection_changes.fetch_add(1, Ordering::SeqCst) + 1;
        let mut lines = Vec::new();
        for (file, _) in &sealed {
            match self.storage.read(file.clone())? {
                Some(old) => {
                    self.storage.write(prev_name(file), old)?;
                    lines.push(format!("keep {file}"));
                }
                None => lines.push(format!("new {file}")),
            }
        }
        if !lines.iter().any(|l| l.starts_with("keep ")) {
            return self.write_sealed(sealed, &["suite"]).map_err(|(_, e)| e);
        }
        std::fs::create_dir_all(&self.local_dir)?;
        super::storage::write_atomic(&self.local_dir, SAVE_JOURNAL, lines.join("\n").as_bytes())?;
        if let Err((_, e)) = self.write_sealed(sealed, &["suite"]) {
            #[cfg(test)]
            if STOP_AFTER_FAILED_WRITE.with(std::cell::Cell::get) {
                return Err(e);
            }
            if let Err(back) = self.finish_earlier_save_held() {
                eprintln!("GhulSuite: the files before a failed save could not be put back yet: {back}");
            }
            return Err(e);
        }
        // Written: from now on the new files count (r11 N1: marked in one step, so a journal
        // that cannot be deleted right now puts nothing back later), then the copies go.
        super::storage::write_atomic(&self.local_dir, SAVE_JOURNAL, SAVE_DONE.as_bytes())?;
        let _ = self.end_save_journal();
        Ok(())
    }

    /// Puts back the files of a save that failed or stopped in the middle (see
    /// [`Self::write_journaled`]) and removes the copies; copies without a journal (a stop
    /// before it was written) are only removed. Called before every unlock.
    pub(super) fn finish_earlier_save(&self) -> Result<()> {
        let _one = self.save_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_earlier_save_held()
    }

    fn finish_earlier_save_held(&self) -> Result<()> {
        let journal = self.local_dir.join(SAVE_JOURNAL);
        let list = match std::fs::read_to_string(&journal) {
            Ok(list) => list,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::from(SAVE_DONE),
            Err(e) => return Err(e.into()),
        };
        // r11 H1: only for the suite in the folder now. A journal of a suite deleted since
        // (and a new one made) never writes the old one over the new one. r12 N4: the suite
        // the copies are of is read from the first kept copy (the suite file's when it was
        // kept, else a vault's), so a suite file this save made new does not hide it.
        let id = |name: &str| -> Result<Option<[u8; 16]>> {
            Ok(self
                .storage
                .read(name.to_owned())?
                .map(|d| LockedContainer::parse(&d).map_or([0; 16], |l| l.header().suite_id)))
        };
        let kept: Vec<&str> = list.lines().filter_map(|l| l.strip_prefix("keep ")).collect();
        let first = kept.iter().find(|f| **f == SUITE_FILE).or(kept.first()).copied();
        let ours = match (list != SAVE_DONE, first) {
            (true, Some(first)) => {
                let now = id(SUITE_FILE)?;
                now.is_none() || now == id(&prev_name(first))?
            }
            _ => false,
        };
        if ours {
            for line in list.lines() {
                if let Some(file) = line.strip_prefix("keep ") {
                    // r12 N1: a copy that is missing is an error, never skipped (the file
                    // would stay with the protection of the failed save).
                    let old = self.storage.read(prev_name(file))?.ok_or_else(|| GhulError::Storage {
                        msg: format!("the copy of {file} before the save is missing"),
                    })?;
                    self.storage.write(file.to_owned(), old)?;
                } else if let Some(file) = line.strip_prefix("new ") {
                    // r11 N4: a file only this save made goes again.
                    self.storage.delete(file.to_owned())?;
                }
            }
            super::storage::write_atomic(&self.local_dir, SAVE_JOURNAL, SAVE_DONE.as_bytes())?;
            // r12 N5: the `.bak` of the files of the failed save go as well.
            self.purge_copies(false);
        }
        let _ = self.end_save_journal();
        Ok(())
    }

    /// Removes the journal of a save and every copy `<file>.prev` (with the `.tmp` and `.bak`
    /// of writing them). What cannot be removed now is noted for later ([`PURGE_PENDING`]):
    /// the copies hold the protection before. Also when the suite is deleted or a new one made.
    /// Returns whether something stayed (r12 N2).
    pub(super) fn end_save_journal(&self) -> bool {
        let mut left = false;
        let journal = self.local_dir.join(SAVE_JOURNAL);
        if let Err(e) = std::fs::remove_file(&journal) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("GhulSuite: the journal of a save could not be removed yet: {e}");
                // r13 N4: a journal marked done that stays does no harm (it puts nothing back,
                // its copies go below); only one that could not even be marked is a rest.
                let done = std::fs::read_to_string(&journal).is_ok_and(|j| j == SAVE_DONE)
                    || super::storage::write_atomic(&self.local_dir, SAVE_JOURNAL, SAVE_DONE.as_bytes()).is_ok();
                left |= !done;
            }
        }
        for name in self.storage.list().unwrap_or_default() {
            if name.ends_with(PREV_SUFFIX) && self.storage.delete(name.clone()).is_err() {
                left = true;
            }
        }
        if let Some(dir) = self.storage.content_dir().map(PathBuf::from) {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if is_prev_copy(&n) && std::fs::remove_file(e.path()).is_err() {
                    left = true;
                }
            }
        }
        if left {
            let _ = std::fs::create_dir_all(&self.local_dir);
            let _ = std::fs::write(self.local_dir.join(PURGE_PENDING), b"");
        }
        left
    }

    /// r12 N1: whether the copies of a save are still needed: a journal that is not done.
    /// r13 N4: not for a deleted suite: nothing would put its copies back. r14 N1: the suite
    /// counts as there while any of its files is (a save may have written the suite file last).
    fn save_journal_open(&self) -> bool {
        std::fs::read_to_string(self.local_dir.join(SAVE_JOURNAL)).is_ok_and(|j| j != SAVE_DONE)
            && self.exists().unwrap_or(true)
    }

    /// Writes sealed containers in this order (the suite last). On an error: how many were
    /// written before it.
    /// `areas`: what is noted as unsaved (see `mark_unsaved`).
    fn write_sealed(
        &self,
        sealed: Vec<(String, Vec<u8>)>,
        areas: &[&str],
    ) -> std::result::Result<(), (usize, GhulError)> {
        // Marked before the first file changes (see `mark_unsaved`).
        self.mark_unsaved(areas).map_err(|e| (0, e))?;
        self.note_content_change();
        for (i, (file, data)) in sealed.into_iter().enumerate() {
            #[cfg(test)]
            if WRITES_BEFORE_FAILURE.with(|c| c.get().is_some_and(|n| i >= n)) {
                return Err((i, GhulError::Storage { msg: String::from("test: the storage went away") }));
            }
            self.storage.write(file, data).map_err(|e| (i, e.into()))?;
        }
        Ok(())
    }

    /// H-D1: adds entries to `vaults` (`add` only appends to their entries) and saves them with
    /// the suite. If they do not fit (all vaults besides GhulDocs, see `check_other_room`, or one
    /// vault file, see `MAX_PAYLOAD_LEN`) or cannot be written, the added entries go again: a
    /// vault that cannot be saved never stays in memory (every later save of it, and every
    /// change of the protection, would fail).
    pub(super) fn add_entries<T>(
        &self,
        o: &mut Open,
        vaults: &[String],
        add: impl FnOnce(&mut Open) -> Result<T>,
    ) -> Result<T> {
        let mut before = Vec::with_capacity(vaults.len());
        for id in vaults {
            if !before.iter().any(|(v, _)| v == id) {
                before.push((id.clone(), o.vault_mut(id)?.1.payload.entries.len()));
            }
        }
        let mut wrote = 0;
        let result = add(o).and_then(|t| {
            let unchanged =
                before.iter().all(|(id, len)| o.vault_mut(id).is_ok_and(|(_, c)| c.payload.entries.len() == *len));
            if unchanged {
                return Ok(t);
            }
            super::archive::check_other_room(o, 0)?;
            let mut sealed = Vec::with_capacity(before.len() + 1);
            for (id, _) in &before {
                let (r, c) = o.vault_mut(id)?;
                if !crate::vault::payload::fits(c.payload.to_cbor()?.len()) {
                    return Err(GhulError::InvalidInput { msg: VAULT_FULL.into() });
                }
                let file = r.file.clone();
                sealed.push((file, c.seal(&mut OsRng)?));
            }
            let areas: Vec<&str> = sealed.iter().map(|(f, _)| super::archive::unsaved_area_of(f)).collect();
            sealed.push((SUITE_FILE.to_owned(), o.suite.seal(&mut OsRng)?));
            self.write_sealed(sealed, &areas).map_err(|(n, e)| {
                wrote = n;
                e
            })?;
            Ok(t)
        });
        if result.is_err() {
            for (id, len) in &before {
                if let Ok((_, c)) = o.vault_mut(id) {
                    c.payload.entries.truncate(*len);
                }
            }
            // Some files were written before the error: they get the state without the entries.
            if wrote > 0 {
                for (id, _) in &before {
                    if let Err(e) = self.save_vault(o, id) {
                        eprintln!("GhulSuite: a vault could not be written back after a failed save: {e}");
                    }
                }
            }
        }
        result
    }

    /// Opens the folder's vaults with the suite's password and factor secret and keeps the
    /// session, unless the suite was locked since the call started (`gen`, see `set_open`).
    pub(super) fn finish_unlock(
        &self,
        password_key: PasswordKey,
        suite: UnlockedContainer,
        full: bool,
        gen: Gen,
    ) -> Result<()> {
        let open = self.open_folder(password_key, suite, full, gen.changes)?;
        self.set_open(open, gen)
    }

    /// Opens the folder's vaults with the suite's password and factor secret (not kept yet).
    pub(super) fn open_folder(
        &self,
        password_key: PasswordKey,
        suite: UnlockedContainer,
        full: bool,
        changes: u64,
    ) -> Result<Open> {
        #[cfg(test)]
        if let Some(f) = FOLDER_HOOK.with(|h| h.borrow_mut().take()) {
            f();
        }
        let s = FactorSecret::from_bytes(*suite.factor_secret().expose());
        let kdf = suite.header().kdf.clone();
        let opener = Opener::Factors { pk: &password_key, s: &s, suite_id: suite.header().suite_id, kdf: &kdf };
        let vaults = self.scan(&opener)?;
        let mut open = Open { password_key, suite, vaults, recovered: false, changes };
        let suite_missing = self.storage.read(SUITE_FILE.to_owned())?.is_none();
        let harmonized = open.harmonize(now_ms())?;
        {
            // r13 N1: nothing is written for files read before a save of all containers or a
            // deletion (the deleted suite would come back).
            let _one = self.save_lock.lock().unwrap_or_else(|e| e.into_inner());
            if self.protection_changes.load(Ordering::SeqCst) != open.changes {
                return Err(GhulError::Locked);
            }
            // r11 N3: a note of the epoch that failed after a change of the protection.
            if let Err(e) = self.note_epoch(&open.suite.header().suite_id, open.epoch()) {
                eprintln!("GhulSuite: the epoch could not be noted: {e}");
            }
            if !harmonized && suite_missing {
                self.save_container(&mut open.suite, SUITE_FILE)?;
            }
        }
        if harmonized {
            self.save_all(&mut open)?;
            // Vaults that had another protection: their copies before go (H7).
            self.purge_old_copies();
        }
        // Copies that could not be removed after an earlier change go now.
        self.retry_purge();
        if full {
            super::quick::refresh_stamps(&self.local_dir, open.password_key.expose(), now_ms());
        }
        self.docs_tidy(&open);
        Ok(open)
    }

    /// Creates a vault with the suite's protection and recovery code.
    pub(super) fn add_vault(&self, o: &mut Open, kind: VaultKind, name: &str) -> Result<String> {
        refuse_docs_vault_over_limit(o, kind)?;
        let mut c = o.new_container(kind.container())?;
        set_vault_name(&mut c, name);
        let id = crate::kdf::hex(&c.container_id());
        let file = file_name(kind, &id);
        let reference =
            VaultRef { id: id.clone(), kind: kind.container().name().into(), name: name.trim().to_owned(), file };
        o.vaults.push(OpenVault { reference, container: Some(c) });
        Ok(id)
    }

    /// Adds an opened vault (already with the suite's protection) and writes it to the folder.
    pub(super) fn insert_vault(&self, o: &mut Open, mut c: UnlockedContainer, name: &str) -> Result<String> {
        let kind = VaultKind::from_container(c.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() })?;
        let id = crate::kdf::hex(&c.container_id());
        if o.vaults.iter().any(|v| v.reference.id == id) {
            return Err(GhulError::AlreadyExists);
        }
        refuse_docs_vault_over_limit(o, kind)?;
        if !name.trim().is_empty() {
            set_vault_name(&mut c, name);
        }
        if kind != VaultKind::Docs {
            super::archive::check_other_room(o, super::archive::container_weight(&c)?)?;
        }
        let file = file_name(kind, &id);
        // The file name holds only the first half of the id: another vault (one listed, or a
        // damaged one that is only in the folder) with that name is never overwritten.
        if self.vault_file_taken(o, &file)? {
            return Err(GhulError::AlreadyExists);
        }
        let name = vault_name(&c, &file);
        let reference = VaultRef { id: id.clone(), kind: kind.container().name().into(), name, file: file.clone() };
        o.vaults.push(OpenVault { reference, container: Some(c) });
        if let Err(e) = self.save_vault(o, &id) {
            // Not half added: neither in the open suite nor in the folder (the suite file on the
            // disk does not list vaults, a vault file alone would be found at the next unlock).
            if let Some(pos) = o.vaults.iter().position(|v| v.reference.id == id) {
                let mut v = o.vaults.remove(pos);
                if let Some(c) = v.container.as_mut() {
                    c.payload.entries.iter_mut().for_each(zeroize::Zeroize::zeroize);
                }
            }
            let _ = self.storage.delete(file);
            return Err(e);
        }
        Ok(id)
    }

    /// Whether a vault file `file` is there already: listed in the open suite, or in the folder
    /// (e.g. a vault that cannot be read and is kept as it is).
    pub(super) fn vault_file_taken(&self, o: &Open, file: &str) -> Result<bool> {
        if o.vaults.iter().any(|v| v.reference.file == file) {
            return Ok(true);
        }
        Ok(self.storage.list()?.iter().any(|n| n == file))
    }

    /// Switches a vault opened with another protection to this suite's password, keys and
    /// recovery code, with a new container key (the file it came from still opens with its old
    /// protection).
    pub(super) fn take_over(&self, o: &Open, c: &mut UnlockedContainer) -> Result<()> {
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
            c.payload.slots.insert(crate::kdf::hex(slot.id()), SlotMeta { label, created, extra: Default::default() });
        }
        if let Some(code) = o.suite.recovery_code() {
            c.set_recovery_code(&code, "", now, &mut OsRng)?;
        }
        c.renew_container_key(&o.password_key, &mut OsRng)?;
        Ok(())
    }

    /// Saves a change of the protection made in `o`: the protection before is noted as an
    /// earlier one, the epoch grows in the suite, every file is written, and only then this
    /// device notes the new epoch (M4: files with an older protection are not opened from now
    /// on) and removes every older copy of the containers (H7, see `purge_old_copies`).
    pub(super) fn commit_protection(&self, o: &mut Open) -> Result<()> {
        // The suite file on the disk still has the protection before (it is saved after this).
        self.note_protection_left(&o.suite.header().suite_id)?;
        o.bump_epoch()?;
        self.save_all(o)?;
        // r11 N3: the change is saved; a note that fails now is made at the next unlock
        // (`open_folder` notes the epoch of the suite it opens).
        if let Err(e) = self.note_epoch(&o.suite.header().suite_id, o.epoch()) {
            eprintln!("GhulSuite: the new epoch could not be noted yet: {e}");
        }
        // The change is saved: tidying up never makes it fail (it is tried again later).
        self.purge_old_copies();
        Ok(())
    }

    /// H7: after a change of the protection, no copy of a container with the protection before
    /// stays on the device: the `.bak` and `.tmp` of the suite file and of every vault file in
    /// the working copy (`write_atomic` keeps the version before as `.bak`), and the copies of a
    /// replaced working copy in the local folder (`replace-old`) unless a replace still needs
    /// them (its journal is there). Called once the new files are written. It never fails the
    /// action it follows: a copy that cannot be removed now (Windows: a virus scanner or a
    /// backup program holds it) is noted ([`PURGE_PENDING`]) and removed at the next unlock or
    /// start ([`Self::retry_purge`]).
    pub(super) fn purge_old_copies(&self) {
        // r12 N1: the copies of a save only while no save runs (it may be copying right now).
        match self.save_lock.try_lock() {
            Ok(_one) => self.purge_copies(true),
            Err(_) => {
                self.purge_copies(false);
                let _ = std::fs::create_dir_all(&self.local_dir);
                let _ = std::fs::write(self.local_dir.join(PURGE_PENDING), b"");
            }
        }
    }

    /// [`Self::purge_old_copies`]; `with_saves`: also the copies of a save (`save_lock` held).
    fn purge_copies(&self, with_saves: bool) {
        let mut left = 0usize;
        let saves = with_saves && !self.save_journal_open();
        if let Some(dir) = self.storage.content_dir().map(PathBuf::from) {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                // r11 M3: the copies of a save as well, unless its journal still needs them.
                let prev = saves && is_prev_copy(&n);
                let stem = n.strip_suffix(".bak").or_else(|| n.strip_suffix(".tmp"));
                let old = stem.is_some_and(|stem| stem == SUITE_FILE || is_vault_file(stem));
                if (prev || old) && e.file_type().is_ok_and(|t| t.is_file()) {
                    #[cfg(test)]
                    if PURGE_FAILS.with(std::cell::Cell::get) {
                        left += 1;
                        continue;
                    }
                    // Only removed, never overwritten in place: a `.bak` may be a hard link to
                    // the live file (a write whose rename failed), and overwriting it would wipe
                    // that file. It is ciphertext; on flash overwriting is no sure wipe anyway.
                    if let Err(err) = std::fs::remove_file(e.path()) {
                        eprintln!("GhulSuite: an older copy could not be removed yet ({n}): {err}");
                        left += 1;
                    }
                }
            }
        }
        self.purge_replace_old();
        let mark = self.local_dir.join(PURGE_PENDING);
        if left > 0 {
            let _ = std::fs::create_dir_all(&self.local_dir);
            let _ = std::fs::write(&mark, b"");
        } else if with_saves {
            // Without the copies of a save looked at, a mark for them stays.
            let _ = std::fs::remove_file(&mark);
        }
    }

    /// Removes the older copies an earlier [`Self::purge_old_copies`] could not remove.
    pub(super) fn retry_purge(&self) {
        if self.local_dir.join(PURGE_PENDING).exists() {
            self.purge_old_copies();
        }
    }

    /// Copies kept for a working copy that is replaced: gone unless its journal still needs them.
    pub(super) fn purge_replace_old(&self) {
        if !self.local_dir.join(super::archive::REPLACE_JOURNAL).exists() {
            let _ = std::fs::remove_dir_all(self.local_dir.join(super::archive::REPLACE_OLD_DIR));
        }
    }

    /// Copies a new factor slot (created in the suite) into every vault and saves all.
    pub(super) fn insert_everywhere(&self, o: &mut Open, slot: Slot, label: &str, now: i64) -> Result<()> {
        for v in &mut o.vaults {
            if let Some(c) = v.container.as_mut() {
                c.insert_slot(slot.clone(), label, now)?;
            }
        }
        self.commit_protection(o)
    }

    /// A change of the protection of the open suite (M4). `f` checks first and sets `began`
    /// before it changes anything in memory; if it fails after that (e.g. a file cannot be
    /// written), the suite is locked at once: neither the new protection nor a mix of both stays
    /// in memory for the next change to write. The next unlock reads what is on the disk.
    pub(super) fn change_protection<T>(&self, f: impl FnOnce(&mut Open, &mut bool) -> Result<T>) -> Result<T> {
        let mut guard = self.guard();
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
        let mut began = false;
        let result = match guard.as_mut() {
            None => Err(GhulError::Locked),
            Some(o) => f(o, &mut began),
        };
        if result.is_err() && began {
            *guard = None;
            *self.factors_confirmed.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        if self.lock_pending.swap(false, Ordering::SeqCst) {
            *guard = None;
        }
        result
    }
}

impl GhulSuite {
    /// Opens everything with the recovery code and writes it again with `new_password` (see
    /// `unlock_with_recovery_code`); not kept yet.
    pub(super) fn open_with_code(&self, code: String, new_password: String, changes: u64) -> Result<Open> {
        let code = Zeroizing::new(code);
        let new_password = Zeroizing::new(new_password);
        let code = RecoveryCode::parse(&code)?;
        check_master_password(&new_password)?;
        let anchor = self.anchor()?.recover(&code)?;
        let mut kdf = anchor.header().kdf.clone();
        OsRng.fill_bytes(&mut kdf.salt);
        let pk = PasswordKey::derive(&new_password, &kdf)?;
        let suite = self.suite_from_anchor(anchor, &pk)?;
        let vaults = self.scan(&Opener::Code(&code))?;
        let mut open =
            Open { password_key: PasswordKey::from_bytes(*pk.expose()), suite, vaults, recovered: true, changes };
        open.suite.rekey(&pk, kdf.clone(), &mut OsRng)?;
        open.harmonize(now_ms())?;
        for (c, _) in open.containers_mut() {
            c.rekey(&pk, kdf.clone(), &mut OsRng)?;
        }
        open.renew_keys(false, now_ms())?;
        self.commit_protection(&mut open)?;
        super::quick::delete_all(&self.local_dir);
        super::quick::refresh_stamps(&self.local_dir, open.password_key.expose(), now_ms());
        self.docs_tidy(&open);
        Ok(open)
    }
}

#[uniffi::export]
impl GhulSuite {
    /// `storage`: the suite's folder. `local_dir`: private app directory for device-bound data
    /// (quick unlock).
    #[uniffi::constructor]
    pub fn new(storage: Arc<dyn VaultStorage>, local_dir: String) -> Arc<Self> {
        let suite = Arc::new(GhulSuite {
            storage,
            local_dir: PathBuf::from(local_dir),
            open: Mutex::new(None),
            lock_pending: AtomicBool::new(false),
            lock_gen: std::sync::atomic::AtomicU64::new(0),
            seen_lock: Mutex::new(()),
            replace_lock: Mutex::new(()),
            save_lock: Mutex::new(()),
            protection_changes: std::sync::atomic::AtomicU64::new(0),
            zones: Mutex::new(None),
            pending: Mutex::new(None),
            changes: std::sync::atomic::AtomicU64::new(0),
            content_changes: std::sync::atomic::AtomicU64::new(0),
            pending_gen: std::sync::atomic::AtomicU64::new(0),
            wiped: AtomicBool::new(false),
            docs_limit: std::sync::atomic::AtomicU64::new(super::docs::DOCS_LIMIT),
            content_copies: Mutex::new(Default::default()),
            built: Mutex::new(None),
            unsaved_lock: Mutex::new(()),
            docs_adds: std::sync::atomic::AtomicUsize::new(0),
            factors_confirmed: Mutex::new(None),
        });
        // A suite file that replaced the working copy when the device went off: the state
        // before comes back (complete), before anything else looks at the folders.
        if let Err(e) = suite.recover_replace() {
            eprintln!("GhulSuite: the working copy could not be put back after a stop: {e}");
        }
        // Older copies an earlier change could not remove (H7), old documents of a replace that
        // ended (round 5, M2).
        suite.retry_purge();
        suite.tidy_asides();
        suite
    }

    /// Whether the folder contains a suite (settings file or any vault file).
    pub fn exists(&self) -> Result<bool> {
        Ok(self.storage.read(SUITE_FILE.to_owned())?.is_some() || !self.vault_files()?.is_empty())
    }

    pub fn storage_name(&self) -> String {
        self.storage.describe()
    }

    pub fn is_unlocked(&self) -> bool {
        if self.lock_pending.load(Ordering::SeqCst) {
            return false;
        }
        match self.open.try_lock() {
            Ok(g) => g.is_some(),
            Err(TryLockError::Poisoned(p)) => self.poisoned(p.into_inner()).is_some(),
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
        let gen = self.lock_gen_now();
        let password = Zeroizing::new(options.password.clone());
        let pin = pin.map(Zeroizing::new);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        check_master_password(&password)?;
        // A journal of a replace of a suite that is gone (round 6, M1): it must never put that
        // suite back over the new one.
        self.discard_journal();
        let keyfile = parse_keyfile(options.keyfile.clone())?;
        let cost = KdfCost { memory_kib: options.memory_kib, iterations: options.iterations, parallelism: 1 };
        let keys = SuiteKeys::create(&password, cost, &mut OsRng)?;
        let mut factor: Vec<(Slot, &str)> = Vec::new();
        match (&transport, &pin) {
            (Some(t), Some(pin)) => {
                let (cred_id, prf_input, output) =
                    with_authenticator(t, listener.as_ref(), |a| register_key(a, pin, &[]))?;
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
        // A key alone still needs its backup (added next, see `add_security_key`).
        let backup_follows = factor.len() == 1 && matches!(factor[0].0, Slot::Fido2(_));
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
            recovered: false,
            changes: gen.changes,
        };
        for v in &options.vaults {
            self.add_vault(&mut open, v.kind, &v.name)?;
        }
        self.save_all(&mut open)?;
        self.set_open(open, gen)?;
        // Both factors were just used: the backup of a key may follow with the password alone.
        self.set_ticket(backup_follows.then(std::time::Instant::now), gen)?;
        Ok(SetupResult { recovery_code: Some(code.to_display().to_string()) })
    }

    /// Starts a suite in this (empty) folder from a single vault file, opened with its own
    /// password plus key or key file. The vault keeps its password and second factor; it always
    /// gets a new recovery code (returned) and a new container key (r9 M-D1).
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
        let gen = self.lock_gen_now();
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        // As in `create`: journals of a suite gone before never act on this one (r11 H1).
        self.discard_journal();
        let locked = LockedContainer::parse(&vault_file)?;
        let kind = VaultKind::from_container(locked.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "this is a suite file, open its folder instead".into() })?;
        refuse_docs_file(kind)?;
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
        // r9 M-D1: always a new recovery code and a new container key. The file may come from
        // someone else (a vault passed on): the code of the one who made it must not open the
        // new suite and what is added to it later, also not with an old copy of the file.
        let (_, code) = vault.add_recovery_slot(&recovery_label, now_ms(), &mut OsRng)?;
        vault.renew_container_key(&pk, &mut OsRng)?;
        let new_code = Some(code.to_display().to_string());
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
            vaults: vec![OpenVault { reference, container: Some(vault) }],
            recovered: false,
            changes: gen.changes,
        };
        self.save_all(&mut open)?;
        self.set_open(open, gen)?;
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
        let gen = self.lock_gen_now();
        self.finish_earlier_replace()?;
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
        self.finish_unlock(pk, suite, true, gen)
    }

    /// Emergency: opens everything with the recovery code alone and sets a new master password.
    /// Keys, key files and the recovery code stay valid. Until the next lock, a change of the
    /// protection needs the new master password alone (see `opened_with_recovery_code`).
    pub fn unlock_with_recovery_code(&self, code: String, new_password: String) -> Result<()> {
        let gen = self.lock_gen_now();
        self.finish_earlier_replace()?;
        let open = self.open_with_code(code, new_password, gen.changes)?;
        self.set_open(open, gen)
    }

    /// Whether the open suite was opened with the recovery code (since the last lock): a change
    /// of the protection then takes the master password alone as proof, so the app asks only
    /// for it (H3). False while locked.
    pub fn opened_with_recovery_code(&self) -> bool {
        self.with_open(|o| Ok(o.recovered)).unwrap_or(false)
    }

    /// Locks immediately; all secrets are wiped from memory.
    /// Locks at once, also while another call (e.g. a slow import) still works with the open
    /// suite: the caller (the app's main thread) never waits for it (F92). The suite counts as
    /// locked immediately; its keys and decrypted data are dropped right away or, if that call
    /// still holds them, as soon as it ends. Callers then run `finish_lock` on a background
    /// thread: it covers a call ending in the same moment.
    pub fn lock(&self) {
        // Counted before the lock is marked (see `set_open`).
        self.lock_gen.fetch_add(1, Ordering::SeqCst);
        self.lock_now();
    }

    /// After `lock`, on a background thread: waits until a call still holding the open suite has
    /// ended, and makes sure it is gone (unless unlocked again since).
    pub fn finish_lock(&self) {
        self.drop_open(true);
    }

    // --- quick unlock ------------------------------------------------------------------------

    /// Enables quick unlock with the fingerprint (`name` `bio`; the app PIN has
    /// [`Self::quick_unlock_enable_pin`]) for `max_age_seconds` after each full unlock; returns
    /// the quick key for the platform to protect. Like a change of the protection it needs both
    /// factors (`proof`, or the password alone right after `confirm_factors`, H6). The validity
    /// is stored inside the quick unlock file, under its encryption: it can only be changed by
    /// enabling quick unlock again.
    pub fn quick_unlock_enable(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        name: String,
        max_age_seconds: u64,
    ) -> Result<Vec<u8>> {
        if max_age_seconds == 0 {
            return Err(GhulError::InvalidInput { msg: "quick unlock needs a validity".into() });
        }
        // Checked before a key is touched.
        super::quick::path(&self.local_dir, &name)?;
        if name == super::quick::PIN_NAME {
            return Err(GhulError::InvalidInput { msg: super::quick::PIN_HAS_SLOTS.into() });
        }
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        self.with_open(|o| {
            // The device's quick unlock belongs to the own suite.
            super::single::refuse_single(o)?;
            let qk = super::quick::create(
                &self.local_dir,
                &name,
                &o.suite.header().suite_id,
                o.password_key.expose(),
                o.suite.factor_secret().expose(),
                now_ms(),
                (max_age_seconds.min(i64::MAX as u64 / 1000) * 1000) as i64,
            )?;
            Ok(qk.to_vec())
        })
    }

    /// Enables the app PIN (M19; see `quick.rs`): at least 8 digits, valid `max_age_seconds`
    /// after each full unlock, both factors as for `quick_unlock_enable`. `slot_keys`: two new
    /// random keys of 32 bytes made by the app; after this call returned the app imports them
    /// into the Android Keystore as single-use HMAC-SHA256 keys (`setMaxUsageCount(1)`) under the
    /// aliases of the returned generation and slots 0 and 1, then zeroes them. Returns the
    /// generation.
    #[allow(clippy::too_many_arguments)]
    pub fn quick_unlock_enable_pin(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: String,
        max_age_seconds: u64,
        slot_keys: Vec<Vec<u8>>,
    ) -> Result<u32> {
        let pin = Zeroizing::new(pin);
        if max_age_seconds == 0 {
            return Err(GhulError::InvalidInput { msg: "quick unlock needs a validity".into() });
        }
        super::quick::check_pin(&pin)?;
        let keys = slot_keys_of(slot_keys)?;
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        let (salt, input) = super::quick::new_pin_input(&pin)?;
        self.with_open(|o| {
            super::single::refuse_single(o)?;
            let suite_id = o.suite.header().suite_id;
            // A generation the aliases of an earlier PIN never had.
            let generation = super::quick::read_pin_header(&self.local_dir, &suite_id)
                .map_or(1, |h| h.generation.wrapping_add(1).max(1));
            let header = super::quick::PinHeader {
                salt,
                created_ms: now_ms(),
                max_age_ms: (max_age_seconds.min(i64::MAX as u64 / 1000) * 1000) as i64,
                generation,
            };
            super::quick::write_pin(
                &self.local_dir,
                &suite_id,
                o.password_key.expose(),
                o.suite.factor_secret().expose(),
                &input,
                &header,
                &keys,
            )?;
            Ok(generation)
        })
    }

    /// The generation of the app PIN's file. The app deletes every Keystore key of another
    /// generation before an attempt (and at the start).
    /// - `Ok(None)`: there is no PIN file (or no suite): no PIN set up, turn it off in the app.
    /// - `Ok(Some(g))`: the file of generation `g`.
    /// - `Err(QuickUnlockOutdated)`: a PIN file of before version 4 (deleted): set up again.
    /// - `Err(QuickUnlockUnavailable)`: a damaged file or one of another suite (deleted).
    /// - `Err(Storage)` (or another error reading the suite file): could not be read now; keep
    ///   the PIN and its keys, try again later (a full unlock is still possible).
    pub fn quick_pin_generation(&self) -> Result<Option<u32>> {
        let Some(locked) = self.read_locked(SUITE_FILE)? else { return Ok(None) };
        super::quick::pin_generation(&self.local_dir, &locked.header().suite_id)
    }

    /// The first step of an attempt with the app PIN: what the Keystore key of the slot signs
    /// (HMAC-SHA256), `message` = "ghul/v4/quick-pin-tag" || Argon2id(PIN). Refused before any
    /// key is used if the PIN quick unlock is expired (`QuickUnlockExpired`), outdated
    /// (`QuickUnlockOutdated`, a file of version 3: deleted) or not there.
    pub fn quick_pin_challenge(&self, pin: String) -> Result<QuickPinChallenge> {
        let pin = Zeroizing::new(pin);
        self.finish_earlier_replace()?;
        let locked = self.read_locked(SUITE_FILE)?.ok_or(GhulError::QuickUnlockUnavailable)?;
        let header = super::quick::read_pin_header(&self.local_dir, &locked.header().suite_id)?;
        if super::quick::pin_expired_by_header(&self.local_dir, &header, now_ms()) {
            return Err(GhulError::QuickUnlockExpired);
        }
        let input = super::quick::pin_challenge(&pin, &header)?;
        let mut message = super::quick::PIN_TAG_INFO.to_vec();
        message.extend_from_slice(input.as_ref());
        Ok(QuickPinChallenge { generation: header.generation, message })
    }

    /// The second step: unlocks with the tag the Keystore key of `slot` (0 or 1) of the
    /// challenge's generation computed over `message` (from `quick_pin_challenge`). A wrong PIN
    /// is `QuickUnlockInvalid` and changes nothing (that slot's key is used up). A right PIN
    /// seals the file anew for `next_slot_keys` (two new keys the app imported before the call
    /// as the next generation) with the same validity, removes the copy before, and then
    /// unlocks. Returns the generation the file has now: the next one (the app deletes the keys
    /// of the old generation), or, if sealing anew failed (e.g. a full storage, the suite is
    /// open all the same), the old one (the app deletes the new keys and keeps the old slot it
    /// has left).
    pub fn unlock_quick_pin(
        &self,
        slot: u32,
        tag: Vec<u8>,
        message: Vec<u8>,
        next_slot_keys: Vec<Vec<u8>>,
    ) -> Result<u32> {
        let gen = self.lock_gen_now();
        let tag = Zeroizing::new(tag);
        let message = Zeroizing::new(message);
        let keys = slot_keys_of(next_slot_keys)?;
        let input: [u8; 32] = message
            .strip_prefix(super::quick::PIN_TAG_INFO)
            .and_then(|p| p.try_into().ok())
            .ok_or_else(|| GhulError::InvalidInput { msg: "not a challenge of the app PIN".into() })?;
        let input = Zeroizing::new(input);
        self.finish_earlier_replace()?;
        let locked = self.read_locked(SUITE_FILE)?.ok_or(GhulError::QuickUnlockUnavailable)?;
        let suite_id = locked.header().suite_id;
        let (secret, header) = super::quick::open_pin(&self.local_dir, &suite_id, slot, &tag, now_ms())?;
        let pk = PasswordKey::from_bytes(*secret.password_key);
        let suite = match locked.unlock(&pk, &FactorSecret::from_bytes(*secret.factor_secret)) {
            Ok(s) => s,
            Err(_) => {
                super::quick::delete_all(&self.local_dir);
                return Err(GhulError::QuickUnlockUnavailable);
            }
        };
        // Sealed anew before the suite opens: a failure of the unlock afterwards (a full
        // storage) costs no attempt. Same salt, same validity, the next generation.
        let next = super::quick::PinHeader { generation: header.generation.wrapping_add(1).max(1), ..header };
        let now = match super::quick::write_pin(
            &self.local_dir,
            &suite_id,
            &secret.password_key,
            &secret.factor_secret,
            &input,
            &next,
            &keys,
        ) {
            Ok(()) => next.generation,
            Err(e) => {
                eprintln!("GhulSuite: the app PIN could not be sealed anew: {e}");
                header.generation
            }
        };
        self.finish_unlock(pk, suite, false, gen)?;
        Ok(now)
    }

    pub fn quick_unlock_exists(&self, name: String) -> bool {
        super::quick::path(&self.local_dir, &name).map(|p| p.exists()).unwrap_or(false)
    }

    /// Unlocks with the fingerprint's quick unlock (`bio`; the app PIN: `unlock_quick_pin`),
    /// within the validity chosen when it was enabled (counted from the last full unlock).
    pub fn unlock_quick(&self, name: String, quick_key: Vec<u8>) -> Result<()> {
        let gen = self.lock_gen_now();
        self.finish_earlier_replace()?;
        let quick_key = Zeroizing::new(quick_key);
        let locked = self.read_locked(SUITE_FILE)?.ok_or(GhulError::QuickUnlockUnavailable)?;
        let secret = super::quick::open_file(&self.local_dir, &name, &locked.header().suite_id, &quick_key, now_ms())?;
        let pk = PasswordKey::from_bytes(*secret.password_key);
        let suite = match locked.unlock(&pk, &FactorSecret::from_bytes(*secret.factor_secret)) {
            Ok(s) => s,
            Err(_) => {
                super::quick::delete_all(&self.local_dir);
                return Err(GhulError::QuickUnlockUnavailable);
            }
        };
        self.finish_unlock(pk, suite, false, gen)
    }

    pub fn quick_unlock_disable(&self, name: String) -> Result<()> {
        let p = super::quick::path(&self.local_dir, &name)?;
        // Also the copies of an earlier write (`.bak`, `.tmp`): they hold the same keys.
        for f in [p.clone(), p.with_extension("ghlq.bak"), p.with_extension("ghlq.tmp")] {
            if f.exists() {
                super::quick::wipe(&f)?;
            }
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
                        .map(|c| match VaultKind::from_name(&v.reference.kind) {
                            VaultKind::Docs => super::docs::Usage::of(&c.payload.entries).files as u32,
                            _ => c.payload.entries.iter().filter(|e| !e.deleted).count() as u32,
                        })
                        .unwrap_or(0),
                })
                .collect())
        })
    }

    pub fn create_vault(&self, kind: VaultKind, name: String) -> Result<String> {
        if name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        self.with_open(|o| {
            super::single::refuse_single(o)?;
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
            super::single::refuse_single(o)?;
            let pos = o.vaults.iter().position(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
            let file = o.vaults[pos].reference.file.clone();
            self.note_content_change();
            self.mark_unsaved(&["suite"])?;
            // M8: the vault leaves the open suite only once its file is gone; if that fails it
            // stays listed (and goes into the next suite file), as on the disk.
            self.storage.delete(file).map_err(|e| match GhulError::from(e) {
                full @ GhulError::NoSpace { .. } => full,
                other => {
                    eprintln!("GhulSuite: a vault file could not be deleted: {other}");
                    GhulError::Storage { msg: VAULT_NOT_REMOVED.into() }
                }
            })?;
            let v = o.vaults.remove(pos);
            // The content files of a GhulDocs vault go with it (what is left goes at the next unlock).
            if v.reference.kind == "docs" {
                if let Ok(dir) = self.docs_dir(&v.reference.id) {
                    self.remove_contents(vec![dir]);
                }
            }
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    /// Current file of a vault (for export / backup). A GhulDocs vault goes with its content
    /// files: `docs_export_vault`.
    pub fn export_vault(&self, id: String) -> Result<Vec<u8>> {
        let file = self.with_open(|o| {
            let v = o.vaults.iter().find(|v| v.reference.id == id).ok_or(GhulError::NotFound)?;
            refuse_docs_file(VaultKind::from_name(&v.reference.kind))?;
            Ok(v.reference.file.clone())
        })?;
        self.storage.read(file)?.ok_or(GhulError::NotFound)
    }

    /// Adds a vault file (e.g. picked by the user) that already has this suite's protection.
    /// Returns `DifferentProtection` if it needs its own password (then use `adopt_vault`).
    pub fn add_vault_file(&self, data: Vec<u8>, name: String) -> Result<String> {
        let data = Zeroizing::new(data);
        self.with_open(|o| {
            super::single::refuse_single(o)?;
            let locked = LockedContainer::parse(&data)?;
            let kind = VaultKind::from_container(locked.kind())
                .ok_or_else(|| GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() })?;
            refuse_docs_file(kind)?;
            if locked.header().suite_id != o.suite.header().suite_id || locked.header().kdf != o.kdf() {
                return Err(GhulError::DifferentProtection);
            }
            let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let c = locked.unlock(&o.password_key, &s).map_err(|_| GhulError::DifferentProtection)?;
            self.insert_vault(o, c, &name)
        })
    }

    /// Takes over a vault with another protection: it is opened once with its own password plus
    /// key or key file and then switched to this suite's password, keys and recovery code.
    #[allow(clippy::too_many_arguments)]
    pub fn adopt_vault(
        &self,
        data: Vec<u8>,
        name: String,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
    ) -> Result<String> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let data = Zeroizing::new(data);
        let locked = LockedContainer::parse(&data)?;
        let kind = VaultKind::from_container(locked.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "a suite file cannot be added as a vault".into() })?;
        refuse_docs_file(kind)?;
        // Refused before the key is touched or the password derived.
        self.with_open(|o| super::single::refuse_single(o))?;
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
            super::single::refuse_single(o)?;
            self.take_over(o, &mut c)?;
            self.insert_vault(o, c, &name)
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
            let (_, c) = o.entry_vault_mut(&vault_id)?;
            let e = c.payload.entries.iter().find(|e| e.id == id && !e.deleted).ok_or(GhulError::NotFound)?;
            Ok(EntryData {
                id: e.id.clone(),
                entry_type: e.entry_type.clone(),
                fields: e
                    .fields
                    .iter()
                    .filter_map(|(k, v)| to_field_value(v).map(|value| FieldData { name: k.clone(), value }))
                    .collect(),
                placement: Some(EntryPlacement { tags: e.tags.clone(), folder: e.folder.clone() }),
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
            let (_, c) = o.entry_vault_mut(&vault_id)?;
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
            // r10 M5: through `add_entries` (limits, and the entry goes again on an error).
            self.add_entries(o, std::slice::from_ref(&vault_id), |o| {
                o.vault_mut(&vault_id)?.1.payload.entries.push(e);
                Ok(id)
            })
        })
    }

    pub fn rename_totp(&self, vault_id: String, id: Vec<u8>, issuer: String, account: String) -> Result<()> {
        self.with_open(|o| {
            let (r, c) = o.entry_vault_mut(&vault_id)?;
            if r.kind != "totp" {
                return Err(GhulError::InvalidInput { msg: "not a GhulAuth vault".into() });
            }
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
            let targets: Vec<String> = [&pass_vault, &totp_vault].into_iter().flatten().cloned().collect();
            self.add_entries(o, &targets, |o| {
                let now = now_ms();
                let mut result = ImportResult {
                    entries: 0,
                    totps: 0,
                    duplicates: 0,
                    skipped: 0,
                    skipped_items: Vec::new(),
                    kept: 0,
                };
                if let Some(vid) = &pass_vault {
                    let (r, c) = o.vault_mut(vid)?;
                    if r.kind != "pass" {
                        return Err(GhulError::InvalidInput { msg: "not a GhulPass vault".into() });
                    }
                    // N-D1: a set (a list made a large file take hours).
                    let mut seen: std::collections::HashSet<String> = c
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
                        seen.insert(key);
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
                Ok(result)
            })
        })
    }

    /// Unencrypted export of one vault: GhulPass as CSV, GhulAuth as `otpauth://` links (one per
    /// line). Requires the master password, because the result is readable by anyone.
    /// `spreadsheet_safe`: the CSV is for a spreadsheet program (cells that would run as a
    /// formula get an apostrophe); without it the values go as they are, for another password
    /// manager (M29). The own import reads both.
    pub fn export_plain(&self, vault_id: String, password: String, spreadsheet_safe: bool) -> Result<Vec<u8>> {
        if !self.verify_password(password)? {
            return Err(GhulError::WrongPassword);
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            refuse_docs_file(VaultKind::from_name(&r.kind))?;
            let text = if r.kind == "pass" {
                super::import::export_csv(&c.payload.entries, spreadsheet_safe)
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
    /// export, only with the master password; `spreadsheet_safe` as there.
    pub fn export_entries(
        &self,
        vault_id: String,
        ids: Vec<Vec<u8>>,
        password: String,
        spreadsheet_safe: bool,
    ) -> Result<Vec<u8>> {
        if !self.verify_password(password)? {
            return Err(GhulError::WrongPassword);
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&vault_id)?;
            refuse_docs_file(VaultKind::from_name(&r.kind))?;
            let picked: Vec<Entry> =
                c.payload.entries.iter().filter(|e| !e.deleted && ids.contains(&e.id)).cloned().collect();
            if picked.is_empty() {
                return Err(GhulError::NotFound);
            }
            let text = if r.kind == "pass" {
                super::import::export_csv(&picked, spreadsheet_safe)
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
                    }
                })
                .collect())
        })
    }

    /// Registers a security key for all files (two touches). Nothing else changes: other keys,
    /// key files and the recovery code stay valid.
    /// `proof`: both factors of now; a key of now is touched through `proof_transport` first (in
    /// the app the same USB or NFC connection: one key after the other), then the new one.
    #[allow(clippy::too_many_arguments)]
    pub fn add_security_key(
        &self,
        proof: FactorProof,
        proof_transport: Option<Arc<dyn HidTransport>>,
        transport: Arc<dyn HidTransport>,
        listener: Arc<dyn FidoListener>,
        pin: String,
        label: String,
    ) -> Result<()> {
        let pin = Zeroizing::new(pin);
        // Refused before the new key is touched.
        self.require_factors(&proof, proof_transport.as_ref(), Some(&listener))?;
        // A key of this suite refuses (registered already: it would be no backup).
        let known: Vec<Vec<u8>> = self.with_open(|o| {
            Ok(o.suite
                .slots()
                .iter()
                .filter_map(|s| if let Slot::Fido2(f) = s { Some(f.credential_id.clone()) } else { None })
                .collect())
        })?;
        let (cred_id, prf_input, output) =
            with_authenticator(&transport, Some(&listener), |a| register_key(a, &pin, &known))?;
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            let reg = Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output };
            let now = now_ms();
            *began = true;
            let slot = o.suite.add_fido2_slot(&reg, &label, now, &mut OsRng)?;
            self.insert_everywhere(o, slot, &label, now)
        })
    }

    /// Adds a key file (from `generate_keyfile`) as second factor for all files.
    pub fn add_keyfile(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        keyfile: Vec<u8>,
        label: String,
    ) -> Result<()> {
        let keyfile = Keyfile::parse(&Zeroizing::new(keyfile))?;
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            let id = keyfile.id();
            if o.suite.slots().iter().any(|s| matches!(s, Slot::Keyfile(k) if k.keyfile_id == id)) {
                return Err(GhulError::AlreadyExists);
            }
            let now = now_ms();
            *began = true;
            let slot = o.suite.add_keyfile_slot(&keyfile, &label, now, &mut OsRng)?;
            self.insert_everywhere(o, slot, &label, now)
        })
    }

    /// Creates a new recovery code for all files; the previous code stops working.
    pub fn new_recovery_code(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        label: String,
    ) -> Result<String> {
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            let code = RecoveryCode::generate(&mut OsRng);
            let now = now_ms();
            *began = true;
            for (c, _) in o.containers_mut() {
                c.set_recovery_code(&code, &label, now, &mut OsRng)?;
            }
            // The old code with an old copy of a file must not open what is written now.
            o.renew_keys(false, now)?;
            self.commit_protection(o)?;
            Ok(code.to_display().to_string())
        })
    }

    /// Removes a security key or key file from all files. The last one cannot be removed (a
    /// second factor is always required). Recovery codes are replaced, not removed.
    pub fn remove_slot(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        slot_id: Vec<u8>,
    ) -> Result<()> {
        let id: [u8; 16] =
            slot_id.as_slice().try_into().map_err(|_| GhulError::InvalidInput { msg: "invalid slot id".into() })?;
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            let slot = o.suite.slots().iter().find(|s| *s.id() == id).cloned().ok_or(GhulError::NotFound)?;
            if !slot.provides_factor() {
                return Err(GhulError::InvalidInput { msg: "recovery codes are replaced, not removed".into() });
            }
            if o.suite.slots().iter().filter(|s| s.provides_factor()).count() <= 1 {
                return Err(GhulError::LastFactor);
            }
            // A security key keeps a backup: a second key or a key file.
            let left: Vec<&Slot> = o.suite.slots().iter().filter(|s| s.provides_factor() && *s.id() != id).collect();
            if left.len() < 2 && left.iter().any(|s| matches!(s, Slot::Fido2(_))) {
                return Err(GhulError::BackupFactorRequired);
            }
            *began = true;
            for (c, _) in o.containers_mut() {
                match c.remove_slot(&id) {
                    Ok(()) | Err(crate::vault::VaultError::SlotNotFound) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            // The removed key with an old copy of a file must not open what is written now:
            // new factor secret for the remaining keys and key files, new container keys.
            o.renew_keys(true, now_ms())?;
            self.commit_protection(o)?;
            super::quick::delete_all(&self.local_dir);
            Ok(())
        })
    }

    /// Changes the master password (new salt) for all files. Both factors of now must be given
    /// (`proof`: the current password plus a key or key file): who knows the password and finds
    /// the app unlocked cannot lock the owner out.
    pub fn change_password(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        new_password: String,
    ) -> Result<()> {
        let new_password = Zeroizing::new(new_password);
        // Checked before a key is touched.
        check_master_password(&new_password)?;
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        let current = Zeroizing::new(proof.password.clone());
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            let kdf = o.kdf();
            let old_pk = PasswordKey::derive(&current, &kdf)?;
            if !o.suite.verify_password_key(&old_pk) {
                return Err(GhulError::WrongPassword);
            }
            let mut new_kdf = kdf;
            OsRng.fill_bytes(&mut new_kdf.salt);
            let new_pk = PasswordKey::derive(&new_password, &new_kdf)?;
            *began = true;
            for (c, _) in o.containers_mut() {
                c.rekey(&new_pk, new_kdf.clone(), &mut OsRng)?;
            }
            o.password_key = new_pk;
            // The old password with an old copy of a file must not open what is written now.
            o.renew_keys(false, now_ms())?;
            self.commit_protection(o)?;
            super::quick::delete_all(&self.local_dir);
            Ok(())
        })
    }

    /// Both factors of now, shown before the next change of the protection (a security key is
    /// touched here; the change itself then needs the password alone, once, within two minutes).
    /// For a new key on a phone: the key of now goes out before the new one comes in.
    pub fn confirm_factors(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
    ) -> Result<()> {
        let gen = self.lock_gen_now();
        // Always both factors: a confirmation is not carried on by the password alone.
        *self.factors_confirmed.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.require_factors(&proof, transport.as_ref(), listener.as_ref())?;
        // Not kept if the suite was locked meanwhile (H10).
        self.set_ticket(Some(std::time::Instant::now()), gen)
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

impl GhulSuite {
    /// Both factors, as for unlocking (password plus security key or key file), before a change
    /// of the protection: who finds the app unlocked must not give himself lasting access (a
    /// recovery code, a key, a key file) nor lock the owner out. Checked against the suite file
    /// of the working copy, without holding the open suite.
    pub(super) fn require_factors(
        &self,
        proof: &FactorProof,
        transport: Option<&Arc<dyn HidTransport>>,
        listener: Option<&Arc<dyn FidoListener>>,
    ) -> Result<()> {
        let recovered = self.with_open(|o| {
            super::single::refuse_single(o)?;
            Ok(o.recovered)
        })?;
        if proof.pin.is_none() && proof.keyfile.is_none() {
            // Opened with the recovery code (H3): the new master password alone, until the lock.
            if recovered {
                return self.require_password(&Zeroizing::new(proof.password.clone()));
            }
            // Confirmed a moment ago (`confirm_factors`): the password alone, once.
            let confirmed = self.factors_confirmed.lock().unwrap_or_else(|e| e.into_inner()).take();
            if confirmed.is_some_and(|t| ticket_age(t) < TICKET_VALIDITY) {
                return self.require_password(&Zeroizing::new(proof.password.clone()));
            }
        }
        let locked = self.anchor()?;
        let keyfile = parse_keyfile(proof.keyfile.clone())?;
        let pin = proof.pin.clone().map(Zeroizing::new);
        let s = second_factor(&locked, transport, listener, pin.as_deref().map(|p| p.as_str()), keyfile.as_ref())?;
        let pk = PasswordKey::derive(&Zeroizing::new(proof.password.clone()), &locked.header().kdf)?;
        locked.unlock(&pk, &s).map(|_| ())?;
        Ok(())
    }

    /// Keeps (or clears) the confirmation of both factors, unless the suite was locked since the
    /// call started (`gen`): then it is not kept and the call ends with `Locked` (H10).
    pub(super) fn set_ticket(&self, t: Option<std::time::Instant>, gen: Gen) -> Result<()> {
        #[cfg(test)]
        run_lock_hook();
        let mut ticket = self.factors_confirmed.lock().unwrap_or_else(|e| e.into_inner());
        if self.lock_gen.load(Ordering::SeqCst) != gen.lock {
            *ticket = None;
            return Err(GhulError::Locked);
        }
        *ticket = t;
        Ok(())
    }

    /// The master password, before a change of the protection or of quick unlock: who finds the
    /// app unlocked must not give himself lasting access (a recovery code, a key, an app PIN)
    /// nor lock the owner out. Derived without holding the open suite.
    pub(super) fn require_password(&self, password: &str) -> Result<()> {
        let kdf = self.with_open(|o| Ok(o.kdf()))?;
        let pk = PasswordKey::derive(password, &kdf)?;
        self.with_open(|o| if o.suite.verify_password_key(&pk) { Ok(()) } else { Err(GhulError::WrongPassword) })
    }
}
