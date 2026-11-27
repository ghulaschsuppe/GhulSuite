// SPDX-License-Identifier: AGPL-3.0-or-later
//! Changing the second factors in one step (`change_factors`): replacing a key file or a
//! key, switching between key and key file, the rules, and a stop in the middle.

use super::super::factors::NewFactor;
use super::super::suite::{SlotInfo, STOP_AFTER_FAILED_WRITE, WRITES_BEFORE_FAILURE};
use super::archive::save;
use super::*;

fn slot_of(s: &GhulSuite, kind: SlotKind) -> Vec<SlotInfo> {
    s.slots().unwrap().into_iter().filter(|x| x.kind == kind).collect()
}

fn with_keyfile(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "P"), login("Bank", "ich", "x")).unwrap();
    (d, s, kf)
}

/// A key file replaced by a new one, proven with the old one: the old one opens neither the
/// working copy nor a suite file saved now; the new one does.
#[test]
fn a_keyfile_is_replaced_proven_with_itself() {
    let (_d, s, old) = with_keyfile("factors-kf");
    let new = generate_keyfile();
    let old_slot = slot_of(&s, SlotKind::Keyfile)[0].slot_id.clone();
    s.change_factors(
        kf_proof(PW, &old),
        None,
        Some(NewFactor::Keyfile { keyfile: new.clone() }),
        None,
        None,
        "Neuer Stick".into(),
        vec![old_slot],
    )
    .unwrap();
    let kfs = slot_of(&s, SlotKind::Keyfile);
    assert_eq!((kfs.len(), kfs[0].label.as_str()), (1, "Neuer Stick"));
    assert_eq!(slot_of(&s, SlotKind::RecoveryCode).len(), 1, "the code stays");
    let file = save(&s);
    s.lock();
    assert!(matches!(s.unlock(PW.into(), None, None, None, Some(old.clone())), Err(GhulError::WrongKeyfile)));
    s.unlock(PW.into(), None, None, None, Some(new.clone())).unwrap();
    assert_eq!(titles(&s), vec!["Bank".to_owned()]);
    // The suite file saved now: the same.
    let e = Dirs::new("factors-kf-other");
    let t = e.suite();
    assert!(t.archive_open_bytes(file.clone(), PW.into(), None, None, None, Some(old)).is_err());
    t.archive_open_bytes(file, PW.into(), None, None, None, Some(new)).unwrap();
}

/// A key replaced by another key (the key file proves and stays as the backup).
#[test]
fn a_key_is_replaced_by_another() {
    let (_d, s, kf) = with_keyfile("factors-key");
    let (a, b) = (key(), key());
    s.add_security_key(kf_proof(PW, &kf), None, a.clone(), listener(), "1234".into(), "Key A".into()).unwrap();
    let a_slot = slot_of(&s, SlotKind::SecurityKey)[0].slot_id.clone();
    s.change_factors(
        kf_proof(PW, &kf),
        None,
        Some(NewFactor::SecurityKey { pin: "1234".into() }),
        Some(b.clone()),
        Some(listener()),
        "Key B".into(),
        vec![a_slot],
    )
    .unwrap();
    let keys = slot_of(&s, SlotKind::SecurityKey);
    assert_eq!((keys.len(), keys[0].label.as_str()), (1, "Key B"));
    s.lock();
    assert!(s.unlock(PW.into(), Some(a), Some(listener()), Some("1234".into()), None).is_err());
    s.unlock(PW.into(), Some(b), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
}

/// Switching: two keys become one key file; then the key file becomes a key with a backup
/// key (the key file is replaced once a second key is there).
#[test]
fn switching_between_key_and_keyfile() {
    let d = Dirs::new("factors-switch");
    let s = d.suite();
    let (a, b) = (key(), key());
    s.create(options(PW, None, &[(VaultKind::Pass, "P")]), Some(a.clone()), Some(listener()), Some("1234".into()))
        .unwrap();
    s.add_security_key(key_proof(PW), Some(a.clone()), b.clone(), listener(), "1234".into(), "Backup".into()).unwrap();
    let keys: Vec<Vec<u8>> = slot_of(&s, SlotKind::SecurityKey).into_iter().map(|x| x.slot_id).collect();
    assert_eq!(keys.len(), 2);
    // Key and backup key -> one key file, proven with the key.
    let kf = generate_keyfile();
    s.change_factors(
        key_proof(PW),
        Some(a.clone()),
        Some(NewFactor::Keyfile { keyfile: kf.clone() }),
        None,
        Some(listener()),
        "Datei".into(),
        keys,
    )
    .unwrap();
    assert!(slot_of(&s, SlotKind::SecurityKey).is_empty());
    s.lock();
    assert!(s.unlock(PW.into(), Some(a), Some(listener()), Some("1234".into()), None).is_err());
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    // Key file -> key + backup: first a key next to the key file, then the key file replaced
    // by a second key. A key file straight to one key alone is refused (no backup).
    let (c, e) = (key(), key());
    let kf_slot = slot_of(&s, SlotKind::Keyfile)[0].slot_id.clone();
    let r = s.change_factors(
        kf_proof(PW, &kf),
        None,
        Some(NewFactor::SecurityKey { pin: "1234".into() }),
        Some(c.clone()),
        Some(listener()),
        "Key C".into(),
        vec![kf_slot.clone()],
    );
    assert!(matches!(r, Err(GhulError::BackupFactorRequired)), "{r:?}");
    s.change_factors(
        kf_proof(PW, &kf),
        None,
        Some(NewFactor::SecurityKey { pin: "1234".into() }),
        Some(c.clone()),
        Some(listener()),
        "Key C".into(),
        vec![],
    )
    .unwrap();
    s.change_factors(
        kf_proof(PW, &kf),
        None,
        Some(NewFactor::SecurityKey { pin: "1234".into() }),
        Some(e.clone()),
        Some(listener()),
        "Key E".into(),
        vec![kf_slot],
    )
    .unwrap();
    assert_eq!((slot_of(&s, SlotKind::SecurityKey).len(), slot_of(&s, SlotKind::Keyfile).len()), (2, 0));
    s.lock();
    assert!(s.unlock(PW.into(), None, None, None, Some(kf)).is_err());
    s.unlock(PW.into(), Some(c), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    s.unlock(PW.into(), Some(e), Some(listener()), Some("1234".into()), None).unwrap();
}

/// The rules: a key keeps a backup, one factor stays, the recovery code is not removed here,
/// something must change; nothing is written then.
#[test]
fn the_rules_of_the_factors_hold() {
    let (_d, s, kf) = with_keyfile("factors-rules");
    let a = key();
    s.add_security_key(kf_proof(PW, &kf), None, a.clone(), listener(), "1234".into(), "Key A".into()).unwrap();
    let kf_slot = slot_of(&s, SlotKind::Keyfile)[0].slot_id.clone();
    let a_slot = slot_of(&s, SlotKind::SecurityKey)[0].slot_id.clone();
    let code_slot = slot_of(&s, SlotKind::RecoveryCode)[0].slot_id.clone();
    let change = |new: Option<NewFactor>, remove: Vec<Vec<u8>>| {
        s.change_factors(kf_proof(PW, &kf), None, new, None, None, "x".into(), remove)
    };
    assert!(matches!(change(None, vec![kf_slot.clone()]), Err(GhulError::BackupFactorRequired)));
    assert!(matches!(change(None, vec![kf_slot.clone(), a_slot.clone()]), Err(GhulError::LastFactor)));
    assert!(matches!(change(None, vec![code_slot]), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(change(None, vec![]), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(change(None, vec![vec![9; 16]]), Err(GhulError::NotFound)));
    assert!(matches!(change(None, vec![vec![1; 3]]), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(change(Some(NewFactor::Keyfile { keyfile: kf.clone() }), vec![]), Err(GhulError::AlreadyExists)));
    assert!(matches!(
        change(Some(NewFactor::SecurityKey { pin: "1234".into() }), vec![]),
        Err(GhulError::InvalidInput { .. })
    ));
    // The proof is needed (the password alone is not enough).
    let r = s.change_factors(no_key_proof(PW), None, None, None, None, "x".into(), vec![a_slot.clone()]);
    assert!(r.is_err());
    // Nothing changed: the key alone may go (the key file stays, it needs no backup).
    assert_eq!(s.slots().unwrap().len(), 3);
    change(None, vec![a_slot]).unwrap();
    assert!(slot_of(&s, SlotKind::SecurityKey).is_empty());
}

/// A stop in the middle of a replacement leaves the state before (the old key file opens,
/// the new one does not).
#[test]
fn a_stop_in_the_middle_of_a_replacement_leaves_the_state_before() {
    let (d, s, old) = with_keyfile("factors-stop");
    let new = generate_keyfile();
    let old_slot = slot_of(&s, SlotKind::Keyfile)[0].slot_id.clone();
    WRITES_BEFORE_FAILURE.with(|c| c.set(Some(1)));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(true));
    let r = s.change_factors(
        kf_proof(PW, &old),
        None,
        Some(NewFactor::Keyfile { keyfile: new.clone() }),
        None,
        None,
        "Neu".into(),
        vec![old_slot],
    );
    WRITES_BEFORE_FAILURE.with(|c| c.set(None));
    STOP_AFTER_FAILED_WRITE.with(|c| c.set(false));
    assert!(r.is_err());
    drop(s);
    let s = d.suite();
    assert!(s.unlock(PW.into(), None, None, None, Some(new)).is_err());
    s.unlock(PW.into(), None, None, None, Some(old)).unwrap();
    assert_eq!(titles(&s), vec!["Bank".to_owned()]);
    assert!(s.list_vaults().unwrap().iter().all(|v| v.available));
}
