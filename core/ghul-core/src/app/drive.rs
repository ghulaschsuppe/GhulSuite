// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulDrive: files on the user's own server, encrypted on the device.
//!
//! ```text
//! <base>/drive/index.ghldrv              "GHULDRV1" ‖ nonce[24] ‖ AEAD(index key, padded CBOR index)
//! <base>/drive/blocks/<ab>/<block id>    "GHULBLK1" ‖ nonce[24] ‖ AEAD(block key, block, aad = magic ‖ id)
//!
//! DK          = random 256 bit, kept in the suite's encrypted settings (synchronised)
//! index key   = HKDF(DK, "ghul/v1/drive/index")
//! block key   = HKDF(DK, "ghul/v1/drive/block")
//! block id    = hex(HMAC-SHA256(HKDF(DK, "ghul/v1/drive/block-id"), plaintext block))
//! ```
//!
//! Files are split into blocks of at most 4 MiB. The server only sees block sizes and their
//! number; names, folders and sizes of files exist only in the encrypted index. Equal blocks are
//! stored once. Every change reloads the index from the server first, so two devices do not
//! overwrite each other's changes (the last writer of the same item wins).

use std::collections::BTreeSet;
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
const BLOCK_MAGIC: &[u8; 8] = b"GHULBLK1";
/// Largest block (the app reads files in chunks of this size).
pub const BLOCK_SIZE: u32 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Default)]
struct DriveSettings {
    /// Id of the server Drive uses (its own one in the Drive file, or one of the suite's).
    #[serde(default)]
    server: String,
    /// Use the server this device synchronises with (`server` is then empty).
    #[serde(default)]
    use_sync: bool,
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
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
    parent: String,
    name: String,
    blocks: Vec<String>,
    size: u64,
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
    block: Zeroizing<[u8; 32]>,
    block_id: Zeroizing<[u8; 32]>,
}

impl Keys {
    fn new(dk: &[u8]) -> Keys {
        Keys {
            index: hkdf(dk, &[b"ghul/v1/drive/index"]),
            block: hkdf(dk, &[b"ghul/v1/drive/block"]),
            block_id: hkdf(dk, &[b"ghul/v1/drive/block-id"]),
        }
    }

    fn block_id(&self, data: &[u8]) -> String {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(self.block_id.as_ref()).expect("any key size");
        m.update(data);
        crate::kdf::hex(&m.finalize().into_bytes())
    }
}

/// The server Drive uses on this device.
fn drive_server_id(o: &Open, d: &DriveSettings) -> Option<String> {
    if d.use_sync {
        super::sync::sync_server(o)
    } else {
        (!d.server.is_empty()).then(|| d.server.clone())
    }
}

/// How GhulDrive stores its files.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveMode {
    /// Not set up.
    None,
    /// On the server this device synchronises with.
    Sync,
    /// On its own server, stored in the Drive file.
    Own,
    /// On one of the suite's servers (Drive set up with an older app).
    Suite,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DriveStorage {
    pub mode: DriveMode,
    /// The server in use; `None` when not set up, or `Sync` while synchronisation is off.
    pub server: Option<ServerInfo>,
    /// Synchronisation is set up on this device ("wie Synchronisation" can be offered).
    pub sync_available: bool,
}

fn block_path(id: &str) -> String {
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
        let server = drive_server_id(o, &d).ok_or(GhulError::NotConfigured)?;
        if d.key.len() != 32 {
            return Err(GhulError::NotConfigured);
        }
        let fs = self.remote(o, &server)?;
        Ok((fs, Keys::new(&d.key)))
    }

    /// Points Drive at a server (`server` empty with `use_sync`) and checks it: creates Drive's
    /// key the first time, opens an existing index with it (or creates one) and saves.
    fn drive_attach(&self, o: &mut Open, server: String, use_sync: bool) -> Result<()> {
        self.ensure_area(o, VaultKind::Drive)?;
        let mut d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
        if d.key.len() != 32 {
            let mut k = vec![0u8; 32];
            OsRng.fill_bytes(&mut k);
            d.key = k;
        }
        let before = (std::mem::replace(&mut d.server, server), std::mem::replace(&mut d.use_sync, use_sync));
        put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = None;
        let checked = (|| {
            let (fs, keys) = self.drive_ctx(o)?;
            // An existing index must open with our key (otherwise it belongs to someone else).
            let mut index = load_index(fs.as_ref(), &keys)?;
            if fs.read(INDEX)?.is_none() {
                save_index(fs.as_ref(), &keys, &mut index)?;
            }
            Ok(())
        })();
        if let Err(e) = checked {
            (d.server, d.use_sync) = before;
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            return Err(e);
        }
        // Drive's own server is only kept while Drive uses it.
        let own: Vec<StoredServer> = servers::drive_servers(o).into_iter().filter(|s| s.id == d.server).collect();
        put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
        self.save_area(o, VaultKind::Drive)
    }

    /// Loads the current index from the server, applies `f`, saves it.
    fn drive_change<T>(
        &self,
        o: &mut Open,
        f: impl FnOnce(&mut DriveIndex, &dyn RemoteFs, &Keys) -> Result<T>,
    ) -> Result<T> {
        let (fs, keys) = self.drive_ctx(o)?;
        let mut index = load_index(fs.as_ref(), &keys)?;
        let out = f(&mut index, fs.as_ref(), &keys)?;
        save_index(fs.as_ref(), &keys, &mut index)?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = Some(index);
        Ok(out)
    }

    fn drive_index(&self, o: &mut Open, refresh: bool) -> Result<DriveIndex> {
        if !refresh {
            if let Some(i) = &self.net.lock().unwrap_or_else(|p| p.into_inner()).drive {
                return Ok(i.clone());
            }
        }
        let (fs, keys) = self.drive_ctx(o)?;
        let index = load_index(fs.as_ref(), &keys)?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = Some(index.clone());
        Ok(index)
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Sets up GhulDrive on a stored server (creates the Drive key once; keeps an existing Drive).
    /// Uses one of the suite's servers for Drive (by id, not following later changes of the
    /// synchronisation; for that use `drive_use_sync`).
    pub fn drive_setup(&self, server_id: String) -> Result<()> {
        self.with_open(|o| {
            if !servers::all_servers(o).iter().any(|s| s.id == server_id) {
                return Err(GhulError::NotFound);
            }
            self.drive_attach(o, server_id, false)
        })
    }

    /// Drive uses the server this device synchronises with ("wie Synchronisation").
    pub fn drive_use_sync(&self) -> Result<()> {
        self.with_open(|o| {
            super::sync::sync_server(o).ok_or(GhulError::NotConfigured)?;
            self.drive_attach(o, String::new(), true)
        })
    }

    /// Checks a server for Drive alone and stores it in the Drive file (replacing Drive's earlier
    /// own server). The synchronisation does not see it. Fingerprints as with `save_server`.
    pub fn drive_save_server(&self, input: ServerInput, confirmed: Option<String>) -> Result<ServerSaved> {
        self.with_open(|o| {
            let stored = match servers::check_server(o, &input, confirmed.as_deref())? {
                Ok(s) => s,
                Err(fingerprint) => return Ok(ServerSaved::Confirm { fingerprint }),
            };
            let id = stored.id.clone();
            self.ensure_area(o, VaultKind::Drive)?;
            let mut own = servers::drive_servers(o);
            own.push(stored);
            put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            if let Err(e) = self.drive_attach(o, id.clone(), false) {
                own.retain(|s| s.id != id);
                put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &own)?;
                return Err(e);
            }
            Ok(ServerSaved::Saved { id })
        })
    }

    /// Where Drive stores its files.
    pub fn drive_storage(&self) -> Result<DriveStorage> {
        self.with_open(|o| {
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            let mode = if d.use_sync {
                DriveMode::Sync
            } else if d.server.is_empty() || d.key.len() != 32 {
                DriveMode::None
            } else if servers::drive_servers(o).iter().any(|s| s.id == d.server) {
                DriveMode::Own
            } else {
                DriveMode::Suite
            };
            let server = drive_server_id(o, &d).and_then(|id| servers::server(o, &id).ok()).map(|s| servers::info(&s));
            Ok(DriveStorage { mode, server, sync_available: super::sync::sync_server(o).is_some() })
        })
    }

    /// Before the synchronisation server is removed or changed: Drive keeps using that server,
    /// as its own one (a copy of the login goes into the Drive file). Nothing when Drive does
    /// not follow the synchronisation.
    pub fn drive_detach_from_sync(&self) -> Result<()> {
        self.with_open(|o| {
            let d: DriveSettings = area_setting(o, VaultKind::Drive, DRIVE_KEY);
            if !d.use_sync {
                return Ok(());
            }
            let sync = super::sync::sync_server(o).ok_or(GhulError::NotConfigured)?;
            let mut copy = servers::server(o, &sync)?;
            copy.id = crate::kdf::hex(&new_id()[..8]);
            copy.share_dir.clear();
            copy.share_url.clear();
            let id = copy.id.clone();
            put_area_setting(o, VaultKind::Drive, servers::SERVERS_KEY, &vec![copy])?;
            let mut d = d;
            d.server = id;
            d.use_sync = false;
            put_area_setting(o, VaultKind::Drive, DRIVE_KEY, &d)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = None;
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
                let parent = parent.unwrap_or_default();
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
                let target = folder.unwrap_or_default();
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
            self.drive_change(o, |index, fs, _| {
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
                for b in freed.iter().filter(|b| !still.contains(b)) {
                    fs.delete(&block_path(b))?;
                }
                Ok(())
            })
        })
    }

    /// Starts an upload into `folder`; returns the upload id for `drive_upload_chunk`.
    pub fn drive_upload_begin(&self, folder: Option<String>, name: String) -> Result<String> {
        let name = check_name(&name)?;
        self.with_open(|o| {
            self.drive_ctx(o)?;
            let id = crate::kdf::hex(&new_id());
            let up = Upload { parent: folder.unwrap_or_default(), name, blocks: vec![], size: 0 };
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
        self.with_open(|o| {
            let (fs, keys) = self.drive_ctx(o)?;
            let id = keys.block_id(&data);
            let known = self
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
                fs.write(&path, &encrypt(BLOCK_MAGIC, &keys.block, &data, id.as_bytes()))?;
            }
            let mut cache = self.net.lock().unwrap_or_else(|p| p.into_inner());
            let up = cache.uploads.get_mut(&upload).ok_or(GhulError::NotFound)?;
            up.blocks.push(id);
            up.size += data.len() as u64;
            Ok(())
        })
    }

    /// Finishes an upload: the file appears in the folder. Returns its id.
    pub fn drive_upload_finish(&self, upload: String) -> Result<String> {
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
                        let old = Version { size: n.size, modified: n.modified, blocks: std::mem::take(&mut n.blocks) };
                        n.versions.insert(0, old);
                        n.versions.truncate(MAX_VERSIONS);
                    }
                    n.blocks = up.blocks.clone();
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
                    ..Default::default()
                });
                Ok(id)
            })
        })
    }

    /// Cancels an upload (blocks already written are removed when unused).
    pub fn drive_upload_cancel(&self, upload: String) -> Result<()> {
        self.with_open(|o| {
            let up = self.net.lock().unwrap_or_else(|p| p.into_inner()).uploads.remove(&upload);
            let Some(up) = up else { return Ok(()) };
            let (fs, keys) = self.drive_ctx(o)?;
            let index = load_index(fs.as_ref(), &keys)?;
            let used = used_blocks(&index);
            for b in up.blocks.iter().filter(|b| !used.contains(b)) {
                fs.delete(&block_path(b))?;
            }
            Ok(())
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
            let (fs, keys) = self.drive_ctx(o)?;
            let data = match std::fs::read(self.offline_dir().join(&block)) {
                Ok(d) => d,
                Err(_) => fs
                    .read(&block_path(&block))?
                    .ok_or_else(|| GhulError::Corrupted { msg: "Drive block missing".into() })?,
            };
            let plain = decrypt(BLOCK_MAGIC, &keys.block, &data, block.as_bytes())?;
            if keys.block_id(&plain) != block {
                return Err(GhulError::Corrupted { msg: "Drive block does not match".into() });
            }
            Ok(plain.to_vec())
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
                let cur = Version { size: n.size, modified: n.modified, blocks: std::mem::take(&mut n.blocks) };
                n.versions.insert(0, cur);
                n.blocks = v.blocks;
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
    /// password – for e-mail or a USB stick. At most 200 MB.
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
    /// Name and content of a Drive file, or all files of a folder with their paths (for shares).
    /// Refuses more than `limit` bytes in total.
    pub(super) fn drive_collect(
        &self,
        o: &mut Open,
        id: &str,
        limit: u64,
    ) -> Result<Vec<(String, Zeroizing<Vec<u8>>)>> {
        let index = self.drive_index(o, false)?;
        let (fs, keys) = self.drive_ctx(o)?;
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
            for block in &n.blocks {
                let enc = fs
                    .read(&block_path(block))?
                    .ok_or_else(|| GhulError::Corrupted { msg: "Drive block missing".into() })?;
                let plain = decrypt(BLOCK_MAGIC, &keys.block, &enc, block.as_bytes())?;
                if keys.block_id(&plain) != *block {
                    return Err(GhulError::Corrupted { msg: "Drive block does not match".into() });
                }
                data.extend_from_slice(&plain);
            }
            out.push((path, data));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
