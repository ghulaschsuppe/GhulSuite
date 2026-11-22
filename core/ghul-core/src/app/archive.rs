// SPDX-License-Identifier: AGPL-3.0-or-later
//! The suite as one file (`.ghlzip`): opened into the working copy and saved from it.
//!
//! A `.ghlzip` is a ZIP with exactly the suite file, its vault files, the content files of its
//! GhulDocs vaults and the header `export.ghlexp`, a container of its own (sealed like the
//! vaults) that lists every vault file with its kind, container id and SHA-256, and says which
//! state the file is: a random state id, the state it was made from, a counter and the
//! protection epoch. The content files are listed with their SHA-256 in the index of their
//! GhulDocs vault, which comes before them; they are kept on the disk while they are checked,
//! never in memory. Opening checks everything before
//! anything is replaced; nothing is matched or merged. Saving seals everything again and writes
//! a new state.
//!
//! What the device saw last (highest counter, its state id and the highest protection epoch)
//! is kept per suite in the device's own folder, so an older file, a branch or a file with an
//! older protection is noticed even after the working copy was replaced.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::docs::{content_files, parse_entry_name, remove_all_contents, stage_entry, ContentFile, Expected, Staging};
use super::error::{GhulError, Result};
use super::storage::StorageError;
use super::suite::{
    is_vault_file, now_ms, parse_keyfile, put_setting, second_factor, setting, FactorProof, GhulSuite, ProtectionInfo,
    SUITE_FILE,
};
use super::zipstream::{zip_len, EntryInfo, ZipError, ZipReader, ZipWriter, CHUNK};
use crate::vault::format::MAX_FILE_LEN;
use crate::vault::{ContainerKind, FactorSecret, LockedContainer, PasswordKey, RecoveryCode, Slot, UnlockedContainer};
use crate::{FidoListener, HidTransport};

/// Name of the header inside a `.ghlzip`.
pub const HEADER_FILE: &str = "export.ghlexp";
/// The header's setting with its content.
const HEADER_KEY: &str = "export";
/// The suite setting naming the state the working copy is (loaded from or saved as).
const STATE_KEY: &str = "archive_state";
/// What this device saw last, per suite (in the device's own folder).
const SEEN_FILE: &str = "archive-seen.json";
/// Marks unsaved changes in the working copy (in the device's own folder).
const UNSAVED_FILE: &str = "archive-unsaved";
/// The message for anything that is no `.ghlzip` (translated by the app).
const NOT_A_SUITE_FILE: &str = "not a GhulSuite file";
/// M10: a suite file or GhulDocs package that ends early (not written completely).
pub const INCOMPLETE_FILE: &str = "the suite file is incomplete";
const DOCS_PACKAGE_NOT_SUITE: &str = "this is a GhulDocs package, not a suite file";
const SUITE_NOT_DOCS_PACKAGE: &str = "this is a suite file, not a GhulDocs package";
/// A suite file with more GhulDocs vaults than a suite may have is invalid.
const DOCS_VAULTS_MANY: &str = "a suite file holds at most 3 GhulDocs vaults";
/// Largest `.ghlzip` read (counted while reading, not taken from the ZIP): full GhulDocs vaults
/// (3 GB together) plus the other vaults, 3.75 GiB. Below 4 GiB with room for the central
/// directory: the ZIP has no ZIP64, and FAT32 sticks take it.
pub(crate) const MAX_ARCHIVE: u64 = 3840 * 1024 * 1024;
/// What the vaults besides GhulDocs (and the suite file) may take in a suite file together, so a
/// full GhulDocs ([`super::docs::stored_limit`] of the content files of all GhulDocs vaults plus the ZIP entries of
/// [`super::docs::DOCS_MAX_FILES`] documents) and these always fit into [`MAX_ARCHIVE`]; checked
/// when attachments or vaults are added (estimated from the payloads, see [`container_weight`]).
pub(crate) const OTHER_LIMIT: u64 = 200 * 1024 * 1024;
/// Room a container takes besides its payload (header, slots, encryption), counted generously.
const CONTAINER_EXTRA: u64 = 64 * 1024;
/// The message when a suite file would be larger than [`MAX_ARCHIVE`].
/// Most entries a `.ghlzip` may have (a suite has a handful of vault files and up to
/// [`super::docs::DOCS_MAX_FILES`] documents).
pub(crate) const MAX_ENTRIES: usize = super::docs::DOCS_MAX_FILES as usize + 1000;
/// How many earlier states a file and a device remember (to tell newer states from branches).
const MAX_ANCESTORS: usize = 1024;

/// While a suite file replaces the working copy (in the local folder): what was there before, so
/// that a device that went off in the middle finds the state before again at the next start.
pub(super) const REPLACE_JOURNAL: &str = "replace-journal.json";
/// The files of the working copy before, kept until the replacing is complete.
pub(super) const REPLACE_OLD_DIR: &str = "replace-old";

/// See [`REPLACE_JOURNAL`].
#[derive(Serialize, Deserialize, Default)]
struct ReplaceJournal {
    /// The suite file and vault files of the working copy before (copies in [`REPLACE_OLD_DIR`]).
    old_files: Vec<String>,
    /// The content folders of GhulDocs before (`docs-<id>`).
    old_docs: Vec<String>,
    /// The folder they are moved aside to, once it exists: its name in the staging folder of the
    /// content root (round 6: never an absolute path, a portable data folder may come back under
    /// another drive letter). Journals of before held the absolute path; both are read.
    aside: Option<String>,
}

#[cfg(test)]
thread_local! {
    /// Tests: the replacing stops after this many files written, as if the device went off.
    pub(crate) static CRASH_AFTER_WRITES: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
thread_local! {
    /// Tests: the next rollback of a replace that failed fails too (e.g. the storage went away).
    pub(crate) static ROLLBACK_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: deleting the working copy fails after it began (a file held by another program).
    pub(crate) static DELETE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: putting back the state before an unfinished replace fails (while set).
    pub(crate) static RECOVER_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A replace of the working copy could not be finished and its state before not be put back yet
/// (translated by the app: restart the app, free some space).
pub const REPLACE_UNFINISHED: &str = "an earlier opening of a suite file could not be finished";

/// Tests: the device "went off" (see [`CRASH_AFTER_WRITES`]).
#[cfg(test)]
const CRASHED: &str = "device went off (test)";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct ArchiveFile {
    name: String,
    kind: String,
    id: String,
    sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct ArchiveHeader {
    files: Vec<ArchiveFile>,
    /// Random, new with every save.
    state: String,
    /// The state the saving device had loaded (empty for the first save).
    previous: String,
    counter: u64,
    epoch: u64,
    /// When it was saved (Unix ms) and on which device.
    time: i64,
    device: String,
    /// The states this one was carried on from, the latest first (at most [`MAX_ANCESTORS`]).
    ancestors: Vec<String>,
    /// `ancestors` reaches back to the first save: a state not in it is no ancestor.
    complete: bool,
}

/// The state a working copy is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct StateRef {
    state: String,
    counter: u64,
    /// The state before (still in the file when the last built one was not written).
    previous: String,
    /// The states this one was carried on from, the latest first.
    ancestors: Vec<String>,
    /// See [`ArchiveHeader::complete`].
    complete: bool,
    /// The id of the header container of the file this state was last written to or opened
    /// from (hex; visible without a key, new with every save). Empty before.
    #[serde(default)]
    file: String,
}

/// What a device saw last of a suite.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct Seen {
    state: String,
    counter: u64,
    epoch: u64,
    /// The states the seen one was carried on from (see [`ArchiveHeader::ancestors`]).
    ancestors: Vec<String>,
    complete: bool,
    /// The counter of `state` (`counter` is the highest one seen).
    at: u64,
    /// The header container id of the file this device last saved or opened (see
    /// [`StateRef::file`]): after a change of the protection the file cannot be read with the new
    /// keys, but it is still known as this device's own.
    #[serde(default)]
    file: String,
    /// When the state of `state` was saved into a file (Unix ms; the time in the file's header
    /// for one that was opened; 0 if not known).
    #[serde(default)]
    saved: i64,
    /// The protections this device had for this suite (see [`protection_print`]), the latest
    /// last: a file with one of them is older than the protection of now, not newer.
    #[serde(default)]
    protections: Vec<String>,
}

/// How many earlier protections a device keeps in mind.
const MAX_PROTECTIONS: usize = 64;

/// The protection of a container as its plaintext header shows it: the KDF (salt) and the
/// factor slots in full (with the wrapping of the factor secret: a removed key gives the other
/// slots a new factor secret under the same slot ids). The recovery slot is new in every
/// container and holds neither the password key nor the factor secret. Two files with the same
/// print open with the same password key and factor secret; the factor slots are the same bytes
/// in the suite file, the vaults and the header of a `.ghlzip`.
pub(super) fn protection_print(h: &crate::vault::format::Header) -> String {
    let mut slots: Vec<&Slot> = h.slots.iter().filter(|s| !matches!(s, Slot::Recovery(_))).collect();
    slots.sort_by_key(|s| *s.id());
    let mut d = Sha256::new();
    d.update(h.kdf.memory_kib.to_le_bytes());
    d.update(h.kdf.iterations.to_le_bytes());
    d.update(h.kdf.parallelism.to_le_bytes());
    d.update(h.kdf.salt);
    for s in slots {
        let mut bytes = Vec::new();
        s.encode(&mut bytes);
        d.update((bytes.len() as u64).to_le_bytes());
        d.update(&bytes);
    }
    crate::kdf::hex(&d.finalize()[..16])
}

fn note_protection(e: &mut Seen, print: String) {
    e.protections.retain(|p| *p != print);
    e.protections.push(print);
    if e.protections.len() > MAX_PROTECTIONS {
        e.protections.remove(0);
    }
}

/// What [`GhulSuite::archive_build`] built, kept until [`GhulSuite::archive_saved`] notes it:
/// that works also when the suite was locked while the file was written.
pub(super) struct BuiltState {
    suite_id: [u8; 16],
    state: StateRef,
    epoch: u64,
}

/// The file at the place a suite is saved to, before overwriting it (see
/// [`GhulSuite::archive_target_info`]).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ArchiveTargetInfo {
    pub target: ArchiveTarget,
    /// The device that saved it (empty if not known, e.g. not readable with the keys of now).
    pub device: String,
    /// When it was saved (Unix ms; 0 if not known).
    pub time: i64,
}

/// How a `.ghlzip` relates to what this device has.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveRelation {
    /// No suite on this device yet.
    NoSuite,
    /// Another suite than the one on this device: it replaces everything.
    OtherSuite,
    /// The state this device has.
    Same,
    /// A newer state of this suite.
    Newer,
    /// An older state than the one this device saw last.
    Older,
    /// A state that was carried on from another one than this device's (a branch).
    Branch,
}

/// The result of checking a `.ghlzip` (with the password or the recovery code). Nothing is
/// replaced yet: `archive_accept` does that, `archive_cancel` forgets it.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ArchiveCheck {
    pub relation: ArchiveRelation,
    /// The working copy has changes that were not saved into a file: they would be lost.
    pub unsaved_changes: bool,
    /// Opened with the recovery code: the seals could not be checked.
    pub unverified: bool,
    /// When and where the file was saved.
    pub saved_at: i64,
    pub device: String,
    /// How many vaults (areas) the file holds.
    pub vaults: u32,
}

/// The file at the place a suite is saved to, compared with the working copy.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveTarget {
    /// No file there yet.
    New,
    /// The state this working copy was loaded from or last saved as: overwriting loses nothing.
    Same,
    /// Saved there from somewhere else meanwhile.
    ChangedElsewhere,
    /// Another suite (or no GhulSuite file at all).
    OtherSuite,
    /// This suite with a protection this device does not have (changed elsewhere: a new
    /// password, a key removed): overwriting it would undo that change. Open it first.
    OtherProtection,
}

/// A built `.ghlzip`, written to the sink of `archive_build`: once the file is in place (and
/// [`archive_written_matches`] found it complete), confirm with `archive_saved(state, change)`.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ArchiveBuilt {
    pub state: String,
    /// The change of the working copy it holds: a change made while the file is written is
    /// not in it and stays unsaved.
    pub change: u64,
    /// Size and SHA-256 (hex) of the whole file as written.
    pub size: u64,
    pub sha256: String,
}

/// A suite file being read: the platform opens it (a path, a document of Android's Storage
/// Access Framework), the core takes it piece by piece.
#[uniffi::export(with_foreign)]
pub trait ArchiveSource: Send + Sync {
    /// The size in bytes, 0 if not known (only for the progress; the core counts itself).
    fn size(&self) -> u64;
    /// Up to `max` next bytes; empty at the end.
    fn read(&self, max: u32) -> std::result::Result<Vec<u8>, StorageError>;
}

/// Where a suite file is written to, piece by piece.
#[uniffi::export(with_foreign)]
pub trait ArchiveSink: Send + Sync {
    fn write(&self, data: Vec<u8>) -> std::result::Result<(), StorageError>;
}

/// How far writing or reading a suite file is.
#[uniffi::export(with_foreign)]
pub trait ArchiveProgress: Send + Sync {
    /// `done` of `total` bytes (`total` 0: not known).
    fn progress(&self, done: u64, total: u64);
}

/// The files of a suite file: name and content.
type Files = Vec<(String, Vec<u8>)>;

/// A checked file waiting for `archive_accept`.
pub(super) struct Pending {
    files: Files,
    header: ArchiveHeader,
    suite_id: [u8; 16],
    /// With the recovery code: the code and the new password (set on accepting).
    recovery: Option<(Zeroizing<String>, Zeroizing<String>)>,
    /// With the password: the password key and the factor secret (the files verified with them).
    keys: Option<(PasswordKey, FactorSecret)>,
    /// The checked content files of its GhulDocs vaults (on the disk, removed when dropped).
    staged: Option<Staging>,
    /// The id of its header container (see [`StateRef::file`]).
    file_id: [u8; 16],
    /// The content change counter of the working copy when it was checked (M6: what was unsaved
    /// then is what the user was told about; see `content_change`).
    change: u64,
}

/// An [`ArchiveSource`] as a reader.
/// The piece held is wiped when the next one comes and when it is dropped (it may be the
/// plaintext of a document being added).
pub(super) struct SourceReader {
    source: Arc<dyn ArchiveSource>,
    buf: Zeroizing<Vec<u8>>,
    pos: usize,
}

impl SourceReader {
    pub(super) fn new(source: Arc<dyn ArchiveSource>) -> Self {
        SourceReader { source, buf: Zeroizing::new(Vec::new()), pos: 0 }
    }
}

impl Read for SourceReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos == self.buf.len() {
            // The old piece is wiped when it is replaced.
            self.buf = Zeroizing::new(self.source.read(CHUNK as u32).map_err(std::io::Error::other)?);
            self.pos = 0;
            if self.buf.is_empty() {
                return Ok(0);
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// An [`ArchiveSink`] as a writer (pieces of up to [`CHUNK`] bytes), hashing what it writes.
/// The piece being collected is wiped once it was handed on and when the writer is dropped (it
/// may be the plaintext of a document being read).
pub(super) struct SinkWriter {
    sink: Arc<dyn ArchiveSink>,
    buf: Zeroizing<Vec<u8>>,
    len: u64,
    sha: Sha256,
}

impl SinkWriter {
    pub(super) fn new(sink: Arc<dyn ArchiveSink>) -> Self {
        SinkWriter { sink, buf: Zeroizing::new(Vec::with_capacity(CHUNK)), len: 0, sha: Sha256::new() }
    }

    fn send(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            // The sink gets a copy (UniFFI copies it anyway); ours is wiped right after.
            let r = self.sink.write(self.buf.to_vec());
            zeroize::Zeroize::zeroize(&mut *self.buf);
            r.map_err(std::io::Error::other)?;
        }
        Ok(())
    }
}

impl Write for SinkWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let n = data.len().min(CHUNK - self.buf.len());
        self.buf.extend_from_slice(&data[..n]);
        self.sha.update(&data[..n]);
        self.len += n as u64;
        if self.buf.len() == CHUNK {
            self.send()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

/// Reports progress to the app, at most once per megabyte (and at the end).
pub(super) fn reporter(progress: Option<Arc<dyn ArchiveProgress>>, total: u64) -> impl FnMut(u64) + Send + 'static {
    let mut last = 0u64;
    move |done| {
        if let Some(p) = &progress {
            if done == total || done >= last + CHUNK as u64 || done < last {
                last = done;
                p.progress(done, total);
            }
        }
    }
}

/// What is wrong stays out of the message: the app names it in plain words.
pub(super) fn zip_error(e: ZipError) -> GhulError {
    match e {
        ZipError::Io(e) => super::error::storage_message(e.to_string()),
        // M10: it began as a GhulSuite file (its first entry was there) and ended early: a file
        // that was not written completely, not another kind of file.
        ZipError::Truncated => GhulError::InvalidInput { msg: INCOMPLETE_FILE.into() },
        _ => GhulError::InvalidInput { msg: NOT_A_SUITE_FILE.into() },
    }
}

/// An error before the first entry of a file was read: an early end says nothing about the
/// file being ours (see [`zip_error`]).
pub(super) fn first_entry_error(e: ZipError) -> GhulError {
    if matches!(e, ZipError::Truncated) {
        not_a_suite_file()
    } else {
        zip_error(e)
    }
}

pub(super) fn not_a_suite_file() -> GhulError {
    GhulError::InvalidInput { msg: NOT_A_SUITE_FILE.into() }
}

/// A GhulDocs package (`.ghldocs`) where a suite file was expected.
pub(super) fn docs_package_not_suite() -> GhulError {
    GhulError::InvalidInput { msg: DOCS_PACKAGE_NOT_SUITE.into() }
}

/// A suite file (`.ghlzip`) where a GhulDocs package was expected.
pub(super) fn suite_not_docs_package() -> GhulError {
    GhulError::InvalidInput { msg: SUITE_NOT_DOCS_PACKAGE.into() }
}

/// The first container of a `.ghlzip`: anything that is no container of this format is "not a
/// GhulSuite file", except a container of an older or newer format version, which says so.
pub(super) fn parse_first(data: &[u8]) -> Result<LockedContainer> {
    LockedContainer::parse(data).map_err(|e| match e {
        crate::vault::VaultError::UnsupportedVersion(_) | crate::vault::VaultError::NewerKind => GhulError::from(e),
        _ => not_a_suite_file(),
    })
}

pub(super) fn suite_zip(
    source: Arc<dyn ArchiveSource>,
    progress: Option<Arc<dyn ArchiveProgress>>,
) -> ZipReader<SourceReader> {
    let total = source.size();
    ZipReader::new(SourceReader::new(source), MAX_ARCHIVE, MAX_ENTRIES, reporter(progress, total))
}

/// Reads the data of the entry just found, a container (at most [`MAX_FILE_LEN`]).
pub(super) fn read_container(z: &mut ZipReader<SourceReader>, e: &EntryInfo) -> Result<(Vec<u8>, String)> {
    if e.size > MAX_FILE_LEN as u64 {
        return Err(not_a_suite_file());
    }
    let mut data = Vec::with_capacity(e.size as usize);
    let sha = z.read_entry(&mut data).map_err(zip_error)?;
    Ok((data, crate::kdf::hex(&sha)))
}

/// The header, the first entry of a `.ghlzip`.
fn read_header(z: &mut ZipReader<SourceReader>) -> Result<LockedContainer> {
    // Not even a first entry: nothing says this is a GhulSuite file.
    let e = z.next_entry().map_err(first_entry_error)?.ok_or_else(not_a_suite_file)?;
    let (data, _) = read_container(z, &e)?;
    let l = parse_first(&data)?;
    if l.kind() == ContainerKind::Docs && e.name.ends_with(".ghldoc") {
        return Err(docs_package_not_suite());
    }
    if e.name != HEADER_FILE || l.kind() != ContainerKind::Export {
        return Err(not_a_suite_file());
    }
    Ok(l)
}

pub(super) fn mismatch(_why: &str) -> GhulError {
    GhulError::Corrupted { msg: "the suite file does not match its list".into() }
}

fn sha256_hex(data: &[u8]) -> String {
    crate::kdf::hex(&Sha256::digest(data))
}

fn random_id() -> String {
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    crate::kdf::hex(&id)
}

/// How the checked containers are opened.
enum Key<'a> {
    Password(&'a PasswordKey, &'a FactorSecret),
    Code(&'a RecoveryCode),
}

fn open_container(l: LockedContainer, key: &Key<'_>) -> Result<UnlockedContainer> {
    Ok(match key {
        Key::Password(pk, s) => l.unlock(pk, s)?,
        Key::Code(code) => l.recover(code)?,
    })
}

/// Reads and checks a whole `.ghlzip` (header from `z` already read as `locked`): the header
/// opens (with the password: its seal holds), the entries are exactly the listed files with
/// their hashes, every container belongs to the same suite, has its kind and id and opens, and
/// the content files are exactly those the indexes of its GhulDocs vaults list, with their
/// hashes. Every rule holds while reading: only plain names of the suite's files or content
/// files, no folders, no entry twice, each checked before the next one is read; a content file
/// only after its vault. Content files go to a staging folder in `root` (the content root of
/// the working copy), the containers stay in memory.
fn read_checked(
    z: &mut ZipReader<SourceReader>,
    locked: LockedContainer,
    key: &Key<'_>,
    root: Option<&std::path::Path>,
) -> Result<(Files, ArchiveHeader, [u8; 16], Option<Staging>)> {
    let header_c = open_container(locked, key)?;
    let header: ArchiveHeader = setting(&header_c, HEADER_KEY);
    let suite_id = header_c.header().suite_id;
    if !header.files.iter().any(|f| f.name == SUITE_FILE) {
        return Err(mismatch("no suite file"));
    }
    // Every container is listed with its kind (checked below): no more GhulDocs vaults than a
    // suite may have (this app never writes such a file).
    if header.files.iter().filter(|f| f.kind == ContainerKind::Docs.name()).count() as u64
        > super::docs::DOCS_SUITE_VAULTS
    {
        return Err(GhulError::InvalidInput { msg: DOCS_VAULTS_MANY.into() });
    }
    let mut out: Files = Vec::new();
    let mut docs: BTreeMap<String, Expected> = BTreeMap::new();
    let mut staging: Option<Staging> = None;
    let mut suite_epoch = None;
    while let Some(e) = z.next_entry().map_err(zip_error)? {
        let name = e.name.clone();
        if let Some((vid, content)) = parse_entry_name(&name) {
            if !docs.contains_key(&vid) {
                return Err(mismatch("a document without its vault"));
            }
            if staging.is_none() {
                let root =
                    root.ok_or_else(|| GhulError::Storage { msg: "documents cannot be stored in this folder".into() })?;
                // H12: room for every document before the first one is written (the indexes of
                // all GhulDocs vaults come before their documents).
                super::error::check_free_space(root, docs.values().map(Expected::bytes).sum())?;
                staging = Some(Staging::new(root)?);
            }
            let expected = docs.get_mut(&vid).ok_or_else(|| mismatch("a document without its vault"))?;
            stage_entry(z, &e, &vid, &content, expected, staging.as_ref().ok_or(GhulError::NotFound)?)?;
            continue;
        }
        if !super::storage::plain_file_name(&name) || !(name == SUITE_FILE || is_vault_file(&name)) {
            return Err(not_a_suite_file());
        }
        if out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            return Err(not_a_suite_file());
        }
        let f = header.files.iter().find(|f| f.name == name).ok_or_else(|| mismatch("a file too many"))?;
        let (data, sha) = read_container(z, &e)?;
        if sha != f.sha256 {
            return Err(mismatch("a file changed"));
        }
        let l = LockedContainer::parse(&data)?;
        if l.kind().name() != f.kind || l.header().suite_id != suite_id {
            return Err(mismatch(&format!("{} belongs elsewhere", f.name)));
        }
        if (f.name == SUITE_FILE) != (l.kind() == ContainerKind::Suite) || l.kind() == ContainerKind::Export {
            return Err(mismatch(&format!("{} has the wrong kind", f.name)));
        }
        let c = open_container(l, key)?;
        if crate::kdf::hex(&c.container_id()) != f.id {
            return Err(mismatch(&format!("{} is another vault", f.name)));
        }
        match c.kind() {
            ContainerKind::Suite => suite_epoch = Some(setting::<u64>(&c, super::suite::EPOCH_KEY)),
            ContainerKind::Docs if docs.insert(f.id.clone(), Expected::of_suite_file(&c)?).is_some() => {
                return Err(mismatch("a vault twice"));
            }
            _ => {}
        }
        out.push((name, data));
    }
    if out.len() != header.files.len() {
        return Err(mismatch("files missing"));
    }
    if !docs.values().all(Expected::complete) {
        return Err(mismatch("documents missing"));
    }
    if suite_epoch != Some(header.epoch) {
        return Err(mismatch("protection"));
    }
    Ok((out, header, suite_id, staging))
}

/// Writes a `.ghlzip` of `files` (in memory, in this order: the header or the vault first) and
/// then the content files `contents`, copied from the disk. Returns size and SHA-256 of the
/// whole file.
pub(super) fn write_zip(
    files: &[(String, Vec<u8>)],
    contents: &[ContentFile],
    sink: Arc<dyn ArchiveSink>,
    progress: Option<Arc<dyn ArchiveProgress>>,
) -> Result<(u64, String)> {
    let total = archive_len(files, contents)?;
    let storage = |e: std::io::Error| GhulError::Storage { msg: e.to_string() };
    let mut w = ZipWriter::new(SinkWriter::new(sink), reporter(progress, total));
    for (n, d) in files {
        w.add_bytes(n, d).map_err(storage)?;
    }
    for c in contents {
        let f = std::fs::File::open(&c.path).map_err(storage)?;
        w.add_reader(&c.name, c.len, c.crc, f).map_err(storage)?;
    }
    let out = w.finish().map_err(storage)?;
    Ok((out.len, crate::kdf::hex(&out.sha.finalize())))
}

/// The size of the `.ghlzip` of `files` and `contents`; more than [`MAX_ARCHIVE`] (the most that
/// is read) is refused before anything is written: such a file could never be opened.
pub(super) fn archive_len(files: &[(String, Vec<u8>)], contents: &[ContentFile]) -> Result<u64> {
    let total = zip_len(
        files
            .iter()
            .map(|(n, d)| (n.as_str(), d.len() as u64))
            .chain(contents.iter().map(|c| (c.name.as_str(), c.len))),
    );
    // As many entries as are read at most (a vault file each, a content file each): a file with
    // more could never be opened.
    if total > limits().0 || files.len() + contents.len() > MAX_ENTRIES {
        return Err(GhulError::InvalidInput { msg: "the suite file would be too large".into() });
    }
    Ok(total)
}

/// The largest suite file written and [`OTHER_LIMIT`] (smaller in tests, so they can be
/// reached without writing a gigabyte).
fn limits() -> (u64, u64) {
    #[cfg(test)]
    if let Some(l) = TEST_LIMITS.with(std::cell::Cell::get) {
        return l;
    }
    (MAX_ARCHIVE, OTHER_LIMIT)
}

#[cfg(test)]
thread_local! {
    /// Tests only: other values for [`limits`] (in this thread).
    pub(crate) static TEST_LIMITS: std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}

/// About the room a container takes in a suite file: its payload plus [`CONTAINER_EXTRA`].
pub(super) fn container_weight(c: &UnlockedContainer) -> Result<u64> {
    Ok(c.payload.to_cbor()?.len() as u64 + CONTAINER_EXTRA)
}

/// Whether `add` more bytes still fit next to the suite and its vaults besides GhulDocs
/// ([`OTHER_LIMIT`]).
pub(super) fn check_other_room(o: &super::suite::Open, add: u64) -> Result<()> {
    let mut have = container_weight(&o.suite)?;
    for c in o.vaults.iter().filter(|v| v.reference.kind != "docs").filter_map(|v| v.container.as_ref()) {
        have += container_weight(c)?;
    }
    if have.saturating_add(add) > limits().1 {
        return Err(GhulError::InvalidInput { msg: "the vaults besides GhulDocs are full".into() });
    }
    Ok(())
}

/// Whether the file at a place is exactly the one written (`size` bytes with this SHA-256,
/// from [`ArchiveBuilt`]): read back after writing, before the working copy counts as saved.
#[uniffi::export]
pub fn archive_written_matches(
    source: Arc<dyn ArchiveSource>,
    size: u64,
    sha256: String,
    progress: Option<Arc<dyn ArchiveProgress>>,
) -> Result<bool> {
    let mut report = reporter(progress, size);
    let mut r = SourceReader::new(source);
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut len = 0u64;
    loop {
        let n = r.read(&mut buf).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        if n == 0 {
            break;
        }
        len += n as u64;
        if len > size {
            return Ok(false);
        }
        sha.update(&buf[..n]);
        report(len);
    }
    Ok(len == size && crate::kdf::hex(&sha.finalize()) == sha256)
}

impl GhulSuite {
    fn seen_all(&self) -> BTreeMap<String, Seen> {
        // The copy before the last write when the file cannot be read (see `write_atomic`): the
        // epoch in it keeps older protections out.
        let read = |name: &str| -> Option<BTreeMap<String, Seen>> {
            std::fs::read(self.local_dir.join(name)).ok().and_then(|d| serde_json::from_slice(&d).ok())
        };
        read(SEEN_FILE).or_else(|| read(&format!("{SEEN_FILE}.bak"))).unwrap_or_default()
    }

    /// This device has `state` now (opened or saved; also an older one or a branch the user
    /// took). The counter only grows: a later save goes above everything seen.
    fn note_archive_seen(&self, suite_id: &[u8; 16], state: &StateRef, epoch: u64, saved: i64) {
        let _one = self.seen_guard();
        let mut all = self.seen_all();
        let e = all.entry(crate::kdf::hex(suite_id)).or_default();
        e.counter = e.counter.max(state.counter);
        e.at = state.counter;
        e.state = state.state.clone();
        e.ancestors = state.ancestors.iter().take(MAX_ANCESTORS).cloned().collect();
        e.complete = state.complete;
        e.epoch = e.epoch.max(epoch);
        e.file = state.file.clone();
        e.saved = saved;
        if let Ok(a) = self.anchor() {
            note_protection(e, protection_print(a.header()));
        }
        self.write_seen(&all);
    }

    /// The protection of the working copy as it is now is about to change: it is kept in mind as
    /// an earlier one (a file saved with it elsewhere later is older, not newer).
    pub(super) fn note_protection_left(&self, suite_id: &[u8; 16]) -> Result<()> {
        let Ok(a) = self.anchor() else { return Ok(()) };
        let _one = self.seen_guard();
        let mut all = self.seen_all();
        let e = all.entry(crate::kdf::hex(suite_id)).or_default();
        note_protection(e, protection_print(a.header()));
        self.try_write_seen(&all)
    }

    /// The protection of a suite changed on this device (its epoch is `epoch` now).
    /// It must be noted: without it, a file with the older protection would open again.
    pub(super) fn note_epoch(&self, suite_id: &[u8; 16], epoch: u64) -> Result<()> {
        let _one = self.seen_guard();
        let mut all = self.seen_all();
        let e = all.entry(crate::kdf::hex(suite_id)).or_default();
        if epoch <= e.epoch {
            return Ok(());
        }
        e.epoch = epoch;
        self.try_write_seen(&all)
    }

    /// Read-modify-write of the device's notes one at a time (M5): two calls (a save noting its
    /// state, a change of the protection noting its epoch) never write over each other.
    fn seen_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.seen_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write_seen(&self, all: &BTreeMap<String, Seen>) {
        let _ = self.try_write_seen(all);
    }

    fn try_write_seen(&self, all: &BTreeMap<String, Seen>) -> Result<()> {
        let json = serde_json::to_vec(all).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        std::fs::create_dir_all(&self.local_dir)?;
        super::storage::write_atomic(&self.local_dir, SEEN_FILE, &json)
    }

    /// The working copy is about to change (called before a container is written, so the mark
    /// is there even if the device goes off right after): it differs from the file it was loaded
    /// from or saved into. The mark holds the time of the first unsaved change.
    pub(super) fn mark_unsaved(&self) -> Result<()> {
        let _one = self.unsaved_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.changes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.unsaved() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.local_dir)?;
        super::storage::write_atomic(&self.local_dir, UNSAVED_FILE, now_ms().to_string().as_bytes())
    }

    fn unsaved(&self) -> bool {
        self.local_dir.join(UNSAVED_FILE).exists()
    }

    /// M6: grows with every change of the content (a vault file written or removed), not with a
    /// write of the suite file alone (settings, the state noted after a save).
    pub(super) fn content_change(&self) -> u64 {
        self.content_changes.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(super) fn note_content_change(&self) {
        self.content_changes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// The change counter of the working copy (grows with every write of a container).
    pub(super) fn change(&self) -> u64 {
        self.changes.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Clears the unsaved mark if nothing changed since `change` was read.
    pub(super) fn clear_unsaved_at(&self, change: u64) {
        let _one = self.unsaved_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.change() == change {
            let _ = std::fs::remove_file(self.local_dir.join(UNSAVED_FILE));
        }
    }

    pub(super) fn clear_unsaved(&self) {
        let _one = self.unsaved_lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::fs::remove_file(self.local_dir.join(UNSAVED_FILE));
    }

    /// The suite id and state of the working copy (read from its suite file without opening it
    /// for the id; the state needs the open suite).
    fn working_suite_id(&self) -> Option<[u8; 16]> {
        let data = self.storage.read(SUITE_FILE.to_owned()).ok()??;
        LockedContainer::parse(&data).ok().map(|l| l.header().suite_id)
    }

    fn relation(&self, header: &ArchiveHeader, suite_id: &[u8; 16]) -> Result<ArchiveRelation> {
        let seen = self.seen_all().get(&crate::kdf::hex(suite_id)).cloned().unwrap_or_default();
        if header.epoch < seen.epoch {
            return Err(GhulError::ProtectionOutdated);
        }
        let working = self.working_suite_id();
        if working.is_none() && seen.state.is_empty() {
            return Ok(ArchiveRelation::NoSuite);
        }
        if working.is_some_and(|w| &w != suite_id) {
            return Ok(ArchiveRelation::OtherSuite);
        }
        if seen.state.is_empty() {
            return Ok(ArchiveRelation::Newer);
        }
        if header.state == seen.state {
            return Ok(ArchiveRelation::Same);
        }
        // By the lineage: carried on from this device's state, or an earlier state of it.
        if header.ancestors.contains(&seen.state) {
            return Ok(ArchiveRelation::Newer);
        }
        if seen.ancestors.contains(&header.state) {
            return Ok(ArchiveRelation::Older);
        }
        let at = seen.at;
        // Neither, and the file's lineage is complete: it was not carried on from this state.
        if header.complete {
            return Ok(if seen.complete || header.counter >= at {
                ArchiveRelation::Branch
            } else {
                ArchiveRelation::Older
            });
        }
        // A lineage that had to be cut off (more than MAX_ANCESTORS saves): by the counter.
        Ok(if header.counter < at {
            ArchiveRelation::Older
        } else if header.counter == at || (header.counter == at + 1 && header.previous != seen.state) {
            ArchiveRelation::Branch
        } else {
            ArchiveRelation::Newer
        })
    }

    fn check_of(&self, header: &ArchiveHeader, suite_id: &[u8; 16], unverified: bool) -> Result<ArchiveCheck> {
        let relation = self.relation(header, suite_id)?;
        Ok(ArchiveCheck {
            relation,
            unsaved_changes: self.unsaved() && relation != ArchiveRelation::NoSuite,
            unverified,
            saved_at: header.time,
            device: header.device.clone(),
            vaults: header.files.iter().filter(|f| f.name != SUITE_FILE).count() as u32,
        })
    }

    pub(super) fn set_pending(&self, p: Option<Pending>) {
        if p.is_none() {
            self.pending_gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = p;
    }

    /// Keeps a checked file unless it was cancelled or locked while it was checked (`gen` from
    /// before the check).
    fn keep_pending(&self, gen: u64, p: Pending) -> Result<()> {
        let mut slot = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if self.pending_gen.load(std::sync::atomic::Ordering::SeqCst) != gen {
            return Err(GhulError::Locked);
        }
        *slot = Some(p);
        Ok(())
    }

    fn pending_gen_now(&self) -> u64 {
        self.pending_gen.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Replaces the working copy with the pending files: files of the old copy that the new one
    /// does not have are removed, the suite file is written last; if writing fails midway, the
    /// old copy is put back. The content files of GhulDocs move as whole folders: the old ones
    /// aside, the checked ones (`staged`) into place, the old ones removed at the end.
    fn replace_working_copy(&self, files: &[(String, Vec<u8>)], staged: Option<&Staging>) -> Result<()> {
        let _one = self.replace_lock.lock().unwrap_or_else(|e| e.into_inner());
        // An earlier replace that could not be finished: its state before first (M1 of round 5);
        // its copies must not be overwritten by this one.
        self.finish_earlier_replace_held()?;
        let mut before = Vec::new();
        for n in self.storage.list()? {
            if n == SUITE_FILE || is_vault_file(&n) {
                if let Some(d) = self.storage.read(n.clone())? {
                    before.push((n, d));
                }
            }
        }
        // First on the disk what was there before (a journal): the device may go off at any point
        // from here on, the next start puts the state before back (`recover_replace`).
        let old_dir = self.local_dir.join(REPLACE_OLD_DIR);
        let _ = std::fs::remove_dir_all(&old_dir);
        std::fs::create_dir_all(&old_dir)?;
        for (n, d) in &before {
            super::storage::write_atomic(&old_dir, n, d)?;
        }
        let mut journal = ReplaceJournal {
            old_files: before.iter().map(|(n, _)| n.clone()).collect(),
            old_docs: self.content_root().ok().map(|r| docs_folders(&r)).unwrap_or_default(),
            aside: None,
        };
        self.write_journal(&journal)?;
        let aside = self.docs_aside(staged, &mut |dir| {
            journal.aside = dir.file_name().map(|n| n.to_string_lossy().into_owned());
            self.write_journal(&journal)
        })?;
        let new: Vec<(String, Vec<u8>)> = files.iter().filter(|(n, _)| n != HEADER_FILE).cloned().collect();
        if let Err(e) = self.put_files(&new, &before) {
            #[cfg(test)]
            if matches!(&e, GhulError::Storage { msg } if msg == CRASHED) {
                // Gone without a word: the journal and the folders stay as they are.
                if let Some(a) = aside {
                    std::mem::forget(a);
                }
                return Err(e);
            }
            // Half replaced is neither state: back to the one before. The journal goes only once
            // that worked (M3); otherwise it stays with the folders, and the next start puts the
            // state before back (`recover_replace`).
            if self.put_files(&before, &new).is_err() || self.rollback_fails() {
                if let Some(a) = aside {
                    std::mem::forget(a);
                }
                return Err(e);
            }
            if let Some(a) = aside {
                a.back()?;
            }
            self.end_journal();
            return Err(e);
        }
        // Complete: the journal goes first, then what it kept (the old documents with `aside`),
        // and every copy of the working copy before (H7: it may have had another protection).
        self.end_journal();
        drop(aside);
        self.purge_old_copies();
        Ok(())
    }

    /// Tests: the rollback of a replace that failed fails as well (see [`ROLLBACK_FAILS`]).
    fn rollback_fails(&self) -> bool {
        #[cfg(test)]
        return ROLLBACK_FAILS.with(|c| c.replace(false));
        #[cfg(not(test))]
        false
    }

    /// A replace of the working copy that could not be finished (its journal is still there, e.g.
    /// its way back failed): the state before is put back now, before anything else is unlocked
    /// or replaced (round 5, M1). If that fails again, the call is refused with
    /// [`REPLACE_UNFINISHED`] and nothing is changed or deleted.
    pub(super) fn finish_earlier_replace(&self) -> Result<()> {
        let _one = self.replace_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.finish_earlier_replace_held()
    }

    fn finish_earlier_replace_held(&self) -> Result<()> {
        if !self.local_dir.join(REPLACE_JOURNAL).exists() {
            return Ok(());
        }
        #[cfg(test)]
        if RECOVER_FAILS.with(std::cell::Cell::get) {
            return Err(GhulError::Storage { msg: REPLACE_UNFINISHED.into() });
        }
        self.recover_replace().map_err(|e| {
            eprintln!("GhulSuite: the working copy before a replace could not be put back: {e}");
            GhulError::Storage { msg: REPLACE_UNFINISHED.into() }
        })
    }

    fn write_journal(&self, journal: &ReplaceJournal) -> Result<()> {
        let json = serde_json::to_vec(journal).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        super::storage::write_atomic(&self.local_dir, REPLACE_JOURNAL, &json)
    }

    /// Before a new suite is created in an empty folder: a journal left there belongs to a suite
    /// that is gone (round 6, M1).
    pub(super) fn discard_journal(&self) {
        let _one = self.replace_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.end_journal();
    }

    fn end_journal(&self) {
        let _ = std::fs::remove_file(self.local_dir.join(REPLACE_JOURNAL));
        let _ = std::fs::remove_file(self.local_dir.join(format!("{REPLACE_JOURNAL}.bak")));
        let _ = std::fs::remove_dir_all(self.local_dir.join(REPLACE_OLD_DIR));
    }

    /// A suite file replaced the working copy when the device went off (the journal is still
    /// there): the working copy before comes back, complete, with its documents. Called when the
    /// suite starts, before anything tidies the folders. A journal that cannot be read stays (and
    /// the folders with it) for a later look; nothing is deleted then.
    pub(super) fn recover_replace(&self) -> Result<()> {
        let path = self.local_dir.join(REPLACE_JOURNAL);
        let Ok(data) = std::fs::read(&path) else { return Ok(()) };
        let journal: ReplaceJournal =
            serde_json::from_slice(&data).map_err(|e| GhulError::Storage { msg: format!("replace journal: {e}") })?;
        // The documents: what was moved aside goes back, what came in goes. The folder they were
        // moved to must be found (by its name, under the content root of now); if it is not, the
        // journal stays and nothing is changed (round 6, M2).
        let aside = match (&journal.aside, self.content_root()) {
            (Some(a), Ok(root)) => {
                let name = std::path::Path::new(a).file_name().map(|n| n.to_owned()).unwrap_or_default();
                let p = super::docs::staging_root(&root).join(name);
                if !p.is_dir() && !journal.old_docs.is_empty() {
                    return Err(GhulError::Storage { msg: "the documents before the replace were not found".into() });
                }
                Some(p)
            }
            _ => None,
        };
        if let Ok(root) = self.content_root() {
            if let Some(aside) = &aside {
                for n in &journal.old_docs {
                    let kept = aside.join(n);
                    if kept.is_dir() {
                        let _ = std::fs::remove_dir_all(root.join(n));
                        std::fs::rename(&kept, root.join(n))?;
                    }
                }
            }
            for n in docs_folders(&root) {
                if !journal.old_docs.contains(&n) {
                    std::fs::remove_dir_all(root.join(&n))?;
                }
            }
        }
        // The files: those before, written again; those that came in, removed.
        let old_dir = self.local_dir.join(REPLACE_OLD_DIR);
        for n in &journal.old_files {
            let d = std::fs::read(old_dir.join(n))?;
            self.storage.write(n.clone(), d)?;
        }
        for n in self.storage.list()? {
            if (n == SUITE_FILE || is_vault_file(&n)) && !journal.old_files.contains(&n) {
                self.storage.delete(n)?;
            }
        }
        if let Some(aside) = aside {
            let _ = std::fs::remove_dir_all(aside);
        }
        self.end_journal();
        Ok(())
    }

    /// Folders of old documents of a replace (marked [`super::docs::JOURNAL_MARK`]) whose
    /// journal is gone: the replace ended (or was put back) and the stop came before the folder
    /// was removed (round 5, M2). Only this suite knows where its journal is, so it tidies them,
    /// not `tidy_staging`.
    pub(super) fn tidy_asides(&self) {
        if self.local_dir.join(REPLACE_JOURNAL).exists() {
            return;
        }
        let Ok(root) = self.content_root() else { return };
        for e in std::fs::read_dir(super::docs::staging_root(&root)).into_iter().flatten().flatten() {
            if e.path().join(super::docs::JOURNAL_MARK).exists() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    /// Moves the content folders of the working copy aside and the staged ones into place
    /// (`None` without a content root and nothing to stage).
    /// `noted`: the folder the old ones go to, told before anything is moved (the journal).
    fn docs_aside(
        &self,
        staged: Option<&Staging>,
        noted: &mut dyn FnMut(&std::path::Path) -> Result<()>,
    ) -> Result<Option<Aside>> {
        let root = match self.content_root() {
            Ok(r) => r,
            Err(_) if staged.is_none() => return Ok(None),
            Err(e) => return Err(e),
        };
        // Every checked document must still be there before anything is moved.
        if let Some(s) = staged {
            s.verify()?;
        }
        let old = Staging::new(&root)?;
        // Not tidied away while the journal still needs it; once the journal is gone (also after
        // a stop between the end of the journal and the removal of this folder), it is.
        std::fs::write(old.path().join(super::docs::JOURNAL_MARK), REPLACE_JOURNAL.as_bytes())?;
        noted(old.path())?;
        let mut aside = Aside { root: root.clone(), old, moved_in: Vec::new(), moved_out: Vec::new() };
        let result = (|| {
            for d in std::fs::read_dir(&root).into_iter().flatten().flatten() {
                let name = d.file_name().to_string_lossy().into_owned();
                if name.starts_with(super::docs::DIR_PREFIX) && d.file_type().is_ok_and(|t| t.is_dir()) {
                    std::fs::rename(d.path(), aside.old.path().join(&name))?;
                    aside.moved_out.push(name);
                }
            }
            if let Some(s) = staged {
                for name in s.vaults() {
                    std::fs::rename(s.path().join(&name), root.join(&name))?;
                    aside.moved_in.push(name);
                }
            }
            Ok::<(), std::io::Error>(())
        })();
        if let Err(e) = result {
            aside.back()?;
            return Err(e.into());
        }
        Ok(Some(aside))
    }

    /// Writes `files` (the suite file last) and removes what only `old` had.
    fn put_files(&self, files: &[(String, Vec<u8>)], old: &[(String, Vec<u8>)]) -> Result<()> {
        #[cfg(test)]
        let crash = || {
            CRASH_AFTER_WRITES.with(|c| match c.get() {
                Some(0) => {
                    c.set(None);
                    Err(GhulError::Storage { msg: CRASHED.into() })
                }
                Some(n) => {
                    c.set(Some(n - 1));
                    Ok(())
                }
                None => Ok(()),
            })
        };
        #[cfg(not(test))]
        let crash = || Ok::<(), GhulError>(());
        for (n, d) in files.iter().filter(|(n, _)| n != SUITE_FILE) {
            crash()?;
            self.storage.write(n.clone(), d.clone())?;
        }
        if let Some((_, d)) = files.iter().find(|(n, _)| n == SUITE_FILE) {
            crash()?;
            self.storage.write(SUITE_FILE.to_owned(), d.clone())?;
        }
        for (n, _) in old {
            if !files.iter().any(|(m, _)| m == n) {
                self.storage.delete(n.clone())?;
            }
        }
        Ok(())
    }

    /// What belongs to this device and the state it had: quick unlock (it holds the old password
    /// key).
    fn wipe_device_data(&self) {
        super::quick::delete_all(&self.local_dir);
    }
}

#[uniffi::export]
impl GhulSuite {
    /// What a `.ghlzip` needs to be opened (password plus key or key file). Reads only its
    /// header; nothing is opened.
    pub fn archive_protection(&self, source: Arc<dyn ArchiveSource>) -> Result<ProtectionInfo> {
        Ok(super::suite::protection_of(&read_header(&mut suite_zip(source, None))?))
    }

    /// Opens a `.ghlzip` with the password plus key or key file and checks all of it (see the
    /// module). Nothing is replaced yet: the result says how it relates to this device, then
    /// `archive_accept` takes it. A file with an older protection than this device saw is
    /// refused (`ProtectionOutdated`), whatever else it holds. The file is read once, as a
    /// stream; `progress` gets the bytes read.
    #[allow(clippy::too_many_arguments)]
    pub fn archive_open(
        &self,
        source: Arc<dyn ArchiveSource>,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<ArchiveCheck> {
        let gen = self.pending_gen_now();
        let change = self.content_change();
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let mut z = suite_zip(source, progress);
        let locked = read_header(&mut z)?;
        let file_id = locked.header().container_id;
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let root = self.content_root().ok();
        let (files, header, suite_id, staged) = read_checked(&mut z, locked, &Key::Password(&pk, &s), root.as_deref())?;
        let check = self.check_of(&header, &suite_id, false)?;
        self.keep_pending(
            gen,
            Pending { files, header, suite_id, recovery: None, keys: Some((pk, s)), staged, file_id, change },
        )?;
        Ok(check)
    }

    /// Emergency: opens a `.ghlzip` with the recovery code alone. The seals cannot be checked
    /// without the password (`unverified`); everything else is. On accepting, everything is
    /// written again with `new_password`.
    pub fn archive_open_with_code(
        &self,
        source: Arc<dyn ArchiveSource>,
        code: String,
        new_password: String,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<ArchiveCheck> {
        let gen = self.pending_gen_now();
        let change = self.content_change();
        let code = Zeroizing::new(code);
        let parsed = RecoveryCode::parse(&code)?;
        // The new password follows the rules before anything is read or replaced.
        super::tools::check_master_password(&new_password)?;
        let mut z = suite_zip(source, progress);
        let locked = read_header(&mut z)?;
        let file_id = locked.header().container_id;
        let root = self.content_root().ok();
        let (files, header, suite_id, staged) = read_checked(&mut z, locked, &Key::Code(&parsed), root.as_deref())?;
        let check = self.check_of(&header, &suite_id, true)?;
        self.keep_pending(
            gen,
            Pending {
                files,
                header,
                suite_id,
                recovery: Some((code, Zeroizing::new(new_password))),
                keys: None,
                staged,
                file_id,
                change,
            },
        )?;
        Ok(check)
    }

    /// Forgets a checked file.
    pub fn archive_cancel(&self) {
        self.set_pending(None);
    }

    /// Takes the checked file: the working copy is replaced (the old one stays one step back),
    /// this device's data of the old state goes, and the suite is open. A file of another suite
    /// than the one on this device (`ArchiveRelation::OtherSuite`) replaces that suite: only with
    /// its proof, `archive_accept_replacing` (H5); here it is refused with
    /// [`OTHER_SUITE_NEEDS_PROOF`]. Refused while documents are added or copied
    /// ([`DOCS_BUSY`]) and when the working copy changed since the file was checked
    /// ([`CHANGED_SINCE_CHECK`], M6): the checked file stays for another try. A lock during the
    /// call wins (H10): the files are replaced, but the suite stays locked (`Locked`).
    pub fn archive_accept(&self) -> Result<()> {
        self.accept_pending(None)
    }

    /// [`Self::archive_accept`] for a file of another suite (`OtherSuite`): it deletes the suite
    /// on this device, so both factors of that suite are needed (`proof`: its master password
    /// plus a key file or the PIN of a security key touched through `transport`), checked
    /// against its files without opening it.
    pub fn archive_accept_replacing(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
    ) -> Result<()> {
        self.accept_pending(Some(ExistingProof::Factors(&proof, transport.as_ref(), listener.as_ref())))
    }

    /// [`Self::archive_accept_replacing`] with the recovery code of the suite on this device.
    pub fn archive_accept_replacing_with_code(&self, code: String) -> Result<()> {
        let code = Zeroizing::new(code);
        self.accept_pending(Some(ExistingProof::Code(&code)))
    }

    /// Deletes the suite on this device for good (to set up a new one in its place): it is
    /// locked, then this device's data of it goes (quick unlock, what was seen of its suite
    /// files, the mark of unsaved changes), the content folders of GhulDocs (staging and rescued
    /// folders included), every vault file and the suite file last. Suite files (`.ghlzip`)
    /// elsewhere are not touched. If a file cannot be deleted, the error says so and what is
    /// left can be deleted again.
    ///
    /// H5: only with the proof of that suite, also while it is locked: both factors (`proof`:
    /// its master password plus a key file or the PIN of a security key touched through
    /// `transport`), checked against its files without opening it, or its recovery code
    /// (`delete_working_copy_with_code`). A working copy without one readable container (nothing
    /// there that could be opened) needs none.
    pub fn delete_working_copy(
        &self,
        proof: FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
    ) -> Result<()> {
        self.prove_existing(ExistingProof::Factors(&proof, transport.as_ref(), listener.as_ref()))?;
        self.delete_proven()
    }

    /// [`Self::delete_working_copy`] with the recovery code of the suite.
    pub fn delete_working_copy_with_code(&self, code: String) -> Result<()> {
        let code = Zeroizing::new(code);
        self.prove_existing(ExistingProof::Code(&code))?;
        self.delete_proven()
    }

    /// Whether the last `archive_accept` removed this device's data of the old state (quick
    /// unlock among it), also when it failed afterwards: the app's part of it goes as well.
    pub fn archive_device_data_wiped(&self) -> bool {
        self.wiped.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the working copy has changes that are not in a file yet.
    pub fn archive_unsaved(&self) -> bool {
        self.unsaved()
    }

    /// When the first change not in a file yet was made (Unix ms), 0 if nothing is unsaved.
    /// An older mark holds no time: then the time the mark was written.
    pub fn archive_unsaved_since(&self) -> i64 {
        let path = self.local_dir.join(UNSAVED_FILE);
        let Ok(data) = std::fs::read(&path) else { return 0 };
        if let Some(t) = std::str::from_utf8(&data).ok().and_then(|s| s.trim().parse::<i64>().ok()).filter(|t| *t > 1) {
            return t;
        }
        std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(1, |d| d.as_millis() as i64)
    }

    /// When the state of the working copy was last saved into a suite file or came from one
    /// (Unix ms), 0 if never or not known. Also while locked.
    pub fn archive_saved_at(&self) -> i64 {
        let Some(id) = self.working_suite_id() else { return 0 };
        self.seen_all().get(&crate::kdf::hex(&id)).map_or(0, |s| s.saved)
    }

    /// Compares the file at the place the suite is saved to (reading only its header; an empty
    /// file counts as none) with the working copy, before overwriting it.
    pub fn archive_target(&self, source: Arc<dyn ArchiveSource>) -> Result<ArchiveTarget> {
        Ok(self.archive_target_info(source)?.target)
    }

    /// [`Self::archive_target`] with the device and time of the file there, when its header can
    /// be read (shown before overwriting a file saved elsewhere).
    pub fn archive_target_info(&self, source: Arc<dyn ArchiveSource>) -> Result<ArchiveTargetInfo> {
        let bare = |target| ArchiveTargetInfo { target, device: String::new(), time: 0 };
        let mut z = suite_zip(source, None);
        let locked = match read_header(&mut z) {
            Ok(l) => l,
            Err(e @ GhulError::Storage { .. }) => return Err(e),
            Err(_) if z.position() == 0 => return Ok(bare(ArchiveTarget::New)),
            Err(_) => return Ok(bare(ArchiveTarget::OtherSuite)),
        };
        self.with_open(|o| {
            if locked.header().suite_id != o.suite.header().suite_id {
                return Ok(bare(ArchiveTarget::OtherSuite));
            }
            let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let file_id = crate::kdf::hex(&locked.header().container_id);
            let print = protection_print(locked.header());
            let Ok(c) = locked.unlock(&o.password_key, &s) else {
                // Not readable with the keys of now: the file this device saved or opened last,
                // from before a change of the protection, is its own (overwriting it is what
                // the change asks for). Another file with a protection this device had before
                // is older: saved elsewhere meanwhile, but overwriting undoes no change. Any
                // other protection is newer: it was changed elsewhere.
                let seen = self.seen_all().remove(&crate::kdf::hex(&o.suite.header().suite_id));
                let own = seen.as_ref().is_some_and(|s| !s.file.is_empty() && s.file == file_id);
                let older = seen.is_some_and(|s| s.protections.contains(&print));
                return Ok(bare(if own {
                    ArchiveTarget::Same
                } else if older {
                    ArchiveTarget::ChangedElsewhere
                } else {
                    ArchiveTarget::OtherProtection
                }));
            };
            let header: ArchiveHeader = setting(&c, HEADER_KEY);
            let mine: StateRef = setting(&o.suite, STATE_KEY);
            // The file holds a state of this working copy's own line (also after a build that was
            // never written): overwriting it loses nothing.
            let ours = !header.state.is_empty()
                && (header.state == mine.state
                    || header.state == mine.previous
                    || mine.ancestors.contains(&header.state));
            Ok(ArchiveTargetInfo {
                target: if ours { ArchiveTarget::Same } else { ArchiveTarget::ChangedElsewhere },
                device: header.device,
                time: header.time,
            })
        })
    }

    /// Builds the `.ghlzip` of the working copy into `sink`: every container sealed again, a
    /// new state. Once the file is in place (and read back with [`archive_written_matches`]),
    /// confirm with `archive_saved`. `device`: shown when the file is opened elsewhere.
    /// `progress` gets the bytes written. The open suite is held only while sealing, not while
    /// the file is written.
    pub fn archive_build(
        &self,
        device: String,
        sink: Arc<dyn ArchiveSink>,
        progress: Option<Arc<dyn ArchiveProgress>>,
    ) -> Result<ArchiveBuilt> {
        // Content files deleted while the file is written stay until it is written.
        let _copy = self.content_copy();
        let (files, contents, state, change) = self.with_open(|o| {
            let suite_id = o.suite.header().suite_id;
            let seen = self.seen_all().get(&crate::kdf::hex(&suite_id)).cloned().unwrap_or_default();
            let mine: StateRef = setting(&o.suite, STATE_KEY);
            super::single::refuse_single(o)?;
            // A vault that cannot be read would be missing from the file, and gone wherever it
            // is opened: not without the user knowing.
            if o.vaults.iter().any(|v| v.container.is_none()) {
                return Err(GhulError::InvalidInput { msg: "a vault of the suite cannot be read".into() });
            }
            let state = random_id();
            let counter = seen.counter.max(mine.counter) + 1;
            let previous = mine.state.clone();
            let mut ancestors: Vec<String> = Vec::new();
            if !mine.state.is_empty() {
                ancestors.push(mine.state.clone());
                ancestors.extend(mine.ancestors.iter().cloned());
            }
            // Complete from the first save on, as long as nothing had to be cut off.
            let complete = mine.state.is_empty() || (mine.complete && ancestors.len() <= MAX_ANCESTORS);
            ancestors.truncate(MAX_ANCESTORS);
            put_setting(
                &mut o.suite,
                STATE_KEY,
                &StateRef {
                    state: state.clone(),
                    counter,
                    previous,
                    ancestors: ancestors.clone(),
                    complete,
                    file: String::new(),
                },
            )?;
            let mut files: Vec<(String, Vec<u8>)> = Vec::new();
            for v in &mut o.vaults {
                if let Some(c) = v.container.as_mut() {
                    files.push((v.reference.file.clone(), c.seal(&mut OsRng)?));
                }
            }
            files.push((SUITE_FILE.to_owned(), o.suite.seal(&mut OsRng)?));
            let mut listed = Vec::new();
            for (n, d) in &files {
                let l = LockedContainer::parse(d)?;
                let c_id = if n == SUITE_FILE {
                    o.suite.container_id()
                } else {
                    o.vaults
                        .iter()
                        .find(|v| &v.reference.file == n)
                        .and_then(|v| v.container.as_ref())
                        .map(|c| c.container_id())
                        .ok_or(GhulError::NotFound)?
                };
                listed.push(ArchiveFile {
                    name: n.clone(),
                    kind: l.kind().name().to_owned(),
                    id: crate::kdf::hex(&c_id),
                    sha256: sha256_hex(d),
                });
            }
            let header = ArchiveHeader {
                files: listed,
                state: state.clone(),
                previous: mine.state.clone(),
                counter,
                epoch: o.epoch(),
                time: now_ms(),
                device: device.trim().chars().take(80).collect(),
                ancestors,
                complete,
            };
            // Every document goes along, also those in the trash: each must be there.
            let mut contents: Vec<ContentFile> = Vec::new();
            for v in o.vaults.iter().filter(|v| v.reference.kind == "docs") {
                if let Some(c) = &v.container {
                    let id = &v.reference.id;
                    contents.extend(content_files(&self.docs_dir(id)?, id, &c.payload.entries)?);
                }
            }
            let mut hc = o.new_container(ContainerKind::Export)?;
            put_setting(&mut hc, HEADER_KEY, &header)?;
            // Which file this state goes into (kept here only; noted when it is saved).
            let mut written: StateRef = setting(&o.suite, STATE_KEY);
            written.file = crate::kdf::hex(&hc.container_id());
            put_setting(&mut o.suite, STATE_KEY, &written)?;
            let header_file = (HEADER_FILE.to_owned(), hc.seal(&mut OsRng)?);
            // A file that could never be opened is not written (and the working copy keeps its
            // state).
            let mut all = vec![header_file];
            all.extend(files);
            if let Err(e) = archive_len(&all, &contents) {
                put_setting(&mut o.suite, STATE_KEY, &mine)?;
                return Err(e);
            }
            // The working copy is this state as well (sealed the same way).
            for (n, d) in &all[1..] {
                self.storage.write(n.clone(), d.clone())?;
            }
            *self.built.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(BuiltState { suite_id: o.suite.header().suite_id, state: written, epoch: o.epoch() });
            // The header first: opening knows every hash before the files come.
            Ok((all, contents, state, self.change()))
        })?;
        let (size, sha256) = write_zip(&files, &contents, sink, progress)?;
        Ok(ArchiveBuilt { state, change, size, sha256 })
    }

    /// H12: about how large the suite file `archive_build` writes now will be, in bytes (never
    /// less than it will be: every container padded as it is sealed, plus room for its header,
    /// and every content file). The old file stays at its place until the new one is complete,
    /// so the place needs this much free next to it; the app checks that before writing.
    pub fn archive_estimated_size(&self) -> Result<u64> {
        self.with_open(|o| {
            let sealed = |c: &UnlockedContainer| -> Result<u64> {
                Ok(crate::vault::payload::padded_len(c.payload.to_cbor()?.len()) as u64 + CONTAINER_EXTRA)
            };
            let mut entries: Vec<(String, u64)> =
                vec![(HEADER_FILE.to_owned(), 2 * CONTAINER_EXTRA), (SUITE_FILE.to_owned(), sealed(&o.suite)?)];
            for v in &o.vaults {
                let Some(c) = &v.container else { continue };
                entries.push((v.reference.file.clone(), sealed(c)?));
                if v.reference.kind == "docs" {
                    let id = &v.reference.id;
                    for f in content_files(&self.docs_dir(id)?, id, &c.payload.entries)? {
                        entries.push((f.name, f.len));
                    }
                }
            }
            Ok(zip_len(entries.iter().map(|(n, l)| (n.as_str(), *l))))
        })
    }

    /// The built file was written: this device has this state now. Nothing is unsaved unless
    /// the working copy changed after it was built (`change` from [`ArchiveBuilt`]).
    /// Also after the suite was locked while the file was written (the file is whole then, and
    /// this device's own).
    pub fn archive_saved(&self, state: String, change: u64) -> Result<()> {
        let built = {
            let mut b = self.built.lock().unwrap_or_else(|e| e.into_inner());
            if b.as_ref().is_some_and(|b| b.state.state == state) {
                b.take()
            } else {
                None
            }
        };
        match built {
            Some(b) => self.note_archive_seen(&b.suite_id, &b.state, b.epoch, now_ms()),
            None => self.with_open(|o| {
                let mine: StateRef = setting(&o.suite, STATE_KEY);
                if mine.state != state {
                    return Err(GhulError::InvalidInput { msg: "another state was built meanwhile".into() });
                }
                self.note_archive_seen(&o.suite.header().suite_id, &mine, o.epoch(), now_ms());
                Ok(())
            })?,
        }
        self.clear_unsaved_at(change);
        Ok(())
    }
}

impl GhulSuite {
    /// Deletes the working copy (see `delete_working_copy`), its proof given.
    /// An error after the deletion began is [`GhulError::DeleteIncomplete`]: part of the suite
    /// (at least this device's data of it) is gone; deleting again removes the rest. Any other
    /// error (the proof) means nothing was deleted.
    fn delete_proven(&self) -> Result<()> {
        self.delete_now().map_err(|e| GhulError::DeleteIncomplete { msg: e.to_string() })
    }

    fn delete_now(&self) -> Result<()> {
        let suite_id = self.working_suite_id();
        // No replace runs meanwhile; one that could not be finished goes with the suite (round
        // 6, M1): its journal would put the deleted suite back over a new one.
        let _one = self.replace_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.end_journal();
        self.lock();
        self.finish_lock();
        *self.built.lock().unwrap_or_else(|e| e.into_inner()) = None;
        super::quick::delete_all(&self.local_dir);
        for e in std::fs::read_dir(&self.local_dir).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("quick-") && n.ends_with(".stamp") {
                let _ = std::fs::remove_file(e.path());
            }
        }
        if let Some(id) = suite_id {
            let _one = self.seen_guard();
            let mut all = self.seen_all();
            if all.remove(&crate::kdf::hex(&id)).is_some() {
                self.write_seen(&all);
            }
        }
        self.clear_unsaved();
        if let Ok(root) = self.content_root() {
            remove_all_contents(&root)?;
        }
        #[cfg(test)]
        if DELETE_FAILS.with(std::cell::Cell::get) {
            return Err(GhulError::Storage { msg: String::from("in use (test)") });
        }
        let names = self.storage.list()?;
        for n in names.iter().filter(|n| *n != SUITE_FILE && (is_vault_file(n) || *n == HEADER_FILE)) {
            self.storage.delete(n.clone())?;
        }
        self.storage.delete(SUITE_FILE.to_owned())?;
        // Temporary files of an earlier write that did not end.
        if let Ok(root) = self.content_root() {
            for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                let stem = n.trim_start_matches('.').trim_end_matches(".tmp").trim_end_matches(".bak");
                let leftover = n.ends_with(".tmp") || n.ends_with(".bak");
                if leftover && (stem == SUITE_FILE || is_vault_file(stem)) && e.file_type().is_ok_and(|t| t.is_file()) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Ok(())
    }

    /// H5: the proof of the suite on this device, checked against its files (the suite file, or
    /// any vault file) without opening it. Without one readable container there is nothing a
    /// proof could be checked against, and nothing that could be opened: no proof is needed.
    fn prove_existing(&self, proof: ExistingProof<'_>) -> Result<()> {
        let locked = match self.anchor_with(true) {
            Ok(l) => l,
            Err(GhulError::NoVault) => return Ok(()),
            Err(e) => return Err(e),
        };
        match proof {
            ExistingProof::Factors(p, transport, listener) => {
                let password = Zeroizing::new(p.password.clone());
                let pin = p.pin.clone().map(Zeroizing::new);
                let keyfile = parse_keyfile(p.keyfile.clone())?;
                let s =
                    second_factor(&locked, transport, listener, pin.as_deref().map(|p| p.as_str()), keyfile.as_ref())?;
                let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
                locked.unlock(&pk, &s)?;
            }
            ExistingProof::Code(code) => {
                let code = RecoveryCode::parse(code)?;
                locked.recover(&code)?;
            }
        }
        Ok(())
    }

    /// Documents are being added or content files copied right now (M6).
    fn docs_busy(&self) -> bool {
        self.content_copies_running() || self.docs_adds.load(std::sync::atomic::Ordering::SeqCst) > 0
    }

    /// See `archive_accept`; `existing`: the proof of the suite on this device, for a file of
    /// another suite.
    fn accept_pending(&self, existing: Option<ExistingProof<'_>>) -> Result<()> {
        // A lock from now on wins (H10).
        let gen = self.lock_gen_now();
        self.wiped.store(false, std::sync::atomic::Ordering::SeqCst);
        if self.docs_busy() {
            return Err(GhulError::InvalidInput { msg: DOCS_BUSY.into() });
        }
        self.finish_earlier_replace()?;
        let p = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or(GhulError::NotFound)?;
        // Refused before anything changes: the checked file stays for another try.
        let keep = |p: Pending, e: GhulError| -> Result<()> {
            let mut slot = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if self.lock_gen_now() == gen && slot.is_none() {
                *slot = Some(p);
            }
            Err(e)
        };
        // Checked again right before replacing: another file may have been opened meanwhile.
        let relation = self.relation(&p.header, &p.suite_id)?;
        // What the user was told about unsaved changes is still what would be lost (M6).
        // The checked file is dropped: the app opens it again and asks anew (round 5).
        if relation != ArchiveRelation::NoSuite && self.content_change() != p.change {
            drop(p);
            return Err(GhulError::InvalidInput { msg: CHANGED_SINCE_CHECK.into() });
        }
        if relation == ArchiveRelation::OtherSuite {
            let Some(proof) = existing else {
                return keep(p, GhulError::InvalidInput { msg: OTHER_SUITE_NEEDS_PROOF.into() });
            };
            if let Err(e) = self.prove_existing(proof) {
                return keep(p, e);
            }
        }
        // The checked documents are all still there (nothing is locked or removed otherwise).
        if let Some(Err(e)) = p.staged.as_ref().map(Staging::verify) {
            return keep(p, e);
        }
        self.lock_now();
        self.finish_lock();
        // Quick unlock holds the password key and the factor secret of the working copy. They stay
        // the same for a newer state of the same suite with the same protection: the same KDF
        // (salt) and the same slots, compared in the headers themselves (two devices can each
        // reach the same epoch with different changes). Anything else, or the recovery code:
        // they go.
        let same_header = || -> bool {
            let Ok(old) = self.anchor() else { return false };
            let Some((_, data)) = p.files.iter().find(|(n, _)| n == SUITE_FILE) else { return false };
            let Ok(new) = LockedContainer::parse(data) else { return false };
            protection_print(old.header()) == protection_print(new.header())
        };
        let same_protection = p.recovery.is_none()
            && self.working_suite_id() == Some(p.suite_id)
            && self.seen_all().get(&crate::kdf::hex(&p.suite_id)).is_some_and(|s| s.epoch == p.header.epoch)
            && same_header();
        if !same_protection {
            self.wipe_device_data();
            self.wiped.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        *self.built.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.replace_working_copy(&p.files, p.staged.as_ref())?;
        let mut open = match (&p.recovery, p.keys) {
            (Some((code, new_password)), _) => self.open_with_code(code.to_string(), new_password.to_string())?,
            (None, Some((pk, s))) => {
                let data = &p.files.iter().find(|(n, _)| n == SUITE_FILE).ok_or(GhulError::NoVault)?.1;
                let suite = LockedContainer::parse(data)?.unlock(&pk, &s)?;
                self.open_folder(pk, suite, true)?
            }
            _ => return Err(GhulError::NotFound),
        };
        let accepted = StateRef {
            state: p.header.state.clone(),
            counter: p.header.counter,
            previous: String::new(),
            ancestors: p.header.ancestors.clone(),
            complete: p.header.complete,
            file: crate::kdf::hex(&p.file_id),
        };
        // Noted on the open suite before it is kept: also if a lock came meanwhile, the working
        // copy knows the state it is.
        put_setting(&mut open.suite, STATE_KEY, &accepted)?;
        let epoch = open.epoch();
        self.save_container(&mut open.suite, SUITE_FILE)?;
        // Nothing of the working copy before stays next to it (H7; never fails the accept).
        self.purge_old_copies();
        self.note_archive_seen(&p.suite_id, &accepted, epoch.max(p.header.epoch), p.header.time);
        // After the recovery code everything was written anew: that is a change to save.
        if p.recovery.is_none() {
            self.clear_unsaved();
        }
        self.set_open(open, gen)
    }
}

/// The proof of the suite on this device before it is deleted or replaced (H5).
pub(super) enum ExistingProof<'a> {
    /// Both factors: password plus key file or security key (PIN, touched through the transport).
    Factors(&'a FactorProof, Option<&'a Arc<dyn HidTransport>>, Option<&'a Arc<dyn FidoListener>>),
    /// The recovery code.
    Code(&'a str),
}

/// `archive_accept` while documents are added or content files copied (translated by the app).
pub const DOCS_BUSY: &str = "documents are being added or copied, try again when that is done";
/// `archive_accept` after the working copy changed since the file was checked (M6). The checked
/// file is forgotten: open it again (the app then asks about unsaved changes anew).
pub const CHANGED_SINCE_CHECK: &str = "the working copy changed since the file was checked";
/// `archive_accept` for a file of another suite: `archive_accept_replacing` with the proof of the
/// suite on this device is needed (H5).
pub const OTHER_SUITE_NEEDS_PROOF: &str = "replacing the suite on this device needs its proof";

/// The content folders of the working copy moved aside while a suite file replaces it: put back
/// if replacing fails, removed with the staging folder when it worked.
struct Aside {
    root: std::path::PathBuf,
    old: Staging,
    moved_in: Vec<String>,
    moved_out: Vec<String>,
}

impl Aside {
    /// Back to the content folders before. If one of them cannot be put back, none of them is
    /// removed: the folder they were moved to is kept (renamed to `rescued-documents-<id>` in the
    /// content root, never tidied) and the error says so.
    fn back(self) -> Result<()> {
        let Aside { root, old, moved_in, moved_out } = self;
        for name in &moved_in {
            let _ = std::fs::remove_dir_all(root.join(name));
        }
        let mut all_back = true;
        for name in &moved_out {
            if std::fs::rename(old.path().join(name), root.join(name)).is_err() {
                all_back = false;
            }
        }
        if all_back {
            return Ok(());
        }
        old.keep(&root, &format!("rescued-documents-{}", random_id()));
        Err(GhulError::Storage {
            msg: "the documents before could not be put back, they were kept in a folder of their own".into(),
        })
    }
}

/// Suite files in memory, for tests.
#[cfg(test)]
pub(crate) mod in_memory {
    use super::*;
    use std::sync::Mutex;

    /// Reads from bytes, in pieces of the asked size.
    pub(crate) struct BytesSource(Mutex<(Vec<u8>, usize)>);

    pub(crate) fn source(data: Vec<u8>) -> Arc<dyn ArchiveSource> {
        Arc::new(BytesSource(Mutex::new((data, 0))))
    }

    impl ArchiveSource for BytesSource {
        fn size(&self) -> u64 {
            self.0.lock().unwrap().0.len() as u64
        }
        fn read(&self, max: u32) -> std::result::Result<Vec<u8>, StorageError> {
            let mut g = self.0.lock().unwrap();
            let (data, pos) = &mut *g;
            let n = (max as usize).min(data.len() - *pos);
            let piece = data[*pos..*pos + n].to_vec();
            *pos += n;
            Ok(piece)
        }
    }

    /// Collects what is written.
    #[derive(Default)]
    pub(crate) struct BytesSink(pub Mutex<Vec<u8>>);

    impl ArchiveSink for BytesSink {
        fn write(&self, data: Vec<u8>) -> std::result::Result<(), StorageError> {
            self.0.lock().unwrap().extend_from_slice(&data);
            Ok(())
        }
    }

    /// A built file with its bytes.
    pub(crate) struct Built {
        pub data: Vec<u8>,
        pub state: String,
        pub change: u64,
    }

    impl GhulSuite {
        pub(crate) fn archive_build_bytes(&self, device: String) -> Result<Built> {
            let sink = Arc::new(BytesSink::default());
            let b = self.archive_build(device, sink.clone(), None)?;
            let data = std::mem::take(&mut *sink.0.lock().unwrap());
            assert_eq!(data.len() as u64, b.size);
            assert!(archive_written_matches(source(data.clone()), b.size, b.sha256.clone(), None).unwrap());
            Ok(Built { data, state: b.state, change: b.change })
        }

        pub(crate) fn archive_protection_bytes(&self, data: Vec<u8>) -> Result<ProtectionInfo> {
            self.archive_protection(source(data))
        }

        pub(crate) fn archive_open_bytes(
            &self,
            data: Vec<u8>,
            password: String,
            transport: Option<Arc<dyn HidTransport>>,
            listener: Option<Arc<dyn FidoListener>>,
            pin: Option<String>,
            keyfile: Option<Vec<u8>>,
        ) -> Result<ArchiveCheck> {
            self.archive_open(source(data), password, transport, listener, pin, keyfile, None)
        }

        pub(crate) fn archive_open_with_code_bytes(
            &self,
            data: Vec<u8>,
            code: String,
            new_password: String,
        ) -> Result<ArchiveCheck> {
            self.archive_open_with_code(source(data), code, new_password, None)
        }

        pub(crate) fn archive_target_bytes(&self, data: Vec<u8>) -> Result<ArchiveTarget> {
            self.archive_target(source(data))
        }

        pub(crate) fn archive_target_info_bytes(&self, data: Vec<u8>) -> Result<ArchiveTargetInfo> {
            self.archive_target_info(source(data))
        }
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;

    fn dummy() -> Pending {
        Pending {
            files: Vec::new(),
            header: ArchiveHeader::default(),
            suite_id: [0; 16],
            recovery: None,
            keys: None,
            staged: None,
            file_id: [0; 16],
            change: 0,
        }
    }

    fn temp_suite(name: &str) -> (std::path::PathBuf, Arc<GhulSuite>) {
        let root =
            std::env::temp_dir().join(format!("ghul-{name}-{}", crate::kdf::hex(&super::super::suite::new_id())));
        let s = GhulSuite::new(
            super::super::storage::local_storage(root.join("vault").to_string_lossy().into_owned()),
            root.join("local").to_string_lossy().into_owned(),
        );
        (root, s)
    }

    /// H13: a lineage that had to be cut off (`complete` false, more than MAX_ANCESTORS saves):
    /// the counters decide, a file carried on from this device's state by one save is newer.
    #[test]
    fn a_cut_off_lineage_goes_by_the_counter() {
        let (root, s) = temp_suite("cut-lineage");
        let id = [7u8; 16];
        let seen = Seen {
            state: "mine".into(),
            counter: 10,
            at: 10,
            ancestors: vec!["older".into()],
            complete: false,
            ..Default::default()
        };
        let mut all = BTreeMap::new();
        all.insert(crate::kdf::hex(&id), seen);
        s.try_write_seen(&all).unwrap();
        let header = |counter: u64, previous: &str, complete: bool| ArchiveHeader {
            state: format!("s{counter}{previous}"),
            previous: previous.into(),
            counter,
            complete,
            ancestors: vec!["x".into()],
            ..Default::default()
        };
        let rel = |h: ArchiveHeader| s.relation(&h, &id).unwrap();
        assert_eq!(rel(header(9, "", false)), ArchiveRelation::Older);
        assert_eq!(rel(header(10, "", false)), ArchiveRelation::Branch, "the same counter, another state");
        assert_eq!(rel(header(11, "mine", false)), ArchiveRelation::Newer, "carried on from this state");
        assert_eq!(rel(header(11, "other", false)), ArchiveRelation::Branch, "one more, from another state");
        assert_eq!(rel(header(12, "other", false)), ArchiveRelation::Newer);
        // A complete lineage without this device's state: a branch, or older below the counter
        // (this device's own lineage is cut off, so it may simply not know it).
        assert_eq!(rel(header(12, "", true)), ArchiveRelation::Branch);
        assert_eq!(rel(header(9, "", true)), ArchiveRelation::Older);
        // By the lineage: the ancestors of this device's state are older.
        let mut old = header(3, "", false);
        old.state = "older".into();
        assert_eq!(rel(old), ArchiveRelation::Older);
        let _ = std::fs::remove_dir_all(root);
    }

    /// M5: notes of the device written from many threads at once: none is lost.
    #[test]
    fn notes_of_the_device_are_written_one_at_a_time() {
        let (root, s) = temp_suite("seen-lock");
        let threads: Vec<_> = (0..8u8)
            .map(|t| {
                let s = s.clone();
                std::thread::spawn(move || {
                    for i in 0..10u8 {
                        s.note_epoch(&[t * 16 + i; 16], 1).unwrap();
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(s.seen_all().len(), 80);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A full GhulDocs (content files of the largest stored size, as many documents as allowed)
    /// and the other vaults at their limit always fit into the largest suite file read.
    #[test]
    fn a_full_suite_fits_into_the_largest_file() {
        use super::super::docs::{stored_limit, DOCS_MAX_FILES, DOCS_SUITE_LIMIT};
        let doc = format!("docs-{0}/{0}.ghlblob", "0".repeat(32));
        let entries = DOCS_MAX_FILES * super::super::zipstream::entry_len(&doc, 0);
        // The header, the suite file and a few hundred vaults (their names; their content is in
        // OTHER_LIMIT).
        let vaults = 500 * super::super::zipstream::entry_len(&format!("{}.ghlpss", "0".repeat(32)), 0);
        let full = stored_limit(DOCS_SUITE_LIMIT) + entries + OTHER_LIMIT + vaults;
        assert!(full <= MAX_ARCHIVE, "{full} > {MAX_ARCHIVE}");
        // The ZIP has 32-bit offsets and sizes (no ZIP64): the largest file and its central
        // directory (every entry with a name of at most 80 bytes: `docs-<32>/<32>.ghlblob` is 78)
        // stay below 4 GiB.
        let central = (MAX_ENTRIES as u64) * (46 + 80) + 22;
        assert!(MAX_ARCHIVE + central < u64::from(u32::MAX), "{MAX_ARCHIVE} + {central}");
    }

    /// A file with more entries than are read is not written either.
    #[test]
    fn a_file_with_too_many_entries_is_not_written() {
        let many: Vec<(String, Vec<u8>)> = (0..=MAX_ENTRIES).map(|i| (format!("{i:032x}.ghlpss"), vec![1])).collect();
        assert!(matches!(
            archive_len(&many, &[]),
            Err(GhulError::InvalidInput { msg }) if msg == "the suite file would be too large"
        ));
        assert!(archive_len(&many[..MAX_ENTRIES], &[]).is_ok());
    }

    /// A file larger than is read is not written at all (nothing reaches the sink).
    #[test]
    fn a_file_too_large_to_read_is_not_written() {
        struct Never;
        impl ArchiveSink for Never {
            fn write(&self, _: Vec<u8>) -> std::result::Result<(), StorageError> {
                panic!("nothing may be written");
            }
        }
        let big = ContentFile {
            name: format!("docs-{0}/{0}.ghlblob", "0".repeat(32)),
            path: std::path::PathBuf::from("/nonexistent"),
            len: MAX_ARCHIVE,
            crc: 0,
        };
        let r = write_zip(&[("a.ghlpss".into(), vec![1, 2, 3])], &[big], Arc::new(Never), None);
        assert!(
            matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "the suite file would be too large"),
            "{r:?}"
        );
    }

    /// Content folders that cannot be put back are never removed: kept in a folder of their own.
    #[test]
    fn content_folders_that_cannot_go_back_are_kept() {
        let root = std::env::temp_dir().join(format!("ghul-aside-{}", random_id()));
        let old_docs = format!("docs-{}", "a".repeat(32));
        std::fs::create_dir_all(root.join(&old_docs)).unwrap();
        std::fs::write(root.join(&old_docs).join("x.ghlblob"), b"old document").unwrap();
        let old = Staging::new(&root).unwrap();
        std::fs::rename(root.join(&old_docs), old.path().join(&old_docs)).unwrap();
        // Something is in the way where the folder belongs (not one this replace moved in).
        std::fs::create_dir_all(root.join(&old_docs).join("in-the-way")).unwrap();
        let aside = Aside { root: root.clone(), old, moved_in: Vec::new(), moved_out: vec![old_docs.clone()] };
        let r = aside.back();
        assert!(
            matches!(&r, Err(GhulError::Storage { msg }) if msg.contains("kept in a folder of their own")),
            "{r:?}"
        );
        let rescued: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("rescued-documents-"))
            .collect();
        assert_eq!(rescued.len(), 1);
        let kept = rescued[0].path().join(&old_docs).join("x.ghlblob");
        assert_eq!(std::fs::read(kept).unwrap(), b"old document");
        // Tidying (also much later) leaves it alone.
        super::super::docs::tidy_staging(&root);
        assert_eq!(std::fs::read(rescued[0].path().join(&old_docs).join("x.ghlblob")).unwrap(), b"old document");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A check that ends after a lock (or cancel) is not kept: its keys must not outlive it.
    #[test]
    fn a_check_ending_after_a_lock_is_not_kept() {
        let dir = std::env::temp_dir().join(format!("ghul-gen-{}", random_id()));
        let s = GhulSuite::new(
            super::super::storage::local_storage(dir.join("v").to_string_lossy().into_owned()),
            dir.join("l").to_string_lossy().into_owned(),
        );
        let gen = s.pending_gen_now();
        s.lock();
        assert!(matches!(s.keep_pending(gen, dummy()), Err(GhulError::Locked)));
        assert!(s.pending.lock().unwrap().is_none());
        let gen = s.pending_gen_now();
        s.keep_pending(gen, dummy()).unwrap();
        assert!(s.pending.lock().unwrap().is_some());
        s.archive_cancel();
        assert!(s.pending.lock().unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The content folders of GhulDocs in `root` (`docs-<id>`).
fn docs_folders(root: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(super::docs::DIR_PREFIX))
        .collect()
}
