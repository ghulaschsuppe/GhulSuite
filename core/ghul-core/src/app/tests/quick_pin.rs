// SPDX-License-Identifier: AGPL-3.0-or-later
//! M19: the app PIN with two single-use Keystore keys (see `quick.rs`). The Keystore is modelled
//! by `PinKeystore` (one use per key); the app's sequence around the core is the one in
//! `PinKeystore::attempt`.

use super::super::quick::{pin_tag, PIN_CLEANUP_FAILS, PIN_RULE, PIN_TAG_INFO};
use super::super::suite::NOW_SHIFT_MS;
use super::*;

const PW: &str = "pw-123456-Kq7wZ";
const PIN: &str = "24681357";

fn suite_with_pin(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, PinKeystore) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let mut ks = PinKeystore::default();
    ks.enable(&s, kf_proof(PW, &kf), None, PIN).unwrap();
    s.lock();
    (d, s, kf, ks)
}

fn pin_file(d: &Dirs) -> std::path::PathBuf {
    std::path::PathBuf::from(d.local()).join("quick-pin.ghlq")
}

/// The PIN rule: at least 8 digits, nothing else; set up with both factors only.
#[test]
fn the_app_pin_needs_eight_digits_and_both_factors() {
    let d = Dirs::new("pin-rule");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    for bad in ["1234567", "12345678a", "abcdefgh", "１２３４５６７８"] {
        let r = s.quick_unlock_enable_pin(kf_proof(PW, &kf), None, None, bad.into(), 3600, pin_keys());
        assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == PIN_RULE), "{bad}: {r:?}");
    }
    let r = s.quick_unlock_enable_pin(no_key_proof(PW), None, None, PIN.into(), 3600, pin_keys());
    assert!(matches!(r, Err(GhulError::KeyfileRequired)), "{r:?}");
    let r = s.quick_unlock_enable_pin(kf_proof(PW, &kf), None, None, PIN.into(), 3600, vec![vec![1; 32]]);
    assert!(matches!(r, Err(GhulError::InvalidInput { .. })), "two keys");
    // Not through the functions of the fingerprint.
    let r = s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "pin".into(), 3600);
    assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == super::super::quick::PIN_HAS_SLOTS), "{r:?}");
    assert!(!s.quick_unlock_exists("pin".into()));
    assert_eq!(super::super::tools::security_rules().app_pin_min_length, 8);
}

/// Both slots work; a right PIN gives a new generation with two new keys, and the keys of the
/// generation before open nothing any more.
#[test]
fn a_right_pin_opens_and_seals_anew() {
    let (d, s, _, mut ks) = suite_with_pin("pin-right");
    let old = ks.keys.clone();
    assert_eq!(s.quick_pin_generation().unwrap(), Some(1));
    assert_eq!(ks.attempt(&s, PIN).unwrap(), 2);
    assert!(s.is_unlocked());
    assert_eq!(titles(&s), Vec::<String>::new());
    assert_eq!(ks.slots(2), vec![0, 1], "two new attempts");
    s.lock();
    // The second slot of the old generation (left over, e.g. after a stop) opens nothing.
    let input = s.quick_pin_challenge(PIN.into()).unwrap();
    let tag = pin_tag(&old[&(1, 1)], input.message[PIN_TAG_INFO.len()..].try_into().unwrap());
    assert!(matches!(
        s.unlock_quick_pin(1, tag.to_vec(), input.message, pin_keys()),
        Err(GhulError::QuickUnlockInvalid)
    ));
    // Slot 1 of the new generation works as well.
    ks.keys.remove(&(2, 0));
    assert_eq!(ks.attempt(&s, PIN).unwrap(), 3);
    let left: Vec<String> = std::fs::read_dir(d.local())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("quick-pin.ghlq."))
        .collect();
    assert!(left.is_empty(), "no copy of the file before: {left:?}");
}

/// One wrong PIN leaves one attempt; a right one then gives two again. Two wrong PINs: no key
/// left, the PIN is off, only a full unlock opens.
#[test]
fn two_wrong_pins_end_the_pin() {
    let (_d, s, kf, mut ks) = suite_with_pin("pin-wrong");
    assert!(matches!(ks.attempt(&s, "11111111"), Err(GhulError::QuickUnlockInvalid)));
    assert_eq!(ks.slots(1), vec![1], "one attempt left");
    ks.attempt(&s, PIN).unwrap();
    assert_eq!(ks.slots(2), vec![0, 1], "two attempts again");
    s.lock();
    assert!(matches!(ks.attempt(&s, "11111111"), Err(GhulError::QuickUnlockInvalid)));
    assert!(matches!(ks.attempt(&s, "22222222"), Err(GhulError::QuickUnlockInvalid)));
    assert!(!s.is_unlocked());
    // No key left: also the right PIN cannot be tried; the app turns the PIN off.
    assert!(matches!(ks.attempt(&s, PIN), Err(GhulError::QuickUnlockUnavailable)));
    assert!(!s.quick_unlock_exists("pin".into()));
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
}

/// Sealing anew keeps the validity: it is counted from the last full unlock, never extended by
/// a quick unlock. An expired PIN costs no key.
#[test]
fn sealing_anew_keeps_the_validity() {
    let (_d, s, kf, mut ks) = suite_with_pin("pin-validity");
    let hour = 3_600_000;
    NOW_SHIFT_MS.with(|c| c.set(hour * 3 / 4));
    ks.attempt(&s, PIN).unwrap();
    s.lock();
    // 70 minutes after the last full unlock (the enabling): expired, although the PIN was used
    // 25 minutes ago. Refused before a key is used.
    NOW_SHIFT_MS.with(|c| c.set(hour * 7 / 6));
    assert!(matches!(ks.attempt(&s, PIN), Err(GhulError::QuickUnlockExpired)));
    let generation = s.quick_pin_generation().unwrap().unwrap();
    assert_eq!(ks.slots(generation), vec![0, 1], "no key used");
    // A full unlock makes it valid again.
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    s.lock();
    ks.attempt(&s, PIN).unwrap();
    NOW_SHIFT_MS.with(|c| c.set(0));
}

/// A stop at any point costs at most the one attempt being made.
#[test]
fn a_stop_costs_at_most_one_attempt() {
    let (_d, s, _, mut ks) = suite_with_pin("pin-crash");
    // Stop after the key was used, before the core's call: that slot is gone, the other works.
    let challenge = s.quick_pin_challenge(PIN.into()).unwrap();
    let _used = ks.sign(1, 0, &challenge.message).unwrap();
    assert_eq!(ks.attempt(&s, PIN).unwrap(), 2, "the second slot");
    s.lock();
    // Stop after the next keys were imported, before the challenge: they are orphans, the
    // next attempt drops them and both slots of the generation are still there.
    ks.import(3, &pin_keys());
    assert_eq!(ks.attempt(&s, PIN).unwrap(), 3);
    s.lock();
    // Stop after the core sealed anew, before the app deleted the old keys: they open nothing,
    // and the next attempt drops them.
    let generation = s.quick_pin_generation().unwrap().unwrap();
    let next = pin_keys();
    ks.import(generation + 1, &next);
    let challenge = s.quick_pin_challenge(PIN.into()).unwrap();
    let tag = ks.sign(generation, 0, &challenge.message).unwrap();
    assert_eq!(s.unlock_quick_pin(0, tag, challenge.message, next).unwrap(), generation + 1);
    s.lock();
    assert!(ks.slots(generation).contains(&1), "old key left over");
    assert_eq!(ks.attempt(&s, PIN).unwrap(), generation + 2);
    assert!(ks.slots(generation).is_empty());
}

/// The file alone allows no check of a PIN: without a Keystore key, no tag opens it, also not
/// the challenge itself or a tag of another key.
#[test]
fn the_file_alone_checks_no_pin() {
    let (d, s, _, _) = suite_with_pin("pin-offline");
    let challenge = s.quick_pin_challenge(PIN.into()).unwrap();
    let input: [u8; 32] = challenge.message[PIN_TAG_INFO.len()..].try_into().unwrap();
    for tag in [input.to_vec(), pin_tag(&[7; 32], &input).to_vec(), vec![0; 32]] {
        for slot in [0, 1] {
            let r = s.unlock_quick_pin(slot, tag.clone(), challenge.message.clone(), pin_keys());
            assert!(matches!(r, Err(GhulError::QuickUnlockInvalid)), "{r:?}");
        }
    }
    // The challenge (what the Keystore signs) is not stored in the file.
    let a = std::fs::read(pin_file(&d)).unwrap();
    assert!(!a.windows(32).any(|w| w == input), "the challenge is not in the file");
}

/// A PIN file of version 3 is outdated and deleted; the fingerprint's file of version 3 stays
/// as it is.
#[test]
fn an_old_pin_file_is_refused_and_the_fingerprint_stays() {
    let (d, s, kf, _) = suite_with_pin("pin-v3");
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let bio = s.quick_unlock_enable(kf_proof(PW, &kf), None, None, "bio".into(), 3600).unwrap();
    s.lock();
    // A PIN file as version 3 wrote it: the fingerprint's layout with the PIN byte set.
    let mut v3 = std::fs::read(std::path::PathBuf::from(d.local()).join("quick-bio.ghlq")).unwrap();
    v3[58] = 1;
    std::fs::write(pin_file(&d), &v3).unwrap();
    assert!(matches!(s.quick_pin_challenge(PIN.into()), Err(GhulError::QuickUnlockOutdated)));
    assert!(!pin_file(&d).exists(), "deleted");
    assert_eq!(s.quick_pin_generation().unwrap(), None);
    s.unlock_quick("bio".into(), bio).unwrap();
}

/// After the new file is in place, a copy before that cannot be removed is only reported: the
/// core gives the new generation, so the app keeps the new keys and the PIN goes on working.
#[test]
fn a_copy_left_after_sealing_anew_costs_nothing() {
    let (d, s, _, mut ks) = suite_with_pin("pin-cleanup");
    let bak = std::path::PathBuf::from(d.local()).join("quick-pin.ghlq.bak");
    std::fs::write(&bak, b"old copy").unwrap();
    PIN_CLEANUP_FAILS.with(|c| c.set(true));
    let now = ks.attempt(&s, PIN);
    PIN_CLEANUP_FAILS.with(|c| c.set(false));
    assert_eq!(now.unwrap(), 2, "the new generation, not the old one");
    assert!(s.is_unlocked());
    assert!(bak.exists(), "left over, only reported");
    assert_eq!(ks.slots(2), vec![0, 1]);
    s.lock();
    assert_eq!(ks.attempt(&s, PIN).unwrap(), 3);
    assert!(!bak.exists(), "removed by the next write");
}

/// The generation: `None` only without a file; a file that cannot be read is an error (the app
/// keeps the PIN), a damaged one `QuickUnlockUnavailable`.
#[test]
fn the_generation_tells_a_missing_file_from_a_read_error() {
    let (d, s, kf, _) = suite_with_pin("pin-generation");
    assert_eq!(s.quick_pin_generation().unwrap(), Some(1));
    // Cannot be read: a folder in the place of the file.
    let file = pin_file(&d);
    let saved = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(matches!(s.quick_pin_generation(), Err(GhulError::Storage { .. })));
    assert!(matches!(s.quick_pin_challenge(PIN.into()), Err(GhulError::Storage { .. })));
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, &saved).unwrap();
    assert_eq!(s.quick_pin_generation().unwrap(), Some(1), "back again");
    // Damaged (too short): unavailable and deleted, then no file.
    std::fs::write(&file, &saved[..saved.len() - 1]).unwrap();
    assert!(matches!(s.quick_pin_generation(), Err(GhulError::QuickUnlockUnavailable)));
    assert_eq!(s.quick_pin_generation().unwrap(), None);
    // A full unlock writes no PIN file of its own.
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.quick_pin_generation().unwrap(), None);
}

/// A file that changes between the challenge and the unlock (shorter now) is refused, it does
/// not panic: the header and the slots come from one read.
#[test]
fn a_file_changed_before_the_unlock_is_refused() {
    let (d, s, _, mut ks) = suite_with_pin("pin-changed");
    let challenge = s.quick_pin_challenge(PIN.into()).unwrap();
    let tag = ks.sign(1, 0, &challenge.message).unwrap();
    let file = pin_file(&d);
    let data = std::fs::read(&file).unwrap();
    std::fs::write(&file, &data[..90]).unwrap();
    let r = s.unlock_quick_pin(0, tag, challenge.message, pin_keys());
    assert!(matches!(r, Err(GhulError::QuickUnlockUnavailable)), "{r:?}");
    assert!(!s.is_unlocked());
}
