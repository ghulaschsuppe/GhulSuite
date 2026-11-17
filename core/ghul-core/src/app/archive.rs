// SPDX-License-Identifier: AGPL-3.0-or-later
//! The suite as one file (`.ghlzip`): opened into the working copy and saved from it.
//!
//! A `.ghlzip` is a ZIP with exactly the suite file, its vault files and the header
//! `export.ghlexp`, a container of its own (sealed like the vaults) that lists every file with
//! its kind, container id and SHA-256, and says which state the file is: a random state id, the
//! state it was made from, a counter and the protection epoch. Opening checks everything before
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

use super::error::{GhulError, Result};
use super::suite::{
    is_vault_file, now_ms, parse_keyfile, put_setting, second_factor, setting, GhulSuite, ProtectionInfo, SUITE_FILE,
};
use crate::vault::{ContainerKind, FactorSecret, LockedContainer, PasswordKey, RecoveryCode, UnlockedContainer};
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
/// The working copy as it was before a file was opened over it (one step back).
const BEFORE_FILE: &str = "before-open.ghlzip";
/// The message for anything that is no `.ghlzip` (translated by the app).
const NOT_A_SUITE_FILE: &str = "not a GhulSuite file";
/// Largest `.ghlzip` read (counted while reading, not taken from the ZIP).
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
/// Most entries a `.ghlzip` may have (a suite has a handful of files).
const MAX_ENTRIES: usize = 1000;
/// How many earlier states a file and a device remember (to tell newer states from branches).
const MAX_ANCESTORS: usize = 64;

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
    #[serde(default)]
    ancestors: Vec<String>,
    /// `ancestors` reaches back to the first save: a state not in it is no ancestor.
    #[serde(default)]
    complete: bool,
}

/// The state a working copy is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct StateRef {
    state: String,
    counter: u64,
    /// The state before (still in the file when the last built one was not written).
    #[serde(default)]
    previous: String,
    /// The states this one was carried on from, the latest first.
    #[serde(default)]
    ancestors: Vec<String>,
    /// See [`ArchiveHeader::complete`].
    #[serde(default)]
    complete: bool,
}

/// What a device saw last of a suite.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
struct Seen {
    state: String,
    counter: u64,
    epoch: u64,
    /// The states the seen one was carried on from (see [`ArchiveHeader::ancestors`]).
    #[serde(default)]
    ancestors: Vec<String>,
    #[serde(default)]
    complete: bool,
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
    /// Saved there from somewhere else meanwhile (or with a protection this device cannot open).
    ChangedElsewhere,
    /// Another suite (or no GhulSuite file at all).
    OtherSuite,
}

/// A built `.ghlzip`: write `data`, then confirm with `archive_saved(state, change)`.
#[derive(uniffi::Record, Clone)]
pub struct ArchiveBuilt {
    pub data: Vec<u8>,
    pub state: String,
    /// The change of the working copy it holds: a change made while the file is written is
    /// not in it and stays unsaved.
    pub change: u64,
}

/// A checked file waiting for `archive_accept`.
pub(super) struct Pending {
    files: Vec<(String, Vec<u8>)>,
    header: ArchiveHeader,
    suite_id: [u8; 16],
    /// With the recovery code: the code and the new password (set on accepting).
    recovery: Option<(Zeroizing<String>, Zeroizing<String>)>,
    /// With the password: the password key and the factor secret (the files verified with them).
    keys: Option<(PasswordKey, FactorSecret)>,
}

/// The entries of a `.ghlzip`, read with every rule: only plain names, no folders, no entry
/// twice, sizes counted while reading.
fn read_zip(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    read_zip_limited(data, MAX_ARCHIVE)
}

/// [`read_zip`] with another size limit (tests).
pub(crate) fn read_zip_limited(data: &[u8], max: u64) -> Result<Vec<(String, Vec<u8>)>> {
    // What is wrong stays out of the message: the app names it in plain words.
    let bad = |_why: &str| GhulError::InvalidInput { msg: NOT_A_SUITE_FILE.into() };
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|_| bad("no ZIP"))?;
    if z.len() > MAX_ENTRIES {
        return Err(bad("too many entries"));
    }
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|_| bad("damaged entry"))?;
        let name = f.name().map_err(|_| bad("entry name"))?.to_string();
        if f.is_dir()
            || !super::storage::plain_file_name(&name)
            || !(name == HEADER_FILE || name == SUITE_FILE || is_vault_file(&name))
        {
            return Err(bad("unexpected entry"));
        }
        if out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            return Err(bad("entry twice"));
        }
        let mut buf = Vec::new();
        (&mut f).take(max - total + 1).read_to_end(&mut buf).map_err(|_| bad("damaged entry"))?;
        total += buf.len() as u64;
        if total > max {
            return Err(bad("too large"));
        }
        out.push((name, buf));
    }
    Ok(out)
}

fn write_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let err = |e: &dyn std::fmt::Display| GhulError::Storage { msg: e.to_string() };
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (n, d) in files {
            z.start_file(n.as_str(), opts).map_err(|e| err(&e))?;
            z.write_all(d).map_err(|e| err(&e))?;
        }
        z.finish().map_err(|e| err(&e))?;
    }
    Ok(out.into_inner())
}

fn sha256_hex(data: &[u8]) -> String {
    crate::kdf::hex(&Sha256::digest(data))
}

fn random_id() -> String {
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    crate::kdf::hex(&id)
}

/// The header container of the entries.
fn header_of(files: &[(String, Vec<u8>)]) -> Result<LockedContainer> {
    let data = files
        .iter()
        .find(|(n, _)| n == HEADER_FILE)
        .map(|(_, d)| d)
        .ok_or_else(|| GhulError::InvalidInput { msg: NOT_A_SUITE_FILE.into() })?;
    let l = LockedContainer::parse(data)?;
    if l.kind() != ContainerKind::Export {
        return Err(GhulError::InvalidInput { msg: NOT_A_SUITE_FILE.into() });
    }
    Ok(l)
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

/// Checks every entry against the header: the same suite everywhere, exactly the listed files
/// with their kinds, ids and hashes, every container opens (with the password: its seal holds).
fn verify(files: &[(String, Vec<u8>)], key: &Key<'_>) -> Result<(ArchiveHeader, [u8; 16])> {
    let mismatch = |_why: &str| GhulError::Corrupted { msg: "the suite file does not match its list".into() };
    let header_c = open_container(header_of(files)?, key)?;
    let header: ArchiveHeader = setting(&header_c, HEADER_KEY);
    let suite_id = header_c.header().suite_id;
    let listed: Vec<&str> = header.files.iter().map(|f| f.name.as_str()).collect();
    let present: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).filter(|n| *n != HEADER_FILE).collect();
    if listed.len() != present.len() || present.iter().any(|n| !listed.contains(n)) {
        return Err(mismatch("files missing or too many"));
    }
    if !listed.contains(&SUITE_FILE) {
        return Err(mismatch("no suite file"));
    }
    let mut suite_epoch = None;
    for f in &header.files {
        let data = &files.iter().find(|(n, _)| n == &f.name).expect("checked above").1;
        if sha256_hex(data) != f.sha256 {
            return Err(mismatch(&format!("{} changed", f.name)));
        }
        let l = LockedContainer::parse(data)?;
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
        if c.kind() == ContainerKind::Suite {
            suite_epoch = Some(setting::<u64>(&c, super::suite::EPOCH_KEY));
        }
    }
    if suite_epoch != Some(header.epoch) {
        return Err(mismatch("protection"));
    }
    Ok((header, suite_id))
}

impl GhulSuite {
    fn seen_all(&self) -> BTreeMap<String, Seen> {
        std::fs::read(self.local_dir.join(SEEN_FILE))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default()
    }

    /// This device has `state` now (opened or saved; also an older one or a branch the user
    /// took). The counter only grows: a later save goes above everything seen.
    fn note_archive_seen(&self, suite_id: &[u8; 16], state: &StateRef, epoch: u64) {
        let mut all = self.seen_all();
        let e = all.entry(crate::kdf::hex(suite_id)).or_default();
        e.counter = e.counter.max(state.counter);
        e.state = state.state.clone();
        e.ancestors = state.ancestors.iter().take(MAX_ANCESTORS).cloned().collect();
        e.complete = state.complete;
        e.epoch = e.epoch.max(epoch);
        self.write_seen(&all);
    }

    /// The protection of a suite changed on this device (its epoch is `epoch` now).
    pub(super) fn note_epoch(&self, suite_id: &[u8; 16], epoch: u64) {
        let mut all = self.seen_all();
        let e = all.entry(crate::kdf::hex(suite_id)).or_default();
        if epoch <= e.epoch {
            return;
        }
        e.epoch = epoch;
        self.write_seen(&all);
    }

    fn write_seen(&self, all: &BTreeMap<String, Seen>) {
        if let Ok(json) = serde_json::to_vec(all) {
            let _ = std::fs::create_dir_all(&self.local_dir);
            let _ = super::storage::write_atomic(&self.local_dir, SEEN_FILE, &json);
        }
    }

    /// The working copy changed since it was loaded from or saved into a file.
    pub(super) fn mark_unsaved(&self) {
        if self.quiet.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.changes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = std::fs::create_dir_all(&self.local_dir);
        let _ = std::fs::write(self.local_dir.join(UNSAVED_FILE), b"1");
    }

    fn unsaved(&self) -> bool {
        self.local_dir.join(UNSAVED_FILE).exists()
    }

    /// The change counter of the working copy (grows with every write of a container).
    pub(super) fn change(&self) -> u64 {
        self.changes.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Clears the unsaved mark if nothing changed since `change` was read.
    pub(super) fn clear_unsaved_at(&self, change: u64) {
        if self.change() == change {
            self.clear_unsaved();
        }
    }

    /// Forgets the working copy as it was before the last file was opened.
    pub(super) fn forget_before_open(&self) {
        let _ = std::fs::remove_file(self.local_dir.join(BEFORE_FILE));
    }

    pub(super) fn clear_unsaved(&self) {
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
        // Neither, and the file's lineage is complete: it was not carried on from this state.
        if header.complete {
            return Ok(if seen.complete || header.counter >= seen.counter {
                ArchiveRelation::Branch
            } else {
                ArchiveRelation::Older
            });
        }
        // Files without a complete lineage (older app versions): by the counter.
        Ok(if header.counter < seen.counter {
            ArchiveRelation::Older
        } else if header.counter == seen.counter
            || (header.counter == seen.counter + 1 && header.previous != seen.state)
        {
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

    /// Replaces the working copy with the pending files: the current one is kept in
    /// [`BEFORE_FILE`] (one step back), files of the old copy that the new one does not have are
    /// removed, the suite file is written last.
    fn replace_working_copy(&self, files: &[(String, Vec<u8>)]) -> Result<()> {
        let mut before = Vec::new();
        for n in self.storage.list()? {
            if n == SUITE_FILE || is_vault_file(&n) {
                if let Some(d) = self.storage.read(n.clone())? {
                    before.push((n, d));
                }
            }
        }
        if before.is_empty() {
            // Nothing to keep: an older one-step-back would belong to another state.
            let _ = std::fs::remove_file(self.local_dir.join(BEFORE_FILE));
        } else {
            let _ = std::fs::create_dir_all(&self.local_dir);
            super::storage::write_atomic(&self.local_dir, BEFORE_FILE, &write_zip(&before)?)?;
        }
        let new: Vec<(String, Vec<u8>)> = files.iter().filter(|(n, _)| n != HEADER_FILE).cloned().collect();
        if let Err(e) = self.put_files(&new, &before) {
            // Half replaced is neither state: back to the one before.
            let _ = self.put_files(&before, &new);
            return Err(e);
        }
        Ok(())
    }

    /// Writes `files` (the suite file last) and removes what only `old` had.
    fn put_files(&self, files: &[(String, Vec<u8>)], old: &[(String, Vec<u8>)]) -> Result<()> {
        for (n, d) in files.iter().filter(|(n, _)| n != SUITE_FILE) {
            self.storage.write(n.clone(), d.clone())?;
        }
        if let Some((_, d)) = files.iter().find(|(n, _)| n == SUITE_FILE) {
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
    /// key), the mail cache, Drive's offline copies and unfinished uploads.
    fn wipe_device_data(&self) {
        super::quick::delete_all(&self.local_dir);
        for dir in ["mail-cache", "drive-offline"] {
            let _ = std::fs::remove_dir_all(self.local_dir.join(dir));
        }
        let _ = std::fs::remove_file(self.local_dir.join("drive-pending"));
        *self.net.lock().unwrap_or_else(|p| p.into_inner()) = Default::default();
    }
}

#[uniffi::export]
impl GhulSuite {
    /// What a `.ghlzip` needs to be opened (password plus key or key file). Checks only that it
    /// is one; nothing is opened.
    pub fn archive_protection(&self, data: Vec<u8>) -> Result<ProtectionInfo> {
        let files = read_zip(&data)?;
        Ok(super::suite::protection_of(&header_of(&files)?))
    }

    /// Opens a `.ghlzip` with the password plus key or key file and checks all of it (see the
    /// module). Nothing is replaced yet: the result says how it relates to this device, then
    /// `archive_accept` takes it. A file with an older protection than this device saw is
    /// refused (`ProtectionOutdated`).
    pub fn archive_open(
        &self,
        data: Vec<u8>,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
    ) -> Result<ArchiveCheck> {
        let gen = self.pending_gen_now();
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let files = read_zip(&data)?;
        let locked = header_of(&files)?;
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let (header, suite_id) = verify(&files, &Key::Password(&pk, &s))?;
        let check = self.check_of(&header, &suite_id, false)?;
        self.keep_pending(gen, Pending { files, header, suite_id, recovery: None, keys: Some((pk, s)) })?;
        Ok(check)
    }

    /// Emergency: opens a `.ghlzip` with the recovery code alone. The seals cannot be checked
    /// without the password (`unverified`); everything else is. On accepting, everything is
    /// written again with `new_password`.
    pub fn archive_open_with_code(&self, data: Vec<u8>, code: String, new_password: String) -> Result<ArchiveCheck> {
        let gen = self.pending_gen_now();
        let code = Zeroizing::new(code);
        let parsed = RecoveryCode::parse(&code)?;
        let files = read_zip(&data)?;
        let (header, suite_id) = verify(&files, &Key::Code(&parsed))?;
        let check = self.check_of(&header, &suite_id, true)?;
        self.keep_pending(
            gen,
            Pending { files, header, suite_id, recovery: Some((code, Zeroizing::new(new_password))), keys: None },
        )?;
        Ok(check)
    }

    /// Forgets a checked file.
    pub fn archive_cancel(&self) {
        self.set_pending(None);
    }

    /// Takes the checked file: the working copy is replaced (the old one stays one step back),
    /// this device's data of the old state goes, and the suite is open.
    pub fn archive_accept(&self) -> Result<()> {
        let p = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or(GhulError::NotFound)?;
        // Checked again right before replacing: another file may have been opened meanwhile.
        self.relation(&p.header, &p.suite_id)?;
        self.lock();
        self.finish_lock();
        self.wipe_device_data();
        self.replace_working_copy(&p.files)?;
        match (&p.recovery, p.keys) {
            (Some((code, new_password)), _) => {
                self.unlock_with_recovery_code(code.to_string(), new_password.to_string())?
            }
            (None, Some((pk, s))) => {
                let data = &p.files.iter().find(|(n, _)| n == SUITE_FILE).ok_or(GhulError::NoVault)?.1;
                let suite = LockedContainer::parse(data)?.unlock(&pk, &s)?;
                self.finish_unlock(pk, suite, true)?;
            }
            _ => return Err(GhulError::NotFound),
        }
        let accepted = StateRef {
            state: p.header.state.clone(),
            counter: p.header.counter,
            previous: String::new(),
            ancestors: p.header.ancestors.clone(),
            complete: p.header.complete,
        };
        self.with_open(|o| {
            put_setting(&mut o.suite, STATE_KEY, &accepted)?;
            let epoch = o.epoch();
            self.save_container(&mut o.suite, SUITE_FILE)?;
            self.note_archive_seen(&p.suite_id, &accepted, epoch.max(p.header.epoch));
            Ok(())
        })?;
        // After the recovery code everything was written anew: that is a change to save.
        if p.recovery.is_none() {
            self.clear_unsaved();
        }
        Ok(())
    }

    /// Whether the working copy has changes that are not in a file yet.
    pub fn archive_unsaved(&self) -> bool {
        self.unsaved()
    }

    /// Compares the file at the place the suite is saved to (its content, empty when there is
    /// none) with the working copy, before overwriting it.
    pub fn archive_target(&self, data: Vec<u8>) -> Result<ArchiveTarget> {
        if data.is_empty() {
            return Ok(ArchiveTarget::New);
        }
        let Ok(files) = read_zip(&data) else { return Ok(ArchiveTarget::OtherSuite) };
        let Ok(locked) = header_of(&files) else { return Ok(ArchiveTarget::OtherSuite) };
        self.with_open(|o| {
            if locked.header().suite_id != o.suite.header().suite_id {
                return Ok(ArchiveTarget::OtherSuite);
            }
            let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let Ok(c) = locked.unlock(&o.password_key, &s) else { return Ok(ArchiveTarget::ChangedElsewhere) };
            let header: ArchiveHeader = setting(&c, HEADER_KEY);
            let mine: StateRef = setting(&o.suite, STATE_KEY);
            // The file holds a state of this working copy's own line (also after a build that was
            // never written): overwriting it loses nothing.
            let ours = !header.state.is_empty()
                && (header.state == mine.state
                    || header.state == mine.previous
                    || mine.ancestors.contains(&header.state));
            Ok(if ours { ArchiveTarget::Same } else { ArchiveTarget::ChangedElsewhere })
        })
    }

    /// Builds the `.ghlzip` of the working copy: every container sealed again, a new state.
    /// Write `data` to the file, then confirm with `archive_saved`. `device`: shown when the
    /// file is opened elsewhere.
    pub fn archive_build(&self, device: String) -> Result<ArchiveBuilt> {
        self.with_open(|o| {
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
                &StateRef { state: state.clone(), counter, previous, ancestors: ancestors.clone(), complete },
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
            let mut hc = o.new_container(ContainerKind::Export)?;
            put_setting(&mut hc, HEADER_KEY, &header)?;
            files.push((HEADER_FILE.to_owned(), hc.seal(&mut OsRng)?));
            // The working copy is this state as well (sealed the same way).
            for (n, d) in &files {
                if n != HEADER_FILE {
                    self.storage.write(n.clone(), d.clone())?;
                }
            }
            Ok(ArchiveBuilt { data: write_zip(&files)?, state, change: self.change() })
        })
    }

    /// The built file was written: this device has this state now. Nothing is unsaved unless
    /// the working copy changed after it was built (`change` from [`ArchiveBuilt`]).
    pub fn archive_saved(&self, state: String, change: u64) -> Result<()> {
        self.with_open(|o| {
            let mine: StateRef = setting(&o.suite, STATE_KEY);
            if mine.state != state {
                return Err(GhulError::InvalidInput { msg: "another state was built meanwhile".into() });
            }
            self.note_archive_seen(&o.suite.header().suite_id, &mine, o.epoch());
            Ok(())
        })?;
        self.clear_unsaved_at(change);
        Ok(())
    }

    /// The working copy as it was before the last file was opened over it (to open again).
    pub fn archive_before_open(&self) -> Result<Vec<u8>> {
        std::fs::read(self.local_dir.join(BEFORE_FILE)).map_err(|_| GhulError::NotFound)
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;

    fn dummy() -> Pending {
        Pending { files: Vec::new(), header: ArchiveHeader::default(), suite_id: [0; 16], recovery: None, keys: None }
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
