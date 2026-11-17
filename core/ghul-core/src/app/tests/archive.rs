// SPDX-License-Identifier: AGPL-3.0-or-later
//! The suite as one file (`.ghlzip`): opening, saving, what a device notices (older states,
//! branches, another suite, an older protection) and files changed by someone else.

use super::super::archive::{ArchiveRelation, ArchiveTarget, HEADER_FILE};
use super::*;
use crate::vault::{Keyfile, LockedContainer, RecoveryCode};

const PW: &str = "pw-123456";

/// A device with a new suite (one login), its key file and recovery code.
fn device_with_suite(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, String) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    let res = s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "1")).unwrap();
    (d, s, kf, res.recovery_code.unwrap())
}

/// Saves the suite into a file and confirms it.
fn save(s: &GhulSuite) -> Vec<u8> {
    let built = s.archive_build("Test".into()).unwrap();
    s.archive_saved(built.state, built.change).unwrap();
    built.data
}

fn open(s: &GhulSuite, data: &[u8], kf: &[u8]) -> super::super::archive::ArchiveCheck {
    s.archive_open(data.to_vec(), PW.into(), None, None, None, Some(kf.to_vec())).unwrap()
}

fn entries(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data.to_vec())).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut b).unwrap();
            (f.name().unwrap().to_string(), b)
        })
        .collect()
}

fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (n, d) in files {
            z.start_file(n.as_str(), o).unwrap();
            std::io::Write::write_all(&mut z, d).unwrap();
        }
        z.finish().unwrap();
    }
    out.into_inner()
}

#[test]
fn a_suite_goes_to_another_device_and_back() {
    let (_da, a, kf, _) = device_with_suite("arch-a");
    assert!(a.archive_unsaved(), "a new suite is not in a file yet");
    let file = save(&a);
    assert!(!a.archive_unsaved());
    let names: Vec<String> = entries(&file).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&HEADER_FILE.to_owned()) && names.contains(&"suite.ghlsuite".to_owned()), "{names:?}");

    // A new device: nothing there yet.
    let db = Dirs::new("arch-b");
    let b = db.suite();
    assert_eq!(b.archive_protection(file.clone()).unwrap().keyfiles, 1);
    let wrong = b.archive_open(file.clone(), "falsch".into(), None, None, None, Some(kf.clone()));
    assert!(matches!(wrong, Err(GhulError::WrongPassword)));
    let check = open(&b, &file, &kf);
    assert_eq!(check.relation, ArchiveRelation::NoSuite);
    assert!(!check.unverified && !check.unsaved_changes && check.vaults == 1);
    assert!(!b.is_unlocked(), "nothing replaced before accepting");
    b.archive_accept().unwrap();
    assert!(b.is_unlocked());
    assert_eq!(titles(&b), vec!["Bank"]);
    assert!(!b.archive_unsaved());

    // B works on it and saves; A opens the newer file.
    b.save_entry(vault_id(&b, "Privat"), login("Shop", "u", "2")).unwrap();
    assert!(b.archive_unsaved());
    let file2 = save(&b);
    let check = open(&a, &file2, &kf);
    assert_eq!(check.relation, ArchiveRelation::Newer);
    a.archive_accept().unwrap();
    assert_eq!(titles(&a), vec!["Bank", "Shop"]);
    // The same file again: the same state.
    assert_eq!(open(&a, &file2, &kf).relation, ArchiveRelation::Same);
    a.archive_cancel();
    // The file before: older than what this device saw.
    assert_eq!(open(&a, &file, &kf).relation, ArchiveRelation::Older);
    a.archive_cancel();
    // After the restart of the app it still knows.
    a.lock();
    let a2 = _da.suite();
    a2.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(open(&a2, &file, &kf).relation, ArchiveRelation::Older);
}

#[test]
fn a_branch_unsaved_changes_and_another_suite_are_noticed() {
    let (_da, a, kf, _) = device_with_suite("branch-a");
    let first = save(&a);
    let db = Dirs::new("branch-b");
    let b = db.suite();
    open(&b, &first, &kf);
    b.archive_accept().unwrap();
    // B carries on from the first state.
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "u", "2")).unwrap();
    let from_b = save(&b);
    // A carries on from the first state as well (it never saw B's file).
    a.save_entry(vault_id(&a, "Privat"), login("Von A", "u", "3")).unwrap();
    let check = open(&a, &from_b, &kf);
    assert_eq!(check.relation, ArchiveRelation::Newer);
    assert!(check.unsaved_changes, "A's own change would be lost");
    a.archive_cancel();
    let from_a = save(&a);
    // B gets A's file: carried on from another state than B's.
    assert_eq!(open(&b, &from_a, &kf).relation, ArchiveRelation::Branch);
    b.archive_cancel();

    // A file of another suite.
    let (_dc, c, kfc, _) = device_with_suite("branch-c");
    let other = save(&c);
    let check = c.archive_open(other.clone(), PW.into(), None, None, None, Some(kfc.clone())).unwrap();
    assert_eq!(check.relation, ArchiveRelation::Same);
    c.archive_cancel();
    let check = a.archive_open(other, PW.into(), None, None, None, Some(kfc)).unwrap();
    assert_eq!(check.relation, ArchiveRelation::OtherSuite);
    a.archive_accept().unwrap();
    assert_eq!(titles(&a), vec!["Bank"], "the other suite replaced everything");
}

#[test]
fn a_file_with_an_older_protection_is_not_opened() {
    let (_da, a, kf, _) = device_with_suite("epoch-a");
    let old = save(&a);
    let db = Dirs::new("epoch-b");
    let b = db.suite();
    open(&b, &old, &kf);
    b.archive_accept().unwrap();
    // A changes the password: from now on A refuses the old file, also before saving.
    a.change_password(PW.into(), "pw-neu-7890".into()).unwrap();
    let r = a.archive_open(old.clone(), PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::ProtectionOutdated)), "{r:?}");
    let newer = a.archive_build("A".into()).unwrap();
    a.archive_saved(newer.state, newer.change).unwrap();
    // B opens the new file (new password); then the old one is refused there as well.
    let check = b.archive_open(newer.data, "pw-neu-7890".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(check.relation, ArchiveRelation::Newer);
    b.archive_accept().unwrap();
    let r = b.archive_open(old.clone(), PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::ProtectionOutdated)), "{r:?}");
    // A device that never saw the newer state opens the old file (it cannot know).
    let dc = Dirs::new("epoch-c");
    let c = dc.suite();
    assert_eq!(open(&c, &old, &kf).relation, ArchiveRelation::NoSuite);
}

#[test]
fn saving_over_a_file_saved_elsewhere_is_noticed() {
    let (_da, a, kf, _) = device_with_suite("target-a");
    assert_eq!(a.archive_target(vec![]).unwrap(), ArchiveTarget::New);
    let file = save(&a);
    assert_eq!(a.archive_target(file.clone()).unwrap(), ArchiveTarget::Same);
    // Built but not written: the file there is still "ours".
    let _unwritten = a.archive_build("A".into()).unwrap();
    assert_eq!(a.archive_target(file.clone()).unwrap(), ArchiveTarget::Same);
    // Another device saved over it.
    let db = Dirs::new("target-b");
    let b = db.suite();
    open(&b, &file, &kf);
    b.archive_accept().unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("B", "u", "2")).unwrap();
    let from_b = save(&b);
    assert_eq!(a.archive_target(from_b).unwrap(), ArchiveTarget::ChangedElsewhere);
    // Not a GhulSuite file, another suite.
    assert_eq!(a.archive_target(b"irgendwas".to_vec()).unwrap(), ArchiveTarget::OtherSuite);
    let (_dc, c, _, _) = device_with_suite("target-c");
    assert_eq!(a.archive_target(save(&c)).unwrap(), ArchiveTarget::OtherSuite);
}

#[test]
fn the_recovery_code_opens_a_file_unverified_and_sets_a_new_password() {
    let (_da, a, kf, code) = device_with_suite("code-a");
    let file = save(&a);
    let db = Dirs::new("code-b");
    let b = db.suite();
    assert!(b.archive_open_with_code(file.clone(), "AAAA-BBBB".into(), "neu-pw-123".into()).is_err());
    let check = b.archive_open_with_code(file, code, "neu-pw-123".into()).unwrap();
    assert!(check.unverified);
    b.archive_accept().unwrap();
    assert_eq!(titles(&b), vec!["Bank"]);
    assert!(b.archive_unsaved(), "written anew with the new password: to be saved");
    b.lock();
    b.unlock("neu-pw-123".into(), None, None, None, Some(kf)).unwrap();
}

#[test]
fn opening_a_file_removes_this_devices_data_of_the_old_state() {
    let (_da, a, kf, _) = device_with_suite("wipe-a");
    let file = save(&a);
    a.quick_unlock_enable("pin".into(), Some("1234".into())).unwrap();
    assert!(a.quick_unlock_exists("pin".into()));
    open(&a, &file, &kf);
    a.archive_accept().unwrap();
    assert!(!a.quick_unlock_exists("pin".into()), "quick unlock holds the old password key");
}

/// Every way of changing a `.ghlzip` without the password is refused before anything is
/// replaced, and the working copy stays as it is.
#[test]
fn changed_files_are_refused() {
    let (_da, a, kf, code) = device_with_suite("forge-a");
    let good = save(&a);
    let base = entries(&good);
    let vault = base.iter().find(|(n, _)| n.ends_with(".ghlpss")).unwrap().0.clone();
    let db = Dirs::new("forge-b");
    let b = db.suite();
    let refused = |files: Vec<(String, Vec<u8>)>, what: &str| {
        let r = b.archive_open(zip_of(&files), PW.into(), None, None, None, Some(kf.clone()));
        assert!(r.is_err(), "{what} was accepted");
        assert!(!b.exists().unwrap(), "{what}: nothing written");
    };
    // A file missing, one too many, one twice.
    refused(base.iter().filter(|(n, _)| n != &vault).cloned().collect(), "a missing vault");
    let mut extra = base.clone();
    extra.push(("zusatz.ghltotp".into(), base.iter().find(|(n, _)| n == &vault).unwrap().1.clone()));
    refused(extra, "an extra file");
    refused(base.iter().filter(|(n, _)| n != HEADER_FILE).cloned().collect(), "no header");
    // A byte changed anywhere.
    for name in [vault.as_str(), "suite.ghlsuite", HEADER_FILE] {
        let mut changed = base.clone();
        let f = changed.iter_mut().find(|(n, _)| n == name).unwrap();
        let at = f.1.len() / 2;
        f.1[at] ^= 1;
        refused(changed, &format!("a changed byte in {name}"));
    }
    // The vault of another suite under the same name.
    let (_dc, c, _, _) = device_with_suite("forge-c");
    let theirs = entries(&save(&c));
    let mut swapped = base.clone();
    swapped.iter_mut().find(|(n, _)| n == &vault).unwrap().1 =
        theirs.iter().find(|(n, _)| n.ends_with(".ghlpss")).unwrap().1.clone();
    refused(swapped, "a vault of another suite");
    // An older vault of the same suite (with the newer header).
    a.save_entry(vault_id(&a, "Privat"), login("Neu", "u", "2")).unwrap();
    let newer = entries(&save(&a));
    let mut mixed = newer.clone();
    mixed.iter_mut().find(|(n, _)| n == &vault).unwrap().1 = base.iter().find(|(n, _)| n == &vault).unwrap().1.clone();
    refused(mixed, "an older vault with the newer header");

    // Forged with the recovery code (no password): a vault with an entry of the attacker's, the
    // header made to match. The header's seal cannot be made without the password.
    let code = RecoveryCode::parse(&code).unwrap();
    let mut forged = base.clone();
    let v = forged.iter_mut().find(|(n, _)| n == &vault).unwrap();
    let mut c = LockedContainer::parse(&v.1).unwrap().recover(&code).unwrap();
    let mut e = crate::vault::payload::Entry::new([9; 16], "login", 1);
    e.fields.insert("title".into(), crate::vault::payload::Field::Text("Angreifer".into()));
    c.payload.entries.push(e);
    v.1 = c.seal_without_password(&mut rand_core::OsRng);
    let hash = crate::kdf::hex(&<sha2::Sha256 as sha2::Digest>::digest(&v.1));
    let h = forged.iter_mut().find(|(n, _)| n == HEADER_FILE).unwrap();
    let mut hc = LockedContainer::parse(&h.1).unwrap().recover(&code).unwrap();
    let mut header: serde_json::Value =
        serde_json::to_value(hc.payload.settings.get("export").cloned().unwrap()).unwrap();
    for f in header["files"].as_array_mut().unwrap() {
        if f["name"] == vault.as_str() {
            f["sha256"] = serde_json::Value::String(hash.clone());
        }
    }
    let header: ciborium::value::Value = serde_json::from_value(header).unwrap();
    super::super::suite::put_setting(&mut hc, "export", &header).unwrap();
    h.1 = hc.seal_without_password(&mut rand_core::OsRng);
    let r = b.archive_open(zip_of(&forged), PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::Tampered)), "{:?}", r.err());
    // With the code alone it opens (unverified, the emergency case) but shows it.
    let check = b.archive_open_with_code(zip_of(&forged), code.to_display().to_string(), "x-neu-123".into());
    assert!(check.unwrap().unverified);
    b.archive_cancel();
    // A forged key file slot: the attacker's key file is let in nowhere.
    let evil = Keyfile::parse(&generate_keyfile()).unwrap();
    assert!(b.archive_open(good.clone(), PW.into(), None, None, None, Some(evil.encode().to_vec())).is_err());
}

#[test]
fn zip_tricks_are_refused() {
    let (_da, a, kf, _) = device_with_suite("zip-a");
    let base = entries(&save(&a));
    let db = Dirs::new("zip-b");
    let b = db.suite();
    for name in
        ["../suite.ghlsuite", "/tmp/x.ghlpss", "a/b.ghlpss", "C:\\x.ghlpss", "x.txt", ".ghlpss", "SUITE.GHLSUITE"]
    {
        let mut files = base.clone();
        files.push((name.into(), b"x".to_vec()));
        let r = b.archive_open(zip_of(&files), PW.into(), None, None, None, Some(kf.clone()));
        assert!(r.is_err(), "{name} was accepted");
    }
    assert!(b.archive_open(b"kein zip".to_vec(), PW.into(), None, None, None, Some(kf.clone())).is_err());
    assert!(b.archive_protection(b"kein zip".to_vec()).is_err());
    // A file that unpacks to much more than it says.
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("suite.ghlsuite", o).unwrap();
        for _ in 0..64 {
            std::io::Write::write_all(&mut z, &vec![0u8; 1024 * 1024]).unwrap();
        }
        z.finish().unwrap();
    }
    assert!(super::super::archive::read_zip_limited(&out.into_inner(), 16 * 1024 * 1024).is_err(), "a bomb");
    assert!(!b.exists().unwrap());
}

/// A branch several saves apart is still a branch (not "newer"), and an old state of the own
/// line is older.
#[test]
fn branches_are_noticed_several_saves_apart() {
    let (_da, a, kf, _) = device_with_suite("lineage-a");
    let first = save(&a);
    let db = Dirs::new("lineage-b");
    let b = db.suite();
    open(&b, &first, &kf);
    b.archive_accept().unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "u", "1")).unwrap();
    let from_b = save(&b);
    // A saves three times from the first state, never seeing B's file.
    let mut from_a = Vec::new();
    for i in 0..3 {
        a.save_entry(vault_id(&a, "Privat"), login(&format!("A{i}"), "u", "2")).unwrap();
        from_a = save(&a);
    }
    let check = open(&b, &from_a, &kf);
    assert_eq!(check.relation, ArchiveRelation::Branch, "B's saved state is not in A's line");
    b.archive_cancel();
    // A gets B's file: a branch as well.
    assert_eq!(open(&a, &from_b, &kf).relation, ArchiveRelation::Branch);
    a.archive_cancel();
    // A's own first state: older.
    assert_eq!(open(&a, &first, &kf).relation, ArchiveRelation::Older);
    a.archive_cancel();
    // A device that has the first state takes A's latest as newer.
    let dc = Dirs::new("lineage-c");
    let c = dc.suite();
    open(&c, &first, &kf);
    c.archive_accept().unwrap();
    assert_eq!(open(&c, &from_a, &kf).relation, ArchiveRelation::Newer);
}

/// A change while the file is being written is not in it: it stays unsaved.
#[test]
fn a_change_while_saving_stays_unsaved() {
    let (_da, a, _kf, _) = device_with_suite("change-a");
    let built = a.archive_build("A".into()).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Dazwischen", "u", "1")).unwrap();
    a.archive_saved(built.state, built.change).unwrap();
    assert!(a.archive_unsaved(), "the change after building is not in the file");
    save(&a);
    assert!(!a.archive_unsaved());
}

/// A vault that cannot be read is not silently left out of the file.
#[test]
fn a_damaged_vault_is_not_left_out() {
    let (d, a, kf, _) = device_with_suite("damaged-a");
    let file = a.list_vaults().unwrap()[0].file.clone();
    a.lock();
    a.finish_lock();
    let st = d.storage();
    let mut data = st.read(file.clone()).unwrap().unwrap();
    let n = data.len();
    data[n - 10] ^= 1;
    st.write(file, data).unwrap();
    a.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert!(!a.list_vaults().unwrap()[0].available);
    let r = a.archive_build("A".into());
    assert!(matches!(r, Err(GhulError::InvalidInput { .. })), "{:?}", r.err());
}

/// A checked file's keys do not outlive locking; no copy of an old working copy stays behind.
#[test]
fn locking_forgets_a_checked_file() {
    let (da, a, kf, _) = device_with_suite("forget-a");
    let file = save(&a);
    let db = Dirs::new("forget-b");
    let b = db.suite();
    open(&b, &file, &kf);
    b.lock();
    assert!(matches!(b.archive_accept(), Err(GhulError::NotFound)));
    assert!(!b.archive_device_data_wiped(), "failed before anything was removed");

    // No copy of the old working copy is left behind (it would still open with old passwords).
    let old = da.root.join("local").join("before-open.ghlzip");
    std::fs::write(&old, b"old").unwrap();
    open(&a, &file, &kf);
    a.archive_accept().unwrap();
    assert!(!old.exists());
    // Also on a device that never opens a file again: gone with the next start of the app.
    std::fs::write(&old, b"old").unwrap();
    let _again = da.suite();
    assert!(!old.exists());
    assert!(a.archive_device_data_wiped());
}

/// More entries than any suite has: refused before anything is read.
#[test]
fn a_zip_with_too_many_entries_is_refused() {
    let files: Vec<(String, Vec<u8>)> = (0..1001).map(|i| (format!("v{i}.ghlpss"), vec![0u8])).collect();
    let dd = Dirs::new("many");
    let s = dd.suite();
    let r = s.archive_protection(zip_of(&files));
    assert!(matches!(r, Err(GhulError::InvalidInput { .. })));
}

/// A storage whose next write of the suite file fails (a full or pulled storage).
struct FailOnce {
    inner: Arc<dyn VaultStorage>,
    armed: std::sync::atomic::AtomicBool,
}

impl VaultStorage for FailOnce {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, super::super::storage::StorageError> {
        self.inner.read(name)
    }
    fn write(&self, name: String, data: Vec<u8>) -> Result<(), super::super::storage::StorageError> {
        if name == "suite.ghlsuite" && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err(super::super::storage::StorageError::Io { msg: "full".into() });
        }
        self.inner.write(name, data)
    }
    fn delete(&self, name: String) -> Result<(), super::super::storage::StorageError> {
        self.inner.delete(name)
    }
    fn list(&self) -> Result<Vec<String>, super::super::storage::StorageError> {
        self.inner.list()
    }
    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// Opening a file that cannot be written completely leaves the working copy as it was.
#[test]
fn a_half_written_open_goes_back() {
    let (d, a, kf, _) = device_with_suite("half-a");
    let file = save(&a);
    a.save_entry(vault_id(&a, "Privat"), login("Neu", "u", "1")).unwrap();
    a.lock();
    a.finish_lock();
    let flaky = Arc::new(FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let s = GhulSuite::new(flaky.clone(), d.local());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    open(&s, &file, &kf);
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(s.archive_accept().is_err());
    assert!(!s.is_unlocked());
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"], "the working copy before, unchanged");
}

/// What a device took counts, also an older state: a file carried on from it is newer, and the
/// state it left is newer again (not "the same").
#[test]
fn the_state_taken_counts_also_an_older_one() {
    let (_da, a, kf, _) = device_with_suite("taken-a");
    let first = save(&a);
    a.save_entry(vault_id(&a, "Privat"), login("Zwei", "u", "2")).unwrap();
    let second = save(&a);
    // A goes back to the first state on purpose.
    assert_eq!(open(&a, &first, &kf).relation, ArchiveRelation::Older);
    a.archive_accept().unwrap();
    assert_eq!(open(&a, &first, &kf).relation, ArchiveRelation::Same);
    a.archive_cancel();
    assert_eq!(open(&a, &second, &kf).relation, ArchiveRelation::Newer, "not the state A has now");
    a.archive_cancel();
    // Carried on from the first state again: newer for a device that has the first state.
    a.save_entry(vault_id(&a, "Privat"), login("Drei", "u", "3")).unwrap();
    let third = save(&a);
    let db = Dirs::new("taken-b");
    let b = db.suite();
    open(&b, &first, &kf);
    b.archive_accept().unwrap();
    assert_eq!(open(&b, &third, &kf).relation, ArchiveRelation::Newer);
    b.archive_cancel();
    // A device that has the second state: the third is a branch from it.
    let dc = Dirs::new("taken-c");
    let c = dc.suite();
    open(&c, &second, &kf);
    c.archive_accept().unwrap();
    assert_eq!(open(&c, &third, &kf).relation, ArchiveRelation::Branch);
}

/// A device several saves behind takes the latest as newer; a file saved over by this device's
/// own line (also after a build that was never written) counts as its own.
#[test]
fn several_saves_behind_is_newer_and_own_files_are_own() {
    let (_da, a, kf, _) = device_with_suite("behind-a");
    let first = save(&a);
    let db = Dirs::new("behind-b");
    let b = db.suite();
    open(&b, &first, &kf);
    b.archive_accept().unwrap();
    let mut latest = Vec::new();
    for i in 0..4 {
        a.save_entry(vault_id(&a, "Privat"), login(&format!("A{i}"), "u", "1")).unwrap();
        latest = save(&a);
    }
    assert_eq!(open(&b, &latest, &kf).relation, ArchiveRelation::Newer);
    b.archive_cancel();
    // Built twice without writing: the file there (an older state of A's own line) is A's own.
    let _ = a.archive_build("A".into()).unwrap();
    let _ = a.archive_build("A".into()).unwrap();
    assert_eq!(a.archive_target(latest.clone()).unwrap(), ArchiveTarget::Same);
    assert_eq!(a.archive_target(first).unwrap(), ArchiveTarget::Same);
}
