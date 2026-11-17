// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulDrive: files on the user's own server, encrypted on the device.
//!
//! ```text
//! <base>/drive/index.ghldrv              "GHULDRV1" ‖ nonce[24] ‖ AEAD(index key, padded CBOR index)
//! <base>/drive/blocks/<ab>/<block id>    "GHULBLK1" ‖ nonce[24] ‖ AEAD(block key, block, aad = magic ‖ id)
//!
//! DK          = random 256 bit, kept in the Drive vault's encrypted settings
//! index key   = HKDF(DK, "ghul/v1/drive/index")
//! block key   = HKDF(DK, "ghul/v1/drive/block")
//! block id    = hex(HMAC-SHA256(HKDF(DK, "ghul/v1/drive/block-id"), plaintext block))
//! ```
//!
//! Files are split into blocks of at most 4 MiB. The server only sees block sizes and their
//! number; names, folders and sizes of files exist only in the encrypted index. Equal blocks are
//! stored once. Every change reloads the index from the server first, so two devices do not
//! overwrite each other's changes (the last writer of the same item wins).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use super::areas::{area_setting, put_area_setting};
use super::error::{GhulError, Result};
use super::servers::{self, ServerInfo, ServerInput, ServerSaved, StoredServer};
use super::suite::{new_id, now_ms, GhulSuite, Open, VaultKind};
use crate::net::remote::RemoteFs;
use crate::vault::keys::{hkdf, open, seal};
use crate::vault::payload::{pad, unpad};

const DRIVE_KEY: &str = "drive";
const INDEX: &str = "drive/index.ghldrv";
const INDEX_MAGIC: &[u8; 8] = b"GHULDRV1";
const BLOCK_MAGIC: &[u8; 8] = b"GHULBLK2";
/// Keys of uploads that did not finish, on this device, so a repeated upload continues.
const PENDING_FILE: &str = "drive-pending";
const PENDING_MAGIC: &[u8; 8] = b"GHULPND1";
/// Unfinished uploads by "folder/name": file key and when it was made.
type Pending = BTreeMap<String, (Vec<u8>, i64)>;
/// A pending upload's key is reused for this long.
const PENDING_MS: i64 = 7 * 86_400_000;
/// Largest block (the app reads files in chunks of this size).
pub const BLOCK_SIZE: u32 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Default)]
struct DriveSettings {
    /// Id of the server Drive uses (its own one in the Drive file, or one of the suite's).
    #[serde(default)]
    server: String,
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
    /// B3: the highest index revision seen per server. A lower
    /// one, or no index at all, means the server was reset: nothing is written then.
    #[serde(default)]
    seen: BTreeMap<String, u64>,
    /// The storage location the mail archive writes into (empty: the active one).
    #[serde(default)]
    archive: String,
}

impl Drop for DriveSettings {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.key);
    }
}

/// An older content of a file (kept when a file with the same name is uploaded again).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct Version {
    size: u64,
    modified: i64,
    blocks: Vec<String>,
    /// The key of this content (see [`FileKey`]).
    #[serde(default, with = "serde_bytes")]
    key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct Node {
    id: String,
    /// Empty for the top level.
    parent: String,
    name: String,
    dir: bool,
    size: u64,
    modified: i64,
    #[serde(default)]
    blocks: Vec<String>,
    /// In the trash since (Unix ms); 0 = not deleted.
    #[serde(default)]
    trashed: i64,
    #[serde(default)]
    favorite: bool,
    /// Older contents, newest first.
    #[serde(default)]
    versions: Vec<Version>,
    /// The key of the current content: each content has its own (see [`FileKey`]).
    #[serde(default, with = "serde_bytes")]
    key: Vec<u8>,
}

/// Older versions kept per file.
const MAX_VERSIONS: usize = 5;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub(super) struct DriveIndex {
    revision: u64,
    nodes: Vec<Node>,
}

/// A running upload: blocks written so far.
pub(super) struct Upload {
    /// The storage location it was started on: it is finished there, also after a switch.
    loc: Option<String>,
    parent: String,
    name: String,
    blocks: Vec<String>,
    size: u64,
    key: FileKey,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveItem {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub favorite: bool,
    /// Kept encrypted on this device as well (opens without network).
    pub offline: bool,
    /// In the trash since (Unix ms); 0 = not deleted.
    pub trashed: i64,
    /// Number of older versions.
    pub versions: u32,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveVersion {
    pub index: u32,
    pub size: u64,
    pub modified: i64,
}

/// Space used, by kind of file (bytes).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Default)]
pub struct DriveUsage {
    pub images: u64,
    pub videos: u64,
    pub audio: u64,
    pub documents: u64,
    pub other: u64,
    pub trash: u64,
    pub versions: u64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveStatus {
    /// `None`: GhulDrive is not set up.
    pub server_id: Option<String>,
    pub files: u32,
    pub bytes: u64,
}

struct Keys {
    index: Zeroizing<[u8; 32]>,
    /// Seals the keys of uploads that did not finish, on this device.
    pending: Zeroizing<[u8; 32]>,
}

impl Keys {
    fn new(dk: &[u8]) -> Keys {
        Keys { index: hkdf(dk, &[b"ghul/v1/drive/index"]), pending: hkdf(dk, &[b"ghul/v2/drive/pending"]) }
    }
}

/// The key of one content of a file (random, in the encrypted index). Its blocks are encrypted
/// with AES-256-GCM, which browsers can decrypt: a share link only needs this key, never the
/// Drive key, and nothing is copied. Block names come from this key as well, so equal content
/// in two files cannot be recognised on the server.
#[derive(Clone)]
pub(super) struct FileKey(Zeroizing<[u8; 32]>);

impl FileKey {
    fn random() -> FileKey {
        let mut k = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(k.as_mut());
        FileKey(k)
    }

    pub(super) fn of(bytes: &[u8]) -> Result<FileKey> {
        let k: [u8; 32] =
            bytes.try_into().map_err(|_| GhulError::Corrupted { msg: "Drive file key missing".into() })?;
        Ok(FileKey(Zeroizing::new(k)))
    }

    pub(super) fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn block_id(&self, data: &[u8]) -> String {
        let idk = crate::kdf::hkdf32(self.0.as_ref(), b"ghul/v2/drive/block-id");
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(idk.as_ref()).expect("any key size");
        m.update(data);
        crate::kdf::hex(&m.finalize().into_bytes())
    }

    fn cipher(&self) -> aes_gcm::Aes256Gcm {
        use aes_gcm::KeyInit;
        let bk = crate::kdf::hkdf32(self.0.as_ref(), b"ghul/v2/drive/block");
        aes_gcm::Aes256Gcm::new_from_slice(bk.as_ref()).expect("32 bytes")
    }

    /// `"GHULBLK2" ‖ nonce[12] ‖ AES-256-GCM(block key, data, aad = "GHULBLK2" ‖ id)`.
    fn seal_block(&self, id: &str, data: &[u8]) -> Vec<u8> {
        use aes_gcm::aead::{Aead, Payload};
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let aad = [BLOCK_MAGIC.as_slice(), id.as_bytes()].concat();
        let sealed = self
            .cipher()
            .encrypt(aes_gcm::Nonce::from_slice(&nonce), Payload { msg: data, aad: &aad })
            .expect("AES-GCM encryption of a block");
        [BLOCK_MAGIC.as_slice(), &nonce, &sealed].concat()
    }

    /// Opens a block and checks that it is the block `id` of this content.
    pub(super) fn open_block(&self, id: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        use aes_gcm::aead::{Aead, Payload};
        let bad = || GhulError::Corrupted { msg: "Drive data damaged or from another key".into() };
        if data.len() < 8 + 12 + 16 || &data[..8] != BLOCK_MAGIC {
            return Err(bad());
        }
        let aad = [BLOCK_MAGIC.as_slice(), id.as_bytes()].concat();
        let plain = self
            .cipher()
            .decrypt(aes_gcm::Nonce::from_slice(&data[8..20]), Payload { msg: &data[20..], aad: &aad })
            .map_err(|_| bad())?;
        let plain = Zeroizing::new(plain);
        if self.block_id(&plain) != id {
            return Err(GhulError::Corrupted { msg: "Drive block does not match".into() });
        }
        Ok(plain)
    }
}

/// A file of a share: its path in the share, size, key and blocks (see `app::shares`).
pub(super) struct SharedFile {
    pub path: String,
    pub size: u64,
    pub key: FileKey,
    pub blocks: Vec<String>,
}

/// The server Drive uses on this device.
fn drive_server_id(_o: &Open, d: &DriveSettings) -> Option<String> {
    (!d.server.is_empty()).then(|| d.server.clone())
}

thread_local! {
    /// The storage location the Drive calls of this thread work on instead of the active one
    /// (the mail archive into its own location, removing a location). Only within one call of
    /// the core: an app call never sees another call's location.
    static AT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with Drive's calls on this thread working on the location `id`.
pub(super) fn at_location<T>(id: Option<String>, f: impl FnOnce() -> T) -> T {
    let before = AT.with(|a| a.replace(id));
    let out = f();
    AT.with(|a| *a.borrow_mut() = before);
    out
}

/// How GhulDrive stores its files.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveMode {
    /// Not set up.
    None,
    /// On its own server, stored in the Drive file.
    Own,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveStorage {
    pub mode: DriveMode,
    /// The server in use; `None` when not set up.
    pub server: Option<ServerInfo>,
}

/// One of GhulDrive's storage locations.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveLocation {
    pub server: ServerInfo,
    /// Its files are the ones GhulDrive shows.
    pub active: bool,
    /// The mail archive writes into it.
    pub archive: bool,
}

/// Deletes a file; one that is not there is fine.
fn delete_quiet(fs: &dyn RemoteFs, path: &str) -> Result<()> {
    match fs.delete(path) {
        Ok(()) | Err(crate::net::NetError::NotFound(_)) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Lists a folder; one that is not there is empty.
fn list_quiet(fs: &dyn RemoteFs, dir: &str) -> Result<Vec<crate::net::remote::RemoteEntry>> {
    match fs.list(dir) {
        Ok(e) => Ok(e),
        Err(crate::net::NetError::NotFound(_)) => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Deletes GhulDrive's own files in a location's folder: its index, its blocks and its share
/// files. Nothing else, also not when the folder holds other files (e.g. an empty base folder:
/// the home directory).
fn delete_drive_files(fs: &dyn RemoteFs) -> Result<()> {
    for dir in list_quiet(fs, "drive/blocks")? {
        if !dir.is_dir || dir.name.len() != 2 || !is_hex(&dir.name) {
            continue;
        }
        let path = format!("drive/blocks/{}", dir.name);
        for f in list_quiet(fs, &path)? {
            if !f.is_dir && is_hex(&f.name) && f.name.starts_with(&dir.name) {
                delete_quiet(fs, &format!("{path}/{}", f.name))?;
            }
        }
    }
    delete_quiet(fs, INDEX)?;
    for f in list_quiet(fs, "shares")? {
        let ours = match f.name.rsplit_once('.') {
            _ if f.name.starts_with("check-") => true,
            Some((id, "json" | "state" | "lock")) => is_hex(id),
            _ => false,
        };
        if !f.is_dir && ours {
            delete_quiet(fs, &format!("shares/{}", f.name))?;
        }
    }
    Ok(())
}

/// Two servers with the same folder (the same files): host, port, user and folder (the path
/// the server reported, else the one entered).
fn same_folder(a: &StoredServer, b: &StoredServer) -> bool {
    let folder = |s: &StoredServer| {
        let f = if s.location.is_empty() { &s.base } else { &s.location };
        f.trim_end_matches('/').to_owned()
    };
    a.host.eq_ignore_ascii_case(&b.host) && a.port == b.port && a.user == b.user && folder(a) == folder(b)
}

pub(super) fn block_path(id: &str) -> String {
    format!("drive/blocks/{}/{}", &id[..2], id)
}

pub(super) fn encrypt(magic: &[u8; 8], key: &[u8; 32], plain: &[u8], aad_extra: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let aad = [magic.as_slice(), aad_extra].concat();
    let mut out = Vec::with_capacity(32 + plain.len() + 16);
    out.extend_from_slice(magic);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&seal(key, &nonce, plain, &aad));
    out
}

pub(super) fn decrypt(magic: &[u8; 8], key: &[u8; 32], data: &[u8], aad_extra: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let bad = || GhulError::Corrupted { msg: "Drive data damaged or from another key".into() };
    if data.len() < 32 + 16 || &data[..8] != magic {
        return Err(bad());
    }
    let nonce: [u8; 24] = data[8..32].try_into().expect("24 bytes");
    let aad = [magic.as_slice(), aad_extra].concat();
    open(key, &nonce, &data[32..], &aad).ok_or_else(bad)
}

fn load_index(fs: &dyn RemoteFs, keys: &Keys) -> Result<DriveIndex> {
    match fs.read(INDEX)? {
        None => Ok(DriveIndex::default()),
        Some(data) => {
            let plain = decrypt(INDEX_MAGIC, &keys.index, &data, b"")?;
            let cbor = unpad(&plain)?;
            ciborium::de::from_reader(cbor).map_err(|e| GhulError::Corrupted { msg: format!("Drive index: {e}") })
        }
    }
}

fn save_index(fs: &dyn RemoteFs, keys: &Keys, index: &mut DriveIndex) -> Result<()> {
    index.revision += 1;
    let mut cbor = Zeroizing::new(Vec::new());
    ciborium::ser::into_writer(&*index, &mut *cbor).map_err(|e| GhulError::Corrupted { msg: e.to_string() })?;
    let padded = pad(&cbor)?;
    fs.write(INDEX, &encrypt(INDEX_MAGIC, &keys.index, &padded, b""))?;
    Ok(())
}

fn item_with(n: &Node, offline: &BTreeSet<String>) -> DriveItem {
    DriveItem {
        id: n.id.clone(),
        parent: (!n.parent.is_empty()).then(|| n.parent.clone()),
        name: n.name.clone(),
        is_dir: n.dir,
        size: n.size,
        modified: n.modified,
        favorite: n.favorite,
        offline: offline.contains(&n.id),
        trashed: n.trashed,
        versions: n.versions.len() as u32,
    }
}

fn item(n: &Node) -> DriveItem {
    item_with(n, &BTreeSet::new())
}

/// Whether a node or one of its folders is in the trash.
fn in_trash(index: &DriveIndex, n: &Node) -> bool {
    let mut cur = Some(n);
    let mut steps = 0;
    while let Some(c) = cur {
        if c.trashed != 0 {
            return true;
        }
        if c.parent.is_empty() || steps > 1000 {
            return false;
        }
        steps += 1;
        cur = index.nodes.iter().find(|p| p.id == c.parent);
    }
    false
}

/// All blocks still referenced (current contents and versions).
fn used_blocks(index: &DriveIndex) -> BTreeSet<&String> {
    index.nodes.iter().flat_map(|n| n.blocks.iter().chain(n.versions.iter().flat_map(|v| v.blocks.iter()))).collect()
}

fn kind_of(name: &str) -> u8 {
    match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tif" | "tiff" | "raw" | "dng" => 0,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" | "3gp" => 1,
        "mp3" | "m4a" | "ogg" | "flac" | "wav" | "opus" | "aac" => 2,
        "pdf" | "txt" | "md" | "doc" | "docx" | "odt" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" | "csv"
        | "rtf" => 3,
        _ => 4,
    }
}

/// A name not yet used in the folder: "Foto.jpg" → "Foto (2).jpg".
fn free_name(index: &DriveIndex, parent: &str, name: &str, skip: Option<&str>) -> String {
    let taken = |n: &str| index.nodes.iter().any(|x| x.parent == parent && x.name == n && Some(x.id.as_str()) != skip);
    if !taken(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.to_owned(), String::new()),
    };
    (2..).map(|i| format!("{stem} ({i}){ext}")).find(|n| !taken(n)).expect("unbounded")
}

/// Characters that take no space on screen: a name made only of them looks empty.
fn invisible(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{180e}')
}

/// Direction overrides can hide the real file type ("rechnung\u{202e}fdp.exe" shows as "rechnungexe.pdf").
fn direction_override(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Text without spaces and invisible characters at both ends.
pub(crate) fn trim_invisible(s: &str) -> &str {
    s.trim_matches(invisible)
}

fn check_name(name: &str) -> Result<String> {
    let n = trim_invisible(name);
    let bad = n.is_empty()
        || n == "."
        || n == ".."
        || n.contains('/')
        || n.chars().any(|c| c.is_control() || direction_override(c))
        || n.len() > 255;
    if bad {
        return Err(GhulError::InvalidInput { msg: "invalid name".into() });
    }
    Ok(n.to_owned())
}

impl GhulSuite {
    /// Drive settings, the server connection and the keys.
    fn drive_ctx(&self, o: &mut Open) -> Result<(Arc<dyn RemoteFs>, Keys)> {
        let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        let server = self.drive_location(o).ok_or(GhulError::NotConfigured)?;
        if d.key.len() != 32 {
            return Err(GhulError::NotConfigured);
        }
        let fs = self.remote(o, &server)?;
        Ok((fs, Keys::new(&d.key)))
    }

    /// Checks the location Drive's calls work on: an existing index must open with Drive's key
    /// (otherwise it belongs to someone else); without one, an empty index is made.
    fn drive_attach_check(&self, o: &mut Open) -> Result<()> {
        let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        if d.key.len() != 32 {
            let mut k = vec![0u8; 32];
            OsRng.fill_bytes(&mut k);
            d.key = k;
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
        }
        let (fs, keys) = self.drive_ctx(o)?;
        let mut index = load_index(fs.as_ref(), &keys)?;
        if fs.read(INDEX)?.is_none() {
            save_index(fs.as_ref(), &keys, &mut index)?;
        }
        Ok(())
    }

    /// Points Drive at a server and checks it: creates Drive's key the first time, opens an
    /// existing index with it (or creates one) and saves.
    fn drive_attach(&self, o: &mut Open, server: String) -> Result<()> {
        self.ensure_area(o, VaultKind::Drive)?;
        let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        if d.key.len() != 32 {
            let mut k = vec![0u8; 32];
            OsRng.fill_bytes(&mut k);
            d.key = k;
        }
        let before = std::mem::replace(&mut d.server, server);
        put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = None;
        let checked = self.drive_attach_check(o);
        if let Err(e) = checked {
            d.server = before;
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            return Err(e);
        }
        self.save_area(o, VaultKind::Drive)
    }

    /// The server's index, refused if it is older than the newest this suite has seen (B3: a
    /// server that was reset or a replayed old index must not make files disappear).
    fn load_checked(&self, o: &mut Open, fs: &dyn RemoteFs, keys: &Keys) -> Result<DriveIndex> {
        let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        let server = self.drive_location(o).unwrap_or_default();
        let seen = d.seen.get(&server).copied().unwrap_or(0);
        let present = fs.read(INDEX)?.is_some();
        let index = load_index(fs, keys)?;
        if index.revision < seen || (!present && seen > 0) {
            return Err(GhulError::DriveRolledBack);
        }
        if index.revision > seen {
            self.note_seen(o, index.revision)?;
        }
        Ok(index)
    }

    /// Remembers `revision` as seen on Drive's server (in the Drive file).
    fn note_seen(&self, o: &mut Open, revision: u64) -> Result<()> {
        let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        let server = self.drive_location(o).unwrap_or_default();
        let e = d.seen.entry(server).or_insert(0);
        if revision <= *e {
            return Ok(());
        }
        *e = revision;
        put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
        // Only a note of what was seen on the server: no change to save into the suite file.
        self.save_area_quietly(o, VaultKind::Drive)
    }

    /// Loads the current index from the server, applies `f`, saves it. If another device saved
    /// the index in between, `f` is applied to that one instead (B3: no change is lost).
    fn drive_change<T>(
        &self,
        o: &mut Open,
        mut f: impl FnMut(&mut DriveIndex, &dyn RemoteFs, &Keys) -> Result<T>,
    ) -> Result<T> {
        let (fs, keys) = self.drive_ctx(o)?;
        for _ in 0..8 {
            let mut index = self.load_checked(o, fs.as_ref(), &keys)?;
            let base = index.revision;
            let out = f(&mut index, fs.as_ref(), &keys)?;
            if load_index(fs.as_ref(), &keys)?.revision != base {
                continue;
            }
            save_index(fs.as_ref(), &keys, &mut index)?;
            self.note_seen(o, index.revision)?;
            if self.drive_on_active(o) {
                self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = Some(index);
            }
            return Ok(out);
        }
        Err(GhulError::Network { msg: "GhulDrive is being changed on another device, please try again".into() })
    }

    fn drive_index(&self, o: &mut Open, refresh: bool) -> Result<DriveIndex> {
        // The cache holds the active location's index only.
        let active = self.drive_on_active(o);
        if !refresh && active {
            if let Some(i) = &self.net.lock().unwrap_or_else(|p| p.into_inner()).drive {
                return Ok(i.clone());
            }
        }
        let (fs, keys) = self.drive_ctx(o)?;
        let index = self.load_checked(o, fs.as_ref(), &keys)?;
        if active {
            self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = Some(index.clone());
        }
        Ok(index)
    }

    /// The storage location Drive's calls work on: this call's own one, else the active one.
    fn drive_location(&self, o: &Open) -> Option<String> {
        let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        AT.with(|a| a.borrow().clone()).or_else(|| drive_server_id(o, &d))
    }

    /// Whether this call works on the active location (only then the cached index is used).
    fn drive_on_active(&self, o: &Open) -> bool {
        let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        AT.with(|a| a.borrow().as_ref().map_or(true, |at| Some(at) == drive_server_id(o, &d).as_ref()))
    }

    fn pending_keys(&self, o: &Open) -> Option<(Zeroizing<[u8; 32]>, Pending)> {
        let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        if d.key.is_empty() {
            return None;
        }
        let key = Keys::new(&d.key).pending;
        let map = std::fs::read(self.local_dir.join(PENDING_FILE))
            .ok()
            .and_then(|data| decrypt(PENDING_MAGIC, &key, &data, b"").ok())
            .and_then(|plain| ciborium::de::from_reader::<Pending, _>(&plain[..]).ok())
            .unwrap_or_default();
        Some((key, map))
    }

    fn save_pending(&self, key: &[u8; 32], map: &Pending) {
        let path = self.local_dir.join(PENDING_FILE);
        if map.is_empty() {
            let _ = std::fs::remove_file(path);
            return;
        }
        let mut cbor = Zeroizing::new(Vec::new());
        if ciborium::ser::into_writer(map, &mut *cbor).is_ok() {
            let _ = std::fs::create_dir_all(&self.local_dir);
            let _ = std::fs::write(path, encrypt(PENDING_MAGIC, key, &cbor, b""));
        }
    }

    /// The key for an upload: the one of an unfinished upload of the same file (its blocks are
    /// there already and are skipped), else a new one, remembered until the upload finishes.
    fn pending_key(&self, o: &Open, parent: &str, name: &str) -> Result<FileKey> {
        let Some((key, mut map)) = self.pending_keys(o) else { return Ok(FileKey::random()) };
        let now = now_ms();
        map.retain(|_, (_, at)| now - *at < PENDING_MS);
        let slot = format!("{parent}/{name}");
        // A key some content already has is never used for another one (a link to that content
        // would open the new one as well), e.g. after an upload finished but was not forgotten.
        let in_use = |k: &[u8]| {
            let cache = self.net.lock().unwrap_or_else(|p| p.into_inner());
            cache
                .drive
                .as_ref()
                .map_or(true, |i| i.nodes.iter().any(|n| n.key == k || n.versions.iter().any(|v| v.key == k)))
        };
        if let Some((k, _)) = map.get(&slot) {
            if let Ok(fk) = FileKey::of(k) {
                if !in_use(k) {
                    return Ok(fk);
                }
            }
        }
        let fk = FileKey::random();
        map.insert(slot, (fk.bytes().to_vec(), now));
        self.save_pending(&key, &map);
        Ok(fk)
    }

    fn forget_pending(&self, o: &Open, parent: &str, name: &str) {
        if let Some((key, mut map)) = self.pending_keys(o) {
            if map.remove(&format!("{parent}/{name}")).is_some() {
                self.save_pending(&key, &map);
            }
        }
    }

    /// The folder `name` below `parent` (not in the trash), created when there is none.
    pub(super) fn drive_folder_named(&self, parent: Option<&str>, name: &str) -> Result<String> {
        let name = check_name(name)?;
        let found = self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let p = parent.unwrap_or_default();
            Ok(index
                .nodes
                .iter()
                .find(|n| n.dir && n.parent == p && n.name == name && !in_trash(&index, n))
                .map(|n| n.id.clone()))
        })?;
        match found {
            Some(id) => Ok(id),
            None => self.drive_create_folder(parent.map(str::to_owned), name),
        }
    }

    /// Whether `id` is a folder that is not in the trash.
    pub(super) fn drive_folder_alive(&self, id: &str) -> Result<bool> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            Ok(index.nodes.iter().any(|n| n.id == id && n.dir && !in_trash(&index, n)))
        })
    }

    /// Stores `data` as a file in `folder` (in parts, like an upload from the app).
    pub(super) fn drive_put_file(&self, folder: Option<String>, name: String, data: &[u8]) -> Result<String> {
        let upload = self.drive_upload_begin(folder, name)?;
        let parts =
            data.chunks(BLOCK_SIZE as usize).try_for_each(|c| self.drive_upload_chunk(upload.clone(), c.to_vec()));
        match parts.and_then(|()| self.drive_upload_finish(upload.clone())) {
            Ok(id) => Ok(id),
            Err(e) => {
                let _ = self.drive_upload_cancel(upload);
                Err(e)
            }
        }
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Sets up GhulDrive on a stored server (creates the Drive key once; keeps an existing Drive).
    /// A server of the suite becomes one of Drive's storage locations (a copy in the Drive file).
    pub fn drive_setup(&self, server_id: String) -> Result<()> {
        self.with_open(|o| {
            let srv = servers::server(o, &server_id)?;
            self.ensure_area(o, VaultKind::Drive)?;
            let mut own = servers::drive_servers(o);
            if !own.iter().any(|s| s.id == server_id) {
                own.push(srv);
                put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
            }
            self.drive_attach(o, server_id)
        })
    }

    /// Checks a server and adds it to GhulDrive's storage locations (in the Drive file). The first location becomes the active one; a further
    /// one is only added (switch to it with `drive_switch_location`). Fingerprints as with
    /// `save_server`.
    pub fn drive_save_server(&self, input: ServerInput, confirmed: Option<String>) -> Result<ServerSaved> {
        self.with_open(|o| {
            self.ensure_area(o, VaultKind::Drive)?;
            let stored = match servers::check_server(o, &input, confirmed.as_deref())? {
                Ok(s) => s,
                Err(fingerprint) => return Ok(ServerSaved::Confirm { fingerprint }),
            };
            let id = stored.id.clone();
            let mut own = servers::drive_servers(o);
            // The same folder twice would be two locations with one set of files: removing one
            // with its files would take the other one's as well.
            if own.iter().any(|s| same_folder(s, &stored)) {
                return Err(GhulError::AlreadyExists);
            }
            own.push(stored);
            put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let first = drive_server_id(o, &d).is_none();
            let attached = if first {
                self.drive_attach(o, id.clone())
            } else {
                // Checked like the active one (an index there must open with Drive's key), but
                // the active location stays.
                at_location(Some(id.clone()), || self.drive_attach_check(o))
            };
            if let Err(e) = attached {
                own.retain(|s| s.id != id);
                put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
                return Err(e);
            }
            // From the second location on, the mail archive stays where it is (it does not
            // follow a later switch).
            if !first {
                self.drive_pin_archive(o)?;
            }
            self.save_area(o, VaultKind::Drive)?;
            Ok(ServerSaved::Saved { id })
        })
    }

    /// Where Drive stores its files.
    pub fn drive_storage(&self) -> Result<DriveStorage> {
        self.with_open(|o| {
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let mode = if d.server.is_empty() || d.key.len() != 32 { DriveMode::None } else { DriveMode::Own };
            let server = drive_server_id(o, &d).and_then(|id| servers::server(o, &id).ok()).map(|s| servers::info(&s));
            Ok(DriveStorage { mode, server })
        })
    }

    /// GhulDrive's storage locations: each its own server with its own files, one of them active.
    pub fn drive_locations(&self) -> Result<Vec<DriveLocation>> {
        self.with_open(|o| {
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let active = drive_server_id(o, &d).unwrap_or_default();
            let own = servers::drive_servers(o);
            let archive = if own.iter().any(|s| s.id == d.archive) { d.archive.clone() } else { active.clone() };
            Ok(own
                .iter()
                .map(|s| DriveLocation { server: servers::info(s), active: s.id == active, archive: s.id == archive })
                .collect())
        })
    }

    /// Makes another storage location the active one (its files are shown from now on).
    pub fn drive_switch_location(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            if !servers::drive_servers(o).iter().any(|s| s.id == id) {
                return Err(GhulError::NotFound);
            }
            self.drive_pin_archive(o)?;
            self.drive_attach(o, id)
        })
    }

    /// The storage location the mail archive writes into.
    pub fn drive_set_archive_location(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            if !servers::drive_servers(o).iter().any(|s| s.id == id) {
                return Err(GhulError::NotFound);
            }
            let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            d.archive = id;
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            self.save_area(o, VaultKind::Drive)
        })
    }

    /// Removes a storage location. Its share links end first (on its server; when that cannot
    /// be reached nothing is removed, unless `force`: then the links are only forgotten here
    /// and nothing is deleted on the server). With `delete_files` GhulDrive's own files there
    /// (index, blocks, share files) and its server module are deleted as well, never in a folder
    /// another location or a server of the suite uses; otherwise they stay and come back when
    /// the folder is added again. Another location becomes active if this one was.
    pub fn drive_remove_location(&self, id: String, delete_files: bool, force: bool) -> Result<()> {
        self.with_open(|o| {
            let srv = servers::drive_servers(o).into_iter().find(|s| s.id == id).ok_or(GhulError::NotFound)?;
            let delete_files = delete_files && !force;
            if delete_files {
                let shared = servers::servers(o)
                    .iter()
                    .chain(servers::drive_servers(o).iter())
                    .any(|s| s.id != id && same_folder(s, &srv));
                if shared {
                    return Err(GhulError::InvalidInput {
                        msg: "another storage location or server uses this folder".into(),
                    });
                }
            }
            at_location(Some(id.clone()), || -> Result<()> {
                self.shares_end_on_location(o, &id, force)?;
                if delete_files {
                    let fs = self.remote(o, &id)?;
                    delete_drive_files(fs.as_ref())?;
                    if let Some(m) = &srv.module {
                        delete_quiet(servers::open_fs(o, &srv, Some(&m.dir))?.as_ref(), servers::MODULE_FILE)?;
                    }
                }
                Ok(())
            })?;
            let own: Vec<StoredServer> = servers::drive_servers(o).into_iter().filter(|s| s.id != id).collect();
            put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
            let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            d.seen.remove(&id);
            if d.archive == id {
                d.archive.clear();
            }
            let was_active = d.server == id;
            if was_active {
                d.server = own.first().map(|s| s.id.clone()).unwrap_or_default();
            }
            if d.archive.is_empty() && own.len() > 1 {
                d.archive = d.server.clone();
            }
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            {
                let mut net = self.net.lock().unwrap_or_else(|p| p.into_inner());
                net.conn = None;
                if was_active {
                    net.drive = None;
                }
            }
            self.save_area(o, VaultKind::Drive)
        })
    }

    pub fn drive_status(&self) -> Result<DriveStatus> {
        self.with_open(|o| {
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let Some(server) = drive_server_id(o, &d) else {
                return Ok(DriveStatus { server_id: None, files: 0, bytes: 0 });
            };
            let index = self.drive_index(o, false)?;
            let files = index.nodes.iter().filter(|n| !n.dir && !in_trash(&index, n));
            Ok(DriveStatus {
                server_id: Some(server),
                files: files.clone().count() as u32,
                bytes: files.map(|n| n.size).sum(),
            })
        })
    }

    /// Contents of a folder (`None`: top level), folders first, then by name.
    pub fn drive_list(&self, folder: Option<String>, refresh: bool) -> Result<Vec<DriveItem>> {
        self.with_open(|o| {
            let index = self.drive_index(o, refresh)?;
            let parent = folder.unwrap_or_default();
            let offline = self.offline_ids();
            let mut out: Vec<DriveItem> = index
                .nodes
                .iter()
                .filter(|n| n.parent == parent && !in_trash(&index, n))
                .map(|n| item_with(n, &offline))
                .collect();
            out.sort_by_key(|i| (!i.is_dir, i.name.to_lowercase()));
            Ok(out)
        })
    }

    /// Files and folders whose name contains `query` (case-insensitive).
    pub fn drive_search(&self, query: String) -> Result<Vec<DriveItem>> {
        let q = query.trim().to_lowercase();
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let offline = self.offline_ids();
            Ok(index
                .nodes
                .iter()
                .filter(|n| !q.is_empty() && n.name.to_lowercase().contains(&q) && !in_trash(&index, n))
                .map(|n| item_with(n, &offline))
                .collect())
        })
    }

    pub fn drive_create_folder(&self, parent: Option<String>, name: String) -> Result<String> {
        let name = check_name(&name)?;
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let parent = parent.clone().unwrap_or_default();
                if !parent.is_empty() && !index.nodes.iter().any(|n| n.id == parent && n.dir) {
                    return Err(GhulError::NotFound);
                }
                let id = crate::kdf::hex(&new_id());
                let name = free_name(index, &parent, &name, None);
                index.nodes.push(Node {
                    id: id.clone(),
                    parent,
                    name,
                    dir: true,
                    size: 0,
                    modified: now_ms(),
                    blocks: vec![],
                    ..Default::default()
                });
                Ok(id)
            })
        })
    }

    pub fn drive_rename(&self, id: String, name: String) -> Result<()> {
        let name = check_name(&name)?;
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let parent = index.nodes.iter().find(|n| n.id == id).ok_or(GhulError::NotFound)?.parent.clone();
                let name = free_name(index, &parent, &name, Some(&id));
                let n = index.nodes.iter_mut().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
                n.name = name;
                n.modified = now_ms();
                Ok(())
            })
        })
    }

    /// Moves an item into another folder (`None`: top level).
    pub fn drive_move(&self, id: String, folder: Option<String>) -> Result<()> {
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let target = folder.clone().unwrap_or_default();
                // A folder must not be moved into itself or below.
                let mut cur = target.clone();
                while !cur.is_empty() {
                    if cur == id {
                        return Err(GhulError::InvalidInput { msg: "cannot move a folder into itself".into() });
                    }
                    cur = index.nodes.iter().find(|n| n.id == cur && n.dir).ok_or(GhulError::NotFound)?.parent.clone();
                }
                let name = index.nodes.iter().find(|n| n.id == id).ok_or(GhulError::NotFound)?.name.clone();
                let name = free_name(index, &target, &name, Some(&id));
                let n = index.nodes.iter_mut().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
                n.parent = target;
                n.name = name;
                Ok(())
            })
        })
    }

    /// Moves a file or folder into the trash (restorable until it is emptied).
    pub fn drive_delete(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let n = index.nodes.iter_mut().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
                n.trashed = now_ms();
                Ok(())
            })
        })
    }

    /// Deletes a file or a folder with everything in it for good; unused blocks are removed.
    pub fn drive_delete_forever(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            // Which blocks share links still hand out, before anything changes: if that cannot
            // be read, nothing is deleted (and the index stays as it is, no block is orphaned).
            let pinned = {
                let (fs, _) = self.drive_ctx(o)?;
                super::shares::pinned_blocks(fs.as_ref())?
            };
            let unused = self.drive_change(o, |index, _, _| {
                let mut remove: BTreeSet<String> = BTreeSet::from([id.clone()]);
                loop {
                    let more: Vec<String> = index
                        .nodes
                        .iter()
                        .filter(|n| remove.contains(&n.parent) && !remove.contains(&n.id))
                        .map(|n| n.id.clone())
                        .collect();
                    if more.is_empty() {
                        break;
                    }
                    remove.extend(more);
                }
                if !index.nodes.iter().any(|n| n.id == id) {
                    return Err(GhulError::NotFound);
                }
                let freed: BTreeSet<String> = index
                    .nodes
                    .iter()
                    .filter(|n| remove.contains(&n.id))
                    .flat_map(|n| n.blocks.iter().cloned())
                    .collect();
                let freed: BTreeSet<String> = freed
                    .into_iter()
                    .chain(
                        index
                            .nodes
                            .iter()
                            .filter(|n| remove.contains(&n.id))
                            .flat_map(|n| n.versions.iter().flat_map(|v| v.blocks.iter().cloned())),
                    )
                    .collect();
                index.nodes.retain(|n| !remove.contains(&n.id));
                let still = used_blocks(index);
                Ok(freed.into_iter().filter(|b| !still.contains(b)).collect::<Vec<String>>())
            })?;
            // Only once the index without them is saved: a retry must not lose blocks. Blocks a
            // share link still hands out stay until the link ends; read again, since another
            // device may have shared the file while the index was being saved.
            let (fs, _) = self.drive_ctx(o)?;
            // Unreadable now: nothing is deleted (blocks may be orphaned, no live link breaks).
            let again = super::shares::pinned_blocks(fs.as_ref())?;
            let pinned: BTreeSet<String> = pinned.into_iter().chain(again).collect();
            for b in unused.iter().filter(|b| !pinned.contains(*b)) {
                fs.delete(&block_path(b))?;
            }
            Ok(())
        })
    }

    /// After `DriveRolledBack`: takes the server's index as it is (e.g. a server restored from
    /// a backup on purpose). Files added since that state are then no longer listed.
    pub fn drive_accept_server_state(&self) -> Result<()> {
        self.with_open(|o| {
            let (fs, keys) = self.drive_ctx(o)?;
            let index = load_index(fs.as_ref(), &keys)?;
            let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let server = self.drive_location(o).unwrap_or_default();
            d.seen.insert(server, index.revision);
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            self.save_area(o, VaultKind::Drive)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = Some(index);
            Ok(())
        })
    }

    /// Starts an upload into `folder`; returns the upload id for `drive_upload_chunk`.
    pub fn drive_upload_begin(&self, folder: Option<String>, name: String) -> Result<String> {
        let name = check_name(&name)?;
        self.with_open(|o| {
            self.drive_ctx(o)?;
            let id = crate::kdf::hex(&new_id());
            let parent = folder.unwrap_or_default();
            self.drive_index(o, false)?;
            let key = self.pending_key(o, &parent, &name)?;
            let up = Upload { loc: self.drive_location(o), parent, name, blocks: vec![], size: 0, key };
            self.net.lock().unwrap_or_else(|p| p.into_inner()).uploads.insert(id.clone(), up);
            Ok(id)
        })
    }

    /// Encrypts and uploads the next part of the file (at most `BLOCK_SIZE` bytes).
    pub fn drive_upload_chunk(&self, upload: String, data: Vec<u8>) -> Result<()> {
        let data = Zeroizing::new(data);
        if data.len() > BLOCK_SIZE as usize {
            return Err(GhulError::InvalidInput { msg: "chunk too large".into() });
        }
        let loc = self.upload_location(&upload).ok_or(GhulError::NotFound)?;
        at_location(loc, || {
            self.with_open(|o| {
                let (fs, _) = self.drive_ctx(o)?;
                let key = {
                    let cache = self.net.lock().unwrap_or_else(|p| p.into_inner());
                    cache.uploads.get(&upload).ok_or(GhulError::NotFound)?.key.clone()
                };
                let id = key.block_id(&data);
                let known = self.drive_on_active(o)
                    && self
                        .net
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .drive
                        .as_ref()
                        .is_some_and(|i| i.nodes.iter().any(|n| n.blocks.contains(&id)));
                // After an interrupted upload the same blocks may already be there: skip them
                // (blocks are named by their content, so the upload continues where it stopped).
                let path = block_path(&id);
                let there = !known && {
                    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
                    fs.list(dir).map(|l| l.iter().any(|e| e.name == name)).unwrap_or(false)
                };
                if !known && !there {
                    fs.write(&path, &key.seal_block(&id, &data))?;
                }
                let mut cache = self.net.lock().unwrap_or_else(|p| p.into_inner());
                let up = cache.uploads.get_mut(&upload).ok_or(GhulError::NotFound)?;
                up.blocks.push(id);
                up.size += data.len() as u64;
                Ok(())
            })
        })
    }

    /// The location an upload writes into (`None` inside: the active one at its start).
    fn upload_location(&self, upload: &str) -> Option<Option<String>> {
        self.net.lock().unwrap_or_else(|p| p.into_inner()).uploads.get(upload).map(|u| u.loc.clone())
    }

    /// Finishes an upload: the file appears in the folder. Returns its id.
    pub fn drive_upload_finish(&self, upload: String) -> Result<String> {
        let loc = self.upload_location(&upload).ok_or(GhulError::NotFound)?;
        at_location(loc, || {
            self.with_open(|o| {
                let up = self
                    .net
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .uploads
                    .remove(&upload)
                    .ok_or(GhulError::NotFound)?;
                self.drive_change(o, |index, _, _| {
                    // Same name in the same folder: the old content becomes a version.
                    if let Some(n) = index
                        .nodes
                        .iter_mut()
                        .find(|n| n.parent == up.parent && n.name == up.name && !n.dir && n.trashed == 0)
                    {
                        if n.blocks != up.blocks {
                            let old = Version {
                                size: n.size,
                                modified: n.modified,
                                blocks: std::mem::take(&mut n.blocks),
                                key: std::mem::take(&mut n.key),
                            };
                            n.versions.insert(0, old);
                            n.versions.truncate(MAX_VERSIONS);
                        }
                        n.blocks = up.blocks.clone();
                        n.key = up.key.bytes().to_vec();
                        n.size = up.size;
                        n.modified = now_ms();
                        return Ok(n.id.clone());
                    }
                    let id = crate::kdf::hex(&new_id());
                    let name = free_name(index, &up.parent, &up.name, None);
                    index.nodes.push(Node {
                        id: id.clone(),
                        parent: up.parent.clone(),
                        name,
                        dir: false,
                        size: up.size,
                        modified: now_ms(),
                        blocks: up.blocks.clone(),
                        key: up.key.bytes().to_vec(),
                        ..Default::default()
                    });
                    Ok(id)
                })
                .inspect(|_| self.forget_pending(o, &up.parent, &up.name))
            })
        })
    }

    /// Cancels an upload (blocks already written are removed when unused).
    pub fn drive_upload_cancel(&self, upload: String) -> Result<()> {
        let Some(loc) = self.upload_location(&upload) else { return Ok(()) };
        at_location(loc, || {
            self.with_open(|o| {
                // Kept until its blocks are gone, so a failed cancel can be repeated.
                let up = self
                    .net
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .uploads
                    .get(&upload)
                    .map(|u| (u.parent.clone(), u.name.clone(), u.blocks.clone()));
                let Some((parent, name, blocks)) = up else { return Ok(()) };
                let (fs, keys) = self.drive_ctx(o)?;
                let index = load_index(fs.as_ref(), &keys)?;
                let used = used_blocks(&index);
                let pinned = super::shares::pinned_blocks(fs.as_ref())?;
                for b in blocks.iter().filter(|b| !used.contains(b) && !pinned.contains(*b)) {
                    fs.delete(&block_path(b))?;
                }
                self.net.lock().unwrap_or_else(|p| p.into_inner()).uploads.remove(&upload);
                self.forget_pending(o, &parent, &name);
                Ok(())
            })
        })
    }

    /// Number of parts of a file, to be fetched with `drive_download_block`.
    pub fn drive_file_blocks(&self, id: String) -> Result<u32> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let n = index.nodes.iter().find(|n| n.id == id && !n.dir).ok_or(GhulError::NotFound)?;
            Ok(n.blocks.len() as u32)
        })
    }

    /// Downloads and decrypts part `part` of a file.
    pub fn drive_download_block(&self, id: String, part: u32) -> Result<Vec<u8>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let n = index.nodes.iter().find(|n| n.id == id && !n.dir).ok_or(GhulError::NotFound)?;
            let block = n.blocks.get(part as usize).ok_or(GhulError::NotFound)?.clone();
            let key = FileKey::of(&n.key)?;
            let (fs, _) = self.drive_ctx(o)?;
            let data = match std::fs::read(self.offline_dir().join(&block)) {
                Ok(d) => d,
                Err(_) => fs
                    .read(&block_path(&block))?
                    .ok_or_else(|| GhulError::Corrupted { msg: "Drive block missing".into() })?,
            };
            Ok(key.open_block(&block, &data)?.to_vec())
        })
    }
}

impl GhulSuite {
    fn offline_dir(&self) -> std::path::PathBuf {
        self.local_dir.join("drive-offline")
    }

    /// Files kept on this device (IDs; the blocks themselves stay encrypted).
    fn offline_ids(&self) -> BTreeSet<String> {
        std::fs::read_to_string(self.offline_dir().join("files"))
            .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default()
    }

    fn save_offline_ids(&self, ids: &BTreeSet<String>) -> Result<()> {
        let dir = self.offline_dir();
        std::fs::create_dir_all(&dir).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        let text: String = ids.iter().map(|i| format!("{i}\n")).collect();
        std::fs::write(dir.join("files"), text).map_err(|e| GhulError::Storage { msg: e.to_string() })
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Files and folders in the trash (top level of what was deleted), newest first.
    pub fn drive_trash(&self) -> Result<Vec<DriveItem>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let mut out: Vec<DriveItem> = index.nodes.iter().filter(|n| n.trashed != 0).map(item).collect();
            out.sort_by_key(|i| std::cmp::Reverse(i.trashed));
            Ok(out)
        })
    }

    /// Takes an item out of the trash (into the top level if its folder is gone or deleted).
    pub fn drive_restore(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let parent = index.nodes.iter().find(|n| n.id == id).ok_or(GhulError::NotFound)?.parent.clone();
                let parent_ok =
                    parent.is_empty() || index.nodes.iter().any(|p| p.id == parent && p.dir && !in_trash(index, p));
                let parent = if parent_ok { parent } else { String::new() };
                let name = index.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
                let name = free_name(index, &parent, &name, Some(&id));
                let n = index.nodes.iter_mut().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
                n.trashed = 0;
                n.parent = parent;
                n.name = name;
                Ok(())
            })
        })
    }

    /// Deletes trash items older than `days` days for good (0: everything in the trash).
    pub fn drive_empty_trash(&self, days: u32) -> Result<u32> {
        let limit = now_ms() - days as i64 * 86_400_000;
        let ids: Vec<String> = self.with_open(|o| {
            let index = self.drive_index(o, true)?;
            Ok(index
                .nodes
                .iter()
                .filter(|n| n.trashed != 0 && (days == 0 || n.trashed < limit))
                .map(|n| n.id.clone())
                .collect())
        })?;
        for id in &ids {
            self.drive_delete_forever(id.clone())?;
        }
        Ok(ids.len() as u32)
    }

    pub fn drive_set_favorite(&self, id: String, favorite: bool) -> Result<()> {
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                index.nodes.iter_mut().find(|n| n.id == id).ok_or(GhulError::NotFound)?.favorite = favorite;
                Ok(())
            })
        })
    }

    /// Favourite files and folders.
    pub fn drive_favorites(&self) -> Result<Vec<DriveItem>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let offline = self.offline_ids();
            let mut out: Vec<DriveItem> = index
                .nodes
                .iter()
                .filter(|n| n.favorite && !in_trash(&index, n))
                .map(|n| item_with(n, &offline))
                .collect();
            out.sort_by_key(|i| (!i.is_dir, i.name.to_lowercase()));
            Ok(out)
        })
    }

    /// Recently changed files, newest first.
    pub fn drive_recent(&self, limit: u32) -> Result<Vec<DriveItem>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let offline = self.offline_ids();
            let mut files: Vec<&Node> = index.nodes.iter().filter(|n| !n.dir && !in_trash(&index, n)).collect();
            files.sort_by_key(|n| std::cmp::Reverse(n.modified));
            Ok(files.into_iter().take(limit as usize).map(|n| item_with(n, &offline)).collect())
        })
    }

    /// Files kept on this device.
    pub fn drive_offline_files(&self) -> Result<Vec<DriveItem>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let offline = self.offline_ids();
            Ok(index
                .nodes
                .iter()
                .filter(|n| offline.contains(&n.id) && !in_trash(&index, n))
                .map(|n| item_with(n, &offline))
                .collect())
        })
    }

    /// Keeps a file (still encrypted) on this device so it opens without network, or removes it.
    pub fn drive_set_offline(&self, id: String, offline: bool) -> Result<()> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let n = index.nodes.iter().find(|n| n.id == id && !n.dir).ok_or(GhulError::NotFound)?.clone();
            let mut ids = self.offline_ids();
            let dir = self.offline_dir();
            std::fs::create_dir_all(&dir).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            if offline {
                let (fs, _) = self.drive_ctx(o)?;
                for b in &n.blocks {
                    let p = dir.join(b);
                    if !p.exists() {
                        let data = fs
                            .read(&block_path(b))?
                            .ok_or_else(|| GhulError::Corrupted { msg: "Drive block missing".into() })?;
                        std::fs::write(&p, data).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
                    }
                }
                ids.insert(id);
            } else {
                ids.remove(&id);
                let keep: BTreeSet<&String> =
                    index.nodes.iter().filter(|x| ids.contains(&x.id)).flat_map(|x| x.blocks.iter()).collect();
                for b in n.blocks.iter().filter(|b| !keep.contains(b)) {
                    let _ = std::fs::remove_file(dir.join(b));
                }
            }
            self.save_offline_ids(&ids)
        })
    }

    pub fn drive_versions(&self, id: String) -> Result<Vec<DriveVersion>> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let n = index.nodes.iter().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
            Ok(n.versions
                .iter()
                .enumerate()
                .map(|(i, v)| DriveVersion { index: i as u32, size: v.size, modified: v.modified })
                .collect())
        })
    }

    /// Makes an older version the current content (the current one becomes a version).
    pub fn drive_restore_version(&self, id: String, index_no: u32) -> Result<()> {
        self.with_open(|o| {
            self.drive_change(o, |index, _, _| {
                let n = index.nodes.iter_mut().find(|n| n.id == id && !n.dir).ok_or(GhulError::NotFound)?;
                if index_no as usize >= n.versions.len() {
                    return Err(GhulError::NotFound);
                }
                let v = n.versions.remove(index_no as usize);
                let cur = Version {
                    size: n.size,
                    modified: n.modified,
                    blocks: std::mem::take(&mut n.blocks),
                    key: std::mem::take(&mut n.key),
                };
                n.versions.insert(0, cur);
                n.blocks = v.blocks;
                n.key = v.key;
                n.size = v.size;
                n.modified = now_ms();
                Ok(())
            })
        })
    }

    /// Space used by kind of file, plus trash and older versions.
    pub fn drive_usage(&self) -> Result<DriveUsage> {
        self.with_open(|o| {
            let index = self.drive_index(o, false)?;
            let mut u = DriveUsage::default();
            for n in index.nodes.iter().filter(|n| !n.dir) {
                u.versions += n.versions.iter().map(|v| v.size).sum::<u64>();
                if in_trash(&index, n) {
                    u.trash += n.size;
                    continue;
                }
                match kind_of(&n.name) {
                    0 => u.images += n.size,
                    1 => u.videos += n.size,
                    2 => u.audio += n.size,
                    3 => u.documents += n.size,
                    _ => u.other += n.size,
                }
            }
            Ok(u)
        })
    }

    /// The chosen files (folders with their content) as a ZIP encrypted with AES-256 and a
    /// password, for e-mail or a USB stick. At most 200 MB.
    pub fn drive_zip(&self, ids: Vec<String>, password: String) -> Result<Vec<u8>> {
        let password = Zeroizing::new(password);
        if password.chars().count() < 8 {
            return Err(GhulError::InvalidInput { msg: "password too short (at least 8 characters)".into() });
        }
        self.with_open(|o| {
            let mut files = Vec::new();
            for id in &ids {
                files.extend(self.drive_collect(o, id, 200 * 1024 * 1024)?);
            }
            if files.iter().map(|(_, d)| d.len() as u64).sum::<u64>() > 200 * 1024 * 1024 {
                return Err(GhulError::InvalidInput { msg: "too large (max 200 MB)".into() });
            }
            let mut out = std::io::Cursor::new(Vec::new());
            {
                use std::io::Write;
                let mut z = zip::ZipWriter::new(&mut out);
                let err = |e: zip::result::ZipError| GhulError::Storage { msg: e.to_string() };
                for (name, data) in &files {
                    let opts = zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .with_aes_encryption(zip::AesMode::Aes256, password.as_str());
                    z.start_file(name.as_str(), opts).map_err(err)?;
                    z.write_all(data).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
                }
                z.finish().map_err(err)?;
            }
            Ok(out.into_inner())
        })
    }
}

#[cfg(test)]
impl GhulSuite {
    /// Uses a folder instead of a server (tests).
    pub(crate) fn drive_use_folder(&self, fs: Arc<dyn RemoteFs>) -> Result<()> {
        self.with_open(|o| {
            self.ensure_area(o, VaultKind::Drive)?;
            let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            if d.key.len() != 32 {
                d.key = vec![7u8; 32];
            }
            d.server = "test".into();
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            Ok(())
        })?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = Some(("test".into(), fs));
        Ok(())
    }
}

impl GhulSuite {
    /// The server GhulDrive uses on this device.
    pub(super) fn drive_server(&self, o: &Open) -> Option<String> {
        self.drive_location(o)
    }

    /// The location a share made on server `id` belongs to.
    pub(super) fn drive_location_of(&self, _o: &Open, id: &str) -> String {
        id.to_owned()
    }

    /// The location the mail archive writes into: the chosen one, else the active one, which
    /// is then kept (the archive does not follow a later switch).
    pub(super) fn drive_archive_location(&self) -> Result<Option<String>> {
        self.with_open(|o| {
            self.drive_pin_archive(o)?;
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let own = servers::drive_servers(o);
            if own.iter().any(|s| s.id == d.archive) {
                return Ok(Some(d.archive.clone()));
            }
            Ok(drive_server_id(o, &d))
        })
    }

    /// Without a chosen archive location: the active one becomes it (stored).
    fn drive_pin_archive(&self, o: &mut Open) -> Result<()> {
        let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        if servers::drive_servers(o).iter().any(|s| s.id == d.archive) {
            return Ok(());
        }
        let Some(active) = drive_server_id(o, &d) else { return Ok(()) };
        d.archive = active;
        put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
        self.save_area(o, VaultKind::Drive)
    }

    /// The connection to GhulDrive's server (rooted at its GhulSuite folder).
    pub(super) fn drive_fs(&self, o: &mut Open) -> Result<Arc<dyn RemoteFs>> {
        Ok(self.drive_ctx(o)?.0)
    }

    /// The files of these items as they are now (folders with everything in them, not what is
    /// in the trash): path, size, file key and blocks. Nothing is downloaded.
    pub(super) fn drive_snapshot(&self, o: &mut Open, ids: &[String]) -> Result<Vec<SharedFile>> {
        let index = self.drive_index(o, true)?;
        let mut out = Vec::new();
        for id in ids {
            let root = index.nodes.iter().find(|n| n.id == *id && !in_trash(&index, n)).ok_or(GhulError::NotFound)?;
            let mut todo = vec![(root.name.clone(), root)];
            while let Some((path, n)) = todo.pop() {
                if n.dir {
                    for c in index.nodes.iter().filter(|c| c.parent == n.id && c.trashed == 0) {
                        todo.push((format!("{path}/{}", c.name), c));
                    }
                } else {
                    out.push(SharedFile { path, size: n.size, key: FileKey::of(&n.key)?, blocks: n.blocks.clone() });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Encrypts content that is not in GhulDrive (e.g. a login shared as text) into blocks on
    /// GhulDrive's server, under a new file key; the blocks belong to no file.
    pub(super) fn drive_put_loose(&self, o: &mut Open, name: &str, data: &[u8]) -> Result<SharedFile> {
        let (fs, _) = self.drive_ctx(o)?;
        let key = FileKey::random();
        let mut blocks = Vec::new();
        for part in data.chunks(BLOCK_SIZE as usize) {
            let id = key.block_id(part);
            fs.write(&block_path(&id), &key.seal_block(&id, part))?;
            blocks.push(id);
        }
        Ok(SharedFile { path: name.to_owned(), size: data.len() as u64, key, blocks })
    }

    /// The whole content of a (small) file, decrypted.
    pub(super) fn drive_read_all(&self, id: &str) -> Result<Zeroizing<Vec<u8>>> {
        let mut out = Zeroizing::new(Vec::new());
        for part in 0..self.drive_file_blocks(id.to_owned())? {
            out.extend_from_slice(&self.drive_download_block(id.to_owned(), part)?);
        }
        Ok(out)
    }

    /// Of these blocks, those no file of GhulDrive uses (current contents and versions).
    pub(super) fn drive_unused(&self, o: &mut Open, blocks: &[String]) -> Result<Vec<String>> {
        let index = self.drive_index(o, true)?;
        let used = used_blocks(&index);
        Ok(blocks.iter().filter(|b| !used.contains(b)).cloned().collect())
    }

    /// Deletes blocks that belong to no file (those of `drive_put_loose`), unless a share
    /// still hands them out.
    pub(super) fn drive_delete_loose(&self, fs: &dyn RemoteFs, blocks: &[String]) -> Result<()> {
        let pinned = super::shares::pinned_blocks(fs)?;
        for b in blocks.iter().filter(|b| !pinned.contains(*b)) {
            fs.delete(&block_path(b))?;
        }
        Ok(())
    }

    /// Whether `item` is one of `roots` or lies inside one of them.
    pub(super) fn drive_within(&self, o: &mut Open, item: &str, roots: &[String]) -> Result<bool> {
        let index = self.drive_index(o, false)?;
        let mut cur = Some(item.to_owned());
        let mut steps = 0;
        while let Some(id) = cur {
            if roots.contains(&id) {
                return Ok(true);
            }
            steps += 1;
            if steps > 1000 {
                break;
            }
            cur = index.nodes.iter().find(|n| n.id == id).map(|n| n.parent.clone()).filter(|p| !p.is_empty());
        }
        Ok(false)
    }

    /// Name and content of a Drive file, or all files of a folder with their paths (for shares).
    /// Refuses more than `limit` bytes in total.
    pub(super) fn drive_collect(
        &self,
        o: &mut Open,
        id: &str,
        limit: u64,
    ) -> Result<Vec<(String, Zeroizing<Vec<u8>>)>> {
        let index = self.drive_index(o, false)?;
        let (fs, _) = self.drive_ctx(o)?;
        let root = index.nodes.iter().find(|n| n.id == id).ok_or(GhulError::NotFound)?;
        let mut files: Vec<(String, &Node)> = Vec::new();
        let mut todo = vec![(root.name.clone(), root)];
        while let Some((path, n)) = todo.pop() {
            if n.dir {
                for c in index.nodes.iter().filter(|c| c.parent == n.id && c.trashed == 0) {
                    todo.push((format!("{path}/{}", c.name), c));
                }
            } else {
                files.push((path, n));
            }
        }
        if files.iter().map(|(_, n)| n.size).sum::<u64>() > limit {
            return Err(GhulError::InvalidInput { msg: "too large to share".into() });
        }
        let mut out = Vec::new();
        for (path, n) in files {
            let mut data = Zeroizing::new(Vec::with_capacity(n.size as usize));
            let key = FileKey::of(&n.key)?;
            for block in &n.blocks {
                let enc = fs
                    .read(&block_path(block))?
                    .ok_or_else(|| GhulError::Corrupted { msg: "Drive block missing".into() })?;
                data.extend_from_slice(&key.open_block(block, &enc)?);
            }
            out.push((path, data));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
