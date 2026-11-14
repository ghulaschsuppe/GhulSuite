// SPDX-License-Identifier: AGPL-3.0-or-later
//! B1: a change of the protection also protects against older copies of the files.
//!
//! The attacker has an old copy of the folder (a backup, a `.bak`, an older server version) and
//! what was valid back then: the old recovery code, a key file that has since been removed, or
//! the old password. With that, the old copy opens, but nothing written after the change may:
//! every file gets a new container key, and removing a key also renews the factor secret for
//! the remaining keys and key files (their slots carry public keys, so no touch is needed).

use super::*;
use crate::net::remote::LocalFs;
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

#[test]
fn other_devices_take_over_the_new_protection() {
    let server_dir = std::env::temp_dir().join(format!("ghul-rekey-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let (kf1, kf2, kf3) = (generate_keyfile(), generate_keyfile(), generate_keyfile());
    let (da, db) = (Dirs::new("rekey-a"), Dirs::new("rekey-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf1.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.add_keyfile(kf2.clone(), "Zwei".into()).unwrap();
    a.add_keyfile(kf3.clone(), "Drei".into()).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Bank", "u", "1")).unwrap();
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf1.clone())).unwrap();
    let before = files(&db);

    // 1. A removes "Zwei" (new factor secret). B still has the same code: it takes the new
    //    protection over without asking, and renews its own container keys as well.
    a.remove_slot(slot_id(&a, "Zwei")).unwrap();
    a.sync_folder(&server).unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "b", "2")).unwrap();
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    assert_eq!(titles(&a), vec!["Bank", "Von B"]);
    let loot = loot_with_keyfile(&before, "pw-123456", &kf2);
    assert_nothing_opens(&files(&db), &loot, Some("pw-123456"), "device B after the removal");
    b.lock();
    assert!(b.unlock("pw-123456".into(), None, None, None, Some(kf2)).is_err());
    b.unlock("pw-123456".into(), None, None, None, Some(kf1.clone())).unwrap();

    // 2. A removes "Drei" and makes a new code: B's code no longer opens the server's files.
    //    B must not mistake them for damaged files and overwrite them: it asks for a key.
    a.remove_slot(slot_id(&a, "Drei")).unwrap();
    let new_code = RecoveryCode::parse(&a.new_recovery_code("Neu".into()).unwrap()).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Von A", "a", "3")).unwrap();
    a.sync_folder(&server).unwrap();
    let on_server = std::fs::read(server_dir.join("suite").join("suite.ghlsuite")).unwrap();
    assert!(matches!(b.sync_folder(&server), Err(GhulError::ProtectionChangedElsewhere)));
    assert_eq!(std::fs::read(server_dir.join("suite").join("suite.ghlsuite")).unwrap(), on_server);
    assert!(b.adopt_protection_folder(&server, None, kf3.clone()).is_err(), "removed key file");
    b.adopt_protection_folder(&server, None, kf1.clone()).unwrap();
    assert_eq!(titles(&b), vec!["Bank", "Von A", "Von B"]);
    for (name, data) in files(&db) {
        assert!(LockedContainer::parse(&data).unwrap().recover(&new_code).is_ok(), "{name}: A's new code");
    }
    b.lock();
    b.unlock("pw-123456".into(), None, None, None, Some(kf1)).unwrap();
    assert_eq!(titles(&b), vec!["Bank", "Von A", "Von B"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn a_device_offline_for_a_week_loses_nothing() {
    let server_dir = std::env::temp_dir().join(format!("ghul-rekey-offline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let (kf1, kf2, kf3) = (generate_keyfile(), generate_keyfile(), generate_keyfile());
    let (da, db) = (Dirs::new("offline-a"), Dirs::new("offline-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf1.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.add_keyfile(kf2.clone(), "Zwei".into()).unwrap();
    a.add_keyfile(kf3.clone(), "Drei".into()).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Bank", "u", "1")).unwrap();
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf1.clone())).unwrap();

    // B is offline for a week and keeps working.
    b.save_entry(vault_id(&b, "Privat"), login("Offline 1", "b", "x")).unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Offline 2", "b", "y")).unwrap();
    // Meanwhile A changes a lot, all at once: key removed, new sheet, new password.
    a.save_entry(vault_id(&a, "Privat"), login("Von A", "a", "z")).unwrap();
    a.remove_slot(slot_id(&a, "Zwei")).unwrap();
    a.new_recovery_code("Neu".into()).unwrap();
    a.change_password("pw-123456".into(), "pw-neu-789".into()).unwrap();
    a.sync_folder(&server).unwrap();

    // B comes back: it must not overwrite anything, it asks once (key, then the new password).
    assert!(matches!(b.sync_folder(&server), Err(GhulError::ProtectionChangedElsewhere)));
    assert!(matches!(b.adopt_password_folder(&server, "pw-neu-789"), Err(GhulError::ProtectionChangedElsewhere)));
    assert!(matches!(b.adopt_protection_folder(&server, None, kf1.clone()), Err(GhulError::PasswordChangedElsewhere)));
    assert!(b.adopt_protection_folder(&server, Some("falsch"), kf1.clone()).is_err());
    b.adopt_protection_folder(&server, Some("pw-neu-789"), kf1.clone()).unwrap();
    a.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        assert_eq!(titles(s), vec!["Bank", "Offline 1", "Offline 2", "Von A"]);
    }
    b.lock();
    b.unlock("pw-neu-789".into(), None, None, None, Some(kf1.clone())).unwrap();

    // The other way round: B removes a key while offline, A keeps working. B's change is the
    // newer one: B still reads A's files (it remembers its previous keys) and A takes B's over.
    b.remove_slot(slot_id(&b, "Drei")).unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Offline 3", "b", "w")).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Von A 2", "a", "v")).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        assert_eq!(titles(s), vec!["Bank", "Offline 1", "Offline 2", "Offline 3", "Von A", "Von A 2"]);
    }
    a.lock();
    assert!(a.unlock("pw-neu-789".into(), None, None, None, Some(kf3)).is_err());
    a.unlock("pw-neu-789".into(), None, None, None, Some(kf1)).unwrap();
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn vault_files_get_neutral_names_here_and_on_the_server() {
    let server_dir = std::env::temp_dir().join(format!("ghul-neutral-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let d = Dirs::new("neutral");
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Bankgeheimnis")]), None, None, None).unwrap();
    s.save_entry(vault_id(&s, "Bankgeheimnis"), login("Bank", "u", "1")).unwrap();
    s.sync_folder(&server).unwrap();
    s.lock();
    // As an older app named it: after the vault.
    let st = d.storage();
    let (name, _) = files(&d).into_iter().find(|(n, _)| n.ends_with(".ghlpss")).unwrap();
    let old_name = "bankgeheimnis-0123abcd.ghlpss".to_owned();
    st.write(old_name.clone(), st.read(name.clone()).unwrap().unwrap()).unwrap();
    st.delete(name.clone()).unwrap();
    let srv = server_dir.join("suite");
    std::fs::rename(srv.join(&name), srv.join(&old_name)).unwrap();

    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
    s.sync_folder(&server).unwrap();
    let names = |dir: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> =
            std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    for n in names(&d.root.join("vault")).into_iter().chain(names(&srv)) {
        assert!(!n.to_lowercase().contains("bank"), "{n} names the vault");
    }
    assert!(names(&srv).contains(&name), "{:?}", names(&srv));
    assert_eq!(s.list_vaults().unwrap()[0].name, "Bankgeheimnis");
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn many_changes_while_the_other_device_is_offline() {
    let server_dir = std::env::temp_dir().join(format!("ghul-rekey-many-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("many-a"), Dirs::new("many-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Bank", "u", "1")).unwrap();
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    // A works on, B changes its protection again and again without a connection, for weeks.
    a.save_entry(vault_id(&a, "Privat"), login("Von A", "a", "2")).unwrap();
    a.sync_folder(&server).unwrap();
    for i in 0..40 {
        let extra = generate_keyfile();
        b.add_keyfile(extra, format!("Extra {i}")).unwrap();
        b.remove_slot(slot_id(&b, &format!("Extra {i}"))).unwrap();
        b.new_recovery_code(format!("Blatt {i}")).unwrap();
    }
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "b", "3")).unwrap();
    // Back online: B still reads what A wrote back then and merges it. A's code and factor secret
    // are both outdated now: A confirms once with its key file, then everything is merged.
    b.sync_folder(&server).unwrap();
    assert!(matches!(a.sync_folder(&server), Err(GhulError::ProtectionChangedElsewhere)));
    a.adopt_protection_folder(&server, None, kf.clone()).unwrap();
    b.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        assert_eq!(titles(s), vec!["Bank", "Von A", "Von B"]);
    }
    a.lock();
    a.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn a_removed_key_does_not_come_back_through_a_device_that_was_offline() {
    let server_dir = std::env::temp_dir().join(format!("ghul-b2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let (kf1, kf2, kf3) = (generate_keyfile(), generate_keyfile(), generate_keyfile());
    let (da, db) = (Dirs::new("b2-a"), Dirs::new("b2-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf1.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.add_keyfile(kf2.clone(), "Verloren".into()).unwrap();
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf1.clone())).unwrap();
    // A removes the lost key file; B, offline, adds a new one (its protection change is newer).
    a.remove_slot(slot_id(&a, "Verloren")).unwrap();
    a.sync_folder(&server).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    b.add_keyfile(kf3.clone(), "Neu".into()).unwrap();
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    for (d, s) in [(&da, &a), (&db, &b)] {
        let labels: Vec<String> = s.slots().unwrap().into_iter().map(|x| x.label).collect();
        assert!(!labels.contains(&"Verloren".to_string()), "{labels:?}");
        assert!(labels.contains(&"Neu".to_string()), "{labels:?}");
        let lost = Keyfile::parse(&kf2).unwrap();
        for (name, data) in files(d) {
            let l = LockedContainer::parse(&data).unwrap();
            assert!(l.factor_secret(&SecondFactor::Keyfile(&lost)).is_err(), "{name}: the removed key file is back");
        }
    }
    a.lock();
    assert!(a.unlock("pw-123456".into(), None, None, None, Some(kf2)).is_err());
    a.unlock("pw-123456".into(), None, None, None, Some(kf3)).unwrap();
    let _ = std::fs::remove_dir_all(&server_dir);
}
