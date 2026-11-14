// SPDX-License-Identifier: AGPL-3.0-or-later
//! The encrypted mail archive (KONZEPT 9.3.1, FORMAT 13): mails older than a chosen number of
//! days are copied from the provider to the user's own server, encrypted on the device, checked
//! by reading them back, and only then (if the user switched it on) deleted at the provider.
//!
//! ```text
//! <base>/mailarchive/head.ghlarc                 "GHULARH1" ‖ 0u32 ‖ nonce[24] ‖ AEAD(head key, CBOR head)
//! <base>/mailarchive/a/<account>/m/<month>       "GHULARM1" ‖ epoch ‖ nonce ‖ AEAD(index key(epoch), padded CBOR month)
//! <base>/mailarchive/a/<account>/state           "GHULARS1" ‖ epoch ‖ nonce ‖ AEAD(index key(epoch), CBOR state)
//! <base>/mailarchive/a/<account>/lock            "GHULARL1" ‖ epoch ‖ nonce ‖ AEAD(index key(epoch), CBOR lock)
//! <base>/mailarchive/b/<ab>/<block id>           "GHULARB1" ‖ epoch ‖ nonce ‖ AEAD(block key(epoch), block)
//!
//! K_e          = random 256 bit per key epoch, kept in the Mail file's encrypted settings
//! index key(e) = HKDF(K_e, "ghul/v1/mailarchive/index"), block key(e) = HKDF(K_e, ".../block")
//! names, block ids and the head use keys from the first epoch, so they never change:
//! <account>    = hex(HMAC(HKDF(K_0, ".../name"), "account:" ‖ lower-case address))[..32]
//! <month>      = hex(HMAC(name key, <account> ‖ "/" ‖ "YYYY-MM"))[..32]
//! block id     = hex(HMAC(HKDF(K_0, ".../block-id"), block)); a mail's key is the same over all of it
//! ```
//!
//! The additional data of every AEAD is magic ‖ epoch ‖ the file's path inside `mailarchive/`, so
//! a file cannot be passed off as another. A new epoch starts after every change of the suite's
//! protection: an old backup together with an old factor opens only what was archived until then.
//!
//! Safety rules: a mail counts as archived only after its blocks and its index entry were read
//! back over a new connection and matched; the provider deletes only exactly these UIDs (never
//! "everything flagged"), only while the folder keeps its UIDVALIDITY, only when the user switched
//! deleting on, and never in the first run before the user confirmed it once. One device at a time
//! archives an account (lock in the archive).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use super::areas::{area_setting, put_area_setting};
use super::error::{GhulError, Result};
use super::mail::{MailMessage, MailSummary};
use super::servers::{self, ServerInfo, ServerInput, ServerSaved};
use super::suite::{now_ms, GhulSuite, Open, VaultKind};
use crate::net::imap::{self, Archivable, Folder};
use crate::net::remote::RemoteFs;
use crate::vault::keys::{hkdf, open, seal};
use crate::vault::payload::{pad, unpad};

const SETTINGS_KEY: &str = "mail_archive";
const ROOT: &str = "mailarchive";
const HEAD: &str = "mailarchive/head.ghlarc";
const HEAD_MAGIC: &[u8; 8] = b"GHULARH1";
const MONTH_MAGIC: &[u8; 8] = b"GHULARM1";
const BLOCK_MAGIC: &[u8; 8] = b"GHULARB1";
const STATE_MAGIC: &[u8; 8] = b"GHULARS1";
const LOCK_MAGIC: &[u8; 8] = b"GHULARL1";
const GC_MAGIC: &[u8; 8] = b"GHULARG1";
/// Days until archiving, preselected.
pub const DEFAULT_DAYS: u32 = 30;
/// Largest block of a mail.
const BLOCK: usize = 4 * 1024 * 1024;
/// Mails per step: each step is finished (uploaded, checked, in the index) on its own.
const STEP: usize = 100;
/// Mails larger than this are not archived: they stay at the provider, untouched (and are never
/// deleted there). Fetching one would need that much memory at once; mail providers rarely accept
/// mails of this size anyway.
pub const MAX_MAIL: u64 = 100 * 1024 * 1024;
/// Mails fetched from the provider with one command (bounds the memory a step needs).
const FETCH_CHUNK: usize = 10;
/// Mails uploaded and read back at the same time.
const PARALLEL: usize = 4;
/// A device's lock on an account: renewed every step, given up at the end.
const LOCK_MS: i64 = 15 * 60 * 1000;
/// Mails deleted in the archive stay in its trash this long.
pub const TRASH_DAYS: u32 = 30;
/// Blocks no mail uses are deleted only when they were unused at an earlier clean-up at least
/// this long ago (another device may be archiving and not have written its index yet).
const GC_GRACE_MS: i64 = 24 * 60 * 60 * 1000;
/// Kept of a mail's header and text in the index (for the list, the preview and the search):
/// only the header fields the list needs, so a month file of hundreds of mails stays small.
const HEADER_MAX: usize = 8 * 1024;
const TEXT_MAX: usize = 1024;
const HEADER_FIELDS: &[&str] =
    &["from", "to", "cc", "reply-to", "subject", "date", "message-id", "in-reply-to", "references", "content-type"];

type Key = Zeroizing<[u8; 32]>;

// --- settings (in the Mail file) ------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct Epoch {
    n: u32,
    /// Unix ms; a protection change after this starts a new epoch.
    created: i64,
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
}

impl Drop for Epoch {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.key);
    }
}

/// What happens to the spam folder and the trash.
#[derive(uniffi::Enum, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArchiveFolderRule {
    /// Left alone.
    #[default]
    Leave,
    /// Archived like every other folder.
    Archive,
    /// Deleted at the provider once older than the deadline (not archived).
    Delete,
}

/// A device's copy of the archive (for "a copy of the archive has: ...").
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
struct DeviceCopy {
    name: String,
    mails: u64,
    /// Unix ms of the last update.
    at: i64,
    full: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct Settings {
    #[serde(default)]
    id: String,
    /// Id of the archive's server (its own in the Mail file, or one of the suite's).
    #[serde(default)]
    server: String,
    #[serde(default)]
    use_sync: bool,
    #[serde(default)]
    epochs: Vec<Epoch>,
    #[serde(default = "default_days")]
    days: u32,
    #[serde(default)]
    delete_at_provider: bool,
    #[serde(default = "yes")]
    sent: bool,
    #[serde(default)]
    spam: ArchiveFolderRule,
    #[serde(default)]
    trash: ArchiveFolderRule,
    /// Account ids that are archived.
    #[serde(default)]
    accounts: BTreeSet<String>,
    /// Accounts for which the user confirmed deleting at the provider once (after the first run).
    #[serde(default)]
    confirmed: BTreeSet<String>,
    #[serde(default)]
    copies: BTreeMap<String, DeviceCopy>,
}

fn default_days() -> u32 {
    DEFAULT_DAYS
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            id: String::new(),
            server: String::new(),
            use_sync: false,
            epochs: Vec::new(),
            days: DEFAULT_DAYS,
            delete_at_provider: false,
            sent: true,
            spam: ArchiveFolderRule::Leave,
            trash: ArchiveFolderRule::Leave,
            accounts: BTreeSet::new(),
            confirmed: BTreeSet::new(),
            copies: BTreeMap::new(),
        }
    }
}

fn settings(o: &Open) -> Settings {
    area_setting(o, VaultKind::Mail, SETTINGS_KEY)
}

fn put_settings(o: &mut Open, s: &Settings) -> Result<()> {
    put_area_setting(o, VaultKind::Mail, SETTINGS_KEY, s)
}

/// The archive's server on this device.
fn server_id(o: &Open, s: &Settings) -> Option<String> {
    if s.use_sync {
        super::sync::sync_server(o)
    } else {
        (!s.server.is_empty()).then(|| s.server.clone())
    }
}

// --- keys and encryption --------------------------------------------------------------------

pub(super) struct Keys {
    name: Key,
    block_id: Key,
    head: Key,
    /// (epoch, index key, block key), oldest first.
    epochs: Vec<(u32, Key, Key)>,
}

impl Keys {
    fn new(s: &Settings) -> Result<Keys> {
        let first = s.epochs.first().filter(|e| e.key.len() == 32).ok_or(GhulError::NotConfigured)?;
        let mut epochs = Vec::new();
        for e in &s.epochs {
            if e.key.len() != 32 {
                return Err(GhulError::Corrupted { msg: "mail archive key damaged".into() });
            }
            epochs.push((
                e.n,
                hkdf(&e.key, &[b"ghul/v1/mailarchive/index"]),
                hkdf(&e.key, &[b"ghul/v1/mailarchive/block"]),
            ));
        }
        Ok(Keys {
            name: hkdf(&first.key, &[b"ghul/v1/mailarchive/name"]),
            block_id: hkdf(&first.key, &[b"ghul/v1/mailarchive/block-id"]),
            head: hkdf(&first.key, &[b"ghul/v1/mailarchive/head"]),
            epochs,
        })
    }

    fn current(&self) -> u32 {
        self.epochs.last().map_or(0, |e| e.0)
    }

    fn index_key(&self, epoch: u32) -> Option<&Key> {
        self.epochs.iter().find(|e| e.0 == epoch).map(|e| &e.1)
    }

    fn block_key(&self, epoch: u32) -> Option<&Key> {
        self.epochs.iter().find(|e| e.0 == epoch).map(|e| &e.2)
    }

    fn hmac(key: &[u8; 32], parts: &[&[u8]]) -> String {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("any key size");
        for p in parts {
            m.update(p);
        }
        crate::kdf::hex(&m.finalize().into_bytes())
    }

    fn block_id(&self, data: &[u8]) -> String {
        Self::hmac(&self.block_id, &[data])
    }

    /// Identity of a whole mail: the same mail is archived once per folder.
    fn mail_key(&self, raw: &[u8]) -> String {
        Self::hmac(&self.block_id, &[b"mail:", raw])
    }

    fn account_dir(&self, email: &str) -> String {
        let h = Self::hmac(&self.name, &[b"account:", email.trim().to_lowercase().as_bytes()]);
        format!("{ROOT}/a/{}", &h[..32])
    }

    fn month_path(&self, account_dir: &str, month: &str) -> String {
        let h = Self::hmac(&self.name, &[account_dir.as_bytes(), b"/", month.as_bytes()]);
        format!("{account_dir}/m/{}", &h[..32])
    }
}

fn block_path(id: &str) -> String {
    format!("{ROOT}/b/{}/{}", &id[..2], id)
}

fn seal_file(magic: &[u8; 8], epoch: u32, key: &[u8; 32], path: &str, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let e = epoch.to_be_bytes();
    let aad = [magic.as_slice(), &e, path.as_bytes()].concat();
    let mut out = Vec::with_capacity(36 + plain.len() + 16);
    out.extend_from_slice(magic);
    out.extend_from_slice(&e);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&seal(key, &nonce, plain, &aad));
    out
}

fn damaged() -> GhulError {
    GhulError::Corrupted { msg: "mail archive damaged or from another key".into() }
}

/// Opens a file; `key` picks the key of the epoch written in it.
fn open_file<'k>(
    magic: &[u8; 8],
    path: &str,
    data: &[u8],
    key: impl Fn(u32) -> Option<&'k Key>,
) -> Result<Zeroizing<Vec<u8>>> {
    if data.len() < 36 + 16 || &data[..8] != magic {
        return Err(damaged());
    }
    let epoch = u32::from_be_bytes(data[8..12].try_into().expect("4 bytes"));
    let k = key(epoch).ok_or_else(|| GhulError::Corrupted {
        msg: "mail archive part from a newer key: open the suite on a current device first".into(),
    })?;
    let nonce: [u8; 24] = data[12..36].try_into().expect("24 bytes");
    let aad = [magic.as_slice(), &data[8..12], path.as_bytes()].concat();
    open(k, &nonce, &data[36..], &aad).ok_or_else(damaged)
}

fn to_cbor<T: Serialize>(v: &T) -> Result<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::new());
    ciborium::ser::into_writer(v, &mut *out).map_err(|e| GhulError::Corrupted { msg: e.to_string() })?;
    Ok(out)
}

fn from_cbor<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T> {
    ciborium::de::from_reader(data).map_err(|_| damaged())
}

// --- archive files --------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Head {
    id: String,
    created: i64,
}

/// One archived mail in one folder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(super) struct Item {
    key: String,
    folder: String,
    #[serde(default)]
    role: String,
    /// UIDs at the provider under `validity` (more than one if the server holds the same mail
    /// twice in the folder).
    uids: Vec<u32>,
    validity: u32,
    /// Unix seconds (Date header, else when the provider received it).
    date: i64,
    size: u64,
    blocks: Vec<String>,
    #[serde(with = "serde_bytes")]
    header: Vec<u8>,
    #[serde(with = "serde_bytes")]
    text: Vec<u8>,
    seen: bool,
    flagged: bool,
    #[serde(default)]
    labels: Vec<String>,
    /// Unix ms.
    archived: i64,
    changed: i64,
    /// In the archive's trash since (Unix ms); 0 = not.
    #[serde(default)]
    trashed: i64,
    /// Deleted at the provider (after the check).
    #[serde(default)]
    removed: bool,
}

/// One month of one account.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub(crate) struct Month {
    month: String,
    revision: u64,
    items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct State {
    /// UIDVALIDITY per folder as last seen.
    folders: BTreeMap<String, u32>,
    /// What is archived per folder under its current numbering, so a run knows what is left to do
    /// without reading every month file. Only a shortcut: the month files stay the truth, and
    /// deleting at the provider still reads every mail back from them. `None` until built once.
    #[serde(default)]
    index: Option<BTreeMap<String, FolderIndex>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct FolderIndex {
    validity: u32,
    /// Archived UID and the month file its mail is in.
    uids: BTreeMap<u32, String>,
    /// UIDs already deleted at the provider.
    #[serde(default)]
    removed: BTreeSet<u32>,
    /// UIDs of mails larger than [`MAX_MAIL`] (with their size): left at the provider, not tried again.
    #[serde(default)]
    too_large: BTreeMap<u32, u64>,
}

impl State {
    /// Builds the index from the month files (the first time, or after a lost state file).
    fn ensure_index(&mut self, store: &Store, account_dir: &str) -> Result<()> {
        if self.index.is_some() {
            return Ok(());
        }
        let mut index: BTreeMap<String, FolderIndex> = BTreeMap::new();
        for m in store.months(account_dir)?.values() {
            for i in &m.items {
                let want = self.folders.get(&i.folder).copied().unwrap_or(i.validity);
                if i.validity != want {
                    continue;
                }
                let f = index
                    .entry(i.folder.clone())
                    .or_insert_with(|| FolderIndex { validity: want, ..Default::default() });
                for u in &i.uids {
                    f.uids.insert(*u, m.month.clone());
                    if i.removed {
                        f.removed.insert(*u);
                    }
                }
            }
        }
        self.index = Some(index);
        Ok(())
    }

    fn folder(&mut self, name: &str, validity: u32) -> &mut FolderIndex {
        let index = self.index.get_or_insert_with(BTreeMap::new);
        let f = index.entry(name.to_owned()).or_default();
        if f.validity != validity {
            // Renumbered: nothing is known under the new numbers; matched again by content.
            *f = FolderIndex { validity, ..Default::default() };
        }
        f
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Lock {
    device: String,
    until: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Gc {
    /// Blocks found unused, with when that was first seen (Unix ms).
    unused: BTreeMap<String, i64>,
}

fn month_of(date_secs: i64) -> String {
    let d = super::caldate::Date::from_days(date_secs.max(0).div_euclid(86_400));
    format!("{:04}-{:02}", d.y, d.m)
}

/// Where the archive lives and how it is read.
pub(super) struct Store {
    fs: Arc<dyn RemoteFs>,
    /// A new connection (to read back what was written).
    fresh: Box<dyn Fn() -> Result<Arc<dyn RemoteFs>> + Send + Sync>,
    keys: Keys,
    /// This device's copy (encrypted files as on the server), if it keeps one.
    copy: Option<std::path::PathBuf>,
}

impl Store {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.fs.read(path)?)
    }

    fn keep(&self, path: &str, data: &[u8]) {
        if let Some(dir) = &self.copy {
            let p = dir.join(path);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = p.with_extension("tmp");
            if std::fs::write(&tmp, data).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
    }

    fn kept(&self, path: &str) -> Option<Vec<u8>> {
        self.copy.as_ref().and_then(|d| std::fs::read(d.join(path)).ok())
    }

    fn head(fs: &dyn RemoteFs, keys: &Keys) -> Result<Option<Head>> {
        match fs.read(HEAD)? {
            None => Ok(None),
            Some(data) => {
                let plain = open_file(HEAD_MAGIC, HEAD, &data, |_| Some(&keys.head))?;
                Ok(Some(from_cbor(&plain)?))
            }
        }
    }

    fn write_head(fs: &dyn RemoteFs, keys: &Keys, head: &Head) -> Result<()> {
        fs.write(HEAD, &seal_file(HEAD_MAGIC, 0, &keys.head, HEAD, &to_cbor(head)?))?;
        Ok(())
    }

    fn load_month(fs: &dyn RemoteFs, keys: &Keys, path: &str) -> Result<Option<Month>> {
        match fs.read(path)? {
            None => Ok(None),
            Some(data) => Ok(Some(Self::decode_month(keys, path, &data)?)),
        }
    }

    fn decode_month(keys: &Keys, path: &str, data: &[u8]) -> Result<Month> {
        let plain = open_file(MONTH_MAGIC, path, data, |e| keys.index_key(e))?;
        from_cbor(unpad(&plain)?)
    }

    fn encode_month(&self, path: &str, m: &Month) -> Result<Vec<u8>> {
        let e = self.keys.current();
        let key = self.keys.index_key(e).ok_or(GhulError::NotConfigured)?;
        Ok(seal_file(MONTH_MAGIC, e, key, path, &pad(&to_cbor(m)?)?))
    }

    /// All month files of an account (from the server; from this device's copy when offline).
    fn months(&self, account_dir: &str) -> Result<BTreeMap<String, Month>> {
        let dir = format!("{account_dir}/m");
        let mut out = BTreeMap::new();
        match self.fs.list(&dir) {
            Ok(list) => {
                for e in list.into_iter().filter(|e| !e.is_dir && !e.name.ends_with(".tmp")) {
                    let path = format!("{dir}/{}", e.name);
                    if let Some(data) = self.read(&path)? {
                        let m = Self::decode_month(&self.keys, &path, &data)?;
                        self.keep(&path, &data);
                        out.insert(path, m);
                    }
                }
                Ok(out)
            }
            Err(e) => {
                let Some(copy) = &self.copy else { return Err(e.into()) };
                let Ok(rd) = std::fs::read_dir(copy.join(&dir)) else { return Err(e.into()) };
                for f in rd.flatten() {
                    let name = f.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".tmp") {
                        continue;
                    }
                    let path = format!("{dir}/{name}");
                    if let Ok(data) = std::fs::read(f.path()) {
                        out.insert(path.clone(), Self::decode_month(&self.keys, &path, &data)?);
                    }
                }
                Ok(out)
            }
        }
    }

    /// Loads a month, applies `f` and writes it; if another device wrote the month in between,
    /// `f` is applied to that version instead (nothing gets lost).
    fn change_month<T>(&self, path: &str, month: &str, mut f: impl FnMut(&mut Month) -> T) -> Result<(T, Month)> {
        for _ in 0..8 {
            let mut m = Self::load_month(self.fs.as_ref(), &self.keys, path)?.unwrap_or_else(|| Month {
                month: month.to_owned(),
                revision: 0,
                items: Vec::new(),
            });
            let base = m.revision;
            let out = f(&mut m);
            let now = Self::load_month(self.fs.as_ref(), &self.keys, path)?.map_or(0, |m| m.revision);
            if now != base {
                continue;
            }
            m.revision += 1;
            let data = self.encode_month(path, &m)?;
            self.fs.write(path, &data)?;
            self.keep(path, &data);
            return Ok((out, m));
        }
        Err(GhulError::Network { msg: "the mail archive is being changed on another device, please try again".into() })
    }

    fn read_state(&self, account_dir: &str) -> Result<State> {
        let path = format!("{account_dir}/state");
        match self.read(&path)? {
            None => Ok(State::default()),
            Some(d) => from_cbor(&open_file(STATE_MAGIC, &path, &d, |e| self.keys.index_key(e))?),
        }
    }

    fn write_state(&self, account_dir: &str, s: &State) -> Result<()> {
        let path = format!("{account_dir}/state");
        let e = self.keys.current();
        let key = self.keys.index_key(e).ok_or(GhulError::NotConfigured)?;
        self.fs.write(&path, &seal_file(STATE_MAGIC, e, key, &path, &to_cbor(s)?))?;
        Ok(())
    }

    /// Takes the account's lock for this device; `Err(device)` names the device holding it.
    fn lock(&self, account_dir: &str, device: &str, now: i64) -> Result<std::result::Result<(), String>> {
        let path = format!("{account_dir}/lock");
        if let Some(d) = self.read(&path)? {
            if let Ok(plain) = open_file(LOCK_MAGIC, &path, &d, |e| self.keys.index_key(e)) {
                let l: Lock = from_cbor(&plain)?;
                if l.device != device && l.until > now {
                    return Ok(Err(l.device));
                }
            }
        }
        let e = self.keys.current();
        let key = self.keys.index_key(e).ok_or(GhulError::NotConfigured)?;
        let l = Lock { device: device.to_owned(), until: now + LOCK_MS };
        self.fs.write(&path, &seal_file(LOCK_MAGIC, e, key, &path, &to_cbor(&l)?))?;
        Ok(Ok(()))
    }

    fn unlock(&self, account_dir: &str, device: &str) {
        let path = format!("{account_dir}/lock");
        if let Ok(Some(d)) = self.read(&path) {
            if let Ok(plain) = open_file(LOCK_MAGIC, &path, &d, |e| self.keys.index_key(e)) {
                if from_cbor::<Lock>(&plain).is_ok_and(|l| l.device == device) {
                    let _ = self.fs.delete(&path);
                }
            }
        }
    }

    /// Encrypts and writes the blocks of a mail; returns their ids.
    fn put_mail(&self, raw: &[u8]) -> Result<Vec<String>> {
        let e = self.keys.current();
        let key = self.keys.block_key(e).ok_or(GhulError::NotConfigured)?;
        let mut ids = Vec::new();
        for chunk in raw.chunks(BLOCK) {
            let id = self.keys.block_id(chunk);
            let path = block_path(&id);
            self.fs.write(&path, &seal_file(BLOCK_MAGIC, e, key, &path, chunk))?;
            ids.push(id);
        }
        Ok(ids)
    }

    fn block_from(&self, fs: &dyn RemoteFs, id: &str, use_copy: bool) -> Result<Zeroizing<Vec<u8>>> {
        let path = block_path(id);
        let data = match use_copy.then(|| self.kept(&path)).flatten() {
            Some(d) => d,
            None => fs.read(&path)?.ok_or_else(|| GhulError::Corrupted { msg: "mail archive block missing".into() })?,
        };
        let plain = open_file(BLOCK_MAGIC, &path, &data, |e| self.keys.block_key(e))?;
        if self.keys.block_id(&plain) != id {
            return Err(damaged());
        }
        Ok(plain)
    }

    /// The whole mail; from this device's copy if it has it.
    fn get_mail(&self, item: &Item, keep: bool) -> Result<Zeroizing<Vec<u8>>> {
        let mut raw = Zeroizing::new(Vec::with_capacity(item.size as usize));
        for id in &item.blocks {
            let path = block_path(id);
            let from_copy = self.kept(&path);
            let data = match from_copy {
                Some(d) => d,
                None => {
                    let d = self
                        .read(&path)?
                        .ok_or_else(|| GhulError::Corrupted { msg: "mail archive block missing".into() })?;
                    if keep {
                        self.keep(&path, &d);
                    }
                    d
                }
            };
            let plain = open_file(BLOCK_MAGIC, &path, &data, |e| self.keys.block_key(e))?;
            if self.keys.block_id(&plain) != *id {
                return Err(damaged());
            }
            raw.extend_from_slice(&plain);
        }
        if self.keys.mail_key(&raw) != item.key {
            return Err(damaged());
        }
        Ok(raw)
    }

    /// Reads the blocks back over `fs` (a new connection) and compares them byte for byte.
    fn verify_blocks(&self, fs: &dyn RemoteFs, raw: &[u8], blocks: &[String]) -> Result<bool> {
        let chunks: Vec<&[u8]> = raw.chunks(BLOCK).collect();
        if chunks.len() != blocks.len() {
            return Ok(false);
        }
        for (chunk, id) in chunks.iter().zip(blocks) {
            match self.block_from(fs, id, false) {
                Ok(back) if back.as_slice() == *chunk => {}
                Ok(_) | Err(GhulError::Corrupted { .. }) => return Ok(false),
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    /// The archived mail is complete on the server (read over `fs`): every block, and the whole
    /// mail matches its key.
    fn verify_item(&self, fs: &dyn RemoteFs, item: &Item) -> Result<bool> {
        let mut raw = Zeroizing::new(Vec::with_capacity(item.size as usize));
        for id in &item.blocks {
            match self.block_from(fs, id, false) {
                Ok(b) => raw.extend_from_slice(&b),
                Err(GhulError::Corrupted { .. }) => return Ok(false),
                Err(e) => return Err(e),
            }
        }
        Ok(self.keys.mail_key(&raw) == item.key && raw.len() as u64 == item.size)
    }
}

// --- the provider ---------------------------------------------------------------------------

/// What the archive needs from the mail provider (IMAP; a simulated one in the tests).
pub(super) trait Provider {
    fn folders(&self) -> Result<Vec<Folder>>;
    /// UIDVALIDITY and the UIDs of mails received before the day `before` (days since 1970).
    fn old_uids(&self, folder: &str, before: i64) -> Result<(u32, Vec<u32>)>;
    fn fetch(&self, folder: &str, validity: u32, uid: u32) -> Result<Option<Archivable>>;
    /// Sizes of these mails as the provider reports them (missing ones are left out).
    fn sizes(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<BTreeMap<u32, u64>>;
    /// Several mails with one command where the provider can; missing ones are left out.
    fn fetch_many(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<Vec<Archivable>> {
        let mut out = Vec::new();
        for &uid in uids {
            if let Some(m) = self.fetch(folder, validity, uid)? {
                out.push(m);
            }
        }
        Ok(out)
    }
    /// Deletes exactly these UIDs while the folder keeps `validity`; false if it changed.
    fn delete(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<bool>;
}

struct ImapProvider<'a> {
    suite: &'a GhulSuite,
    account: String,
}

impl Provider for ImapProvider<'_> {
    fn folders(&self) -> Result<Vec<Folder>> {
        self.suite.with_imap(&self.account, |s| Box::pin(imap::folders(s)))
    }

    fn old_uids(&self, folder: &str, before: i64) -> Result<(u32, Vec<u32>)> {
        self.suite.with_imap(&self.account, |s| {
            let folder = folder.to_owned();
            Box::pin(async move { imap::old_uids(s, &folder, before).await })
        })
    }

    fn fetch(&self, folder: &str, validity: u32, uid: u32) -> Result<Option<Archivable>> {
        self.suite.with_imap(&self.account, |s| {
            let folder = folder.to_owned();
            Box::pin(async move { imap::fetch_archivable(s, &folder, validity, uid).await })
        })
    }

    fn sizes(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<BTreeMap<u32, u64>> {
        self.suite.with_imap(&self.account, |s| {
            let (folder, uids) = (folder.to_owned(), uids.to_vec());
            Box::pin(async move { imap::sizes(s, &folder, validity, &uids).await })
        })
    }

    fn fetch_many(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<Vec<Archivable>> {
        // Large attachments over a slow line: up to 20 MB per command, 10 minutes for it.
        self.suite.with_imap_for(&self.account, std::time::Duration::from_secs(600), |s| {
            let (folder, uids) = (folder.to_owned(), uids.to_vec());
            Box::pin(async move { imap::fetch_archivable_many(s, &folder, validity, &uids).await })
        })
    }

    fn delete(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<bool> {
        self.suite.with_imap(&self.account, |s| {
            let (folder, uids) = (folder.to_owned(), uids.to_vec());
            Box::pin(async move { imap::delete_if_valid(s, &folder, validity, &uids).await })
        })
    }
}

// --- test hooks -----------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// Name of the step at which the next archive run stops as if the connection broke.
    pub(super) static FAIL_AT: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) };
}

/// Steps: "uploaded", "verified", "indexed", "checked", "deleted".
fn failpoint(_step: &'static str) -> Result<()> {
    #[cfg(test)]
    if FAIL_AT.with(|f| *f.borrow() == Some(_step)) {
        return Err(GhulError::Network { msg: format!("test: connection lost after {_step}") });
    }
    Ok(())
}

// --- public types ---------------------------------------------------------------------------

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailArchiveMode {
    None,
    /// On the server this device synchronises with.
    Sync,
    /// On its own server, stored in the Mail file.
    Own,
    /// On one of the suite's other servers.
    Suite,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchiveOptions {
    pub days: u32,
    pub delete_at_provider: bool,
    pub sent: bool,
    pub spam: ArchiveFolderRule,
    pub trash: ArchiveFolderRule,
    /// Account ids that are archived.
    pub accounts: Vec<String>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchiveCopy {
    pub device: String,
    pub name: String,
    pub mails: u64,
    pub at: i64,
    pub full: bool,
    pub this_device: bool,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchiveStatus {
    pub mode: MailArchiveMode,
    pub server: Option<ServerInfo>,
    pub sync_available: bool,
    pub options: MailArchiveOptions,
    pub copies: Vec<MailArchiveCopy>,
    /// This device keeps a full copy.
    pub full_copy: bool,
    /// Bytes of the archive kept on this device.
    pub offline_bytes: u64,
    /// Accounts whose first run still waits for the user's "remove at the provider now?".
    pub confirmed: Vec<String>,
    /// Accounts at which nothing is deleted, whatever the options say (Gmail, for now).
    pub delete_blocked: Vec<String>,
}

/// How setting up a place for the archive went.
#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum MailArchivePlace {
    /// A new, empty archive was created there.
    Created,
    /// The same archive is already there: connected.
    Connected,
    /// The place is empty but the archive is still reachable at its old place: move it?
    MoveOffered,
    /// Another archive (another key) is there: refused.
    Foreign,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Default)]
pub struct MailArchiveRun {
    /// Archived (and checked) in this run.
    pub archived: u32,
    /// Deleted at the provider after the check.
    pub removed: u32,
    /// Deleted at the provider by the spam/trash rule.
    pub deleted_by_rule: u32,
    /// Old mails still to archive.
    pub remaining: u32,
    /// Archived and checked but still at the provider, waiting for the user's first confirmation.
    pub waiting_for_confirmation: u32,
    /// Folders the provider has renumbered (UIDVALIDITY changed): matched again by content.
    pub renumbered: u32,
    /// Mails larger than 100 MB in the archived folders: left at the provider, never deleted.
    pub too_large: u32,
    /// Another device is archiving this account right now (its name), nothing was done.
    pub busy: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchivedMail {
    /// Identifies the mail in the archive together with `folder`.
    pub key: String,
    pub folder: String,
    pub role: String,
    pub summary: MailSummary,
    /// Still at the provider as well.
    pub at_provider: bool,
    /// In the archive's trash since (Unix ms); 0 = not.
    pub trashed: i64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailArchivePage {
    pub total: u32,
    pub mails: Vec<MailArchivedMail>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct MailArchiveRef {
    pub key: String,
    pub folder: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Default)]
pub struct MailArchiveCount {
    pub mails: u64,
    pub bytes: u64,
    pub in_trash: u64,
}

// --- helpers --------------------------------------------------------------------------------

/// Header and the start of the text of a raw mail.
fn split_mail(raw: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let at = super::mail::body_offset(raw);
    let text = raw[at..].iter().take(TEXT_MAX).copied().collect();
    (slim_header(&raw[..at]), text)
}

/// Only the header fields the list, the preview and the search need (with their folded lines).
fn slim_header(header: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut keep = false;
    for line in header.split_inclusive(|b| *b == b'\n') {
        let folded = line.first().is_some_and(|b| *b == b' ' || *b == b'\t');
        if !folded {
            let name = line.split(|b| *b == b':').next().unwrap_or_default();
            let name = String::from_utf8_lossy(name).trim().to_ascii_lowercase();
            keep = HEADER_FIELDS.contains(&name.as_str());
        }
        if keep && out.len() + line.len() <= HEADER_MAX {
            out.extend_from_slice(line);
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

fn date_of(raw_header: &[u8], received: i64) -> i64 {
    mail_parser::MessageParser::default()
        .parse_headers(raw_header)
        .and_then(|m| m.date().map(|d| d.to_timestamp()))
        .filter(|t| *t > 0)
        .unwrap_or(received)
}

fn summary_of(item: &Item) -> MailSummary {
    let l = imap::Listed {
        uid: 0,
        seen: item.seen,
        flagged: item.flagged,
        size: item.size.min(u32::MAX as u64) as u32,
        header: item.header.clone(),
        text: item.text.clone(),
        labels: item.labels.clone(),
    };
    let mut s = super::mail::summary(&l);
    if s.date == 0 {
        s.date = item.date;
    }
    s
}

fn archived_mail(item: &Item) -> MailArchivedMail {
    MailArchivedMail {
        key: item.key.clone(),
        folder: item.folder.clone(),
        role: item.role.clone(),
        summary: summary_of(item),
        at_provider: !item.removed,
        trashed: item.trashed,
    }
}

/// What happens to a folder.
fn rule_for(s: &Settings, f: &Folder) -> ArchiveFolderRule {
    match f.role.as_str() {
        "drafts" => ArchiveFolderRule::Leave,
        "junk" => s.spam,
        "trash" => s.trash,
        "sent" if !s.sent => ArchiveFolderRule::Leave,
        _ => ArchiveFolderRule::Archive,
    }
}

impl GhulSuite {
    fn device_id(&self) -> String {
        let path = self.local_dir.join("device-id");
        if let Ok(id) = std::fs::read_to_string(&path) {
            if id.trim().len() == 32 {
                return id.trim().to_owned();
            }
        }
        let id = crate::kdf::hex(&super::suite::new_id());
        let _ = std::fs::create_dir_all(&self.local_dir);
        let _ = std::fs::write(&path, &id);
        id
    }

    fn archive_copy_dir(&self, id: &str) -> std::path::PathBuf {
        self.local_dir.join("mail-archive").join(id)
    }

    fn full_copy_here(&self, id: &str) -> bool {
        self.archive_copy_dir(id).join("full").exists()
    }

    /// The archive's store (connects to its server).
    pub(super) fn archive_store(&self, o: &mut Open) -> Result<Store> {
        let s = settings(o);
        if s.id.is_empty() {
            return Err(GhulError::NotConfigured);
        }
        let keys = Keys::new(&s)?;
        let copy = Some(self.archive_copy_dir(&s.id));
        #[cfg(test)]
        if let Some(fs) =
            self.net.lock().unwrap_or_else(|p| p.into_inner()).conn.clone().filter(|c| c.0 == "archive-test")
        {
            let f = fs.1.clone();
            return Ok(Store { fs: fs.1, fresh: Box::new(move || Ok(f.clone())), keys, copy });
        }
        let server = server_id(o, &s).ok_or(GhulError::NotConfigured)?;
        let fs = self.remote(o, &server)?;
        // A second, separate connection: what was written is read back over it.
        let second = servers::connect(o, &server)?;
        let fresh = Box::new(move || Ok(second.clone()));
        Ok(Store { fs, fresh, keys, copy })
    }

    /// Starts a new key epoch if the suite's protection changed since the last one.
    fn archive_epoch(&self, o: &mut Open) -> Result<()> {
        let mut s = settings(o);
        let changed: i64 = super::sync::protection_changed(o);
        let last = s.epochs.last().map_or(i64::MAX, |e| e.created);
        if changed > last {
            let mut key = vec![0u8; 32];
            OsRng.fill_bytes(&mut key);
            let n = s.epochs.last().map_or(0, |e| e.n + 1);
            s.epochs.push(Epoch { n, created: now_ms(), key });
            put_settings(o, &s)?;
            self.save_area(o, VaultKind::Mail)?;
        }
        Ok(())
    }

    fn account_email(&self, o: &Open, account_id: &str) -> Result<String> {
        Ok(super::mail::account(o, account_id)?.email.clone())
    }

    /// Notes how much of the archive this device keeps (shown on the other devices).
    fn note_copy(&self, o: &mut Open, name: &str, mails: u64) -> Result<()> {
        let mut s = settings(o);
        let device = self.device_id();
        let full = self.full_copy_here(&s.id);
        let entry = s.copies.entry(device).or_default();
        if !name.is_empty() {
            entry.name = name.to_owned();
        }
        entry.mails = mails;
        entry.full = full;
        entry.at = now_ms();
        put_settings(o, &s)?;
        self.save_area(o, VaultKind::Mail)
    }
}

/// The run itself, apart from the suite (tests drive it with a simulated provider).
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    store: &Store,
    provider: &dyn Provider,
    s: &Settings,
    account_id: &str,
    account_dir: &str,
    device: &str,
    now: i64,
    budget: u32,
) -> Result<MailArchiveRun> {
    let mut out = MailArchiveRun::default();
    if let Err(other) = store.lock(account_dir, device, now)? {
        out.busy = s.copies.get(&other).map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or(other);
        return Ok(out);
    }
    let result = run_locked(store, provider, s, account_id, account_dir, device, now, budget, &mut out);
    store.unlock(account_dir, device);
    result.map(|()| out)
}

#[allow(clippy::too_many_arguments)]
fn run_locked(
    store: &Store,
    provider: &dyn Provider,
    s: &Settings,
    account_id: &str,
    account_dir: &str,
    device: &str,
    now: i64,
    budget: u32,
    out: &mut MailArchiveRun,
) -> Result<()> {
    let before = now.div_euclid(86_400_000) - i64::from(s.days.max(1));
    let mut state = store.read_state(account_dir)?;
    state.ensure_index(store, account_dir)?;
    let mut left = budget as usize;
    let deleting = s.delete_at_provider && s.confirmed.contains(account_id);
    for folder in provider.folders()? {
        let rule = rule_for(s, &folder);
        if rule == ArchiveFolderRule::Leave {
            continue;
        }
        let (validity, uids) = provider.old_uids(&folder.name, before)?;
        if rule == ArchiveFolderRule::Delete {
            if !uids.is_empty() && provider.delete(&folder.name, validity, &uids)? {
                out.deleted_by_rule += uids.len() as u32;
            }
            continue;
        }
        if let Some(prev) = state.folders.get(&folder.name).copied() {
            if prev != validity {
                out.renumbered += 1;
            }
        }
        if state.folders.get(&folder.name) != Some(&validity) {
            state.folders.insert(folder.name.clone(), validity);
            store.write_state(account_dir, &state)?;
        }
        // What is archived from this folder under the current numbering.
        let known = state.folder(&folder.name, validity);
        out.too_large += uids.iter().filter(|u| known.too_large.contains_key(u)).count() as u32;
        let mut todo: Vec<u32> =
            uids.iter().copied().filter(|u| !known.uids.contains_key(u) && !known.too_large.contains_key(u)).collect();
        // Newest first; a server listing a UID twice does not make it count twice.
        todo.sort_unstable_by(|a, b| b.cmp(a));
        todo.dedup();
        let take = todo.len().min(left);
        out.remaining += (todo.len() - take) as u32;
        for step in todo[..take].chunks(STEP) {
            let (added, too_large) = archive_step(store, provider, &folder, validity, step, account_dir, now, out)?;
            // After the month files were written and read back: the shortcut.
            let f = state.folder(&folder.name, validity);
            out.too_large += too_large.len() as u32;
            f.too_large.extend(too_large);
            for (uid, month) in added {
                f.uids.insert(uid, month);
                f.removed.remove(&uid);
            }
            store.write_state(account_dir, &state)?;
            left -= step.len();
        }
        if deleting && validity != 0 {
            out.removed += remove_at_provider(store, provider, &folder, validity, &uids, account_dir, &mut state)?;
        }
    }
    if !deleting {
        out.waiting_for_confirmation = waiting(&state);
    }
    let _ = device;
    Ok(())
}

/// What a step archived (UID and month) and what was too large (UID and size).
type StepResult = (Vec<(u32, String)>, Vec<(u32, u64)>);

/// One step: fetch, upload, read back, write the index, read the index back. Returns the UIDs
/// that are now archived and checked, with their month, and those too large to archive.
#[allow(clippy::too_many_arguments)]
fn archive_step(
    store: &Store,
    provider: &dyn Provider,
    folder: &Folder,
    validity: u32,
    uids: &[u32],
    account_dir: &str,
    now: i64,
    out: &mut MailArchiveRun,
) -> Result<StepResult> {
    let fresh = (store.fresh)()?;
    let mut items: Vec<Item> = Vec::new();
    // Too large ones are never fetched (the size comes from the provider, before the mail).
    let sizes = provider.sizes(&folder.name, validity, uids)?;
    let too_large: Vec<(u32, u64)> =
        uids.iter().filter_map(|u| sizes.get(u).filter(|s| **s > MAX_MAIL).map(|s| (*u, *s))).collect();
    let uids: Vec<u32> = uids.iter().copied().filter(|u| !too_large.iter().any(|(t, _)| t == u)).collect();
    for chunk in uids.chunks(FETCH_CHUNK) {
        let mails: Vec<Archivable> = provider
            .fetch_many(&folder.name, validity, chunk)?
            .into_iter()
            .filter(|m| m.raw.len() as u64 <= MAX_MAIL)
            .collect();
        // Upload and read back several mails at once (each on its own; the order does not matter).
        let results: Vec<Result<Vec<String>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..PARALLEL.min(mails.len()))
                .map(|t| {
                    let (mails, fresh) = (&mails, &fresh);
                    scope.spawn(move || {
                        let mut mine = Vec::new();
                        for (i, m) in mails.iter().enumerate().filter(|(i, _)| i % PARALLEL == t) {
                            let r = store.put_mail(&m.raw).and_then(|blocks| {
                                if store.verify_blocks(fresh.as_ref(), &m.raw, &blocks)? {
                                    Ok(blocks)
                                } else {
                                    Err(GhulError::Network {
                                        msg: "the mail archive did not read back what was written".into(),
                                    })
                                }
                            });
                            mine.push((i, r));
                        }
                        mine
                    })
                })
                .collect();
            let mut all: Vec<(usize, Result<Vec<String>>)> =
                handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect();
            all.sort_by_key(|(i, _)| *i);
            all.into_iter().map(|(_, r)| r).collect()
        });
        failpoint("uploaded")?;
        failpoint("verified")?;
        for (m, blocks) in mails.into_iter().zip(results) {
            let blocks = blocks?;
            let (header, text) = split_mail(&m.raw);
            let key = store.keys.mail_key(&m.raw);
            // The same mail twice in this step (a server listing it twice): one entry, both UIDs.
            if let Some(i) = items.iter_mut().find(|i| i.key == key) {
                if !i.uids.contains(&m.uid) {
                    i.uids.push(m.uid);
                }
                continue;
            }
            items.push(Item {
                key,
                folder: folder.name.clone(),
                role: folder.role.clone(),
                uids: vec![m.uid],
                validity,
                date: date_of(&header, m.received),
                size: m.raw.len() as u64,
                blocks,
                header,
                text,
                seen: m.seen,
                flagged: m.flagged,
                labels: m.labels,
                archived: now,
                changed: now,
                trashed: 0,
                removed: false,
            });
        }
    }
    // Into the month files, then read back over the new connection.
    let mut by_month: BTreeMap<String, Vec<Item>> = BTreeMap::new();
    for i in items {
        by_month.entry(month_of(i.date)).or_default().push(i);
    }
    let mut added = Vec::new();
    for (month, list) in &by_month {
        let path = store.keys.month_path(account_dir, month);
        store.change_month(&path, month, |m| {
            for new in list {
                match m.items.iter_mut().find(|i| i.key == new.key && i.folder == new.folder) {
                    // Known content (renumbered folder, or archived before): new UIDs, nothing else.
                    Some(old) => {
                        if old.validity == new.validity {
                            for u in &new.uids {
                                if !old.uids.contains(u) {
                                    old.uids.push(*u);
                                }
                            }
                        } else {
                            old.uids = new.uids.clone();
                            old.validity = new.validity;
                        }
                        old.removed = false;
                        old.changed = new.changed;
                    }
                    None => m.items.push(new.clone()),
                }
            }
        })?;
        failpoint("indexed")?;
        let back = Store::load_month(fresh.as_ref(), &store.keys, &path)?.unwrap_or_default();
        for new in list {
            let ok = back.items.iter().any(|i| {
                i.key == new.key && i.folder == new.folder && i.validity == new.validity && i.blocks == new.blocks
            });
            if !ok {
                return Err(GhulError::Network { msg: "the mail archive index did not read back".into() });
            }
            added.extend(new.uids.iter().map(|u| (*u, month.clone())));
        }
    }
    failpoint("checked")?;
    out.archived += by_month.values().map(|l| l.len() as u32).sum::<u32>();
    Ok((added, too_large))
}

/// Deletes at the provider what is archived and checked: exactly these UIDs, each mail read back
/// from the archive over a new connection right before. Only the month files that hold such
/// mails are read.
fn remove_at_provider(
    store: &Store,
    provider: &dyn Provider,
    folder: &Folder,
    validity: u32,
    old_uids: &[u32],
    account_dir: &str,
    state: &mut State,
) -> Result<u32> {
    let still: BTreeSet<u32> = old_uids.iter().copied().collect();
    let f = state.folder(&folder.name, validity);
    let mut months: BTreeSet<String> = BTreeSet::new();
    for (uid, month) in &f.uids {
        if still.contains(uid) && !f.removed.contains(uid) {
            months.insert(month.clone());
        }
    }
    let fresh = (store.fresh)()?;
    let mut removed = 0;
    for month in months {
        let path = store.keys.month_path(account_dir, &month);
        // Over the new connection, not from any cache.
        let Some(m) = Store::load_month(fresh.as_ref(), &store.keys, &path)? else { continue };
        let mut uids = Vec::new();
        let mut keys = Vec::new();
        for i in m.items.iter().filter(|i| i.folder == folder.name && i.validity == validity && !i.removed) {
            let at_provider: Vec<u32> =
                i.uids.iter().copied().filter(|u| still.contains(u) && !uids.contains(u)).collect();
            if at_provider.is_empty() {
                continue;
            }
            if store.verify_item(fresh.as_ref(), i)? {
                uids.extend(at_provider);
                keys.push(i.key.clone());
            }
        }
        if uids.is_empty() {
            continue;
        }
        if !provider.delete(&folder.name, validity, &uids)? {
            // Renumbered meanwhile: nothing was deleted; the next run matches again.
            return Ok(removed);
        }
        failpoint("deleted")?;
        removed += uids.len() as u32;
        store.change_month(&path, &m.month, |mm| {
            for i in mm.items.iter_mut().filter(|i| i.folder == folder.name && keys.contains(&i.key)) {
                i.removed = true;
            }
        })?;
        state.folder(&folder.name, validity).removed.extend(uids.iter().copied());
        store.write_state(account_dir, state)?;
    }
    Ok(removed)
}

/// Archived and checked, still at the provider (before the user's first confirmation).
fn waiting(state: &State) -> u32 {
    state
        .index
        .iter()
        .flat_map(|i| i.values())
        .filter(|f| f.validity != 0)
        .map(|f| f.uids.keys().filter(|u| !f.removed.contains(u)).count() as u32)
        .sum()
}

// --- the API --------------------------------------------------------------------------------

#[uniffi::export]
impl GhulSuite {
    /// Where the archive is, its options and the copies on devices.
    pub fn mail_archive_status(&self) -> Result<MailArchiveStatus> {
        self.with_open(|o| {
            let s = settings(o);
            let mode = if s.id.is_empty() {
                MailArchiveMode::None
            } else if s.use_sync {
                MailArchiveMode::Sync
            } else if servers::archive_servers(o).iter().any(|x| x.id == s.server) {
                MailArchiveMode::Own
            } else {
                MailArchiveMode::Suite
            };
            let server = server_id(o, &s).and_then(|id| servers::server(o, &id).ok()).map(|x| servers::info(&x));
            let me = self.device_id();
            let offline_bytes = if s.id.is_empty() { 0 } else { dir_size(&self.archive_copy_dir(&s.id)) };
            Ok(MailArchiveStatus {
                mode,
                server,
                sync_available: super::sync::sync_server(o).is_some(),
                options: options_of(&s),
                copies: s
                    .copies
                    .iter()
                    .map(|(d, c)| MailArchiveCopy {
                        device: d.clone(),
                        name: c.name.clone(),
                        mails: c.mails,
                        at: c.at,
                        full: c.full,
                        this_device: *d == me,
                    })
                    .collect(),
                full_copy: !s.id.is_empty() && self.full_copy_here(&s.id),
                offline_bytes,
                confirmed: s.confirmed.iter().cloned().collect(),
                delete_blocked: s
                    .accounts
                    .iter()
                    .filter(|a| super::mail::account(o, a).is_ok_and(|acc| is_gmail(&acc.servers.imap_host)))
                    .cloned()
                    .collect(),
            })
        })
    }

    /// Changes the options. Deleting at the provider is off until switched on here; the app asks
    /// the user before.
    pub fn mail_archive_set_options(&self, options: MailArchiveOptions) -> Result<()> {
        if options.days == 0 || options.days > 36_500 {
            return Err(GhulError::InvalidInput { msg: "the number of days must be between 1 and 36500".into() });
        }
        self.with_open(|o| {
            self.ensure_area(o, VaultKind::Mail)?;
            let mut s = settings(o);
            s.days = options.days;
            s.delete_at_provider = options.delete_at_provider;
            s.sent = options.sent;
            s.spam = options.spam;
            s.trash = options.trash;
            s.accounts = options.accounts.into_iter().collect();
            if !s.delete_at_provider {
                // Switching it on again asks again after the next first run.
                s.confirmed.clear();
            }
            put_settings(o, &s)?;
            self.save_area(o, VaultKind::Mail)
        })
    }

    /// Puts the archive on a stored server (`None`: the server this device synchronises with).
    /// Creates the archive the first time; connects if the same archive is there; refuses another
    /// one; offers to move when the place is empty but the archive is reachable elsewhere.
    pub fn mail_archive_setup(&self, server_id: Option<String>) -> Result<MailArchivePlace> {
        self.with_open(|o| self.archive_place(o, server_id, false))
    }

    /// Checks a server for the archive alone and stores it in the Mail file. Fingerprints as with
    /// `save_server`; then like `mail_archive_setup`.
    pub fn mail_archive_save_server(&self, input: ServerInput, confirmed: Option<String>) -> Result<ServerSaved> {
        self.with_open(|o| {
            let stored = match servers::check_server(o, &input, confirmed.as_deref())? {
                Ok(s) => s,
                Err(fingerprint) => return Ok(ServerSaved::Confirm { fingerprint }),
            };
            let id = stored.id.clone();
            self.ensure_area(o, VaultKind::Mail)?;
            let mut own = servers::archive_servers(o);
            own.push(stored);
            put_area_setting(o, VaultKind::Mail, servers::ARCHIVE_SERVERS_KEY, &own)?;
            Ok(ServerSaved::Saved { id })
        })
    }

    /// Moves the archive to another server: copies everything, reads it back, then switches.
    /// The old place keeps its copy until the user deletes it there.
    pub fn mail_archive_move(&self, server_id: Option<String>) -> Result<MailArchivePlace> {
        self.with_open(|o| self.archive_place(o, server_id, true))
    }

    /// Archives up to `max` old mails of an account (newest first), checks them and, if switched
    /// on and confirmed, deletes them at the provider. Call again while `remaining` > 0.
    pub fn mail_archive_run(&self, account_id: String, max: u32) -> Result<MailArchiveRun> {
        let (store, s, dir, blocked) = self.with_open(|o| {
            self.archive_epoch(o)?;
            let s = settings(o);
            if !s.accounts.contains(&account_id) {
                return Err(GhulError::NotConfigured);
            }
            let dir = Keys::new(&s)?.account_dir(&self.account_email(o, &account_id)?);
            let blocked = is_gmail(&super::mail::account(o, &account_id)?.servers.imap_host);
            Ok((self.archive_store(o)?, s, dir, blocked))
        })?;
        let provider = ImapProvider { suite: self, account: account_id.clone() };
        let s = without_deleting(s, blocked);
        let mut out = run(&store, &provider, &s, &account_id, &dir, &self.device_id(), now_ms(), max)?;
        if blocked {
            out.waiting_for_confirmation = 0;
        }
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.remove(&dir);
        Ok(out)
    }

    /// The user's "everything is safely archived; remove N mails at the provider now?": allows
    /// deleting for this account from now on and runs it once.
    pub fn mail_archive_confirm_removal(&self, account_id: String) -> Result<MailArchiveRun> {
        self.with_open(|o| {
            let mut s = settings(o);
            if !s.delete_at_provider {
                return Err(GhulError::InvalidInput { msg: "deleting at the provider is switched off".into() });
            }
            if is_gmail(&super::mail::account(o, &account_id)?.servers.imap_host) {
                return Err(GhulError::InvalidInput { msg: "nothing is deleted at Gmail for now".into() });
            }
            s.confirmed.insert(account_id.clone());
            put_settings(o, &s)?;
            self.save_area(o, VaultKind::Mail)
        })?;
        self.mail_archive_run(account_id, 0)
    }

    /// Archived mails of a folder, newest first. `with_provider`: also those still at the
    /// provider (offline, or deleting is off); otherwise only those the provider no longer has.
    pub fn mail_archive_list(
        &self,
        account_id: String,
        folder: String,
        offset: u32,
        limit: u32,
        with_provider: bool,
    ) -> Result<MailArchivePage> {
        let mut all: Vec<Item> = self
            .archive_items(&account_id, false)?
            .into_iter()
            .filter(|i| i.folder == folder && i.trashed == 0 && (with_provider || i.removed))
            .collect();
        all.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.key.cmp(&b.key)));
        let total = all.len() as u32;
        let mails = all.iter().skip(offset as usize).take(limit as usize).map(archived_mail).collect();
        Ok(MailArchivePage { total, mails })
    }

    /// Searches subject, sender and the start of the text of all archived mails of an account.
    pub fn mail_archive_search(&self, account_id: String, query: String, limit: u32) -> Result<Vec<MailArchivedMail>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut found: Vec<MailArchivedMail> = self
            .archive_items(&account_id, false)?
            .iter()
            .filter(|i| i.trashed == 0)
            .map(archived_mail)
            .filter(|m| {
                let s = &m.summary;
                [&s.subject, &s.from, &s.from_address, &s.preview].iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect();
        found.sort_by_key(|m| std::cmp::Reverse(m.summary.date));
        found.truncate(limit as usize);
        Ok(found)
    }

    /// Reads an archived mail (decrypted on this device; also offline if kept here).
    pub fn mail_archive_get(&self, account_id: String, item: MailArchiveRef) -> Result<MailMessage> {
        let raw = self.archive_raw(&account_id, &item)?;
        let u = self.with_open(|o| {
            let u = super::mail::unwrap_message(o, &account_id, &raw)?;
            if u.changed {
                self.save_area(o, VaultKind::Mail)?;
            }
            Ok(u)
        })?;
        super::mail::message(0, &raw, &u)
    }

    /// The archived mail as received (.eml).
    pub fn mail_archive_raw(&self, account_id: String, item: MailArchiveRef) -> Result<Vec<u8>> {
        Ok(self.archive_raw(&account_id, &item)?.to_vec())
    }

    /// Attachment `index` of an archived mail.
    pub fn mail_archive_attachment(&self, account_id: String, item: MailArchiveRef, index: u32) -> Result<Vec<u8>> {
        let raw = self.archive_raw(&account_id, &item)?;
        let u = self.with_open(|o| super::mail::unwrap_message(o, &account_id, &raw))?;
        super::mail::attachment_of(&u, index)
    }

    pub fn mail_archive_set_seen(&self, account_id: String, items: Vec<MailArchiveRef>, seen: bool) -> Result<()> {
        self.archive_change(&account_id, &items, |i| i.seen = seen)
    }

    pub fn mail_archive_set_flagged(
        &self,
        account_id: String,
        items: Vec<MailArchiveRef>,
        flagged: bool,
    ) -> Result<()> {
        self.archive_change(&account_id, &items, |i| i.flagged = flagged)
    }

    /// Into the archive's trash (kept 30 days, see `mail_archive_empty_trash`).
    pub fn mail_archive_delete(&self, account_id: String, items: Vec<MailArchiveRef>) -> Result<()> {
        let now = now_ms();
        self.archive_change(&account_id, &items, |i| i.trashed = now)
    }

    pub fn mail_archive_restore(&self, account_id: String, items: Vec<MailArchiveRef>) -> Result<()> {
        self.archive_change(&account_id, &items, |i| i.trashed = 0)
    }

    /// The archive's trash, newest first.
    pub fn mail_archive_trash(&self, account_id: String) -> Result<Vec<MailArchivedMail>> {
        let mut list: Vec<MailArchivedMail> =
            self.archive_items(&account_id, true)?.iter().filter(|i| i.trashed > 0).map(archived_mail).collect();
        list.sort_by_key(|m| std::cmp::Reverse(m.trashed));
        Ok(list)
    }

    /// Removes mails that are in the archive's trash for at least `days` days (0: all of them).
    /// Their blocks are deleted at a later clean-up, once no mail uses them for a day.
    pub fn mail_archive_empty_trash(&self, account_id: String, days: u32) -> Result<u32> {
        let cut = now_ms() - i64::from(days) * 86_400_000;
        let (store, dir) = self.archive_ctx(&account_id)?;
        let mut gone = 0u32;
        for (path, m) in store.months(&dir)? {
            if !m.items.iter().any(|i| i.trashed > 0 && i.trashed <= cut) {
                continue;
            }
            let (n, _) = store.change_month(&path, &m.month, |mm| {
                let before = mm.items.len();
                mm.items.retain(|i| !(i.trashed > 0 && i.trashed <= cut));
                (before - mm.items.len()) as u32
            })?;
            gone += n;
        }
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.remove(&dir);
        self.archive_collect(&store)?;
        Ok(gone)
    }

    /// Number and size of the archived mails of an account.
    pub fn mail_archive_count(&self, account_id: String) -> Result<MailArchiveCount> {
        let items = self.archive_items(&account_id, false)?;
        Ok(MailArchiveCount {
            mails: items.iter().filter(|i| i.trashed == 0).count() as u64,
            bytes: items.iter().filter(|i| i.trashed == 0).map(|i| i.size).sum(),
            in_trash: items.iter().filter(|i| i.trashed > 0).count() as u64,
        })
    }

    /// Keeps a full copy of the archive on this device (or not; the copy stays until cleared).
    pub fn mail_archive_set_full_copy(&self, on: bool, device_name: String) -> Result<()> {
        self.with_open(|o| {
            let s = settings(o);
            if s.id.is_empty() {
                return Err(GhulError::NotConfigured);
            }
            let dir = self.archive_copy_dir(&s.id);
            std::fs::create_dir_all(&dir).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            if on {
                std::fs::write(dir.join("full"), b"1").map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            } else {
                let _ = std::fs::remove_file(dir.join("full"));
            }
            let mails = s.copies.get(&self.device_id()).map_or(0, |c| c.mails);
            self.note_copy(o, &device_name, mails)
        })
    }

    /// Loads up to `max` archived mails onto this device that it does not keep yet (full copy).
    /// Returns how many are still missing.
    pub fn mail_archive_download(&self, max: u32, device_name: String) -> Result<u32> {
        let (store, dirs) = self.with_open(|o| {
            let s = settings(o);
            let keys = Keys::new(&s)?;
            let mut dirs = Vec::new();
            for a in &s.accounts {
                if let Ok(e) = self.account_email(o, a) {
                    dirs.push(keys.account_dir(&e));
                }
            }
            Ok((self.archive_store(o)?, dirs))
        })?;
        let mut missing = 0u32;
        let mut done = 0u32;
        let mut mails = 0u64;
        for dir in &dirs {
            for m in store.months(dir)?.values() {
                for i in &m.items {
                    mails += 1;
                    let have = i.blocks.iter().all(|b| store.kept(&block_path(b)).is_some());
                    if have {
                        continue;
                    }
                    if done >= max {
                        missing += 1;
                        continue;
                    }
                    store.get_mail(i, true)?;
                    done += 1;
                }
            }
        }
        if missing == 0 {
            self.with_open(|o| self.note_copy(o, &device_name, mails))?;
        }
        Ok(missing)
    }

    /// Deletes this device's copy of the archive. The app asks for a typed word first while this
    /// may be the only full copy (see `mail_archive_only_copy`).
    pub fn mail_archive_clear_copy(&self) -> Result<()> {
        self.with_open(|o| {
            let mut s = settings(o);
            if s.id.is_empty() {
                return Ok(());
            }
            let _ = std::fs::remove_dir_all(self.archive_copy_dir(&s.id));
            s.copies.remove(&self.device_id());
            put_settings(o, &s)?;
            self.save_area(o, VaultKind::Mail)
        })
    }

    /// This device keeps a full copy and the archive itself cannot be reached, or no other
    /// device has a full copy that is newer: deleting it here could lose mails.
    pub fn mail_archive_only_copy(&self) -> Result<bool> {
        let (full, others) = self.with_open(|o| {
            let s = settings(o);
            let me = self.device_id();
            Ok((!s.id.is_empty() && self.full_copy_here(&s.id), s.copies.iter().any(|(d, c)| *d != me && c.full)))
        })?;
        if !full {
            return Ok(false);
        }
        let reachable = self.with_open(|o| {
            let store = self.archive_store(o)?;
            Ok(Store::head(store.fs.as_ref(), &store.keys)?.is_some())
        });
        Ok(!matches!(reachable, Ok(true)) || !others)
    }

    /// Uploads what this device keeps of the archive and the archive lacks (after a lost server):
    /// adds only, never deletes. Returns how many mails it added.
    pub fn mail_archive_restore_from_copy(&self) -> Result<u32> {
        let (store, id) = self.with_open(|o| Ok((self.archive_store(o)?, settings(o).id)))?;
        let copy = self.archive_copy_dir(&id);
        if Store::head(store.fs.as_ref(), &store.keys)?.is_none() {
            Store::write_head(store.fs.as_ref(), &store.keys, &Head { id: id.clone(), created: now_ms() })?;
        }
        // Blocks first, then the month files (merged entry by entry).
        let mut blocks = Vec::new();
        walk(&copy.join(ROOT).join("b"), &mut blocks);
        for f in blocks {
            let Ok(rel) = f.strip_prefix(&copy) else { continue };
            let path = rel.to_string_lossy().replace('\\', "/");
            if path.ends_with(".tmp") || store.fs.read(&path)?.is_some() {
                continue;
            }
            let data = std::fs::read(&f).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            // Only what opens with this archive's keys.
            open_file(BLOCK_MAGIC, &path, &data, |e| store.keys.block_key(e))?;
            store.fs.write(&path, &data)?;
        }
        let mut months = Vec::new();
        walk(&copy.join(ROOT).join("a"), &mut months);
        let mut added = 0u32;
        for f in months {
            let Ok(rel) = f.strip_prefix(&copy) else { continue };
            let path = rel.to_string_lossy().replace('\\', "/");
            if !path.contains("/m/") || path.ends_with(".tmp") {
                continue;
            }
            let data = std::fs::read(&f).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            let mine = Store::decode_month(&store.keys, &path, &data)?;
            let (n, _) = store.change_month(&path, &mine.month, |m| {
                let mut n = 0u32;
                for i in &mine.items {
                    if !m.items.iter().any(|x| x.key == i.key && x.folder == i.folder) {
                        m.items.push(i.clone());
                        n += 1;
                    }
                }
                n
            })?;
            added += n;
        }
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.clear();
        Ok(added)
    }
}

/// Gmail: labels instead of folders. Deleting a mail in a "folder" over IMAP often only removes a
/// label, the mail stays in "All Mail"; deleting from "All Mail" removes it from every label.
/// Until that is handled and tested on its own, nothing is deleted at Gmail.
pub(super) fn is_gmail(imap_host: &str) -> bool {
    let h = imap_host.trim().trim_end_matches('.').to_ascii_lowercase();
    ["gmail.com", "googlemail.com"].iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

/// The options as they apply to an account at which nothing may be deleted.
fn without_deleting(mut s: Settings, blocked: bool) -> Settings {
    if blocked {
        s.delete_at_provider = false;
        s.confirmed.clear();
        for rule in [&mut s.spam, &mut s.trash] {
            if *rule == ArchiveFolderRule::Delete {
                *rule = ArchiveFolderRule::Leave;
            }
        }
    }
    s
}

fn options_of(s: &Settings) -> MailArchiveOptions {
    MailArchiveOptions {
        days: s.days,
        delete_at_provider: s.delete_at_provider,
        sent: s.sent,
        spam: s.spam,
        trash: s.trash,
        accounts: s.accounts.iter().cloned().collect(),
    }
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut files = Vec::new();
    walk(dir, &mut files);
    files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()
}

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

impl GhulSuite {
    fn archive_ctx(&self, account_id: &str) -> Result<(Store, String)> {
        self.with_open(|o| {
            let s = settings(o);
            let dir = Keys::new(&s)?.account_dir(&self.account_email(o, account_id)?);
            Ok((self.archive_store(o)?, dir))
        })
    }

    /// Every archived mail of an account (cached until the next change or `refresh`).
    fn archive_items(&self, account_id: &str, refresh: bool) -> Result<Vec<Item>> {
        let (store, dir) = self.archive_ctx(account_id)?;
        if !refresh {
            if let Some(m) = self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.get(&dir) {
                return Ok(m.values().flat_map(|m| m.items.iter().cloned()).collect());
            }
        }
        let months = store.months(&dir)?;
        let items = months.values().flat_map(|m| m.items.iter().cloned()).collect();
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.insert(dir, months);
        Ok(items)
    }

    fn archive_raw(&self, account_id: &str, r: &MailArchiveRef) -> Result<Zeroizing<Vec<u8>>> {
        let item = self
            .archive_items(account_id, false)?
            .into_iter()
            .find(|i| i.key == r.key && i.folder == r.folder)
            .ok_or(GhulError::NotFound)?;
        let (store, _) = self.archive_ctx(account_id)?;
        store.get_mail(&item, true)
    }

    fn archive_change(&self, account_id: &str, refs: &[MailArchiveRef], f: impl Fn(&mut Item)) -> Result<()> {
        let (store, dir) = self.archive_ctx(account_id)?;
        let now = now_ms();
        for (path, m) in store.months(&dir)? {
            if !m.items.iter().any(|i| refs.iter().any(|r| r.key == i.key && r.folder == i.folder)) {
                continue;
            }
            store.change_month(&path, &m.month, |mm| {
                for i in mm.items.iter_mut().filter(|i| refs.iter().any(|r| r.key == i.key && r.folder == i.folder)) {
                    f(i);
                    i.changed = now;
                }
            })?;
        }
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.remove(&dir);
        Ok(())
    }

    /// Two-step clean-up of blocks no mail uses: noted the first time, deleted a day later if
    /// still unused (another device may be between writing blocks and writing its index).
    fn archive_collect(&self, store: &Store) -> Result<()> {
        let mut used = BTreeSet::new();
        for acct in store.fs.list(&format!("{ROOT}/a"))?.into_iter().filter(|e| e.is_dir) {
            let dir = format!("{ROOT}/a/{}", acct.name);
            for m in store.months(&dir)?.values() {
                for i in &m.items {
                    used.extend(i.blocks.iter().cloned());
                }
            }
        }
        let gc_path = format!("{ROOT}/gc");
        let mut gc: Gc = match store.read(&gc_path)? {
            Some(d) => open_file(GC_MAGIC, &gc_path, &d, |e| store.keys.index_key(e))
                .ok()
                .and_then(|p| from_cbor(&p).ok())
                .unwrap_or_default(),
            None => Gc::default(),
        };
        let now = now_ms();
        let mut next = Gc::default();
        for prefix in store.fs.list(&format!("{ROOT}/b"))?.into_iter().filter(|e| e.is_dir) {
            for b in store.fs.list(&format!("{ROOT}/b/{}", prefix.name))? {
                if b.is_dir || used.contains(&b.name) {
                    continue;
                }
                match gc.unused.remove(&b.name) {
                    Some(since) if now - since >= GC_GRACE_MS => {
                        store.fs.delete(&block_path(&b.name))?;
                    }
                    Some(since) => {
                        next.unused.insert(b.name, since);
                    }
                    None => {
                        next.unused.insert(b.name, now);
                    }
                }
            }
        }
        let e = store.keys.current();
        let key = store.keys.index_key(e).ok_or(GhulError::NotConfigured)?;
        store.fs.write(&gc_path, &seal_file(GC_MAGIC, e, key, &gc_path, &to_cbor(&next)?))?;
        Ok(())
    }

    /// Sets up or changes the archive's place (see `mail_archive_setup`, `mail_archive_move`).
    fn archive_place(&self, o: &mut Open, server: Option<String>, do_move: bool) -> Result<MailArchivePlace> {
        self.ensure_area(o, VaultKind::Mail)?;
        let mut s = settings(o);
        let (new_server, use_sync) = match server {
            Some(id) => {
                servers::server(o, &id)?;
                (id, false)
            }
            None => (super::sync::sync_server(o).ok_or(GhulError::NotConfigured)?, true),
        };
        let fresh_archive = s.id.is_empty();
        if fresh_archive {
            let mut key = vec![0u8; 32];
            OsRng.fill_bytes(&mut key);
            s.id = crate::kdf::hex(&super::suite::new_id());
            s.epochs = vec![Epoch { n: 0, created: now_ms(), key }];
        }
        let keys = Keys::new(&s)?;
        let target = servers::connect(o, &new_server)?;
        let place = match Store::head(target.as_ref(), &keys) {
            Ok(Some(h)) if h.id == s.id => MailArchivePlace::Connected,
            Ok(Some(_)) | Err(GhulError::Corrupted { .. }) => return Ok(MailArchivePlace::Foreign),
            Err(e) => return Err(e),
            Ok(None) => {
                // Empty there. Is the archive reachable at its old place?
                let old = (!fresh_archive).then(|| self.archive_store(o).ok()).flatten();
                let old_has =
                    old.as_ref().is_some_and(|st| matches!(Store::head(st.fs.as_ref(), &st.keys), Ok(Some(_))));
                if old_has && !do_move {
                    return Ok(MailArchivePlace::MoveOffered);
                }
                if let (true, Some(old)) = (old_has, old) {
                    copy_archive(&old, target.as_ref())?;
                } else {
                    Store::write_head(target.as_ref(), &keys, &Head { id: s.id.clone(), created: now_ms() })?;
                }
                MailArchivePlace::Created
            }
        };
        s.server = if use_sync { String::new() } else { new_server.clone() };
        s.use_sync = use_sync;
        put_settings(o, &s)?;
        // The archive's own server is only kept while the archive uses it.
        let own: Vec<_> = servers::archive_servers(o).into_iter().filter(|x| x.id == s.server).collect();
        put_area_setting(o, VaultKind::Mail, servers::ARCHIVE_SERVERS_KEY, &own)?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).archive.clear();
        self.save_area(o, VaultKind::Mail)?;
        Ok(place)
    }
}

/// Copies every file of the archive to `to` and reads each one back.
fn copy_archive(from: &Store, to: &dyn RemoteFs) -> Result<()> {
    let mut files = Vec::new();
    list_all(from.fs.as_ref(), ROOT, &mut files)?;
    // The head last: until then the new place counts as empty.
    files.sort_by_key(|f| f == HEAD);
    for path in files {
        if path.ends_with("/lock") || path.ends_with(".tmp") {
            continue;
        }
        let Some(data) = from.fs.read(&path)? else { continue };
        to.write(&path, &data)?;
        if to.read(&path)?.as_deref() != Some(&data[..]) {
            return Err(GhulError::Network { msg: "the new place did not read back what was written".into() });
        }
    }
    Ok(())
}

fn list_all(fs: &dyn RemoteFs, dir: &str, out: &mut Vec<String>) -> Result<()> {
    for e in fs.list(dir)? {
        let p = format!("{dir}/{}", e.name);
        if e.is_dir {
            list_all(fs, &p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/mailarchive.rs"]
mod tests;
