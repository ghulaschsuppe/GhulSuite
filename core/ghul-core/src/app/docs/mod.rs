// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulDocs: an encrypted store for important documents and scans (FORMAT.md section 11).
//!
//! A GhulDocs vault is a vault file `<id>.ghldoc` like the others (same keys, same write seal)
//! that holds only the index: folders and files with their names, sizes, times, types,
//! favorites, trash and, per file, its own random key and the SHA-256 of its content file. The
//! content of each file is in a content file `docs-<vault id>/<content id>.ghlblob` next to the
//! vault files (see [`blob`]); it is written once and never changed. Plaintext never reaches the
//! disk: content is encrypted while it is read in and decrypted while it is written out.
//!
//! A suite has at most one GhulDocs vault; it holds at most [`DOCS_LIMIT`] bytes of content (also
//! what is in the trash until it is emptied). The content files travel in the suite file
//! (`archive`) and in a GhulDocs package (`.ghldocs`, `vaults`).

pub(crate) mod blob;
mod index;
mod vaults;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

pub use self::vaults::{
    archive_inspect, ArchiveContent, ArchiveInfo, DocsArchive, DocsShared, ShareDocsOptions, SingleDocsWritten,
};
pub(super) use self::vaults::{
    content_files, content_root_of, parse_entry_name, remove_all_contents, stage_entry, tidy_staging, ContentFile,
    Expected, Staging,
};

use self::blob::BlobError;
use self::index::{Id, Tree};
use super::archive::{reporter, ArchiveProgress, ArchiveSink, ArchiveSource, SinkWriter, SourceReader};
use super::error::{GhulError, Result};
use super::suite::{new_id, now_ms, GhulSuite, Open};
use crate::vault::{Entry, Field, UnlockedContainer};

/// Content the GhulDocs vault of a suite may hold (bytes, also the largest document).
pub const DOCS_LIMIT: u64 = 1_000_000_000;
/// Documents a suite may hold (each is an entry of the suite file).
pub const DOCS_MAX_FILES: u64 = 20_000;
/// Longest name of a file or folder (characters).
pub(super) const MAX_NAME: usize = 255;
/// Folder of a vault's content files: `docs-<vault id>`.
pub(super) const DIR_PREFIX: &str = "docs-";
const DEFAULT_MIME: &str = "application/octet-stream";

/// The content files may take this much room on the disk and in the suite file: the content
/// plus up to 10 % for the rounding to 64 KiB, so a full GhulDocs (1 GB) and the other vaults
/// fit into the largest suite file (1.25 GiB).
pub(crate) fn stored_limit(limit: u64) -> u64 {
    limit / 10 * 11
}

/// A file or folder of a GhulDocs vault, as a list shows it.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DocsItem {
    pub vault_id: String,
    /// 32 hex digits.
    pub id: String,
    /// The folder it is in (`None`: at the top).
    pub parent: Option<String>,
    pub name: String,
    pub is_folder: bool,
    /// Bytes of content (0 for folders).
    pub size: u64,
    /// Media type, e.g. `application/pdf` (empty for folders).
    pub mime: String,
    /// Unix ms.
    pub created: i64,
    pub modified: i64,
    pub favorite: bool,
    /// When it was moved to the trash (only on the item that was moved there, not on what is
    /// inside a folder in the trash).
    pub trashed: Option<i64>,
    /// The names of the folders it is in, from the top, joined with `/` (empty at the top).
    pub path: String,
    /// Folders: how many items are directly in it (not counting the trash).
    pub items: u32,
}

/// How much of GhulDocs is used (the trash included).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DocsUsage {
    /// Bytes of content.
    pub used: u64,
    /// The most there may be (1 GB).
    pub limit: u64,
    /// Number of documents.
    pub files: u32,
}

/// A name of a file or folder as it is kept: trimmed, not empty, at most [`MAX_NAME`]
/// characters, no control characters and no `/` or `\`.
pub(super) fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    let bad = n.is_empty()
        || n == "."
        || n == ".."
        || n.chars().count() > MAX_NAME
        || n.chars().any(|c| {
            c.is_control() || c == '/' || c == '\\' || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        });
    if bad {
        return Err(GhulError::InvalidInput { msg: "invalid name".into() });
    }
    Ok(n.to_owned())
}

fn clean_mime(mime: &str) -> String {
    let m = mime.trim();
    if m.is_empty() || m.len() > 255 || !m.contains('/') || m.chars().any(|c| c.is_control()) {
        DEFAULT_MIME.to_owned()
    } else {
        m.to_ascii_lowercase()
    }
}

/// 32 lower-case hex digits to 16 bytes.
pub(super) fn unhex16(s: &str) -> Option<Id> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

fn parse_id(s: &str) -> Result<Id> {
    unhex16(s).ok_or_else(|| GhulError::InvalidInput { msg: "invalid id".into() })
}

fn folder_id(folder: Option<String>) -> Result<Option<Id>> {
    folder.filter(|f| !f.is_empty()).map(|f| parse_id(&f)).transpose()
}

fn damaged_doc() -> GhulError {
    GhulError::Corrupted { msg: "a document is damaged or missing".into() }
}

/// A content file that could not be read or written, as the app sees it.
pub(super) fn blob_error(e: BlobError) -> GhulError {
    match e {
        BlobError::Io(e) | BlobError::Out(e) => GhulError::Storage { msg: e.to_string() },
        BlobError::Damaged(_) => damaged_doc(),
        BlobError::TooLarge => GhulError::DocsLimit { used: 0, limit: DOCS_LIMIT, needed: 0 },
    }
}

/// Content in GhulDocs (or what is added to it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Usage {
    /// Bytes of content.
    pub(super) used: u64,
    /// Bytes of the content files.
    pub(super) stored: u64,
    pub(super) files: u64,
}

impl Usage {
    pub(super) fn file(size: u64) -> Usage {
        Usage { used: size, stored: blob::file_len(size), files: 1 }
    }

    pub(super) fn of(entries: &[Entry]) -> Usage {
        entries
            .iter()
            .filter(|e| index::is_item(e) && !index::is_folder(e))
            .fold(Usage::default(), |u, e| u.plus(Usage::file(index::size(e))))
    }

    pub(super) fn plus(self, o: Usage) -> Usage {
        Usage {
            used: self.used.saturating_add(o.used),
            stored: self.stored.saturating_add(o.stored),
            files: self.files.saturating_add(o.files),
        }
    }
}

/// Whether `add` still fits next to `have` (content, room of the content files, number).
pub(super) fn check_room(have: Usage, add: Usage, limit: u64) -> Result<()> {
    let all = have.plus(add);
    if all.used > limit || all.stored > stored_limit(limit) || all.files > DOCS_MAX_FILES {
        return Err(GhulError::DocsLimit { used: have.used, limit, needed: add.used });
    }
    Ok(())
}

/// What the GhulDocs vault of the open suite holds (there is one at most).
pub(super) fn usage(o: &Open) -> Usage {
    o.vaults
        .iter()
        .filter(|v| v.reference.kind == "docs")
        .filter_map(|v| v.container.as_ref())
        .fold(Usage::default(), |u, c| u.plus(Usage::of(&c.payload.entries)))
}

impl Open {
    pub(super) fn docs(&self, vault_id: &str) -> Result<&UnlockedContainer> {
        let v = self.vaults.iter().find(|v| v.reference.id == vault_id).ok_or(GhulError::NotFound)?;
        if v.reference.kind != "docs" {
            return Err(GhulError::InvalidInput { msg: "not a GhulDocs vault".into() });
        }
        v.container.as_ref().ok_or_else(|| GhulError::Storage { msg: format!("{} is damaged", v.reference.file) })
    }

    fn docs_mut(&mut self, vault_id: &str) -> Result<&mut UnlockedContainer> {
        self.docs(vault_id)?;
        Ok(self.vault_mut(vault_id)?.1)
    }

    /// The GhulDocs vaults (all, or the one asked for).
    fn docs_vaults(&self, vault_id: Option<&str>) -> Result<Vec<(String, &UnlockedContainer)>> {
        if let Some(id) = vault_id {
            return Ok(vec![(id.to_owned(), self.docs(id)?)]);
        }
        Ok(self
            .vaults
            .iter()
            .filter(|v| v.reference.kind == "docs")
            .filter_map(|v| v.container.as_ref().map(|c| (v.reference.id.clone(), c)))
            .collect())
    }
}

fn item(vault_id: &str, tree: &Tree<'_>, e: &Entry) -> DocsItem {
    let folder = index::is_folder(e);
    let id = index::id_of(e);
    DocsItem {
        vault_id: vault_id.to_owned(),
        id: crate::kdf::hex(&e.id),
        parent: index::parent(e).filter(|p| tree.get(p).is_some_and(index::is_folder)).map(|p| crate::kdf::hex(&p)),
        name: index::name(e),
        is_folder: folder,
        size: index::size(e),
        mime: if folder { String::new() } else { index::mime(e) },
        created: index::created(e),
        modified: e.modified,
        favorite: index::favorite(e),
        trashed: index::trashed(e),
        path: tree.path(e),
        items: if folder { tree.children(id).filter(|c| index::trashed(c).is_none()).count() as u32 } else { 0 },
    }
}

fn item_of(vault_id: &str, entries: &[Entry], id: &Id) -> Result<DocsItem> {
    let tree = Tree::new(entries);
    let e = tree.get(id).ok_or(GhulError::NotFound)?;
    Ok(item(vault_id, &tree, e))
}

/// Folders first, then by name.
fn sorted(mut list: Vec<DocsItem>) -> Vec<DocsItem> {
    list.sort_by(|a, b| {
        b.is_folder
            .cmp(&a.is_folder)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    list
}

/// A folder things can go into: it exists and is not in the trash (`None`: the top).
fn check_folder(tree: &Tree<'_>, folder: Option<Id>) -> Result<()> {
    if let Some(f) = folder {
        let e = tree.get(&f).ok_or(GhulError::NotFound)?;
        if !index::is_folder(e) || tree.in_trash(e) {
            return Err(GhulError::NotFound);
        }
    }
    Ok(())
}

/// The position of an item in the entries.
fn position(entries: &[Entry], id: &Id) -> Result<usize> {
    entries.iter().position(|e| index::is_item(e) && e.id.as_slice() == id).ok_or(GhulError::NotFound)
}

fn touch(e: &mut Entry, now: i64) {
    e.revision += 1;
    e.modified = now;
}

/// A new index entry (`kind` folder or file) with its name in `folder`.
fn new_entry(kind: &str, name: String, folder: Option<Id>, now: i64) -> Entry {
    let mut e = Entry::new(new_id(), kind, now);
    e.fields.insert(index::NAME.into(), Field::Text(name));
    index::set_parent(&mut e, folder);
    e.fields.insert(index::CREATED.into(), Field::Int(now));
    e
}

/// Content files being copied into a `.ghlzip` (how many copies run) and the content files
/// deleted while one ran, deleted once none runs any more.
#[derive(Default)]
pub(crate) struct Copies {
    running: usize,
    deferred: Vec<PathBuf>,
}

/// Content files are being copied into a `.ghlzip` while this lives (a save of the suite file,
/// a GhulDocs vault saved or handed on): a document deleted for good meanwhile keeps its content
/// file until the copy ended, so the file being written stays complete.
pub(super) struct ContentCopy<'a>(&'a GhulSuite);

impl Drop for ContentCopy<'_> {
    fn drop(&mut self) {
        let deferred = {
            let mut c = self.0.content_copies.lock().unwrap_or_else(|e| e.into_inner());
            c.running = c.running.saturating_sub(1);
            if c.running > 0 {
                return;
            }
            std::mem::take(&mut c.deferred)
        };
        deferred.iter().for_each(|p| remove_path(p));
    }
}

fn remove_path(p: &std::path::Path) {
    if p.is_dir() {
        let _ = fs::remove_dir_all(p);
    } else {
        let _ = fs::remove_file(p);
    }
}

impl GhulSuite {
    /// Starts copying content files into a `.ghlzip` (see [`ContentCopy`]). Take it while the
    /// suite is held, before the list of content files is made.
    pub(super) fn content_copy(&self) -> ContentCopy<'_> {
        self.content_copies.lock().unwrap_or_else(|e| e.into_inner()).running += 1;
        ContentCopy(self)
    }

    /// The folder of the device the content files are kept in (the suite's folder).
    pub(super) fn content_root(&self) -> Result<PathBuf> {
        content_root_of(self.storage.as_ref())
    }

    /// The folder of a vault's content files.
    pub(super) fn docs_dir(&self, vault_id: &str) -> Result<PathBuf> {
        if unhex16(vault_id).is_none() {
            return Err(GhulError::NotFound);
        }
        Ok(self.content_root()?.join(format!("{DIR_PREFIX}{vault_id}")))
    }

    pub(super) fn docs_limit(&self) -> u64 {
        self.docs_limit.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Tests only: a smaller limit, so it can be reached without writing a gigabyte.
    #[cfg(test)]
    pub(crate) fn set_docs_limit(&self, limit: u64) {
        self.docs_limit.store(limit, std::sync::atomic::Ordering::SeqCst);
    }

    /// Changes the index of a GhulDocs vault and saves it; `change` gets the entries, the time
    /// and what GhulDocs holds now. If anything fails, the index stays as it was.
    fn docs_change<T>(
        &self,
        vault_id: &str,
        change: impl FnOnce(&mut Vec<Entry>, i64, Usage) -> Result<T>,
    ) -> Result<T> {
        self.with_open(|o| {
            let have = usage(o);
            let c = o.docs_mut(vault_id)?;
            let before = c.payload.entries.clone();
            let out =
                change(&mut c.payload.entries, now_ms(), have).and_then(|t| self.save_vault(o, vault_id).map(|_| t));
            let mut dropped = before;
            if out.is_err() {
                // Back to what is on the disk.
                std::mem::swap(&mut o.docs_mut(vault_id)?.payload.entries, &mut dropped);
            }
            dropped.iter_mut().for_each(zeroize::Zeroize::zeroize);
            out
        })
    }

    /// Deletes content files of a vault (after its index no longer has them); while content
    /// files are copied into a `.ghlzip`, only when that ended (see [`ContentCopy`]).
    fn delete_contents(&self, vault_id: &str, blobs: &[Id]) {
        let Ok(dir) = self.docs_dir(vault_id) else { return };
        self.remove_contents(blobs.iter().map(|b| dir.join(blob::file_name(b))).collect());
    }

    /// Removes content files or folders of content files now, or when the copies running
    /// ended (see [`ContentCopy`]).
    pub(super) fn remove_contents(&self, paths: Vec<PathBuf>) {
        {
            let mut c = self.content_copies.lock().unwrap_or_else(|e| e.into_inner());
            if c.running > 0 {
                c.deferred.extend(paths);
                return;
            }
        }
        paths.iter().for_each(|p| remove_path(p));
    }

    /// Removes what no index refers to: content files of an add or a delete that did not end,
    /// folders of vaults that are gone. A vault that cannot be opened (damaged, another
    /// protection) keeps its folder.
    pub(super) fn docs_tidy(&self, o: &Open) {
        // A copy into a `.ghlzip` still runs (begun before a lock): what it copies stays.
        if self.content_copies.lock().unwrap_or_else(|e| e.into_inner()).running > 0 {
            return;
        }
        let Ok(root) = self.content_root() else { return };
        let Ok(list) = fs::read_dir(&root) else { return };
        let files: HashSet<String> = self.storage.list().unwrap_or_default().into_iter().collect();
        for d in list.flatten() {
            let name = d.file_name().to_string_lossy().into_owned();
            let Some(vid) = name.strip_prefix(DIR_PREFIX) else { continue };
            if unhex16(vid).is_none() || !d.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            match o.vaults.iter().find(|v| v.reference.id == vid) {
                Some(v) => {
                    let Some(c) = v.container.as_ref().filter(|_| v.reference.kind == "docs") else { continue };
                    let keep: HashSet<String> = c
                        .payload
                        .entries
                        .iter()
                        .filter_map(|e| index::blob(e).filter(|_| index::is_item(e)))
                        .map(|b| blob::file_name(&b))
                        .collect();
                    for f in fs::read_dir(d.path()).into_iter().flatten().flatten() {
                        if !keep.contains(&f.file_name().to_string_lossy().into_owned()) {
                            let _ = fs::remove_file(f.path());
                        }
                    }
                }
                None if !files.contains(&super::suite::file_name(super::suite::VaultKind::Docs, vid)) => {
                    let _ = fs::remove_dir_all(d.path());
                }
                None => {}
            }
        }
        tidy_staging(&root);
    }
}

#[uniffi::export]
impl GhulSuite {
    /// The items directly in a folder of a GhulDocs vault (`folder` `None`: at the top), folders
    /// first, then by name; nothing that is in the trash.
    pub fn docs_list(&self, vault_id: String, folder: Option<String>) -> Result<Vec<DocsItem>> {
        let folder = folder_id(folder)?;
        self.with_open(|o| {
            let c = o.docs(&vault_id)?;
            let tree = Tree::new(&c.payload.entries);
            if let Some(f) = folder {
                if !tree.get(&f).is_some_and(index::is_folder) {
                    return Err(GhulError::NotFound);
                }
            }
            let list = tree.children(folder).filter(|e| index::trashed(e).is_none()).map(|e| item(&vault_id, &tree, e));
            Ok(sorted(list.collect()))
        })
    }

    /// One file or folder.
    pub fn docs_item(&self, vault_id: String, id: String) -> Result<DocsItem> {
        let id = parse_id(&id)?;
        self.with_open(|o| item_of(&vault_id, &o.docs(&vault_id)?.payload.entries, &id))
    }

    /// Files and folders whose name contains `query` (ignoring case), in one GhulDocs vault or
    /// in all; nothing that is in the trash.
    pub fn docs_search(&self, vault_id: Option<String>, query: String) -> Result<Vec<DocsItem>> {
        let q = query.trim().to_lowercase();
        self.with_open(|o| {
            let mut out = Vec::new();
            if q.is_empty() {
                return Ok(out);
            }
            for (vid, c) in o.docs_vaults(vault_id.as_deref())? {
                let tree = Tree::new(&c.payload.entries);
                out.extend(
                    c.payload
                        .entries
                        .iter()
                        .filter(|e| {
                            index::is_item(e) && index::name(e).to_lowercase().contains(&q) && !tree.in_trash(e)
                        })
                        .map(|e| item(&vid, &tree, e)),
                );
            }
            Ok(sorted(out))
        })
    }

    /// The favorites, in one GhulDocs vault or in all (not those in the trash).
    pub fn docs_favorites(&self, vault_id: Option<String>) -> Result<Vec<DocsItem>> {
        self.with_open(|o| {
            let mut out = Vec::new();
            for (vid, c) in o.docs_vaults(vault_id.as_deref())? {
                let tree = Tree::new(&c.payload.entries);
                out.extend(
                    c.payload
                        .entries
                        .iter()
                        .filter(|e| index::is_item(e) && index::favorite(e) && !tree.in_trash(e))
                        .map(|e| item(&vid, &tree, e)),
                );
            }
            Ok(sorted(out))
        })
    }

    /// What was moved to the trash (a folder with everything in it), the latest first.
    pub fn docs_trash(&self, vault_id: Option<String>) -> Result<Vec<DocsItem>> {
        self.with_open(|o| {
            let mut out = Vec::new();
            for (vid, c) in o.docs_vaults(vault_id.as_deref())? {
                let tree = Tree::new(&c.payload.entries);
                out.extend(
                    c.payload
                        .entries
                        .iter()
                        .filter(|e| index::is_item(e) && index::trashed(e).is_some())
                        // Inside a folder that is in the trash itself: it goes with the folder.
                        .filter(|e| !tree.ancestors(e).iter().any(|f| index::trashed(f).is_some()))
                        .map(|e| item(&vid, &tree, e)),
                );
            }
            let mut out = sorted(out);
            out.sort_by_key(|i| std::cmp::Reverse(i.trashed));
            Ok(out)
        })
    }

    /// A new folder (`folder` `None`: at the top). A name that is taken there: `AlreadyExists`.
    pub fn docs_create_folder(&self, vault_id: String, folder: Option<String>, name: String) -> Result<DocsItem> {
        let name = check_name(&name)?;
        let folder = folder_id(folder)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let tree = Tree::new(entries);
            check_folder(&tree, folder)?;
            if tree.name_taken(folder, &name, None) {
                return Err(GhulError::AlreadyExists);
            }
            let e = new_entry(index::FOLDER, name, folder, now);
            let id = index::id_of(&e).unwrap_or_default();
            entries.push(e);
            item_of(&vault_id, entries, &id)
        })
    }

    /// Renames a file or folder (a name that is taken in its folder: `AlreadyExists`).
    pub fn docs_rename(&self, vault_id: String, id: String, name: String) -> Result<DocsItem> {
        let id = parse_id(&id)?;
        let name = check_name(&name)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let tree = Tree::new(entries);
            let e = tree.get(&id).ok_or(GhulError::NotFound)?;
            if tree.in_trash(e) {
                return Err(GhulError::NotFound);
            }
            if tree.name_taken(index::parent(e), &name, Some(&id)) {
                return Err(GhulError::AlreadyExists);
            }
            let pos = position(entries, &id)?;
            let e = &mut entries[pos];
            e.fields.insert(index::NAME.into(), Field::Text(name));
            touch(e, now);
            item_of(&vault_id, entries, &id)
        })
    }

    /// Moves a file or folder into another folder of its vault (`None`: to the top). A folder
    /// cannot go into itself; a name that is taken there: `AlreadyExists`.
    pub fn docs_move(&self, vault_id: String, id: String, folder: Option<String>) -> Result<DocsItem> {
        let id = parse_id(&id)?;
        let folder = folder_id(folder)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let tree = Tree::new(entries);
            let e = tree.get(&id).ok_or(GhulError::NotFound)?;
            if tree.in_trash(e) {
                return Err(GhulError::NotFound);
            }
            check_folder(&tree, folder)?;
            if folder.is_some_and(|f| tree.is_within(&f, &id)) {
                return Err(GhulError::InvalidInput { msg: "a folder cannot go into itself".into() });
            }
            if tree.name_taken(folder, &index::name(e), Some(&id)) {
                return Err(GhulError::AlreadyExists);
            }
            let pos = position(entries, &id)?;
            index::set_parent(&mut entries[pos], folder);
            touch(&mut entries[pos], now);
            item_of(&vault_id, entries, &id)
        })
    }

    /// Marks a file or folder as favorite (or not).
    pub fn docs_set_favorite(&self, vault_id: String, id: String, favorite: bool) -> Result<()> {
        let id = parse_id(&id)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let pos = position(entries, &id)?;
            let e = &mut entries[pos];
            if favorite {
                e.fields.insert(index::FAVORITE.into(), Field::Bool(true));
            } else {
                e.fields.remove(index::FAVORITE);
            }
            touch(e, now);
            Ok(())
        })
    }

    /// Moves a file or folder (with everything in it) to the trash. Its content stays and
    /// counts until the trash is emptied.
    pub fn docs_to_trash(&self, vault_id: String, id: String) -> Result<()> {
        let id = parse_id(&id)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let pos = position(entries, &id)?;
            let e = &mut entries[pos];
            if index::trashed(e).is_none() {
                index::set_trashed(e, Some(now));
                touch(e, now);
            }
            Ok(())
        })
    }

    /// Takes a file or folder out of the trash. If its folder is gone or in the trash itself,
    /// it goes to the top; a name that is taken gets a number.
    pub fn docs_restore(&self, vault_id: String, id: String) -> Result<DocsItem> {
        let id = parse_id(&id)?;
        self.docs_change(&vault_id, |entries, now, _| {
            let tree = Tree::new(entries);
            let e = tree.get(&id).ok_or(GhulError::NotFound)?;
            if !tree.in_trash(e) {
                return Ok(item(&vault_id, &tree, e));
            }
            let folder =
                index::parent(e).filter(|p| tree.get(p).is_some_and(|f| index::is_folder(f) && !tree.in_trash(f)));
            let name = tree.free_name(folder, &index::name(e), Some(&id));
            let pos = position(entries, &id)?;
            let e = &mut entries[pos];
            index::set_trashed(e, None);
            index::set_parent(e, folder);
            e.fields.insert(index::NAME.into(), Field::Text(name));
            touch(e, now);
            item_of(&vault_id, entries, &id)
        })
    }

    /// Deletes a file or folder (with everything in it) for good, also its content files.
    pub fn docs_delete_forever(&self, vault_id: String, id: String) -> Result<()> {
        let id = parse_id(&id)?;
        let blobs = self.docs_change(&vault_id, |entries, _, _| {
            let tree = Tree::new(entries);
            let gone: Vec<Vec<u8>> = tree.subtree(&id).iter().map(|e| e.id.clone()).collect();
            if gone.is_empty() {
                return Err(GhulError::NotFound);
            }
            let blobs = remove(entries, &gone);
            Ok(blobs)
        })?;
        // The index no longer has them (a content file left behind goes at the next unlock).
        self.delete_contents(&vault_id, &blobs);
        Ok(())
    }

    /// Deletes everything in the trash for good (of one GhulDocs vault or of all).
    pub fn docs_empty_trash(&self, vault_id: Option<String>) -> Result<()> {
        let ids: Vec<String> =
            self.with_open(|o| Ok(o.docs_vaults(vault_id.as_deref())?.into_iter().map(|(id, _)| id).collect()))?;
        for vid in ids {
            let blobs = self.docs_change(&vid, |entries, _, _| {
                let tree = Tree::new(entries);
                let gone: Vec<Vec<u8>> =
                    entries.iter().filter(|e| index::is_item(e) && tree.in_trash(e)).map(|e| e.id.clone()).collect();
                Ok(remove(entries, &gone))
            })?;
            self.delete_contents(&vid, &blobs);
        }
        Ok(())
    }

    /// Adds a document: its content is read from `source` and encrypted while it is read (no
    /// plaintext reaches the disk), into `folder` (`None`: the top). A name that is taken gets a
    /// number. More than GhulDocs may hold: `DocsLimit`, before anything is written when the
    /// source knows its size, otherwise as soon as it is too much (then nothing stays).
    /// `progress` gets the bytes read.
    pub fn docs_add_file(
        &self,
        vault_id: String,
        folder: Option<String>,
        name: String,
        mime: String,
        source: Arc<dyn ArchiveSource>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<DocsItem> {
        let name = check_name(&name)?;
        let mime = clean_mime(&mime);
        let folder = folder_id(folder)?;
        let declared = source.size();
        let limit = self.docs_limit();
        let (dir, room, used) = self.with_open(|o| {
            check_folder(&Tree::new(&o.docs(&vault_id)?.payload.entries), folder)?;
            let have = usage(o);
            check_room(have, Usage::file(declared), limit)?;
            let room = limit.saturating_sub(have.used).min(stored_limit(limit).saturating_sub(have.stored));
            Ok((self.docs_dir(&vault_id)?, room, have.used))
        })?;
        let content_id = new_id();
        let mut key = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(key.as_mut());
        let mut report = reporter(progress, declared);
        let info = blob::write_file(&dir, &content_id, &key, &mut SourceReader::new(source), room, &mut |d| report(d))
            .map_err(|e| match e {
                BlobError::TooLarge => GhulError::DocsLimit { used, limit, needed: 0 },
                e => blob_error(e),
            })?;
        let r = self.docs_change(&vault_id, |entries, now, have| {
            // Checked again with what was really read: another add may have come first.
            check_room(have, Usage::file(info.size), limit)?;
            let tree = Tree::new(entries);
            check_folder(&tree, folder)?;
            let name = tree.free_name(folder, &name, None);
            let mut e = new_entry(index::FILE, name, folder, now);
            e.fields.insert(index::SIZE.into(), Field::Int(info.size as i64));
            e.fields.insert(index::MIME.into(), Field::Text(mime));
            e.fields.insert(index::KEY.into(), Field::Bytes(key.to_vec()));
            e.fields.insert(index::BLOB.into(), Field::Bytes(content_id.to_vec()));
            e.fields.insert(index::SHA256.into(), Field::Bytes(info.sha256.to_vec()));
            e.fields.insert(index::CRC32.into(), Field::Int(i64::from(info.crc32)));
            let id = index::id_of(&e).unwrap_or_default();
            entries.push(e);
            item_of(&vault_id, entries, &id)
        });
        if r.is_err() {
            let _ = fs::remove_file(dir.join(blob::file_name(&content_id)));
        }
        r
    }

    /// Writes the content of a document to `sink`, decrypted while it is read; every section is
    /// checked before it is written. A damaged or missing content file: `Corrupted`, and what
    /// reached the sink is incomplete and must be thrown away. `progress` gets the bytes written.
    pub fn docs_read_file(
        &self,
        vault_id: String,
        id: String,
        sink: Arc<dyn ArchiveSink>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<()> {
        let id = parse_id(&id)?;
        let (dir, content, key) = self.with_open(|o| {
            let tree = Tree::new(&o.docs(&vault_id)?.payload.entries);
            let e = tree.get(&id).ok_or(GhulError::NotFound)?;
            if index::is_folder(e) {
                return Err(GhulError::InvalidInput { msg: "a folder has no content".into() });
            }
            let content = index::content(e).ok_or_else(damaged_doc)?;
            let key = index::key(e).ok_or_else(damaged_doc)?;
            Ok((self.docs_dir(&vault_id)?, content, key))
        })?;
        let mut out = SinkWriter::new(sink);
        let mut report = reporter(progress, content.size);
        blob::read_file(&dir, &content.blob, &key, content.size, &mut out, &mut |d| report(d)).map_err(blob_error)
    }

    /// How much of GhulDocs is used (the trash included) and the limit.
    pub fn docs_usage(&self) -> Result<DocsUsage> {
        self.with_open(|o| {
            let u = usage(o);
            Ok(DocsUsage { used: u.used, limit: self.docs_limit(), files: u.files as u32 })
        })
    }

    /// Checks every content file of a GhulDocs vault (there, complete, its SHA-256 as in the
    /// index) and returns the documents that are damaged or missing.
    pub fn docs_verify(&self, vault_id: String) -> Result<Vec<DocsItem>> {
        let (dir, list) = self.with_open(|o| {
            let c = o.docs(&vault_id)?;
            let tree = Tree::new(&c.payload.entries);
            let list: Vec<(DocsItem, Option<index::Content>)> = c
                .payload
                .entries
                .iter()
                .filter(|e| index::is_item(e) && !index::is_folder(e))
                .map(|e| (item(&vault_id, &tree, e), index::content(e)))
                .collect();
            Ok((self.docs_dir(&vault_id)?, list))
        })?;
        let mut bad = Vec::new();
        for (it, content) in list {
            let ok = content.is_some_and(|c| {
                fs::File::open(dir.join(blob::file_name(&c.blob)))
                    .and_then(super::zipstream::digest)
                    .is_ok_and(|(len, crc, sha)| len == blob::file_len(c.size) && crc == c.crc32 && sha == c.sha256)
            });
            if !ok {
                bad.push(it);
            }
        }
        Ok(sorted(bad))
    }
}

/// Removes the items `gone` from `entries`; returns the content files they had.
fn remove(entries: &mut Vec<Entry>, gone: &[Vec<u8>]) -> Vec<Id> {
    let mut blobs = Vec::new();
    entries.retain_mut(|e| {
        if !index::is_item(e) || !gone.contains(&e.id) {
            return true;
        }
        if let Some(b) = index::blob(e) {
            blobs.push(b);
        }
        zeroize::Zeroize::zeroize(e);
        false
    });
    blobs
}

/// Entry points for fuzzing the parsers of hostile GhulDocs data (`fuzz_tests` and the long
/// libFuzzer runs in `core/fuzz`): the stream ZIP reader, content files and indexes.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) mod fuzz {
    use super::{blob, index, DOCS_LIMIT};

    /// The key the content files are decrypted with (as in the test vector).
    pub(crate) const KEY: [u8; 32] = {
        let mut k = [0u8; 32];
        let mut i = 0;
        while i < 32 {
            k[i] = i as u8;
            i += 1;
        }
        k
    };

    /// `data` as a `.ghlzip`, read as a stream like a suite file (every entry to the end).
    pub(crate) fn zip(data: &[u8]) {
        let mut z = crate::app::zipstream::ZipReader::new(data, 1 << 24, 64, |_| {});
        while let Ok(Some(_)) = z.next_entry() {
            if z.read_entry(&mut std::io::sink()).is_err() {
                return;
            }
        }
    }

    /// Inputs that hold, to start mutating from: a `.ghlzip` like a GhulDocs vault on its own,
    /// a content file (the test vector) and an index with a folder and a file in it.
    #[cfg(test)]
    pub(crate) fn seeds() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        use crate::vault::{Entry, Field};
        let (id, key, content, file, info) = blob::tests::hello_vector();
        assert_eq!(key, KEY);
        let mut blob_seed = id.to_vec();
        blob_seed.extend_from_slice(&(content.len() as u64).to_le_bytes());
        blob_seed.extend_from_slice(&file);
        let mut z = crate::app::zipstream::ZipWriter::new(Vec::new(), |_| {});
        z.add_bytes("0123456789abcdef0123456789abcdef.ghldoc", b"vault").unwrap_or_default();
        let name = format!("docs-0123456789abcdef0123456789abcdef/{}", blob::file_name(&id));
        z.add_bytes(&name, &file).unwrap_or_default();
        let zip_seed = z.finish().unwrap_or_default();
        let mut p = crate::vault::payload::Payload::new(crate::vault::ContainerKind::Docs);
        let mut folder = Entry::new([1u8; 16], index::FOLDER, 1);
        folder.fields.insert(index::NAME.into(), Field::Text("Steuer".into()));
        let mut doc = Entry::new([2u8; 16], index::FILE, 2);
        doc.fields.insert(index::NAME.into(), Field::Text("Bescheid.pdf".into()));
        index::set_parent(&mut doc, Some([1u8; 16]));
        doc.fields.insert(index::SIZE.into(), Field::Int(content.len() as i64));
        doc.fields.insert(index::MIME.into(), Field::Text("application/pdf".into()));
        doc.fields.insert(index::KEY.into(), Field::Bytes(KEY.to_vec()));
        doc.fields.insert(index::BLOB.into(), Field::Bytes(id.to_vec()));
        doc.fields.insert(index::SHA256.into(), Field::Bytes(info.sha256.to_vec()));
        doc.fields.insert(index::CRC32.into(), Field::Int(i64::from(info.crc32)));
        p.entries = vec![folder, doc];
        assert!(index::validate(&p.entries, DOCS_LIMIT));
        let index_seed = p.to_cbor().map(|c| c.to_vec()).unwrap_or_default();
        (zip_seed, blob_seed, index_seed)
    }

    /// `data` as id (16 bytes), size (8 bytes, little endian) and a content file.
    pub(crate) fn content_file(data: &[u8]) {
        if data.len() < 24 {
            return;
        }
        let id: [u8; 16] = data[..16].try_into().unwrap_or_default();
        let size = u64::from_le_bytes(data[16..24].try_into().unwrap_or_default()) % (DOCS_LIMIT + 1);
        let mut f = std::io::Cursor::new(&data[24..]);
        let _ = blob::decrypt(&mut f, &id, &KEY, size, &mut std::io::sink(), &mut |_| {});
    }

    /// `data` as the payload of a GhulDocs vault: its index is checked, and every item is
    /// looked at in its tree like the app does (also when the check fails: the tree must not
    /// hang or crash on cycles or missing folders either).
    pub(crate) fn docs_index(data: &[u8]) {
        let Ok(p) = crate::vault::payload::Payload::from_cbor(data, crate::vault::ContainerKind::Docs) else { return };
        let entries = &p.entries;
        let _ = index::validate(entries, DOCS_LIMIT);
        let tree = index::Tree::new(entries);
        for e in entries.iter().filter(|e| index::is_item(e)) {
            let _ = index::content(e);
            let _ = tree.path(e);
            let _ = tree.in_trash(e);
            if let Some(id) = index::id_of(e) {
                let _ = tree.subtree(&id);
                let _ = tree.free_name(index::parent(e), &index::name(e), Some(&id));
            }
        }
    }
}
