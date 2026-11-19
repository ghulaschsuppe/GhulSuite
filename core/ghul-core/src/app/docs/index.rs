// SPDX-License-Identifier: AGPL-3.0-or-later
//! The index of a GhulDocs vault: its folders and files, kept as the entries of its container
//! (FORMAT.md section 14). Nothing else is in a GhulDocs vault.
//!
//! - `folder`: `name`, `parent` (16 bytes, missing at the top), `created`, `favorite`, `trashed`
//! - `file`: the same and `size`, `mime`, `key` (32 bytes, the document's own key), `blob`
//!   (16 bytes, the id of its content file), `sha256` and `crc32` of the content file
//!
//! An item is in the trash when it or a folder above it has `trashed` (when it was moved there,
//! Unix ms). Unknown fields stay as they are.

use std::collections::{HashMap, HashSet};

use zeroize::Zeroizing;

use crate::vault::{Entry, Field};

pub(super) const FOLDER: &str = "folder";
pub(super) const FILE: &str = "file";
pub(super) const NAME: &str = "name";
pub(super) const PARENT: &str = "parent";
pub(super) const CREATED: &str = "created";
pub(super) const FAVORITE: &str = "favorite";
pub(super) const TRASHED: &str = "trashed";
pub(super) const SIZE: &str = "size";
pub(super) const MIME: &str = "mime";
pub(super) const KEY: &str = "key";
pub(super) const BLOB: &str = "blob";
pub(super) const SHA256: &str = "sha256";
pub(super) const CRC32: &str = "crc32";

pub(super) type Id = [u8; 16];

pub(super) fn id_of(e: &Entry) -> Option<Id> {
    e.id.as_slice().try_into().ok()
}

fn bytes<const N: usize>(e: &Entry, field: &str) -> Option<[u8; N]> {
    match e.fields.get(field) {
        Some(Field::Bytes(b)) => b.as_slice().try_into().ok(),
        _ => None,
    }
}

fn int(e: &Entry, field: &str) -> Option<i64> {
    match e.fields.get(field) {
        Some(Field::Int(i)) => Some(*i),
        _ => None,
    }
}

pub(super) fn is_item(e: &Entry) -> bool {
    !e.deleted && (e.entry_type == FOLDER || e.entry_type == FILE) && id_of(e).is_some()
}

pub(super) fn is_folder(e: &Entry) -> bool {
    e.entry_type == FOLDER
}

pub(super) fn name(e: &Entry) -> String {
    match e.fields.get(NAME) {
        Some(Field::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

pub(super) fn parent(e: &Entry) -> Option<Id> {
    bytes(e, PARENT)
}

pub(super) fn set_parent(e: &mut Entry, parent: Option<Id>) {
    match parent {
        Some(p) => e.fields.insert(PARENT.into(), Field::Bytes(p.to_vec())),
        None => e.fields.remove(PARENT),
    };
}

pub(super) fn trashed(e: &Entry) -> Option<i64> {
    int(e, TRASHED)
}

pub(super) fn set_trashed(e: &mut Entry, at: Option<i64>) {
    match at {
        Some(t) => e.fields.insert(TRASHED.into(), Field::Int(t)),
        None => e.fields.remove(TRASHED),
    };
}

pub(super) fn favorite(e: &Entry) -> bool {
    matches!(e.fields.get(FAVORITE), Some(Field::Bool(true)))
}

pub(super) fn created(e: &Entry) -> i64 {
    int(e, CREATED).unwrap_or(e.modified)
}

/// Bytes of content of a file (0 for folders).
pub(super) fn size(e: &Entry) -> u64 {
    int(e, SIZE).and_then(|s| u64::try_from(s).ok()).unwrap_or(0)
}

pub(super) fn mime(e: &Entry) -> String {
    match e.fields.get(MIME) {
        Some(Field::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

/// The id of the file's content file.
pub(super) fn blob(e: &Entry) -> Option<Id> {
    bytes(e, BLOB)
}

pub(super) fn key(e: &Entry) -> Option<Zeroizing<[u8; 32]>> {
    match e.fields.get(KEY) {
        Some(Field::Bytes(b)) => <[u8; 32]>::try_from(b.as_slice()).ok().map(Zeroizing::new),
        _ => None,
    }
}

pub(super) fn sha256(e: &Entry) -> Option<[u8; 32]> {
    bytes(e, SHA256)
}

pub(super) fn crc32(e: &Entry) -> Option<u32> {
    int(e, CRC32).and_then(|c| u32::try_from(c).ok())
}

/// A file entry and what is needed to read or copy its content file.
pub(super) struct Content {
    pub(super) blob: Id,
    pub(super) size: u64,
    pub(super) sha256: [u8; 32],
    pub(super) crc32: u32,
}

/// The content file of a file entry (`None` for folders and broken entries).
pub(super) fn content(e: &Entry) -> Option<Content> {
    if e.entry_type != FILE {
        return None;
    }
    Some(Content { blob: blob(e)?, size: size(e), sha256: sha256(e)?, crc32: crc32(e)? })
}

/// Checks an index that came from outside (a suite file, a vault handed on): every item is
/// complete, ids and content files are unique, parents are folders of this index and there
/// are no cycles. `max_size`: the largest document allowed.
pub(super) fn validate(entries: &[Entry], max_size: u64) -> bool {
    let items: Vec<&Entry> = entries.iter().filter(|e| !e.deleted).collect();
    if items.iter().any(|e| !is_item(e)) {
        return false;
    }
    let mut ids = HashSet::new();
    let mut blobs = HashSet::new();
    let mut folders = HashSet::new();
    for e in &items {
        let id = id_of(e).unwrap_or_default();
        if !ids.insert(id) || super::check_name(&name(e)).is_err() {
            return false;
        }
        if is_folder(e) {
            folders.insert(id);
            continue;
        }
        let ok = matches!(int(e, SIZE), Some(s) if s >= 0 && s as u64 <= max_size)
            && key(e).is_some()
            && sha256(e).is_some()
            && crc32(e).is_some()
            && blob(e).is_some_and(|b| blobs.insert(b));
        if !ok {
            return false;
        }
    }
    let tree = Tree::new(entries);
    items.iter().all(|e| match parent(e) {
        None => true,
        Some(p) => folders.contains(&p) && !tree.has_cycle(e),
    })
}

/// The items of an index, with their folders and children at hand.
pub(super) struct Tree<'a> {
    entries: &'a [Entry],
    by_id: HashMap<Id, usize>,
    children: HashMap<Option<Id>, Vec<usize>>,
}

impl<'a> Tree<'a> {
    pub(super) fn new(entries: &'a [Entry]) -> Self {
        let mut by_id = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if is_item(e) {
                by_id.entry(id_of(e).unwrap_or_default()).or_insert(i);
            }
        }
        let mut children: HashMap<Option<Id>, Vec<usize>> = HashMap::new();
        for &i in by_id.values() {
            let p = parent(&entries[i]).filter(|p| by_id.get(p).is_some_and(|&j| is_folder(&entries[j])));
            children.entry(p).or_default().push(i);
        }
        Tree { entries, by_id, children }
    }

    pub(super) fn get(&self, id: &Id) -> Option<&'a Entry> {
        self.by_id.get(id).map(|&i| &self.entries[i])
    }

    /// The items directly in `folder` (`None`: at the top).
    pub(super) fn children(&self, folder: Option<Id>) -> impl Iterator<Item = &'a Entry> + '_ {
        self.children.get(&folder).into_iter().flatten().map(|&i| &self.entries[i])
    }

    /// The folder an item is in, if it exists.
    fn up(&self, e: &Entry) -> Option<&'a Entry> {
        parent(e).and_then(|p| self.get(&p)).filter(|f| is_folder(f))
    }

    /// The folders above an item, the nearest first (stops at a cycle).
    pub(super) fn ancestors(&self, e: &Entry) -> Vec<&'a Entry> {
        let mut out: Vec<&'a Entry> = Vec::new();
        let mut cur = self.up(e);
        while let Some(f) = cur {
            if out.len() > self.entries.len() || out.iter().any(|x| x.id == f.id) || f.id == e.id {
                break;
            }
            out.push(f);
            cur = self.up(f);
        }
        out
    }

    fn has_cycle(&self, e: &Entry) -> bool {
        let mut seen = HashSet::new();
        seen.insert(e.id.clone());
        let mut cur = self.up(e);
        while let Some(f) = cur {
            if !seen.insert(f.id.clone()) {
                return true;
            }
            cur = self.up(f);
        }
        false
    }

    /// Whether the item or a folder above it is in the trash.
    pub(super) fn in_trash(&self, e: &Entry) -> bool {
        trashed(e).is_some() || self.ancestors(e).iter().any(|f| trashed(f).is_some())
    }

    /// The names of the folders above, from the top, joined with `/`.
    pub(super) fn path(&self, e: &Entry) -> String {
        let mut names: Vec<String> = self.ancestors(e).iter().map(|f| name(f)).collect();
        names.reverse();
        names.join("/")
    }

    /// The item and everything below it (each once).
    pub(super) fn subtree(&self, id: &Id) -> Vec<&'a Entry> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut todo: Vec<Id> = vec![*id];
        while let Some(cur) = todo.pop() {
            if !seen.insert(cur) {
                continue;
            }
            let Some(e) = self.get(&cur) else { continue };
            out.push(e);
            if is_folder(e) {
                todo.extend(self.children(Some(cur)).filter_map(id_of));
            }
        }
        out
    }

    /// Whether `id` is `of` or below it.
    pub(super) fn is_within(&self, id: &Id, of: &Id) -> bool {
        if id == of {
            return true;
        }
        self.get(id).is_some_and(|e| self.ancestors(e).iter().any(|f| id_of(f).as_ref() == Some(of)))
    }

    /// Whether a name is taken in `folder` by another item that is not in the trash
    /// (case-insensitive).
    pub(super) fn name_taken(&self, folder: Option<Id>, name_: &str, except: Option<&Id>) -> bool {
        let lower = name_.to_lowercase();
        self.children(folder)
            .filter(|c| trashed(c).is_none() && id_of(c).as_ref() != except)
            .any(|c| name(c).to_lowercase() == lower)
    }

    /// `name_`, or `name (2).ext`, `name (3).ext`, ... if it is taken in `folder`.
    pub(super) fn free_name(&self, folder: Option<Id>, name_: &str, except: Option<&Id>) -> String {
        if !self.name_taken(folder, name_, except) {
            return name_.to_owned();
        }
        let (stem, ext) = match name_.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() && !e.is_empty() && e.len() <= 10 => (s, format!(".{e}")),
            _ => (name_, String::new()),
        };
        (2..)
            .map(|i| {
                // Still a valid name: the stem gets shorter if needed.
                let tail = format!(" ({i}){ext}");
                let room = super::MAX_NAME.saturating_sub(tail.chars().count());
                format!("{}{tail}", stem.chars().take(room).collect::<String>())
            })
            .find(|n| !self.name_taken(folder, n, except))
            .unwrap_or_else(|| name_.to_owned())
    }
}
