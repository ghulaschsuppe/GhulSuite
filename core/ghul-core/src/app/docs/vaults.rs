// SPDX-License-Identifier: AGPL-3.0-or-later
//! The content files of GhulDocs in ZIP files: in the suite file (`.ghlzip`) next to their
//! vault, and in a GhulDocs package (`.ghldocs`, FORMAT.md section 10).
//!
//! A GhulDocs package is the vault saved or handed on on its own: a ZIP of the same form as the
//! suite file with the vault file as the first entry and then its content files, nothing else:
//!
//! ```text
//! <id>.ghldoc                         the vault (its index lists every content file)
//! docs-<vault id>/<content id>.ghlblob
//! ```
//!
//! Content files are checked while they are read (size and SHA-256 from the index, which came
//! first) and kept in a staging folder next to the working copy until everything is checked;
//! only then do they move into place.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use rand_core::OsRng;
use zeroize::Zeroizing;

use super::index::{self, Id, Tree};
use super::{blob, check_room, unhex16, usage, Usage, DIR_PREFIX, DOCS_LIMIT};
use crate::app::archive::{
    mismatch, not_a_suite_file, read_container, suite_not_docs_package, suite_zip, write_zip, zip_error,
    ArchiveProgress, ArchiveSink, ArchiveSource, SourceReader, HEADER_FILE,
};
use crate::app::error::{GhulError, Result};
use crate::app::single::single_file;
use crate::app::storage::VaultStorage;
use crate::app::suite::{
    docs_vault_id, file_name, now_ms, parse_keyfile, protection_of, register_key, second_factor, set_vault_name,
    vault_name, with_authenticator, GhulSuite, ProtectionInfo, VaultKind,
};
use crate::app::zipstream::{EntryInfo, ZipReader};
use crate::vault::container::{Fido2Registration, KdfCost, SuiteKeys};
use crate::vault::{ContainerKind, Entry, LockedContainer, PasswordKey, Slot, SlotMeta, UnlockedContainer};
use crate::{FidoListener, HidTransport, RP_ID};

/// Folder (in the content root) of content files read from a ZIP and not yet in place.
const STAGING_DIR: &str = ".staging";
/// A staging folder older than this was left by an app that ended (it is removed), unless it is
/// in use (see [`LIVE`]) or kept on purpose ([`KEEP_MARK`]).
const STAGING_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
/// A file in a staging folder that must never be tidied (it holds documents that could not be
/// put back, see `archive`).
pub(in crate::app) const KEEP_MARK: &str = "KEEP";

/// The staging folders of this process (by their name, a random id): a check waiting for
/// `archive_accept` may wait any time, also over a jump of the clock, and its folder must not be
/// tidied meanwhile.
static LIVE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn live() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The folder of the device the content files are kept in.
pub(in crate::app) fn content_root_of(storage: &dyn VaultStorage) -> Result<PathBuf> {
    storage
        .content_dir()
        .map(PathBuf::from)
        .ok_or_else(|| GhulError::Storage { msg: "documents cannot be stored in this folder".into() })
}

/// The name of a content file in a suite file or GhulDocs package.
pub(in crate::app) fn entry_name(vault_id: &str, content: &Id) -> String {
    format!("{DIR_PREFIX}{vault_id}/{}", blob::file_name(content))
}

/// Vault id and content id of a content file in a ZIP (exactly `docs-<32 hex>/<32
/// hex>.ghlblob`, lower case), `None` for any other name.
pub(in crate::app) fn parse_entry_name(name: &str) -> Option<(String, Id)> {
    let rest = name.strip_prefix(DIR_PREFIX)?;
    let (vid, file) = rest.split_once('/')?;
    let content = file.strip_suffix(&format!(".{}", blob::EXTENSION))?;
    unhex16(vid)?;
    Some((vid.to_owned(), unhex16(content)?))
}

/// A content file to copy into a suite file or GhulDocs package.
pub(in crate::app) struct ContentFile {
    pub(in crate::app) name: String,
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) len: u64,
    pub(in crate::app) crc: u32,
}

/// The content files of an index (all its documents, also those in the trash), from `dir`,
/// named for vault `vault_id` in the ZIP. Each must be there with its size, otherwise
/// nothing is written: a missing document would be gone wherever the file is opened.
pub(in crate::app) fn content_files(dir: &Path, vault_id: &str, entries: &[Entry]) -> Result<Vec<ContentFile>> {
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| index::is_item(e) && !index::is_folder(e)) {
        let c = index::content(e).ok_or_else(super::damaged_doc)?;
        let path = dir.join(blob::file_name(&c.blob));
        let len = blob::file_len(c.size);
        if !fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() == len) {
            return Err(super::damaged_doc());
        }
        out.push(ContentFile { name: entry_name(vault_id, &c.blob), path, len, crc: c.crc32 });
    }
    Ok(out)
}

/// A folder for content files read from a ZIP until they are checked; removed with
/// everything still in it when dropped. It knows every file staged into it (with its size), so
/// nothing moves into place unless all of them are still there.
pub(in crate::app) struct Staging {
    dir: PathBuf,
    name: String,
    files: Mutex<Vec<(PathBuf, u64)>>,
}

impl Staging {
    pub(in crate::app) fn new(root: &Path) -> Result<Self> {
        tidy_staging(root);
        let name = crate::kdf::hex(&crate::app::suite::new_id());
        let dir = root.join(STAGING_DIR).join(&name);
        live().insert(name.clone());
        let s = Staging { dir, name, files: Mutex::new(Vec::new()) };
        fs::create_dir_all(&s.dir)?;
        Ok(s)
    }

    /// A content file was staged (complete and checked).
    fn staged(&self, path: PathBuf, len: u64) {
        self.files.lock().unwrap_or_else(|e| e.into_inner()).push((path, len));
    }

    /// Every file staged is still there with its size (nothing else removed or cut it). Checked
    /// before anything moves into place: a missing document would be gone for good.
    pub(in crate::app) fn verify(&self) -> Result<()> {
        let files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        if files.iter().all(|(p, len)| fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() == *len)) {
            Ok(())
        } else {
            Err(super::damaged_doc())
        }
    }

    /// Keeps the folder and everything in it (also over a restart): it is not removed when
    /// dropped and never tidied. Moved to `root/<rescue>` if possible; returns where it is.
    pub(in crate::app) fn keep(self, root: &Path, rescue: &str) -> PathBuf {
        let to = root.join(rescue);
        let kept = if fs::rename(&self.dir, &to).is_ok() {
            to
        } else {
            let _ = fs::write(self.dir.join(KEEP_MARK), b"");
            self.dir.clone()
        };
        live().remove(&self.name);
        std::mem::forget(self);
        kept
    }

    pub(in crate::app) fn vault_dir(&self, vault_id: &str) -> PathBuf {
        self.dir.join(format!("{DIR_PREFIX}{vault_id}"))
    }

    /// The folders `docs-<id>` in it.
    pub(in crate::app) fn vaults(&self) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.strip_prefix(DIR_PREFIX).is_some_and(|v| unhex16(v).is_some()))
            .collect();
        out.sort();
        out
    }

    /// Moves the content files of a vault to `root/docs-<id>` (a folder there before goes).
    pub(in crate::app) fn move_vault(&self, vault_id: &str, root: &Path) -> Result<()> {
        let to = root.join(format!("{DIR_PREFIX}{vault_id}"));
        if to.exists() {
            fs::remove_dir_all(&to)?;
        }
        let from = self.vault_dir(vault_id);
        self.verify()?;
        if from.exists() {
            fs::rename(from, to)?;
        }
        Ok(())
    }

    pub(in crate::app) fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        live().remove(&self.name);
    }
}

/// Removes staging folders an app left behind when it ended: never one in use by this process,
/// never one kept on purpose.
pub(in crate::app) fn tidy_staging(root: &Path) {
    let now = SystemTime::now();
    let live = live();
    for e in fs::read_dir(root.join(STAGING_DIR)).into_iter().flatten().flatten() {
        if live.contains(e.file_name().to_string_lossy().as_ref()) || e.path().join(KEEP_MARK).exists() {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > STAGING_MAX_AGE);
        if old {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

/// The content files a GhulDocs vault read from a ZIP must come with: from its index,
/// which is checked first (complete items, unique ids, no cycles, sizes within the limit).
pub(in crate::app) struct Expected {
    files: HashMap<Id, (u64, [u8; 32])>,
    seen: HashSet<Id>,
}

impl Expected {
    pub(in crate::app) fn of(c: &UnlockedContainer) -> Result<Self> {
        if !index::validate(&c.payload.entries, DOCS_LIMIT) {
            return Err(GhulError::Corrupted { msg: "the index of a GhulDocs vault is invalid".into() });
        }
        let files = c
            .payload
            .entries
            .iter()
            .filter_map(index::content)
            .map(|x| (x.blob, (blob::file_len(x.size), x.sha256)))
            .collect();
        Ok(Expected { files, seen: HashSet::new() })
    }

    pub(in crate::app) fn complete(&self) -> bool {
        self.seen.len() == self.files.len()
    }
}

/// Reads the content file `e` (of vault `vault_id`) from `z` into the staging folder: it must be
/// one of the vault's, not seen before, with its size, and its SHA-256 must match.
pub(in crate::app) fn stage_entry<R: Read>(
    z: &mut ZipReader<R>,
    e: &EntryInfo,
    vault_id: &str,
    content: &Id,
    expected: &mut Expected,
    staging: &Staging,
) -> Result<()> {
    let &(len, sha) = expected.files.get(content).ok_or_else(|| mismatch("a document too many"))?;
    if !expected.seen.insert(*content) || e.size != len {
        return Err(mismatch("a document twice or with another size"));
    }
    let dir = staging.vault_dir(vault_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(blob::file_name(content));
    let mut w = BufWriter::with_capacity(64 * 1024, File::create(&path)?);
    let got = z.read_entry(&mut w).map_err(zip_error)?;
    let f = w.into_inner().map_err(|e| e.into_error())?;
    f.sync_all()?;
    if got != sha {
        return Err(mismatch("a document changed"));
    }
    staging.staged(path, len);
    Ok(())
}

/// The first entry of a suite file or GhulDocs package: its name and container (not opened).
fn read_first(z: &mut ZipReader<SourceReader>) -> Result<(String, LockedContainer)> {
    let e = z.next_entry().map_err(zip_error)?.ok_or_else(not_a_suite_file)?;
    let (data, _) = read_container(z, &e)?;
    let l = crate::app::archive::parse_first(&data)?;
    Ok((e.name, l))
}

/// The vault of a GhulDocs package (the first entry); a suite file says that it is one.
fn read_docs_head(z: &mut ZipReader<SourceReader>) -> Result<(String, LockedContainer)> {
    let (name, l) = read_first(z)?;
    if l.kind() == ContainerKind::Export && name == HEADER_FILE {
        return Err(suite_not_docs_package());
    }
    if l.kind() != ContainerKind::Docs || !name.ends_with(".ghldoc") || !crate::app::storage::plain_file_name(&name) {
        return Err(not_a_suite_file());
    }
    Ok((name, l))
}

/// The rest of a GhulDocs package: exactly the content files of `c`, checked,
/// into `staging`.
fn stage_docs_rest(
    z: &mut ZipReader<SourceReader>,
    name: &str,
    c: &UnlockedContainer,
    staging: &Staging,
) -> Result<()> {
    let vid = crate::kdf::hex(&c.container_id());
    if name != file_name(VaultKind::Docs, &vid) {
        return Err(mismatch("another vault"));
    }
    let mut expected = Expected::of(c)?;
    while let Some(e) = z.next_entry().map_err(zip_error)? {
        let (v, content) = parse_entry_name(&e.name).ok_or_else(not_a_suite_file)?;
        if v != vid {
            return Err(mismatch("a document of another vault"));
        }
        stage_entry(z, &e, &v, &content, &mut expected, staging)?;
    }
    if !expected.complete() {
        return Err(mismatch("documents missing"));
    }
    Ok(())
}

/// What a suite file or GhulDocs package is (by its first entry, not by its name).
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveContent {
    /// A suite file, `.ghlzip` (open with `archive_open`).
    Suite,
    /// A GhulDocs package, `.ghldocs` (open with `open_single_docs` or add with
    /// `docs_adopt_vault`).
    DocsVault,
}

/// What a suite file or GhulDocs package holds and what it needs to be opened.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ArchiveInfo {
    pub content: ArchiveContent,
    pub protection: ProtectionInfo,
}

/// Reads only the first entry of a `.ghlzip` or `.ghldocs`: whether it is a suite file or a
/// GhulDocs package, and what it needs to be opened. Nothing is opened. The app opens each only
/// as what it is; the core refuses the other with a message that says what it is.
#[uniffi::export]
pub fn archive_inspect(source: Arc<dyn ArchiveSource>) -> Result<ArchiveInfo> {
    let mut z = suite_zip(source, None);
    let (name, l) = read_first(&mut z)?;
    let content = match l.kind() {
        ContainerKind::Export if name == HEADER_FILE => ArchiveContent::Suite,
        ContainerKind::Docs if name.ends_with(".ghldoc") => ArchiveContent::DocsVault,
        _ => return Err(not_a_suite_file()),
    };
    Ok(ArchiveInfo { content, protection: protection_of(&l) })
}

/// A GhulDocs package (`.ghldocs`) as written: size and SHA-256 (hex) of the whole file as written.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DocsArchive {
    pub size: u64,
    pub sha256: String,
}

/// A GhulDocs vault handed on with a protection of its own.
#[derive(uniffi::Record, Clone)]
pub struct DocsShared {
    pub size: u64,
    pub sha256: String,
    /// The new vault's recovery code (shown once).
    pub recovery_code: String,
}

crate::app::redact::redacted_debug!(DocsShared { size, sha256 });

/// The GhulDocs vault opened on its own, written back: the change it holds (for
/// `single_saved`), size and SHA-256 of the file.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SingleDocsWritten {
    pub change: u64,
    pub size: u64,
    pub sha256: String,
}

/// What "hand on a GhulDocs vault" needs besides the second factor.
#[derive(uniffi::Record, Clone)]
pub struct ShareDocsOptions {
    pub vault_id: String,
    /// The picked files and folders (a folder with everything in it); empty: the whole vault.
    /// Nothing that is in the trash goes along.
    pub ids: Vec<String>,
    /// Name of the new vault.
    pub name: String,
    /// The own master password, asked again.
    pub master_password: String,
    /// Password of the new vault.
    pub password: String,
    /// Key file of the new vault (from `generate_keyfile`); `None` with a security key.
    pub keyfile: Option<Vec<u8>>,
    pub key_label: String,
    pub keyfile_label: String,
    pub recovery_label: String,
}

crate::app::redact::redacted_debug!(ShareDocsOptions { vault_id, ids, name, key_label, keyfile_label, recovery_label });

/// Copies of index entries, wiped when dropped (they hold the documents' keys).
struct Entries(Vec<Entry>);

impl Drop for Entries {
    fn drop(&mut self) {
        self.0.iter_mut().for_each(zeroize::Zeroize::zeroize);
    }
}

/// The items to hand on: the picked ones with everything in them and the folders above them
/// (so they keep their place); nothing in the trash.
fn picked(entries: &[Entry], ids: &[Id]) -> Entries {
    let tree = Tree::new(entries);
    let mut keep: HashSet<Vec<u8>> = HashSet::new();
    let roots: Vec<Id> = if ids.is_empty() {
        entries.iter().filter(|e| index::is_item(e) && index::parent(e).is_none()).filter_map(index::id_of).collect()
    } else {
        ids.to_vec()
    };
    for id in roots {
        let Some(e) = tree.get(&id) else { continue };
        if tree.in_trash(e) {
            continue;
        }
        for x in tree.subtree(&id) {
            if !tree.in_trash(x) {
                keep.insert(x.id.clone());
            }
        }
        for f in tree.ancestors(e) {
            keep.insert(f.id.clone());
        }
    }
    Entries(entries.iter().filter(|e| index::is_item(e) && keep.contains(&e.id)).cloned().collect())
}

#[uniffi::export]
impl GhulSuite {
    /// Writes the GhulDocs vault on its own into `sink`: a GhulDocs package (`.ghldocs`) with the vault (with this
    /// suite's protection, like `export_vault`) and all its content files. Open it with
    /// `open_single_docs`, add it with `docs_adopt_vault`.
    pub fn docs_export_vault(
        &self,
        vault_id: String,
        sink: Arc<dyn ArchiveSink>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<DocsArchive> {
        let _copy = self.content_copy();
        let (file, data, contents) = self.with_open(|o| {
            let c = o.docs(&vault_id)?;
            let file = file_name(VaultKind::Docs, &vault_id);
            let data = self.storage.read(file.clone())?.ok_or(GhulError::NotFound)?;
            let contents = content_files(&self.docs_dir(&vault_id)?, &vault_id, &c.payload.entries)?;
            Ok((file, Zeroizing::new(data), contents))
        })?;
        let (size, sha256) = write_zip(&[(file, data.to_vec())], &contents, sink, progress)?;
        Ok(DocsArchive { size, sha256 })
    }

    /// Hands on a GhulDocs vault (or picked files and folders of it) as a vault of its own:
    /// `options.password` plus a new key file (`options.keyfile`) or a security key (`transport`
    /// and `pin`, two touches), and a new recovery code; written into `sink` as a GhulDocs package (`.ghldocs`) with
    /// its content files (copied as they are: the documents' keys are in the new index).
    pub fn docs_share_vault(
        &self,
        options: ShareDocsOptions,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        sink: Arc<dyn ArchiveSink>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<DocsShared> {
        let password = Zeroizing::new(options.password.clone());
        let pin = pin.map(Zeroizing::new);
        if options.name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        if password.chars().count() < 8 {
            return Err(GhulError::InvalidInput { msg: "password too short".into() });
        }
        let keyfile = parse_keyfile(options.keyfile.clone())?;
        if keyfile.is_some() == pin.is_some() {
            return Err(GhulError::InvalidInput { msg: "either a key file or a security key".into() });
        }
        let ids: Vec<Id> = options.ids.iter().map(|i| super::parse_id(i)).collect::<Result<_>>()?;
        if !self.verify_password(options.master_password.clone())? {
            return Err(GhulError::WrongPassword);
        }
        let now = now_ms();
        // Copied first: the key may take a while to be touched, the suite is not held meanwhile.
        let _copy = self.content_copy();
        let (cost, entries, dir) = self.with_open(|o| {
            let c = o.docs(&options.vault_id)?;
            let entries = picked(&c.payload.entries, &ids);
            if !ids.is_empty() && entries.0.is_empty() {
                return Err(GhulError::NotFound);
            }
            let kdf = o.kdf();
            let cost = KdfCost { memory_kib: kdf.memory_kib, iterations: kdf.iterations, parallelism: kdf.parallelism };
            Ok((cost, entries, self.docs_dir(&options.vault_id)?))
        })?;
        let keys = SuiteKeys::create(&password, cost, &mut OsRng)?;
        let (slot, label): (Slot, &str) = match (&keyfile, &transport, &pin) {
            (Some(kf), _, _) => (keys.keyfile_slot(kf, &mut OsRng), &options.keyfile_label),
            (None, Some(t), Some(pin)) => {
                let (cred_id, prf_input, output) = with_authenticator(t, listener.as_ref(), |a| register_key(a, pin))?;
                let slot = keys.fido2_slot(
                    &Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output },
                    &mut OsRng,
                );
                (slot, &options.key_label)
            }
            _ => return Err(GhulError::InvalidInput { msg: "key and PIN must be given together".into() }),
        };
        let mut c = keys.create_container(ContainerKind::Docs, vec![slot.clone()], &mut OsRng)?;
        c.payload.slots.insert(
            crate::kdf::hex(slot.id()),
            SlotMeta { label: label.to_owned(), created: now, extra: Default::default() },
        );
        let (_, code) = c.add_recovery_slot(&options.recovery_label, now, &mut OsRng)?;
        set_vault_name(&mut c, &options.name);
        let new_id = crate::kdf::hex(&c.container_id());
        let contents = content_files(&dir, &new_id, &entries.0)?;
        c.payload.entries = entries.0.clone();
        let data = Zeroizing::new(c.seal(&mut OsRng)?);
        let (size, sha256) =
            write_zip(&[(file_name(VaultKind::Docs, &new_id), data.to_vec())], &contents, sink, progress)?;
        Ok(DocsShared { size, sha256, recovery_code: code.to_display().to_string() })
    }

    /// Adds a GhulDocs package (a `.ghldocs` from `docs_export_vault` or `docs_share_vault`) to
    /// this suite: opened once with its own password plus key or key file. A suite has one
    /// GhulDocs vault: if it has one already, the package's files and folders go into it, in a
    /// new folder at the top named `name` (or the package's name; a name that is taken gets a
    /// number), their content files moved as they are (their keys are in the index). A document
    /// that is in it already (the same content file, e.g. the package was added before):
    /// `AlreadyExists`. Without a GhulDocs vault the package becomes it, switched to this
    /// suite's protection like `adopt_vault`. Its documents count towards the limit
    /// (`DocsLimit` before anything is copied). Returns the id of the suite's GhulDocs vault.
    #[allow(clippy::too_many_arguments)]
    pub fn docs_adopt_vault(
        &self,
        source: Arc<dyn ArchiveSource>,
        name: String,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<String> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        // Refused before the key is touched or the password derived.
        self.with_open(|o| crate::app::single::refuse_single(o))?;
        let root = self.content_root()?;
        let mut z = suite_zip(source, progress);
        let (entry, locked) = read_docs_head(&mut z)?;
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
        let vid = crate::kdf::hex(&c.container_id());
        Expected::of(&c)?;
        let add = Usage::of(&c.payload.entries);
        let blobs: HashSet<Id> = c.payload.entries.iter().filter_map(index::content).map(|x| x.blob).collect();
        let limit = self.docs_limit();
        let (blobs, vid) = (&blobs, vid.as_str());
        let room = move |o: &mut crate::app::suite::Open| {
            crate::app::single::refuse_single(o)?;
            match docs_vault_id(o) {
                Some(dst) => {
                    let have = &o.docs(&dst)?.payload.entries;
                    if have.iter().filter_map(index::blob).any(|b| blobs.contains(&b)) {
                        return Err(GhulError::AlreadyExists);
                    }
                }
                None if o.vaults.iter().any(|v| v.reference.id == vid) => return Err(GhulError::AlreadyExists),
                None => {}
            }
            check_room(usage(o), add, limit)
        };
        self.with_open(|o| room(o))?;
        let staging = Staging::new(&root)?;
        if let Err(e) = stage_docs_rest(&mut z, &entry, &c, &staging) {
            c.payload.entries.iter_mut().for_each(zeroize::Zeroize::zeroize);
            return Err(e);
        }
        self.with_open(move |o| {
            if let Some(dst) = docs_vault_id(o) {
                let folder = if name.trim().is_empty() { vault_name(&c, &entry) } else { name };
                let r = room(o).and_then(|()| {
                    staging.verify()?;
                    self.merge_docs(o, &dst, &c.payload.entries, &folder, &staging.vault_dir(vid))
                });
                c.payload.entries.iter_mut().for_each(zeroize::Zeroize::zeroize);
                return r;
            }
            room(o)?;
            self.take_over(o, &mut c)?;
            staging.move_vault(vid, &root)?;
            let dir = root.join(format!("{DIR_PREFIX}{vid}"));
            self.insert_vault(o, c, &name).inspect_err(|_| {
                let _ = fs::remove_dir_all(&dir);
            })
        })
    }

    /// Opens a GhulDocs package (`.ghldocs`) like `open_single_vault`: this
    /// suite's folder must be empty (the app gives it a temporary one); the vault keeps its
    /// protection and is written back with `single_docs_write`. Returns the vault's id.
    #[allow(clippy::too_many_arguments)]
    pub fn open_single_docs(
        &self,
        source: Arc<dyn ArchiveSource>,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<String> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        let root = self.content_root()?;
        let mut z = suite_zip(source, progress);
        let (entry, locked) = read_docs_head(&mut z)?;
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let vault = locked.unlock(&pk, &s)?;
        let staging = Staging::new(&root)?;
        stage_docs_rest(&mut z, &entry, &vault, &staging)?;
        let vid = crate::kdf::hex(&vault.container_id());
        staging.move_vault(&vid, &root)?;
        self.start_single(vault, pk)?;
        Ok(vid)
    }

    /// The GhulDocs vault opened on its own as it is now, written into `sink` as a `.ghldocs`
    /// (to replace the file it came from), with the change it holds: confirm with
    /// `single_saved(change)` once the file is in place.
    pub fn single_docs_write(
        &self,
        sink: Arc<dyn ArchiveSink>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<SingleDocsWritten> {
        // The counter first, then the files: a change in between stays unsaved.
        let change = self.change();
        let _copy = self.content_copy();
        let (file, data, contents) = self.with_open(|o| {
            let file = single_file(o).ok_or(GhulError::InvalidInput { msg: "no vault is open on its own".into() })?;
            let vid =
                o.vaults.iter().find(|v| v.reference.file == file).ok_or(GhulError::NotFound)?.reference.id.clone();
            let c = o.docs(&vid)?;
            let data = self.storage.read(file.clone())?.ok_or(GhulError::NotFound)?;
            let contents = content_files(&self.docs_dir(&vid)?, &vid, &c.payload.entries)?;
            Ok((file, Zeroizing::new(data), contents))
        })?;
        let (size, sha256) = write_zip(&[(file, data.to_vec())], &contents, sink, progress)?;
        Ok(SingleDocsWritten { change, size, sha256 })
    }
}

impl GhulSuite {
    /// Puts the items `from` of a GhulDocs package into the suite's GhulDocs vault `dst`, in a
    /// new folder `folder` at the top: every item gets a new id (the package may hold ids the
    /// vault has), the content files (checked, in `staged`) move into the vault's folder. If
    /// anything fails, the index stays as it was and the moved content files go.
    fn merge_docs(
        &self,
        o: &mut crate::app::suite::Open,
        dst: &str,
        from: &[Entry],
        folder: &str,
        staged: &Path,
    ) -> Result<String> {
        let dir = self.docs_dir(dst)?;
        let now = now_ms();
        let items: Vec<&Entry> = from.iter().filter(|e| index::is_item(e)).collect();
        let ids: HashMap<Id, Id> =
            items.iter().filter_map(|e| index::id_of(e)).map(|id| (id, crate::app::suite::new_id())).collect();
        let name = {
            let clean: String = folder
                .trim()
                .chars()
                .map(|ch| if ch.is_control() || ch == '/' || ch == '\\' { '_' } else { ch })
                .take(super::MAX_NAME)
                .collect();
            let clean = super::check_name(&clean).unwrap_or_else(|_| "GhulDocs".to_owned());
            Tree::new(&o.docs(dst)?.payload.entries).free_name(None, &clean, None)
        };
        let top = super::new_entry(index::FOLDER, name, None, now);
        let top_id = index::id_of(&top).unwrap_or_default();
        let mut add = Entries(vec![top]);
        for e in items {
            let mut e = e.clone();
            e.id = ids.get(&index::id_of(&e).unwrap_or_default()).ok_or(GhulError::NotFound)?.to_vec();
            let parent = index::parent(&e).and_then(|p| ids.get(&p).copied()).unwrap_or(top_id);
            index::set_parent(&mut e, Some(parent));
            add.0.push(e);
        }
        // The content files first: the index never names one that is not there.
        fs::create_dir_all(&dir)?;
        let mut moved: Vec<PathBuf> = Vec::new();
        let mut out: Result<()> = Ok(());
        for b in add.0.iter().filter_map(index::blob) {
            let to = dir.join(blob::file_name(&b));
            if let Err(e) = fs::rename(staged.join(blob::file_name(&b)), &to) {
                out = Err(e.into());
                break;
            }
            moved.push(to);
        }
        let out = out.and_then(|()| {
            let before = o.docs(dst)?.payload.entries.len();
            o.docs_mut(dst)?.payload.entries.extend(add.0.iter().cloned());
            self.save_vault(o, dst).inspect_err(|_| {
                // Back to what is on the disk.
                if let Ok(c) = o.docs_mut(dst) {
                    c.payload.entries.drain(before..).for_each(|mut e| zeroize::Zeroize::zeroize(&mut e));
                }
            })
        });
        if out.is_err() {
            moved.iter().for_each(|p| {
                let _ = fs::remove_file(p);
            });
        }
        out.map(|()| dst.to_owned())
    }
}
