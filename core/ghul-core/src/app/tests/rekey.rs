// SPDX-License-Identifier: AGPL-3.0-or-later
//! B1: a change of the protection also protects against older copies of the files.
//!
//! The attacker has an old copy of the folder (a backup, a `.bak`, an older server version) and
//! what was valid back then: the old recovery code, a key file that has since been removed, or
//! the old password. With that, the old copy opens, but nothing written after the change may:
//! every file gets a new container key, and removing a key also renews the factor secret for
//! the remaining keys and key files (their slots carry public keys, so no touch is needed).

use super::*;
use crate::vault::container::legacy_keyfile_slot;
use crate::vault::{
    ContainerKind, FactorSecret, KdfCost, Keyfile, LockedContainer, PasswordKey, RecoveryCode, SecondFactor, Slot,
    SlotMeta, SuiteKeys,
};

const EXTENSIONS: &[&str] = &[".ghlsuite", ".ghlpss", ".ghltotp", ".ghlcal", ".ghldrv", ".ghlmail"];

/// The container files of the folder (what a backup or a thief would copy).
fn files(d: &Dirs) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(d.root.join("vault"))
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(n, _)| EXTENSIONS.iter().any(|x| n.ends_with(x)))
        .map(|(n, p)| (n, std::fs::read(p).unwrap()))
        .collect();
    out.sort();
    out
}

/// What an old copy gives away: the container key of every file and the factor secret.
struct Loot {
    vmks: Vec<[u8; 32]>,
    factor_secret: [u8; 32],
}

fn loot_with_code(copy: &[(String, Vec<u8>)], code: &RecoveryCode) -> Loot {
    let mut vmks = Vec::new();
    let mut factor_secret = [0u8; 32];
    for (name, data) in copy {
        let c = LockedContainer::parse(data).unwrap().recover(code).unwrap_or_else(|_| panic!("{name} old code"));
        vmks.push(*c.vmk());
        factor_secret = *c.factor_secret().expose();
    }
    Loot { vmks, factor_secret }
}

fn loot_with_keyfile(copy: &[(String, Vec<u8>)], password: &str, kf: &[u8]) -> Loot {
    let kf = Keyfile::parse(kf).unwrap();
    let mut vmks = Vec::new();
    let mut factor_secret = [0u8; 32];
    for (name, data) in copy {
        let l = LockedContainer::parse(data).unwrap();
        let s = l.factor_secret(&SecondFactor::Keyfile(&kf)).unwrap_or_else(|_| panic!("{name} old key file"));
        let pk = PasswordKey::derive(password, &l.header().kdf).unwrap();
        let c = l.unlock(&pk, &s).unwrap_or_else(|_| panic!("{name} old password"));
        vmks.push(*c.vmk());
        factor_secret = *s.expose();
    }
    Loot { vmks, factor_secret }
}

/// Nothing of `current` may open with what the old copy gave away (plus `password`, which the
/// attacker may know as well).
fn assert_nothing_opens(current: &[(String, Vec<u8>)], loot: &Loot, password: Option<&str>, what: &str) {
    for (name, data) in current {
        for vmk in &loot.vmks {
            assert!(
                LockedContainer::parse(data).unwrap().unlock_with_vmk(vmk).is_err(),
                "{what}: {name} opens with a container key of the old copy"
            );
        }
        if let Some(pw) = password {
            let l = LockedContainer::parse(data).unwrap();
            let pk = PasswordKey::derive(pw, &l.header().kdf).unwrap();
            assert!(
                l.unlock(&pk, &FactorSecret::from_bytes(loot.factor_secret)).is_err(),
                "{what}: {name} opens with the old factor secret and the password"
            );
        }
    }
}

fn slot_id(s: &GhulSuite, label: &str) -> Vec<u8> {
    s.slots().unwrap().into_iter().find(|x| x.label == label).unwrap().slot_id
}

#[test]
fn an_old_copy_and_the_old_recovery_code_do_not_open_the_current_files() {
    let d = Dirs::new("rekey-code");
    let kf = generate_keyfile();
    let s = d.suite();
    let res = s
        .create(
            options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
            None,
            None,
            None,
        )
        .unwrap();
    let old_code = RecoveryCode::parse(&res.recovery_code.unwrap()).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "alt")).unwrap();
    let old = files(&d);
    assert!(old.len() >= 3);

    let new_code = RecoveryCode::parse(&s.new_recovery_code("Neues Blatt".into()).unwrap()).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Neu", "u", "neu")).unwrap();
    let now = files(&d);

    // The lost sheet still opens the old copy completely ...
    let loot = loot_with_code(&old, &old_code);
    // ... but not one current file: neither the old code nor the old container keys.
    for (name, data) in &now {
        assert!(LockedContainer::parse(data).unwrap().recover(&old_code).is_err(), "{name}: old code");
        assert!(LockedContainer::parse(data).unwrap().recover(&new_code).is_ok(), "{name}: new code");
    }
    assert_nothing_opens(&now, &loot, None, "old code");

    // The owner notices nothing: password and key file open everything.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"]);
}

#[test]
fn a_removed_key_file_with_an_old_copy_does_not_open_the_current_files() {
    let d = Dirs::new("rekey-key");
    let (kf1, kf2) = (generate_keyfile(), generate_keyfile());
    let s = d.suite();
    s.create(options("pw-123456", Some(kf1.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.add_keyfile(kf2.clone(), "Verloren".into()).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "alt")).unwrap();
    let old = files(&d);
    assert!(s.slots().unwrap().iter().all(|x| !x.needs_update));

    // The second key file got lost: it is removed.
    s.remove_slot(slot_id(&s, "Verloren")).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Neu", "u", "neu")).unwrap();
    let now = files(&d);

    // Whoever found it and knows the password opens the old copy ...
    let loot = loot_with_keyfile(&old, "pw-123456", &kf2);
    // ... but nothing current: the key file has no slot any more, and its factor secret, the
    // old container keys and the password together do not open the new files either.
    let lost = Keyfile::parse(&kf2).unwrap();
    for (name, data) in &now {
        let l = LockedContainer::parse(data).unwrap();
        assert!(l.factor_secret(&SecondFactor::Keyfile(&lost)).is_err(), "{name}: removed key file");
    }
    assert_nothing_opens(&now, &loot, Some("pw-123456"), "removed key file");

    // The remaining key file opens everything, without having been touched.
    s.lock();
    assert!(s.unlock("pw-123456".into(), None, None, None, Some(kf2)).is_err());
    s.unlock("pw-123456".into(), None, None, None, Some(kf1)).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"]);
}

#[test]
fn a_removed_security_key_with_an_old_copy_does_not_open_the_current_files() {
    let d = Dirs::new("rekey-fido");
    let (k1, k2) = (key(), key());
    let s = d.suite();
    s.create(
        options("pw-123456", None, &[(VaultKind::Pass, "Privat")]),
        Some(k1.clone()),
        Some(listener()),
        Some("1234".into()),
    )
    .unwrap();
    s.add_security_key(k2.clone(), listener(), "1234".into(), "Key B".into()).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "alt")).unwrap();
    // The thief gets an old copy and key B (with its PIN) and the password.
    let old = files(&d);
    let thief = Dirs::new("rekey-fido-thief");
    for (n, data) in &old {
        thief.storage().write(n.clone(), data.clone()).unwrap();
    }
    let t = thief.suite();
    t.unlock("pw-123456".into(), Some(k2.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(titles(&t), vec!["Bank"]);
    t.lock();

    s.remove_slot(slot_id(&s, "Key B")).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Neu", "u", "neu")).unwrap();
    // With the current files, key B opens nothing any more, even with the old copy's help.
    for (n, data) in files(&d) {
        thief.storage().write(n, data).unwrap();
    }
    let t = thief.suite();
    assert!(t.unlock("pw-123456".into(), Some(k2), Some(listener()), Some("1234".into()), None).is_err());
    s.lock();
    s.unlock("pw-123456".into(), Some(k1), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Neu"]);
}

#[test]
fn an_old_copy_and_the_old_password_do_not_open_the_current_files() {
    let d = Dirs::new("rekey-pw");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-alt-123", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "alt")).unwrap();
    let old = files(&d);
    s.change_password("pw-alt-123".into(), "pw-neu-456".into()).unwrap();
    let now = files(&d);
    // Someone who knew the old password and has the key file opens the old copy, but its
    // container keys do not open the current files.
    let loot = loot_with_keyfile(&old, "pw-alt-123", &kf);
    assert_nothing_opens(&now, &loot, Some("pw-alt-123"), "old password");
}

/// A suite as an app before format version 2 wrote it: three key files in slots of type 3.
fn legacy_suite(d: &Dirs, password: &str, keyfiles: &[(&Keyfile, &str)]) -> RecoveryCode {
    let mut rng = rand_core::OsRng;
    let keys = SuiteKeys::create(password, KdfCost { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 }, &mut rng)
        .unwrap();
    let slots: Vec<Slot> =
        keyfiles.iter().map(|(kf, _)| legacy_keyfile_slot(&keys.suite_id, &keys.factor_secret, kf, &mut rng)).collect();
    let code = RecoveryCode::generate(&mut rng);
    let st = d.storage();
    for (kind, file) in [(ContainerKind::Suite, "suite.ghlsuite"), (ContainerKind::Pass, "privat-0123abcd.ghlpss")] {
        let mut c = keys.create_container(kind, slots.clone(), &mut rng).unwrap();
        for (slot, (_, label)) in slots.iter().zip(keyfiles) {
            c.payload.slots.insert(
                crate::kdf::hex(slot.id()),
                SlotMeta { label: (*label).into(), created: 1, extra: Default::default() },
            );
        }
        c.set_recovery_code(&code, "Notfallblatt", 1, &mut rng).unwrap();
        if kind == ContainerKind::Pass {
            c.payload.settings.insert("name".into(), ciborium::value::Value::Text("Privat".into()));
        }
        st.write(file.into(), c.seal_version(1, &mut rng).unwrap()).unwrap();
    }
    code
}

#[test]
fn slots_of_older_apps_get_a_public_key_when_they_are_used() {
    let d = Dirs::new("rekey-legacy");
    let (kf1, kf2, kf3) = (Keyfile::generate(&mut rand_core::OsRng), Keyfile::generate(&mut rand_core::OsRng), {
        Keyfile::generate(&mut rand_core::OsRng)
    });
    let code = legacy_suite(&d, "pw-123456", &[(&kf1, "Eins"), (&kf2, "Zwei"), (&kf3, "Drei")]);
    for (_, data) in files(&d) {
        assert_eq!(u16::from_le_bytes([data[8], data[9]]), 1);
    }
    let s = d.suite();
    s.unlock("pw-123456".into(), None, None, None, Some(kf1.encode().to_vec())).unwrap();
    s.save_entry(vault_id(&s, "Privat"), login("Bank", "u", "alt")).unwrap();
    // The key file just used now has a public key (in every file); the others still wait.
    let need: Vec<(String, bool)> = s.slots().unwrap().into_iter().map(|x| (x.label, x.needs_update)).collect();
    assert!(need.contains(&("Eins".into(), false)) && need.contains(&("Zwei".into(), true)), "{need:?}");
    for (name, data) in files(&d) {
        assert_eq!(u16::from_le_bytes([data[8], data[9]]), 2, "{name} written as version 2");
        let l = LockedContainer::parse(&data).unwrap();
        assert_eq!(l.header().slots.iter().filter(|x| x.is_legacy()).count(), 2, "{name}");
        assert!(l.recover(&code).is_ok(), "{name}: the code stays valid");
    }
    let old = files(&d);

    // Removing "Drei" while "Zwei" has no public key yet: new container keys, but the factor
    // secret stays (it cannot be given to "Zwei" without the key file).
    s.remove_slot(slot_id(&s, "Drei")).unwrap();
    // "Zwei" is used once: its slot gets a public key too.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf2.encode().to_vec())).unwrap();
    assert!(s.slots().unwrap().iter().all(|x| !x.needs_update));
    // The next change renews the factor secret: from now on "Drei" opens nothing current.
    let kf4 = generate_keyfile();
    s.add_keyfile(kf4, "Vier".into()).unwrap();
    s.remove_slot(slot_id(&s, "Vier")).unwrap();
    let loot = loot_with_keyfile(&old, "pw-123456", &kf3.encode());
    assert_nothing_opens(&files(&d), &loot, Some("pw-123456"), "key file of an older app");
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf1.encode().to_vec())).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
}

#[test]
fn quick_unlock_survives_a_new_code_but_not_a_removed_key() {
    let d = Dirs::new("rekey-quick");
    let (kf1, kf2) = (generate_keyfile(), generate_keyfile());
    let s = d.suite();
    s.create(options("pw-123456", Some(kf1), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.add_keyfile(kf2, "Zwei".into()).unwrap();
    let bio = s.quick_unlock_enable("bio".into(), None).unwrap();
    s.new_recovery_code("Neu".into()).unwrap();
    s.lock();
    s.unlock_quick("bio".into(), bio.clone(), None, 3600).unwrap();
    s.remove_slot(slot_id(&s, "Zwei")).unwrap();
    assert!(!s.quick_unlock_exists("bio".into()));
}
