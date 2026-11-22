// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round 9 of the attacks: a vault that cannot be saved never stays in memory and never
//! leaves a change of the protection half on the disk (H-D1), and the smaller findings.

use super::super::cal::{CalCalendar, MAX_ICS_LEN};
use super::super::suite::VAULT_FULL;
use crate::vault::payload::TEST_SEAL_LIMIT;

use super::*;

/// The limit of one vault file for these tests (instead of 128 MiB).
const LIMIT: usize = 256 * 1024;

fn two_vaults(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, String, String) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P"), (VaultKind::Pass, "Q")]), None, None, None)
        .unwrap();
    let (p, q) = (vault_id(&s, "P"), vault_id(&s, "Q"));
    s.save_entry(p.clone(), login("Bank", "ich", "Wichtig1")).unwrap();
    (d, s, kf, p, q)
}

fn count(s: &GhulSuite, vault: &str) -> usize {
    s.list_entries(VaultKind::Pass, Some(vault.into())).unwrap().len()
}

/// The finding as reported: a copy that makes the vault too large goes again; a change of the
/// password afterwards works, and the new password opens everything.
#[test]
fn a_duplicate_over_the_vault_limit_goes_again_and_the_password_change_holds() {
    let (_d, s, kf, p, q) = two_vaults("r9-duplicate");
    let id = s.save_entry(q.clone(), login("Scans", "ich", "x")).unwrap();
    s.add_entry_attachment(q.clone(), id.clone(), "scan.bin".into(), vec![7u8; 150 * 1024]).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(Some(LIMIT)));
    let r = s.duplicate_entry(q.clone(), id, " (Kopie)".into());
    assert!(
        matches!(r, Err(GhulError::InvalidInput { ref msg }) if msg == VAULT_FULL),
        "too large to be sealed: {r:?}"
    );
    assert_eq!(count(&s, &q), 1, "the copy is gone again");
    s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    s.lock();
    assert!(matches!(s.unlock(PW.into(), None, None, None, Some(kf.clone())), Err(GhulError::WrongPassword)));
    s.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&s, &p), 1);
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}

/// A vault that cannot be sealed (here: the limit is lowered afterwards) makes a change of the
/// protection fail before any file is written: the password before still opens every vault.
#[test]
fn a_change_of_the_protection_writes_nothing_if_one_vault_cannot_be_sealed() {
    let (_d, s, kf, p, q) = two_vaults("r9-save-all");
    let id = s.save_entry(q.clone(), login("Scans", "ich", "x")).unwrap();
    s.add_entry_attachment(q.clone(), id, "scan.bin".into(), vec![7u8; 300 * 1024]).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(Some(LIMIT)));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&s, &p), 1, "P is readable with the password before");
    assert_eq!(count(&s, &q), 1);
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}

/// An import that makes the vault too large adds nothing and leaves the vault savable.
#[test]
fn an_import_over_the_vault_limit_adds_nothing() {
    let (_d, s, _kf, _p, q) = two_vaults("r9-import");
    let mut csv = String::from("name,url,username,password\n");
    for i in 0..4000 {
        csv.push_str(&format!("Konto {i},https://example.org/{i},user{i},Passwort-{i}-abcdefghijklmnop\n"));
    }
    TEST_SEAL_LIMIT.with(|l| l.set(Some(LIMIT)));
    let r = s.import_data(csv.into_bytes(), "export.csv".into(), Some(q.clone()), None, false, None);
    assert!(matches!(r, Err(GhulError::InvalidInput { ref msg }) if msg == VAULT_FULL), "{:?}", r.map(|r| r.entries));
    assert_eq!(count(&s, &q), 0, "nothing added");
    s.save_entry(q.clone(), login("Neu", "ich", "x")).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    assert_eq!(count(&s, &q), 1);
}

/// A calendar file over the limit is refused before it is read; one that makes the vault too
/// large adds nothing.
#[test]
fn a_calendar_import_is_limited_and_goes_again() {
    let d = Dirs::new("r9-ics");
    let s = d.suite();
    s.create(options(PW, Some(generate_keyfile()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    let vault = vault_id(&s, "Kalender");
    let cal = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault,
            name: "Privat".into(),
            color: 0xd6336c,
            default_reminder: 15,
            events: 0,
        })
        .unwrap();
    let zones = || Arc::new(super::super::caltext::FixedOffset(60));
    let big = vec![b' '; MAX_ICS_LEN + 1];
    assert!(matches!(s.cal_import_ics(cal.clone(), big, zones()), Err(GhulError::InvalidInput { .. })));
    let mut ics = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for i in 0..3000 {
        ics.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:{i}@x\r\nSUMMARY:Termin {i} mit einem etwas längeren Titel\r\nDTSTART:20270101T{:02}{:02}00\r\nDTEND:20270101T{:02}{:02}00\r\nEND:VEVENT\r\n",
            (i / 60) % 24,
            i % 60,
            (i / 60) % 24,
            i % 60
        ));
    }
    ics.push_str("END:VCALENDAR\r\n");
    TEST_SEAL_LIMIT.with(|l| l.set(Some(LIMIT)));
    let r = s.cal_import_ics(cal.clone(), ics.into_bytes(), zones());
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    assert!(matches!(r, Err(GhulError::InvalidInput { ref msg }) if msg == VAULT_FULL), "{r:?}");
    assert_eq!(s.cal_calendars().unwrap().iter().find(|c| c.id == cal).unwrap().events, 0);
}

/// r9 N3: a FIDO2 slot of another RP ID (a vault file from elsewhere naming a website) is
/// never asked for: the key gives no answer for that RP with the user's PIN.
#[test]
fn a_slot_of_another_rp_id_is_not_asked_for() {
    use crate::ctap2::{prf_salt, Authenticator};
    use crate::ctaphid::CtapHid;
    use crate::vault::container::{Fido2Registration, KdfCost, SuiteKeys};
    use crate::vault::{ContainerKind, LockedContainer};
    let mut r = rand_core::OsRng;
    let mock = key();
    let keys =
        SuiteKeys::create("pw", KdfCost { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 }, &mut r).unwrap();
    let auth = Authenticator::new(CtapHid::open(mock.as_ref(), None).unwrap()).unwrap();
    let cred = auth.make_credential("bank.example", "Bank", "1234", &[]).unwrap();
    let prf_input = SuiteKeys::new_prf_input(&mut r);
    let out = auth.hmac_secret("bank.example", &cred.id, &prf_salt(&prf_input), "1234").unwrap();
    let first: [u8; 32] = out.output.as_slice().try_into().unwrap();
    let slot = keys.fido2_slot(
        &Fido2Registration { rp_id: "bank.example", credential_id: &cred.id, prf_input, hmac_output: &first },
        &mut r,
    );
    let mut c = keys.create_container(ContainerKind::Pass, vec![slot], &mut r).unwrap();
    let locked = LockedContainer::parse(&c.seal(&mut r).unwrap()).unwrap();
    let r = super::super::suite::key_factor(&locked, &mock, None, "1234");
    assert!(matches!(r, Err(GhulError::KeyNotRegistered)), "{:?}", r.err());
}

/// Files of the storage and the local folder that a save in the middle leaves.
fn save_leftovers(d: &Dirs) -> Vec<String> {
    let mut left: Vec<String> = d.storage().list().unwrap().into_iter().filter(|n| n.ends_with(".prev")).collect();
    if std::path::Path::new(&d.local()).join("save-journal").exists() {
        left.push("save-journal".into());
    }
    left
}

/// r10 N5: writing fails after the first file of a change of the password (the storage went
/// away): the files before are put back at once, the password before opens everything.
#[test]
fn a_failed_write_in_the_middle_of_a_password_change_is_undone() {
    use super::super::suite::WRITES_BEFORE_FAILURE;
    let (d, s, kf, p, q) = two_vaults("r10-write-fails");
    s.save_entry(q.clone(), login("Zweiter", "ich", "x")).unwrap();
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    let r = s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    assert!(r.is_err(), "{r:?}");
    assert!(save_leftovers(&d).is_empty(), "{:?}", save_leftovers(&d));
    s.lock();
    assert!(s.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf.clone())).is_err());
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!((count(&s, &p), count(&s, &q)), (1, 1));
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r10 N5: the app stops right after the first file of a change was written: the next unlock
/// puts the files before back (with a fresh suite object, as after a new start).
#[test]
fn a_stop_in_the_middle_of_a_password_change_is_undone_at_the_next_unlock() {
    use super::super::suite::{STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    let (d, s, kf, p, q) = two_vaults("r10-stop");
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    let r = s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    assert!(r.is_err(), "{r:?}");
    assert!(!save_leftovers(&d).is_empty(), "the journal and the copies wait");
    drop(s);
    let s = d.suite();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert!(save_leftovers(&d).is_empty(), "{:?}", save_leftovers(&d));
    assert_eq!((count(&s, &p), count(&s, &q)), (1, 0));
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r10 M5: moving an entry into a vault that then cannot be saved leaves it where it was.
#[test]
fn a_move_into_a_full_vault_leaves_the_entry_where_it_was() {
    let (_d, s, _kf, p, q) = two_vaults("r10-move");
    let id = s.save_entry(q.clone(), login("Scans", "ich", "x")).unwrap();
    s.add_entry_attachment(q.clone(), id.clone(), "scan.bin".into(), vec![7u8; 150 * 1024]).unwrap();
    let big = s.save_entry(p.clone(), login("Groß", "ich", "x")).unwrap();
    s.add_entry_attachment(p.clone(), big, "scan.bin".into(), vec![7u8; 150 * 1024]).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(Some(LIMIT)));
    let r = s.move_entry(q.clone(), id, p.clone());
    assert!(matches!(r, Err(GhulError::InvalidInput { ref msg }) if msg == VAULT_FULL), "{r:?}");
    assert_eq!((count(&s, &p), count(&s, &q)), (2, 1));
    s.save_entry(p.clone(), login("Neu", "ich", "x")).unwrap();
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    assert_eq!(count(&s, &p), 3);
}

/// r10 M5: a 2FA account that makes the vault too large goes again.
#[test]
fn a_totp_into_a_full_vault_goes_again() {
    let d = Dirs::new("r10-totp");
    let s = d.suite();
    s.create(options(PW, Some(generate_keyfile()), &[(VaultKind::Totp, "Codes")]), None, None, None).unwrap();
    let v = vault_id(&s, "Codes");
    TEST_SEAL_LIMIT.with(|l| l.set(Some(1)));
    let r = s.add_totp(v.clone(), totp_input());
    TEST_SEAL_LIMIT.with(|l| l.set(None));
    assert!(matches!(r, Err(GhulError::InvalidInput { ref msg }) if msg == VAULT_FULL), "{r:?}");
    assert!(s.list_entries(VaultKind::Totp, Some(v.clone())).unwrap().is_empty());
    s.add_totp(v, totp_input()).unwrap();
}

/// r11 H1: a change of the password stopped in the middle, then the suite deleted and a new one
/// made: the journal and the copies of the old suite never come back over the new one.
#[test]
fn a_stopped_save_never_comes_back_after_the_suite_was_deleted() {
    use super::super::suite::{STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    let (d, s, kf, _p, _q) = two_vaults("r11-deleted");
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    assert!(!save_leftovers(&d).is_empty());
    s.delete_working_copy(kf_proof(PW, &kf), None, None).unwrap();
    assert!(save_leftovers(&d).is_empty(), "{:?}", save_leftovers(&d));
    let kf2 = generate_keyfile();
    s.create(options("pw-anders-4321-Q", Some(kf2.clone()), &[(VaultKind::Pass, "Neu")]), None, None, None).unwrap();
    s.lock();
    s.unlock("pw-anders-4321-Q".into(), None, None, None, Some(kf2)).unwrap();
    let names: Vec<String> = s.list_vaults().unwrap().into_iter().map(|v| v.name).collect();
    assert_eq!(names, vec!["Neu".to_owned()]);
}

/// r11 H1: the copies of another suite (the journal left there by hand) put nothing back.
#[test]
fn the_journal_of_another_suite_puts_nothing_back() {
    use super::super::suite::{STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    let (d, s, kf, _p, _q) = two_vaults("r11-other-a");
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    // Another suite in another folder gets these leftovers.
    let (e, t, kf_t, _, _) = two_vaults("r11-other-b");
    t.lock();
    for n in d.storage().list().unwrap().into_iter().filter(|n| n.ends_with(".prev")) {
        e.storage().write(n.clone(), d.storage().read(n).unwrap().unwrap()).unwrap();
    }
    let journal = std::path::Path::new(&d.local()).join("save-journal");
    std::fs::copy(&journal, std::path::Path::new(&e.local()).join("save-journal")).unwrap();
    t.unlock(PW.into(), None, None, None, Some(kf_t)).unwrap();
    assert!(save_leftovers(&e).is_empty());
    assert!(t.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r11 N4: a file only the failed save made goes again when the files before are put back.
#[test]
fn a_file_made_by_a_failed_save_goes_again() {
    use super::super::suite::WRITES_BEFORE_FAILURE;
    let (d, s, kf, p, q) = two_vaults("r11-new-file");
    let q_file = s.list_vaults().unwrap().into_iter().find(|v| v.id == q).map(|v| v.id).unwrap();
    let stored: Vec<String> = d.storage().list().unwrap();
    let q_name = stored.iter().find(|n| n.starts_with(&q_file[..16])).unwrap().clone();
    d.storage().delete(q_name.clone()).unwrap();
    // The vault files are written first, the suite last: the suite fails.
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(2)));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    assert!(d.storage().read(q_name).unwrap().is_none(), "made by the failed save: gone again");
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&s, &p), 1);
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r11 N1, M3: a journal marked done puts nothing back; leftovers of writing a copy go at the
/// next unlock.
#[test]
fn a_done_journal_and_stray_copies_are_only_removed() {
    let (d, s, kf, p, _q) = two_vaults("r11-done");
    let old_suite = d.storage().read("suite.ghlsuite".into()).unwrap().unwrap();
    s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).unwrap();
    std::fs::write(std::path::Path::new(&d.local()).join("save-journal"), b"done").unwrap();
    d.storage().write("suite.ghlsuite.prev".into(), old_suite).unwrap();
    let dir = d.root.join("vault");
    std::fs::write(dir.join("suite.ghlsuite.prev.tmp"), b"x").unwrap();
    s.lock();
    s.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&s, &p), 1);
    assert!(save_leftovers(&d).is_empty());
    assert!(!dir.join("suite.ghlsuite.prev.tmp").exists());
}

/// A storage whose writes wait while `hold` is set (r11 M1: a save in the middle).
struct Slow {
    inner: Arc<dyn VaultStorage>,
    hold: std::sync::Mutex<bool>,
    wake: std::sync::Condvar,
    waiting: std::sync::atomic::AtomicBool,
}

impl VaultStorage for Slow {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, super::super::storage::StorageError> {
        self.inner.read(name)
    }
    fn write(&self, name: String, data: Vec<u8>) -> Result<(), super::super::storage::StorageError> {
        if !name.ends_with(".prev") {
            let mut held = self.hold.lock().unwrap();
            while *held {
                self.waiting.store(true, std::sync::atomic::Ordering::SeqCst);
                held = self.wake.wait(held).unwrap();
            }
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
    fn content_dir(&self) -> Option<String> {
        self.inner.content_dir()
    }
}

/// r11 M1: an unlock while a change of the password writes its files waits for it and puts
/// nothing back: the change holds.
#[test]
fn an_unlock_during_a_save_waits_for_it() {
    let (d, s, kf, p, _q) = two_vaults("r11-race");
    s.lock();
    s.finish_lock();
    let slow = Arc::new(Slow {
        inner: d.storage(),
        hold: std::sync::Mutex::new(false),
        wake: std::sync::Condvar::new(),
        waiting: std::sync::atomic::AtomicBool::new(false),
    });
    let a = GhulSuite::new(slow.clone(), d.local());
    a.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    *slow.hold.lock().unwrap() = true;
    let (a2, kf2) = (a.clone(), kf.clone());
    let change =
        std::thread::spawn(move || a2.change_password(kf_proof(PW, &kf2), None, None, "pw-neu-7890-Xy".into()));
    while !slow.waiting.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::yield_now();
    }
    // The journal is there, the first new file waits. The user locks and unlocks meanwhile.
    a.lock();
    let (a3, kf3) = (a.clone(), kf.clone());
    let unlock = std::thread::spawn(move || a3.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf3)));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!unlock.is_finished(), "waits for the save");
    *slow.hold.lock().unwrap() = false;
    slow.wake.notify_all();
    change.join().unwrap().unwrap();
    unlock.join().unwrap().unwrap();
    assert!(save_leftovers(&d).is_empty());
    let b = &a;
    assert_eq!(count(b, &p), 1);
    assert!(b.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r11 N2: files before an unfinished save that cannot be put back yet give a clear message;
/// the next try puts them back.
#[test]
fn files_that_cannot_be_put_back_yet_give_a_clear_message() {
    use super::super::suite::{SAVE_UNFINISHED, STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    use super::archive::FailOnce;
    let (d, s, kf, p, _q) = two_vaults("r11-unfinished");
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    drop(s);
    let flaky = Arc::new(FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(true) });
    let s = GhulSuite::new(flaky, d.local());
    let r = s.unlock(PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == SAVE_UNFINISHED), "{r:?}");
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&s, &p), 1);
    assert!(save_leftovers(&d).is_empty());
}

/// A storage that stops after the first copy of a save (`.prev`) until `go` (r12 N1).
struct PauseAfterPrev {
    inner: Arc<dyn VaultStorage>,
    armed: std::sync::atomic::AtomicBool,
    paused: std::sync::atomic::AtomicBool,
    go: std::sync::atomic::AtomicBool,
}

impl VaultStorage for PauseAfterPrev {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, super::super::storage::StorageError> {
        self.inner.read(name)
    }
    fn write(&self, name: String, data: Vec<u8>) -> Result<(), super::super::storage::StorageError> {
        let r = self.inner.write(name.clone(), data);
        if name.ends_with(".prev") && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.paused.store(true, std::sync::atomic::Ordering::SeqCst);
            while !self.go.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::yield_now();
            }
        }
        r
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
    fn content_dir(&self) -> Option<String> {
        self.inner.content_dir()
    }
}

/// r12 N1 (probe of the review): tidying up while a save copies its files leaves the copies
/// alone; the save that then fails is put back completely.
#[test]
fn tidying_up_during_a_save_leaves_its_copies() {
    use super::super::suite::{STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    let (d, s, kf, _p, _q) = two_vaults("r12-purge-race");
    s.lock();
    s.finish_lock();
    drop(s);
    let st =
        Arc::new(PauseAfterPrev { inner: d.storage(), armed: false.into(), paused: false.into(), go: false.into() });
    let a = GhulSuite::new(st.clone(), d.local());
    a.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    st.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let (a2, kf2) = (a.clone(), kf.clone());
    let y = std::thread::spawn(move || {
        WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
        STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
        a2.change_password(kf_proof(PW, &kf2), None, None, "pw-neu-7890-Xy".into())
    });
    while !st.paused.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::yield_now();
    }
    // Another call tidies up meanwhile (retry_purge in an unlock, the end of a commit).
    a.purge_old_copies();
    st.go.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(y.join().unwrap().is_err());
    drop(a);
    let b = d.suite();
    b.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(b.list_vaults().unwrap().iter().all(|v| v.available));
    b.lock();
    b.finish_lock();
    assert!(matches!(b.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf)), Err(GhulError::WrongPassword)));
}

/// r12 M1 (probe of the review): an unlock that read the files before a change of the
/// password wrote them does not open what it read (`Locked`, the app unlocks again); the
/// change holds, nothing of the protection before comes back.
#[test]
fn an_unlock_that_read_before_a_change_does_not_open_it() {
    use super::super::suite::LOCK_HOOK;
    let (d, s, kf, p, _q) = two_vaults("r12-stale-open");
    let (s2, kf2) = (s.clone(), kf.clone());
    LOCK_HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let r =
                std::thread::spawn(move || s2.change_password(kf_proof(PW, &kf2), None, None, "pw-neu-7890-Xy".into()))
                    .join()
                    .unwrap();
            assert!(r.is_ok(), "{r:?}");
        }))
    });
    let r = s.unlock(PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::Locked)), "{r:?}");
    // Whatever is open now (the session of the change itself) saves with the new protection.
    let _ = s.save_entry(p.clone(), login("Nachher", "ich", "x"));
    s.lock();
    s.finish_lock();
    drop(s);
    let b = d.suite();
    assert!(matches!(b.unlock(PW.into(), None, None, None, Some(kf.clone())), Err(GhulError::WrongPassword)));
    b.unlock("pw-neu-7890-Xy".into(), None, None, None, Some(kf)).unwrap();
    assert!(count(&b, &p) >= 1);
    assert!(b.list_vaults().unwrap().iter().all(|v| v.available));
}

/// r12 N1: a copy named in the journal that is missing is an error when putting back, not
/// skipped (the file would keep the protection of the failed save).
#[test]
fn a_missing_copy_is_not_skipped() {
    use super::super::suite::{SAVE_UNFINISHED, STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
    let (d, s, kf, _p, _q) = two_vaults("r12-missing-copy");
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    assert!(s.change_password(kf_proof(PW, &kf), None, None, "pw-neu-7890-Xy".into()).is_err());
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    let gone =
        d.storage().list().unwrap().into_iter().find(|n| n.ends_with(".prev") && n != "suite.ghlsuite.prev").unwrap();
    d.storage().delete(gone).unwrap();
    drop(s);
    let b = d.suite();
    let r = b.unlock(PW.into(), None, None, None, Some(kf));
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == SAVE_UNFINISHED), "{r:?}");
}

/// r13 N1 (probe of the review): an unlock that began before the suite was deleted writes
/// nothing back (no suite file, no note of the epoch) and does not open.
#[test]
fn an_unlock_begun_before_a_deletion_writes_nothing_back() {
    use super::super::suite::FOLDER_HOOK;
    let (d, s, kf, _p, _q) = two_vaults("r13-del-unlock");
    s.lock();
    s.finish_lock();
    let (s2, kf2) = (s.clone(), kf.clone());
    FOLDER_HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let r = std::thread::spawn(move || s2.delete_working_copy(kf_proof(PW, &kf2), None, None)).join().unwrap();
            assert!(r.is_ok(), "{r:?}");
        }))
    });
    let r = s.unlock(PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::Locked)), "{r:?}");
    assert!(d.storage().list().unwrap().is_empty(), "{:?}", d.storage().list());
    assert!(!s.exists().unwrap());
    drop(s);
    let b = d.suite();
    assert!(b.unlock(PW.into(), None, None, None, Some(kf)).is_err());
}

/// r13 N2: an unlock that read the working copy before a suite file replaced it does not open
/// the copy before; what is open is the accepted state.
#[test]
fn an_unlock_that_read_before_an_accept_does_not_open_it() {
    use super::super::suite::LOCK_HOOK;
    use super::archive::{device_with_suite, open, save};
    let (_da, a, kf, _) = device_with_suite("r13-accept-a");
    let file = save(&a);
    let db = Dirs::new("r13-accept-b");
    let b = db.suite();
    open(&b, &file, &kf);
    b.archive_accept().unwrap();
    let privat = vault_id(&b, "Privat");
    b.save_entry(privat.clone(), login("Von B", "u", "2")).unwrap();
    let newer = save(&b);
    a.lock();
    a.finish_lock();
    let (a2, kf2) = (a.clone(), kf.clone());
    LOCK_HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            std::thread::spawn(move || {
                open(&a2, &newer, &kf2);
                a2.archive_accept().unwrap();
            })
            .join()
            .unwrap();
        }))
    });
    let r = a.unlock(PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::Locked)), "{r:?}");
    let mut t = titles(&a);
    t.sort();
    assert_eq!(t, vec!["Bank".to_owned(), "Von B".to_owned()], "the accepted state is open");
}

/// r13 N4: after a deletion, a journal left without its suite does not keep the copies.
#[test]
fn copies_of_a_deleted_suite_are_not_kept_for_its_journal() {
    let (d, s, _kf, _p, _q) = two_vaults("r13-journal-no-suite");
    s.lock();
    s.finish_lock();
    std::fs::write(std::path::Path::new(&d.local()).join("save-journal"), b"keep suite.ghlsuite").unwrap();
    d.storage().write("suite.ghlsuite.prev".into(), b"old".to_vec()).unwrap();
    for n in d.storage().list().unwrap().into_iter().filter(|n| !n.ends_with(".prev")) {
        d.storage().delete(n).unwrap();
    }
    s.purge_old_copies();
    assert!(d.storage().list().unwrap().is_empty(), "{:?}", d.storage().list());
}

/// r14 N1: a save that stopped before it wrote the missing suite file still needs its copies:
/// tidying up at the next start must not remove them, the unlock puts the vault back.
#[test]
fn an_open_journal_without_a_suite_file_keeps_its_copies() {
    let (d, s, kf, _p, _q) = two_vaults("r14-journal");
    s.lock();
    s.finish_lock();
    drop(s);
    let st = d.storage();
    let vaults: Vec<String> = st.list().unwrap().into_iter().filter(|n| n != "suite.ghlsuite").collect();
    let v = vaults[0].clone();
    let orig = st.read(v.clone()).unwrap().unwrap();
    st.write(format!("{v}.prev"), orig).unwrap();
    st.write(v.clone(), b"half written".to_vec()).unwrap();
    st.delete("suite.ghlsuite".into()).unwrap();
    let local = std::path::PathBuf::from(d.local());
    std::fs::write(local.join("save-journal"), format!("keep {v}\nnew suite.ghlsuite")).unwrap();
    std::fs::write(local.join("purge-pending"), b"").unwrap();
    let b = d.suite();
    let r = b.unlock(PW.into(), None, None, None, Some(kf));
    assert!(r.is_ok(), "{r:?}");
}

/// Owner wish: the unsaved mark names the areas changed since the last save; a mark of an older
/// version (only its time) gives "unknown".
#[test]
fn the_unsaved_mark_names_the_areas() {
    use super::super::cal::CalCalendar;
    use super::archive::save;
    let d = Dirs::new("unsaved-areas");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf), &[(VaultKind::Pass, "P"), (VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    save(&s);
    assert!(s.archive_unsaved_areas().is_empty());
    s.save_entry(vault_id(&s, "P"), login("Bank", "ich", "x")).unwrap();
    assert_eq!(s.archive_unsaved_areas(), vec!["pass".to_owned()]);
    let since = s.archive_unsaved_since();
    assert!(since > 0);
    s.cal_save_calendar(CalCalendar {
        id: String::new(),
        vault_id: vault_id(&s, "Kalender"),
        name: "Privat".into(),
        color: 0xd6336c,
        default_reminder: 15,
        events: 0,
    })
    .unwrap();
    assert_eq!(s.archive_unsaved_areas(), vec!["cal".to_owned(), "pass".to_owned()]);
    assert_eq!(s.archive_unsaved_since(), since, "the time of the first change stays");
    // A change of the suite itself (a vault removed) is "suite".
    s.remove_vault(vault_id(&s, "Kalender")).unwrap();
    assert_eq!(s.archive_unsaved_areas(), vec!["cal".to_owned(), "pass".to_owned(), "suite".to_owned()]);
    save(&s);
    assert!(s.archive_unsaved_areas().is_empty());
    assert!(!s.archive_unsaved());
    // A mark of an older version: the time alone.
    std::fs::write(std::path::Path::new(&d.local()).join("archive-unsaved"), b"12345").unwrap();
    assert_eq!(s.archive_unsaved_areas(), vec!["unknown".to_owned()]);
    assert_eq!(s.archive_unsaved_since(), 12345);
    s.save_entry(vault_id(&s, "P"), login("Forum", "ich", "y")).unwrap();
    assert_eq!(s.archive_unsaved_areas(), vec!["unknown".to_owned()], "still not known");
}
