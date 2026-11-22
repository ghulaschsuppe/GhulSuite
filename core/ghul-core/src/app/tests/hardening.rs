// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fixes of the review of 21.11.2026 (H3, H5 to H10, H12, H13, M3, M4, M6, M25): each with
//! the case that went wrong before.

use super::super::archive::in_memory::source;
use super::super::archive::{CHANGED_SINCE_CHECK, DOCS_BUSY};
use super::super::suite::{EntryPlacement, FactorProof, CLOCK_SHIFT, LOCK_HOOK};
use super::*;

const PW: &str = "pw-123456-Kq7wZ";
const NEW_PW: &str = "Notfall-neu-7Lq!";

fn alone(password: &str) -> FactorProof {
    no_key_proof(password)
}

/// A suite protected by one key file (no backup), with a login.
fn keyfile_suite(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, String) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    let res = s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "1")).unwrap();
    // No confirmation from creating is left.
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    (d, s, kf, res.recovery_code.unwrap())
}

fn save(s: &GhulSuite) -> Vec<u8> {
    let built = s.archive_build_bytes("Test".into()).unwrap();
    s.archive_saved(built.state, built.change).unwrap();
    built.data
}

/// The `.bak` and `.tmp` copies of containers in the working copy.
fn old_copies(d: &Dirs) -> Vec<String> {
    std::fs::read_dir(d.root.join("vault"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bak") || n.ends_with(".tmp"))
        .collect()
}

/// Locks `s` from another thread at the next point where a running call would open the suite.
fn lock_at_the_hook(s: &Arc<GhulSuite>) {
    let other = s.clone();
    LOCK_HOOK
        .with(|h| *h.borrow_mut() = Some(Box::new(move || std::thread::spawn(move || other.lock()).join().unwrap())));
}

/// H3: key file and backup lost; opened with the recovery code, the new master password alone
/// adds a new key file and key, removes the lost one and makes a new code, until the lock.
#[test]
fn after_the_recovery_code_the_new_password_alone_repairs_the_protection() {
    let (d, s, lost_kf, code) = keyfile_suite("h3");
    let lost_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::Keyfile).unwrap().slot_id;
    s.lock();
    assert!(!s.opened_with_recovery_code());
    s.unlock_with_recovery_code(code.clone(), NEW_PW.into()).unwrap();
    assert!(s.opened_with_recovery_code());
    // A wrong password is still refused.
    assert!(matches!(
        s.add_keyfile(alone("falsch-falsch-1"), None, None, generate_keyfile(), "x".into()),
        Err(GhulError::WrongPassword)
    ));
    let new_kf = generate_keyfile();
    s.add_keyfile(alone(NEW_PW), None, None, new_kf.clone(), "Neue Datei".into()).unwrap();
    let k = key();
    s.add_security_key(alone(NEW_PW), None, k.clone(), listener(), "1234".into(), "Neuer Key".into()).unwrap();
    s.remove_slot(alone(NEW_PW), None, None, lost_slot).unwrap();
    // The other rules hold: the key keeps its backup, the last factor stays.
    let file_slot = s.slots().unwrap().into_iter().find(|x| x.label == "Neue Datei").unwrap().slot_id;
    assert!(matches!(s.remove_slot(alone(NEW_PW), None, None, file_slot), Err(GhulError::BackupFactorRequired)));
    let new_code = s.new_recovery_code(alone(NEW_PW), None, None, "Neues Blatt".into()).unwrap();
    s.change_password(alone(NEW_PW), None, None, "Noch-neuer-9Xq!".into()).unwrap();
    assert!(s.opened_with_recovery_code(), "until the lock");
    assert!(old_copies(&d).is_empty(), "no copy with the old protection: {:?}", old_copies(&d));

    // After the lock the old rule holds again: both factors.
    s.lock();
    assert!(!s.opened_with_recovery_code());
    assert!(matches!(
        s.unlock("Noch-neuer-9Xq!".into(), None, None, None, Some(lost_kf)),
        Err(GhulError::WrongKeyfile)
    ));
    assert!(
        matches!(s.unlock_with_recovery_code(code, NEW_PW.into()), Err(GhulError::WrongSecondFactor)),
        "the old code"
    );
    s.unlock("Noch-neuer-9Xq!".into(), None, None, None, Some(new_kf.clone())).unwrap();
    assert!(!s.opened_with_recovery_code());
    assert_eq!(titles(&s), vec!["Bank"]);
    assert!(matches!(
        s.add_keyfile(alone("Noch-neuer-9Xq!"), None, None, generate_keyfile(), "x".into()),
        Err(GhulError::SecurityKeyRequired)
    ));
    s.lock();
    s.unlock_with_recovery_code(new_code, "Und-wieder-neu-3Zq!".into()).unwrap();
    assert!(s.opened_with_recovery_code());
}

/// H13: every change of the protection with the password alone (no key file, no PIN, no
/// confirmation) is refused, with the error that names the factor.
#[test]
fn every_protection_change_with_the_password_alone_is_refused() {
    let (d, s, kf, code) = keyfile_suite("alone");
    let vid = vault_id(&s, "Privat");
    let slot = s.slots().unwrap()[0].slot_id.clone();
    let need = |r: Result<(), GhulError>, what: &str| {
        assert!(matches!(r, Err(GhulError::KeyfileRequired)), "{what}: {r:?}");
    };
    need(s.add_keyfile(alone(PW), None, None, generate_keyfile(), "x".into()), "add_keyfile");
    need(s.add_security_key(alone(PW), None, key(), listener(), "1234".into(), "x".into()), "add_security_key");
    need(s.new_recovery_code(alone(PW), None, None, "x".into()).map(|_| ()), "new_recovery_code");
    need(s.remove_slot(alone(PW), None, None, slot), "remove_slot");
    need(s.change_password(alone(PW), None, None, "Ganz-anders-5Wq!".into()), "change_password");
    need(s.confirm_factors(alone(PW), None, None), "confirm_factors");
    need(
        s.quick_unlock_enable(alone(PW), None, None, "pin".into(), Some("135790".into()), 3600).map(|_| ()),
        "quick_unlock_enable",
    );
    let opts = super::super::single::ShareVaultOptions {
        vault_id: vid,
        ids: Vec::new(),
        name: "Weiter".into(),
        proof: alone(PW),
        password: "weiter-98765".into(),
        keyfile: Some(generate_keyfile()),
        key_label: "K".into(),
        keyfile_label: "D".into(),
        recovery_label: "B".into(),
    };
    need(s.share_as_vault(opts, None, None, None, None).map(|_| ()), "share_as_vault");
    need(s.delete_working_copy(alone(PW), None, None), "delete_working_copy");
    // Nothing changed.
    assert_eq!(s.slots().unwrap().len(), 2);
    assert!(!s.quick_unlock_exists("pin".into()));
    assert!(s.exists().unwrap());
    s.lock();
    s.unlock_with_recovery_code(code, NEW_PW.into()).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);

    // With a security key the error names the key.
    let d2 = Dirs::new("alone-key");
    let s2 = d2.suite();
    let k = key();
    s2.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s2.add_security_key(kf_proof(PW, &kf), None, k.clone(), listener(), "1234".into(), "Key".into()).unwrap();
    for r in [
        s2.new_recovery_code(alone(PW), None, None, "x".into()).map(|_| ()),
        s2.change_password(alone(PW), Some(k.clone()), Some(listener()), "Ganz-anders-5Wq!".into()),
        s2.add_keyfile(alone(PW), Some(k.clone()), Some(listener()), generate_keyfile(), "x".into()),
    ] {
        assert!(matches!(r, Err(GhulError::SecurityKeyRequired)), "{r:?}");
    }
    drop(d);
}

/// H13: a confirmation of both factors holds two minutes (a test clock moves it on).
#[test]
fn a_confirmation_holds_two_minutes() {
    let (_d, s, kf, _) = keyfile_suite("ticket");
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    CLOCK_SHIFT.with(|c| c.set(std::time::Duration::from_secs(121)));
    let r = s.new_recovery_code(alone(PW), None, None, "x".into());
    CLOCK_SHIFT.with(|c| c.set(std::time::Duration::ZERO));
    assert!(matches!(r, Err(GhulError::KeyfileRequired)), "expired: {r:?}");
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    CLOCK_SHIFT.with(|c| c.set(std::time::Duration::from_secs(119)));
    let r = s.new_recovery_code(alone(PW), None, None, "x".into());
    CLOCK_SHIFT.with(|c| c.set(std::time::Duration::ZERO));
    r.unwrap();
    // Once only.
    assert!(matches!(s.new_recovery_code(alone(PW), None, None, "x".into()), Err(GhulError::KeyfileRequired)));
}

/// H6: the validity of quick unlock is chosen when it is enabled (with both factors) and
/// stored in the file under its encryption: the app cannot extend it.
#[test]
fn quick_unlock_keeps_the_validity_it_was_enabled_with() {
    let (d, s, kf, _) = keyfile_suite("quick-validity");
    let qk = s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "pin".into(), Some("135790".into()), 3600).unwrap();
    assert!(matches!(
        s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "pin".into(), Some("1357".into()), 3600),
        Err(GhulError::InvalidInput { .. })
    ));
    s.lock();
    s.unlock_quick("pin".into(), qk.clone(), Some("135790".into())).unwrap();
    s.lock();
    // Someone with the device changes the stored validity: the file no longer opens.
    let file = std::path::PathBuf::from(d.local()).join("quick-pin.ghlq");
    let mut data = std::fs::read(&file).unwrap();
    data[67..75].copy_from_slice(&i64::MAX.to_le_bytes());
    std::fs::write(&file, &data).unwrap();
    assert!(matches!(s.unlock_quick("pin".into(), qk, Some("135790".into())), Err(GhulError::QuickUnlockInvalid)));
}

/// H10: a lock during an unlock, a confirmation or the taking of a suite file wins.
#[test]
fn a_lock_during_unlocking_wins() {
    let (d, s, kf, _) = keyfile_suite("lock-wins");
    s.lock();
    lock_at_the_hook(&s);
    assert!(matches!(s.unlock(PW.into(), None, None, None, Some(kf.clone())), Err(GhulError::Locked)));
    assert!(!s.is_unlocked());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();

    // A confirmation of both factors is not kept past a lock.
    lock_at_the_hook(&s);
    assert!(matches!(s.confirm_factors(kf_proof(PW, &kf), None, None), Err(GhulError::Locked)));
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(matches!(s.new_recovery_code(alone(PW), None, None, "x".into()), Err(GhulError::KeyfileRequired)));

    // Taking a suite file: replaced, but locked; the state it is was noted all the same.
    let file = save(&s);
    s.save_entry(vault_id(&s, "Privat"), login("Danach", "u", "2")).unwrap();
    let file2 = save(&s);
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    lock_at_the_hook(&s);
    assert!(matches!(s.archive_accept(), Err(GhulError::Locked)));
    assert!(!s.is_unlocked());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&s), vec!["Bank"], "the file was taken");
    let check = s.archive_open_bytes(file2, PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(check.relation, super::super::archive::ArchiveRelation::Newer, "the state taken was noted");
    drop(d);
}

/// M6: taking a suite file is refused while documents are copied, and when the working copy
/// changed after the file was checked (what the user was told would be lost is no longer all).
#[test]
fn taking_a_file_after_a_change_or_during_a_copy_is_refused() {
    let (d, s, kf, _) = keyfile_suite("m6");
    let file = save(&s);
    s.archive_open_bytes(file.clone(), PW.into(), None, None, None, Some(kf.clone())).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Neu", "u", "2")).unwrap();
    let r = s.archive_accept();
    assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == CHANGED_SINCE_CHECK), "{r:?}");
    assert_eq!(titles(&s), vec!["Bank", "Neu"], "nothing replaced");
    assert!(matches!(s.archive_accept(), Err(GhulError::NotFound)), "the checked file was forgotten");
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    {
        let _copy = s.content_copy();
        let r = s.archive_accept();
        assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == DOCS_BUSY), "{r:?}");
    }
    s.archive_accept().unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
    assert!(old_copies(&d).is_empty(), "no copy of the working copy before: {:?}", old_copies(&d));
}

/// H5: another suite's file replaces this device's suite only with this suite's proof; the
/// recovery code of this suite is one.
#[test]
fn another_suite_replaces_this_one_only_with_its_proof() {
    let (_da, a, _kfa, code_a) = keyfile_suite("h5-a");
    let (_db, b, kfb, code_b) = keyfile_suite("h5-b");
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "u", "2")).unwrap();
    let other = save(&b);
    a.lock();
    a.archive_open_bytes(other.clone(), PW.into(), None, None, None, Some(kfb.clone())).unwrap();
    assert!(matches!(a.archive_accept_replacing_with_code(code_b.clone()), Err(GhulError::WrongSecondFactor)));
    assert!(matches!(a.archive_accept_replacing_with_code("AAAAA".into()), Err(GhulError::InvalidRecoveryCode)));
    a.archive_accept_replacing_with_code(code_a.clone()).unwrap();
    assert_eq!(titles(&a), vec!["Bank", "Von B"]);
    // Deleting with the recovery code works too (and needs one of this suite).
    a.lock();
    assert!(matches!(a.delete_working_copy_with_code(code_a), Err(GhulError::WrongSecondFactor)));
    assert!(a.exists().unwrap());
    a.delete_working_copy_with_code(code_b).unwrap();
    assert!(!a.exists().unwrap());
}

/// H7, M4: a change of the protection that fails while saving locks the suite: nothing of the
/// new protection stays in memory, the device does not note an epoch that was never saved.
#[test]
fn a_change_of_the_protection_that_fails_locks_the_suite() {
    let d = Dirs::new("m4");
    let kf = generate_keyfile();
    let s = d.suite();
    let res = s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "1")).unwrap();
    s.lock();
    let flaky =
        Arc::new(super::archive::FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let s = GhulSuite::new(flaky.clone(), d.local());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let epoch = || -> u64 {
        let text =
            std::fs::read_to_string(std::path::PathBuf::from(d.local()).join("archive-seen.json")).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        v.as_object().and_then(|m| m.values().next()).and_then(|e| e["epoch"].as_u64()).unwrap_or(0)
    };
    let before = epoch();
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let r = s.change_password(kf_proof(PW, &kf), None, None, "Ganz-anders-5Wq!".into());
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == "full"), "{r:?}");
    assert!(!s.is_unlocked(), "locked: the new protection is not in memory");
    assert_eq!(epoch(), before, "no epoch noted that was never saved");
    // The suite file still has the protection before; the recovery code opens everything.
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    s.lock();
    s.unlock_with_recovery_code(res.recovery_code.unwrap(), NEW_PW.into()).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
    assert!(old_copies(&d).is_empty(), "{:?}", old_copies(&d));
}

/// H8: tags and folder of an entry (here from an import) stay when an edit leaves them out.
#[test]
fn an_edit_keeps_tags_and_folder() {
    let (_d, s, _, _) = keyfile_suite("h8");
    let v = vault_id(&s, "Privat");
    let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
               Arbeit,,login,GitLab,,,0,https://gitlab.example,ghul,pw1,\n";
    s.import_data(csv.as_bytes().to_vec(), "bitwarden.csv".into(), Some(v.clone()), None, false, None).unwrap();
    let id = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().find(|e| e.title == "GitLab").unwrap().id;
    let mut e = s.get_entry(v.clone(), id.clone()).unwrap();
    assert_eq!(e.placement.as_ref().unwrap().folder.as_deref(), Some("Arbeit"));
    e.placement = Some(EntryPlacement { tags: vec!["dienst".into()], folder: Some("Arbeit".into()) });
    s.save_entry(v.clone(), e).unwrap();
    // An edit that does not show tags and folder.
    let mut e = s.get_entry(v.clone(), id.clone()).unwrap();
    e.placement = None;
    e.fields.push(FieldData { name: "notes".into(), value: FieldValue::Text { value: "neu".into() } });
    s.save_entry(v.clone(), e).unwrap();
    let e = s.get_entry(v.clone(), id.clone()).unwrap();
    assert_eq!(e.placement, Some(EntryPlacement { tags: vec!["dienst".into()], folder: Some("Arbeit".into()) }));
    // Cleared on purpose.
    let mut e = e;
    e.placement = Some(EntryPlacement::default());
    s.save_entry(v.clone(), e).unwrap();
    assert_eq!(s.get_entry(v, id).unwrap().placement, Some(EntryPlacement::default()));
}

/// H9: an event keeps the fields a newer version added, when it is edited and when it moves to
/// a calendar in another vault (with its `extra` as well).
#[test]
fn events_keep_what_this_version_does_not_know() {
    use super::super::cal::{CalCalendar, CalEvent};
    use crate::vault::Field;
    let d = Dirs::new("h9");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options(PW, Some(kf), &[(VaultKind::Cal, "K1"), (VaultKind::Cal, "K2")]), None, None, None).unwrap();
    let cal = |vault: &str| {
        s.cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, vault),
            name: vault.into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap()
    };
    let (c1, c2) = (cal("K1"), cal("K2"));
    let id = s
        .cal_save_event(CalEvent {
            calendar_id: c1.clone(),
            title: "Termin".into(),
            start: "2026-12-01T10:00".into(),
            end: "2026-12-01T11:00".into(),
            ..Default::default()
        })
        .unwrap();
    let k1 = vault_id(&s, "K1");
    s.with_open(|o| {
        let (_, c) = o.vault_mut(&k1)?;
        let e = c.payload.entries.iter_mut().find(|e| crate::kdf::hex(&e.id) == id).unwrap();
        e.fields.insert("video_link".into(), Field::Url("https://meet.example/x".into()));
        e.extra.insert("future".into(), ciborium::value::Value::Integer(7.into()));
        Ok(())
    })
    .unwrap();
    let known = |s: &GhulSuite, vault: &str, title: &str| -> (Option<Field>, Option<ciborium::value::Value>) {
        let vid = vault_id(s, vault);
        s.with_open(|o| {
            let (_, c) = o.vault_mut(&vid)?;
            let e = c
                .payload
                .entries
                .iter()
                .find(|e| !e.deleted && matches!(e.fields.get("title"), Some(Field::Text(t)) if t == title))
                .unwrap();
            Ok((e.fields.get("video_link").cloned(), e.extra.get("future").cloned()))
        })
        .unwrap()
    };
    let mut ev = s.cal_get_event(id.clone()).unwrap();
    ev.title = "Termin neu".into();
    s.cal_save_event(ev).unwrap();
    let (f, x) = known(&s, "K1", "Termin neu");
    assert_eq!(f, Some(Field::Url("https://meet.example/x".into())), "kept on edit");
    assert_eq!(x, Some(ciborium::value::Value::Integer(7.into())));
    let mut ev = s.cal_get_event(id).unwrap();
    ev.calendar_id = c2;
    ev.title = "Verschoben".into();
    s.cal_save_event(ev).unwrap();
    let (f, x) = known(&s, "K2", "Verschoben");
    assert_eq!(f, Some(Field::Url("https://meet.example/x".into())), "kept on the move");
    assert_eq!(x, Some(ciborium::value::Value::Integer(7.into())), "extra kept on the move");
}

/// M3: when taking a file fails and putting the state before back fails as well, the journal
/// stays: the next start puts the state before back.
#[test]
fn a_failed_rollback_keeps_the_journal() {
    let (d, a, kf, _) = keyfile_suite("m3");
    let file = save(&a);
    a.save_entry(vault_id(&a, "Privat"), login("Neu", "u", "2")).unwrap();
    a.lock();
    let flaky =
        Arc::new(super::archive::FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let s = GhulSuite::new(flaky.clone(), d.local());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    super::super::archive::ROLLBACK_FAILS.with(|c| c.set(true));
    assert!(matches!(s.archive_accept(), Err(GhulError::Storage { msg }) if msg == "full"));
    super::super::archive::ROLLBACK_FAILS.with(|c| c.set(false));
    let local = std::path::PathBuf::from(d.local());
    assert!(local.join("replace-journal.json").exists(), "the journal stays");
    assert!(local.join("replace-old").is_dir(), "with the copies of the state before");
    // Round 5, M1: in the same run, nothing unlocks the half replaced state: putting the state
    // before back comes first; while that fails, the unlock is refused and nothing is deleted.
    super::super::archive::RECOVER_FAILS.with(|c| c.set(true));
    let r = s.unlock(PW.into(), None, None, None, Some(kf.clone()));
    super::super::archive::RECOVER_FAILS.with(|c| c.set(false));
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == super::super::archive::REPLACE_UNFINISHED), "{r:?}");
    assert!(local.join("replace-journal.json").exists() && local.join("replace-old").is_dir(), "nothing deleted");
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"], "the state before is back, in the same run");
    assert!(!local.join("replace-journal.json").exists());
    drop(s);
    let s = d.suite();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"]);
}

/// H13: a device that goes off while a newer file with a new GhulPass vault and a new GhulDocs
/// vault replaces the working copy: at the next start neither of them is there.
#[test]
fn going_off_while_new_vaults_come_in_leaves_the_state_before() {
    use super::super::archive::CRASH_AFTER_WRITES;
    let (_da, a, kf, _) = keyfile_suite("journal-new-a");
    let first = save(&a);
    let db = Dirs::new("journal-new-b");
    let b = db.suite();
    b.archive_open_bytes(first, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    b.archive_accept().unwrap();
    let pass2 = a.create_vault(VaultKind::Pass, "Zweiter".into()).unwrap();
    a.save_entry(pass2, login("Im zweiten", "u", "3")).unwrap();
    let docs = a.create_vault(VaultKind::Docs, "Dokumente".into()).unwrap();
    a.docs_add_file(docs.clone(), None, "neu.txt".into(), "text/plain".into(), source(b"neu".to_vec()), None).unwrap();
    let newer = save(&a);
    let mut crash_at = 0;
    loop {
        b.archive_open_bytes(newer.clone(), PW.into(), None, None, None, Some(kf.clone())).unwrap();
        CRASH_AFTER_WRITES.with(|c| c.set(Some(crash_at)));
        let r = b.archive_accept();
        let crashed = CRASH_AFTER_WRITES.with(|c| c.get()).is_none() && r.is_err();
        CRASH_AFTER_WRITES.with(|c| c.set(None));
        if !crashed {
            r.unwrap();
            assert_eq!(b.list_vaults().unwrap().len(), 3);
            break;
        }
        let b2 = db.suite();
        b2.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
        let names: Vec<String> = b2.list_vaults().unwrap().into_iter().map(|v| v.name).collect();
        assert_eq!(names, vec!["Privat".to_string()], "after {crash_at}");
        let docs_dirs = std::fs::read_dir(db.root.join("vault"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("docs-"))
            .count();
        assert_eq!(docs_dirs, 0, "no documents of the new vault after {crash_at}");
        b2.lock();
        b.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
        crash_at += 1;
    }
    assert!(crash_at >= 2, "the new vaults were written one by one");
}

/// H12: no room for the documents of a suite file: a clear error before anything is staged;
/// a staging folder an app left behind goes at the next staging, at once.
#[test]
fn no_room_is_said_before_staging() {
    use super::super::error::TEST_FREE_SPACE;
    let d = Dirs::new("h12");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Docs, "Dokumente")]), None, None, None).unwrap();
    let docs = vault_id(&s, "Dokumente");
    s.docs_add_file(docs, None, "a.bin".into(), "application/octet-stream".into(), source(vec![7u8; 200_000]), None)
        .unwrap();
    let file = save(&s);
    let left = d.root.join("vault").join(".staging").join("left-behind");
    std::fs::create_dir_all(&left).unwrap();
    TEST_FREE_SPACE.with(|c| c.set(Some(1000)));
    let r = s.archive_open_bytes(file.clone(), PW.into(), None, None, None, Some(kf.clone()));
    TEST_FREE_SPACE.with(|c| c.set(None));
    assert!(matches!(r, Err(GhulError::NoSpace { free: 1000, needed }) if needed > 200_000), "{r:?}");
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    assert!(!left.exists(), "a staging folder of an app that ended goes at once");
    // A full storage says so, from the system and from the app's storage.
    let full = std::io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 });
    assert!(matches!(GhulError::from(full), GhulError::NoSpace { .. }));
    // By the code, whatever the language of the message.
    let code = if cfg!(windows) { 112 } else { 28 };
    for msg in [
        format!("Auf dem Datenträger ist nicht genügend Speicherplatz vorhanden. (os error {code})"),
        "write failed: ENOSPC (No space left on device)".to_owned(),
    ] {
        let e = super::super::storage::StorageError::Io { msg: msg.clone() };
        assert!(matches!(GhulError::from(e), GhulError::NoSpace { .. }), "{msg}");
    }
    let e = super::super::storage::StorageError::Io { msg: "No space left on device".into() };
    assert!(matches!(GhulError::from(e), GhulError::Storage { .. }), "words alone say nothing");
    let e = super::super::storage::StorageError::Io { msg: "permission denied (os error 13)".into() };
    assert!(matches!(GhulError::from(e), GhulError::Storage { .. }));
}

/// M25: folders nest at most DOCS_MAX_DEPTH deep, also by moving.
#[test]
fn folders_nest_only_so_deep() {
    use super::super::docs::FOLDERS_TOO_DEEP;
    let d = Dirs::new("m25");
    let s = d.suite();
    s.create(options(PW, Some(generate_keyfile()), &[(VaultKind::Docs, "D")]), None, None, None).unwrap();
    let v = vault_id(&s, "D");
    let mut parent: Option<String> = None;
    for i in 0..super::super::docs::DOCS_MAX_DEPTH {
        parent = Some(s.docs_create_folder(v.clone(), parent.clone(), format!("f{i}")).unwrap().id);
    }
    let r = s.docs_create_folder(v.clone(), parent.clone(), "zu tief".into());
    assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == FOLDERS_TOO_DEEP), "{r:?}");
    let top = s.docs_create_folder(v.clone(), None, "oben".into()).unwrap().id;
    s.docs_create_folder(v.clone(), Some(top.clone()), "drin".into()).unwrap();
    let r = s.docs_move(v.clone(), top, parent);
    assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == FOLDERS_TOO_DEEP), "{r:?}");
}

/// M10: a suite file that ends early says so; something that is not even a first entry stays
/// "not a GhulSuite file".
#[test]
fn a_cut_suite_file_is_incomplete() {
    use super::super::archive::INCOMPLETE_FILE;
    let (_d, s, kf, _) = keyfile_suite("m10");
    let file = save(&s);
    for cut in [file.len() / 2, file.len() - 10, 5000] {
        let r = s.archive_open_bytes(file[..cut].to_vec(), PW.into(), None, None, None, Some(kf.clone()));
        assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == INCOMPLETE_FILE), "cut at {cut}: {r:?}");
    }
    for cut in [0, 3, 20] {
        let r = s.archive_open_bytes(file[..cut].to_vec(), PW.into(), None, None, None, Some(kf.clone()));
        assert!(
            matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "not a GhulSuite file"),
            "cut at {cut}: {r:?}"
        );
    }
}

/// H7 (review): copies that cannot be removed right now (another program holds them) never make
/// the change of the protection fail; they go at the next unlock.
#[test]
fn a_copy_that_cannot_be_removed_does_not_fail_the_change() {
    use super::super::suite::{PURGE_FAILS, PURGE_PENDING};
    let (d, s, kf, _) = keyfile_suite("purge-later");
    PURGE_FAILS.with(|c| c.set(true));
    let r = s.change_password(kf_proof(PW, &kf), None, None, "Ganz-anders-5Wq!".into());
    PURGE_FAILS.with(|c| c.set(false));
    r.unwrap();
    assert!(s.is_unlocked(), "the change counts");
    assert!(!old_copies(&d).is_empty(), "the copies are still there");
    let mark = std::path::PathBuf::from(d.local()).join(PURGE_PENDING);
    assert!(mark.exists(), "noted for later");
    s.lock();
    s.unlock("Ganz-anders-5Wq!".into(), None, None, None, Some(kf)).unwrap();
    assert!(old_copies(&d).is_empty(), "gone at the next unlock: {:?}", old_copies(&d));
    assert!(!mark.exists());
}

/// H6 (review): a quick unlock file of the older kind says that it must be set up again.
#[test]
fn an_older_quick_unlock_file_asks_to_set_it_up_again() {
    let (d, s, kf, _) = keyfile_suite("quick-v2");
    let qk = s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "bio".into(), None, 3600).unwrap();
    s.lock();
    let file = std::path::PathBuf::from(d.local()).join("quick-bio.ghlq");
    let mut data = std::fs::read(&file).unwrap();
    data[8..10].copy_from_slice(&2u16.to_le_bytes());
    data.truncate(data.len() - 8);
    std::fs::write(&file, &data).unwrap();
    assert!(matches!(s.unlock_quick("bio".into(), qk.clone(), None), Err(GhulError::QuickUnlockOutdated)));
    assert!(!s.quick_unlock_exists("bio".into()), "never used again");
    assert_eq!(GhulError::QuickUnlockOutdated.to_string(), "quick unlock must be set up again");
}

/// H12 (review): the estimate of the suite file is never smaller than the file written.
#[test]
fn the_estimate_of_the_suite_file_is_not_too_small() {
    let d = Dirs::new("estimate");
    let s = d.suite();
    s.create(
        options(PW, Some(generate_keyfile()), &[(VaultKind::Pass, "P"), (VaultKind::Docs, "D")]),
        None,
        None,
        None,
    )
    .unwrap();
    for i in 0..50 {
        s.save_entry(vault_id(&s, "P"), login(&format!("Konto {i}"), "u", &"x".repeat(200))).unwrap();
    }
    let docs = vault_id(&s, "D");
    for (i, size) in [10usize, 70_000, 300_000].iter().enumerate() {
        s.docs_add_file(
            docs.clone(),
            None,
            format!("d{i}.bin"),
            "application/octet-stream".into(),
            source(vec![1u8; *size]),
            None,
        )
        .unwrap();
    }
    let estimate = s.archive_estimated_size().unwrap();
    let size = s.archive_build_bytes("Test".into()).unwrap().data.len() as u64;
    assert!(estimate >= size, "{estimate} < {}", size);
    assert!(estimate < size * 3 + 1_000_000, "not far off: {estimate} for {}", size);
}

/// M6 (review): a change of the settings only (no vault written) after the check does not refuse
/// the file; a checked file whose documents went missing stays for another try.
#[test]
fn only_a_change_of_the_content_refuses_taking_a_file() {
    let (_d, s, kf, code) = keyfile_suite("m6-settings");
    let file = save(&s);
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    // A write of the suite file alone (here: a group of the recovery code typed back).
    assert!(s.confirm_recovery_code(0, code.split('-').next().unwrap().to_owned()).unwrap());
    s.archive_accept().unwrap();
}

/// Staging (review): a staging folder of another process that still runs is not removed; one of
/// a process that ended is.
#[cfg(unix)]
#[test]
fn a_staging_folder_of_a_running_process_stays() {
    let d = Dirs::new("staging-owner");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Docs, "D")]), None, None, None).unwrap();
    s.docs_add_file(vault_id(&s, "D"), None, "a.txt".into(), "text/plain".into(), source(b"a".to_vec()), None).unwrap();
    let file = save(&s);
    let staging = d.root.join("vault").join(".staging");
    let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    let running = staging.join("of-a-running-process");
    std::fs::create_dir_all(&running).unwrap();
    std::fs::write(running.join("OWNER"), child.id().to_string()).unwrap();
    let ended = staging.join("of-an-ended-process");
    std::fs::create_dir_all(&ended).unwrap();
    std::fs::write(ended.join("OWNER"), u32::MAX.to_string()).unwrap();
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    s.archive_cancel();
    let _ = child.kill();
    let _ = child.wait();
    assert!(running.exists(), "another process may still use it");
    assert!(!ended.exists());
}

/// Round 5: a `.bak` that is a hard link to the live file (a write whose rename failed) is only
/// removed by the clean-up, never overwritten: the live file stays as it is.
#[cfg(unix)]
#[test]
fn a_hard_linked_copy_is_removed_not_overwritten() {
    use super::super::suite::PURGE_PENDING;
    let (d, s, kf, _) = keyfile_suite("hardlink-bak");
    s.lock();
    let vault = d.root.join("vault");
    let live = std::fs::read_dir(&vault)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "ghlpss"))
        .unwrap();
    let bak = std::path::PathBuf::from(format!("{}.bak", live.display()));
    let _ = std::fs::remove_file(&bak);
    std::fs::hard_link(&live, &bak).unwrap();
    let before = std::fs::read(&live).unwrap();
    std::fs::write(std::path::PathBuf::from(d.local()).join(PURGE_PENDING), b"").unwrap();
    let s = d.suite();
    assert!(!bak.exists(), "the copy is gone");
    assert_eq!(std::fs::read(&live).unwrap(), before, "the live file is untouched");
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
}

/// Round 5: with a damaged suite file the vault files stand in: the suite can be deleted with
/// their proof (and not without it).
#[test]
fn a_damaged_suite_file_does_not_block_deleting() {
    let (d, s, kf, _) = keyfile_suite("damaged-suite-file");
    s.lock();
    std::fs::write(d.root.join("vault").join("suite.ghlsuite"), vec![0u8; 4096]).unwrap();
    assert!(matches!(
        s.delete_working_copy(kf_proof(PW, &generate_keyfile()), None, None),
        Err(GhulError::WrongKeyfile)
    ));
    assert!(s.exists().unwrap());
    s.delete_working_copy(kf_proof(PW, &kf), None, None).unwrap();
    assert!(!s.exists().unwrap());
}

/// Round 5 (r5-s2 #10): a deletion that began and could not end says so with its own error;
/// every other error means nothing was deleted. Deleting again removes the rest.
#[test]
fn a_deletion_that_did_not_end_says_so() {
    use super::super::archive::DELETE_FAILS;
    let (_d, s, kf, _) = keyfile_suite("delete-incomplete");
    s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "pin".into(), Some("135790".into()), 3600).unwrap();
    assert!(matches!(
        s.delete_working_copy(kf_proof("falsch-falsch-1", &kf), None, None),
        Err(GhulError::WrongPassword)
    ));
    assert!(s.quick_unlock_exists("pin".into()), "nothing deleted");
    DELETE_FAILS.with(|c| c.set(true));
    let r = s.delete_working_copy(kf_proof(PW, &kf), None, None);
    DELETE_FAILS.with(|c| c.set(false));
    assert!(matches!(&r, Err(GhulError::DeleteIncomplete { .. })), "{r:?}");
    assert!(!s.quick_unlock_exists("pin".into()), "this device's data is gone");
    assert!(s.exists().unwrap(), "the files are still there");
    s.delete_working_copy(kf_proof(PW, &kf), None, None).unwrap();
    assert!(!s.exists().unwrap());
}

/// Round 5, M2: the folder of the old documents of a replace is tidied once its journal is gone
/// (a stop between the end of the journal and its removal).
#[test]
fn old_documents_of_a_finished_replace_are_tidied() {
    let d = Dirs::new("aside-left");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Docs, "D")]), None, None, None).unwrap();
    s.docs_add_file(vault_id(&s, "D"), None, "a.txt".into(), "text/plain".into(), source(b"a".to_vec()), None).unwrap();
    let file = save(&s);
    let staging = d.root.join("vault").join(".staging");
    let journal = std::path::PathBuf::from(d.local()).join("replace-journal.json");
    let aside = staging.join("old-documents");
    std::fs::create_dir_all(aside.join("docs-x")).unwrap();
    std::fs::write(aside.join("JOURNAL"), b"replace-journal.json").unwrap();
    // Staging never removes it (only the suite knows its journal), also without a journal.
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    s.archive_cancel();
    assert!(aside.exists());
    // The suite: kept while a journal is there, gone at the next start without one.
    std::fs::write(&journal, b"{\"old_files\":[],\"old_docs\":[],\"aside\":null}").unwrap();
    s.tidy_asides();
    assert!(aside.exists());
    std::fs::remove_file(&journal).unwrap();
    drop(s);
    let _s = d.suite();
    assert!(!aside.exists(), "gone without its journal");
}

/// Round 5: the new password of opening with the code is checked before the file is read.
#[test]
fn opening_with_the_code_checks_the_new_password_first() {
    let (_d, s, _, code) = keyfile_suite("code-weak");
    let file = save(&s);
    let r = s.archive_open_with_code_bytes(file, code, "kurz".into());
    assert!(
        matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == super::super::tools::PASSWORD_TOO_SHORT),
        "{r:?}"
    );
}

/// Round 5: a call that panics while it holds the open suite locks it (the core unwinds now).
#[test]
fn a_panic_while_the_suite_is_held_locks_it() {
    let (_d, s, kf, _) = keyfile_suite("poison");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = s.with_open(|_| -> Result<(), GhulError> { panic!("test") });
    }));
    assert!(r.is_err());
    assert!(!s.is_unlocked());
    assert!(matches!(s.list_vaults(), Err(GhulError::Locked)));
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
}

/// Round 5: a series with more exceptions than allowed (an older version had no limit) can be
/// edited: the oldest exceptions go.
#[test]
fn a_series_with_too_many_exceptions_can_be_edited() {
    use super::super::cal::{CalCalendar, CalEvent, CalRepeat};
    let d = Dirs::new("many-exceptions");
    let s = d.suite();
    s.create(options(PW, Some(generate_keyfile()), &[(VaultKind::Cal, "K")]), None, None, None).unwrap();
    let cal = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, "K"),
            name: "K".into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap();
    let mut day = crate::app::caldate::Date::parse("2020-01-02").unwrap();
    let mut exceptions = Vec::new();
    for _ in 0..1005 {
        exceptions.push(day.to_string());
        day = day.add_days(1);
    }
    let id = s
        .cal_save_event(CalEvent {
            calendar_id: cal,
            title: "Täglich".into(),
            start: "2020-01-01T08:00".into(),
            end: "2020-01-01T09:00".into(),
            repeat: CalRepeat::Daily,
            exceptions: exceptions.clone(),
            ..Default::default()
        })
        .unwrap();
    let ev = s.cal_get_event(id).unwrap();
    assert_eq!(ev.exceptions.len(), 1000);
    assert_eq!(ev.exceptions.first(), Some(&exceptions[5]), "the oldest went");
}

/// Round 6, M1: deleting the suite ends a replace journal that could not be finished; a suite
/// created afterwards is never overwritten by the deleted one.
#[test]
fn a_new_suite_after_deleting_is_not_overwritten_by_an_old_journal() {
    let (d, a, kf, _) = keyfile_suite("journal-delete");
    let file = save(&a);
    a.save_entry(vault_id(&a, "Privat"), login("Neu", "u", "2")).unwrap();
    a.lock();
    let flaky =
        Arc::new(super::archive::FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let s = GhulSuite::new(flaky.clone(), d.local());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    super::super::archive::ROLLBACK_FAILS.with(|c| c.set(true));
    assert!(s.archive_accept().is_err());
    super::super::archive::ROLLBACK_FAILS.with(|c| c.set(false));
    let local = std::path::PathBuf::from(d.local());
    assert!(local.join("replace-journal.json").exists());
    s.delete_working_copy(kf_proof(PW, &kf), None, None).unwrap();
    assert!(
        !local.join("replace-journal.json").exists() && !local.join("replace-old").exists(),
        "the journal went with it"
    );
    drop(s);
    let s = d.suite();
    let kf2 = generate_keyfile();
    s.create(options(PW, Some(kf2.clone()), &[(VaultKind::Pass, "Ganz neu")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Ganz neu"), login("Erster", "u", "1")).unwrap();
    s.lock();
    drop(s);
    let s = d.suite();
    s.unlock(PW.into(), None, None, None, Some(kf2)).unwrap();
    assert_eq!(titles(&s), vec!["Erster"], "the new suite, unchanged");
}

/// Round 6, M1: a journal left in an empty folder (its suite is gone) is dropped when a new suite
/// is created there.
#[test]
fn creating_a_suite_drops_a_journal_without_a_suite() {
    let d = Dirs::new("journal-create");
    let local = std::path::PathBuf::from(d.local());
    let s = d.suite();
    // Left behind after the folder of its suite was emptied by hand.
    std::fs::create_dir_all(local.join("replace-old")).unwrap();
    std::fs::write(local.join("replace-old").join("suite.ghlsuite"), b"old").unwrap();
    std::fs::write(
        local.join("replace-journal.json"),
        b"{\"old_files\":[\"suite.ghlsuite\"],\"old_docs\":[],\"aside\":null}",
    )
    .unwrap();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    assert!(!local.join("replace-journal.json").exists() && !local.join("replace-old").exists());
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.list_vaults().unwrap()[0].name, "P", "the new suite");
}

/// Round 6, M4: a damaged suite file still refuses unlocking (a vault as anchor would make a new
/// suite file without the protection epoch); only the proof before deleting passes over it.
#[test]
fn a_damaged_suite_file_still_refuses_unlocking() {
    let (d, s, kf, _) = keyfile_suite("damaged-unlock");
    s.lock();
    std::fs::write(d.root.join("vault").join("suite.ghlsuite"), vec![0u8; 4096]).unwrap();
    let r = s.unlock(PW.into(), None, None, None, Some(kf));
    assert!(matches!(r, Err(GhulError::Corrupted { .. })), "{r:?}");
}

/// Round 6, M2: the folder of the old documents is found by its name under the content root of
/// now (a portable data folder may come back under another drive letter); if it is gone, the
/// journal stays and nothing is changed.
#[test]
fn a_journal_finds_its_old_documents_by_name() {
    let d = Dirs::new("journal-relative");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Docs, "D")]), None, None, None).unwrap();
    s.lock();
    drop(s);
    let vault = d.root.join("vault");
    let local = std::path::PathBuf::from(d.local());
    // As written before a move of the data folder: the absolute path of another drive.
    let aside = vault.join(".staging").join("old-docs");
    std::fs::create_dir_all(aside.join("docs-keep")).unwrap();
    std::fs::write(aside.join("docs-keep").join("x.ghlblob"), b"old").unwrap();
    std::fs::write(aside.join("JOURNAL"), b"replace-journal.json").unwrap();
    let old = std::fs::read(vault.join("suite.ghlsuite")).unwrap();
    std::fs::create_dir_all(local.join("replace-old")).unwrap();
    std::fs::write(local.join("replace-old").join("suite.ghlsuite"), &old).unwrap();
    let journal =
        |aside: &str| format!("{{\"old_files\":[\"suite.ghlsuite\"],\"old_docs\":[\"docs-keep\"],\"aside\":{aside}}}");
    std::fs::write(local.join("replace-journal.json"), journal("\"X:/Daten/vault/.staging/old-docs\"")).unwrap();
    let s = d.suite();
    assert!(vault.join("docs-keep").join("x.ghlblob").exists(), "put back from where it is now");
    assert!(!local.join("replace-journal.json").exists());
    drop(s);
    // Gone: the journal stays, nothing is removed.
    std::fs::create_dir_all(local.join("replace-old")).unwrap();
    std::fs::write(local.join("replace-old").join("suite.ghlsuite"), &old).unwrap();
    std::fs::write(local.join("replace-journal.json"), journal("\"missing\"")).unwrap();
    let s = d.suite();
    assert!(local.join("replace-journal.json").exists(), "the journal stays");
    let r = s.unlock(PW.into(), None, None, None, Some(kf));
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == super::super::archive::REPLACE_UNFINISHED), "{r:?}");
}

/// A storage whose deleting fails while `armed` (a file held by another program).
struct FailDelete {
    inner: Arc<dyn VaultStorage>,
    armed: std::sync::atomic::AtomicBool,
}

impl VaultStorage for FailDelete {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, super::super::storage::StorageError> {
        self.inner.read(name)
    }
    fn write(&self, name: String, data: Vec<u8>) -> Result<(), super::super::storage::StorageError> {
        self.inner.write(name, data)
    }
    fn delete(&self, name: String) -> Result<(), super::super::storage::StorageError> {
        if self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(super::super::storage::StorageError::Io { msg: "in use".into() });
        }
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

/// M8: removing a vault whose file cannot be deleted changes nothing: it stays listed and goes
/// into the next suite file, as on the disk.
#[test]
fn a_vault_whose_file_cannot_be_removed_stays() {
    let (d, s, kf, _) = keyfile_suite("remove-vault-fails");
    s.create_vault(VaultKind::Pass, "Zweiter".into()).unwrap();
    s.lock();
    let flaky = Arc::new(FailDelete { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let s = GhulSuite::new(flaky.clone(), d.local());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let id = vault_id(&s, "Zweiter");
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let r = s.remove_vault(id.clone());
    assert!(matches!(&r, Err(GhulError::Storage { msg }) if msg == super::super::suite::VAULT_NOT_REMOVED), "{r:?}");
    assert!(s.list_vaults().unwrap().iter().any(|v| v.id == id), "still listed");
    let built = s.archive_build_bytes("Test".into()).unwrap();
    let names: Vec<String> = {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(built.data)).unwrap();
        (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect()
    };
    let file = s.list_vaults().unwrap().into_iter().find(|v| v.id == id).unwrap().file;
    assert!(names.contains(&file), "in the next suite file: {names:?}");
    flaky.armed.store(false, std::sync::atomic::Ordering::SeqCst);
    s.remove_vault(id.clone()).unwrap();
    assert!(!s.list_vaults().unwrap().iter().any(|v| v.id == id));
}
