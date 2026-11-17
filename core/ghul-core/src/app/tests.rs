// SPDX-License-Identifier: AGPL-3.0-or-later
//! App API tests against the software authenticator and temporary directories.

use std::sync::Arc;

use super::error::GhulError;
use super::storage::{local_storage, VaultStorage};
use super::suite::{
    generate_keyfile, EntryData, FieldData, FieldValue, GhulSuite, NewVault, SetupOptions, SlotKind, VaultKind,
    VaultSource,
};
use super::tools::{OtpAlgo, TotpInput};
use crate::mock::{MockAuthenticator, MockConfig};
use crate::{FidoListener, FidoStatus, HidTransport};

struct Quiet;
impl FidoListener for Quiet {
    fn on_status(&self, _status: FidoStatus) {}
}

struct Dirs {
    root: std::path::PathBuf,
}

impl Dirs {
    fn new(name: &str) -> Self {
        let mut id = [0u8; 8];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
        Dirs { root: std::env::temp_dir().join(format!("ghul-{name}-{}", crate::kdf::hex(&id))) }
    }
    fn storage(&self) -> Arc<dyn VaultStorage> {
        local_storage(self.root.join("vault").to_string_lossy().into_owned())
    }
    fn local(&self) -> String {
        self.root.join("local").to_string_lossy().into_owned()
    }
    fn suite(&self) -> Arc<GhulSuite> {
        GhulSuite::new(self.storage(), self.local())
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn key() -> Arc<dyn HidTransport> {
    MockAuthenticator::new(MockConfig { protocols: vec![2, 1], fido_2_1: true, pin: Some("1234".into()) })
}

fn listener() -> Arc<dyn FidoListener> {
    Arc::new(Quiet)
}

fn options(password: &str, keyfile: Option<Vec<u8>>, vaults: &[(VaultKind, &str)]) -> SetupOptions {
    SetupOptions {
        password: password.into(),
        keyfile,
        key_label: "Key A".into(),
        keyfile_label: "USB-Stick".into(),
        recovery_label: "Notfallblatt".into(),
        memory_kib: 64 * 1024,
        iterations: 3,
        vaults: vaults.iter().map(|(k, n)| NewVault { kind: *k, name: (*n).into() }).collect(),
    }
}

fn login(title: &str, user: &str, pw: &str) -> EntryData {
    EntryData {
        id: vec![],
        entry_type: "login".into(),
        fields: vec![
            FieldData { name: "title".into(), value: FieldValue::Text { value: title.into() } },
            FieldData { name: "username".into(), value: FieldValue::Text { value: user.into() } },
            FieldData { name: "password".into(), value: FieldValue::Secret { value: pw.into() } },
            FieldData { name: "urls".into(), value: FieldValue::Items { values: vec!["https://example.org".into()] } },
        ],
        tags: vec![],
        folder: None,
        modified: 0,
        revision: 0,
    }
}

fn totp_input() -> TotpInput {
    TotpInput {
        issuer: "Example".into(),
        account: "a@example.org".into(),
        secret_base32: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into(), // "12345678901234567890"
        algorithm: OtpAlgo::Sha1,
        digits: 8,
        period: 30,
    }
}

fn vault_id(s: &GhulSuite, name: &str) -> String {
    s.list_vaults().unwrap().into_iter().find(|v| v.name == name).unwrap().id
}

#[test]
fn suite_with_several_vaults_and_security_key() {
    let d = Dirs::new("multi");
    let k = key();
    let s = d.suite();
    assert!(!s.exists().unwrap());
    let setup = s
        .create(
            options(
                "Master Pw!",
                None,
                &[(VaultKind::Pass, "Privat"), (VaultKind::Pass, "Arbeit"), (VaultKind::Totp, "Privat")],
            ),
            Some(k.clone()),
            Some(listener()),
            Some("1234".into()),
        )
        .unwrap();
    let code = setup.recovery_code.clone().unwrap();
    assert_eq!(code.split('-').count(), 11);
    let p = s.protection().unwrap();
    assert_eq!((p.security_keys, p.keyfiles, p.recovery_codes), (1, 0, 1));
    assert_eq!(s.list_vaults().unwrap().len(), 3);

    let privat = vault_id(&s, "Privat");
    let arbeit = s.list_vaults().unwrap().into_iter().find(|v| v.name == "Arbeit").unwrap().id;
    let auth = s.list_vaults().unwrap().into_iter().find(|v| v.kind == VaultKind::Totp).unwrap().id;
    s.save_entry(privat.clone(), login("Bank", "ich", "a")).unwrap();
    let id = s.save_entry(arbeit.clone(), login("Jira", "work", "b")).unwrap();
    assert!(matches!(s.save_entry(auth.clone(), login("x", "y", "z")), Err(GhulError::InvalidInput { .. })));
    s.add_totp(auth.clone(), totp_input()).unwrap();
    assert!(matches!(s.add_totp(privat.clone(), totp_input()), Err(GhulError::InvalidInput { .. })));

    let all = s.list_entries(VaultKind::Pass, None).unwrap();
    assert_eq!(
        all.iter().map(|e| (e.title.as_str(), e.vault_name.as_str())).collect::<Vec<_>>(),
        vec![("Bank", "Privat"), ("Jira", "Arbeit")]
    );
    assert_eq!(s.list_entries(VaultKind::Pass, Some(arbeit.clone())).unwrap().len(), 1);
    assert_eq!(s.totp_codes(59).unwrap()[0].code, "94287082");
    assert_eq!(s.get_entry(arbeit.clone(), id.clone()).unwrap().fields.len(), 4);

    // Lock and reopen: one key touch opens everything.
    s.lock();
    assert!(matches!(s.list_vaults(), Err(GhulError::Locked)));
    let s2 = d.suite();
    assert!(matches!(s2.unlock("Master Pw!".into(), None, None, None, None), Err(GhulError::SecurityKeyRequired)));
    assert!(matches!(
        s2.unlock("wrong".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None),
        Err(GhulError::WrongPassword)
    ));
    s2.unlock("Master Pw!".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s2.list_entries(VaultKind::Pass, None).unwrap().len(), 2);

    // Emergency: recovery code alone, then a new password.
    s2.lock();
    assert!(matches!(s2.unlock_with_recovery_code("AAAAA".into(), "neu".into()), Err(GhulError::InvalidRecoveryCode)));
    s2.unlock_with_recovery_code(code.to_lowercase(), "Neues Passwort".into()).unwrap();
    assert_eq!(s2.totp_codes(59).unwrap().len(), 1);
    s2.lock();
    // The security key still works, with the new password.
    s2.unlock("Neues Passwort".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s2.get_entry(arbeit, id).unwrap().fields.len(), 4);
}

#[test]
fn keyfile_then_security_key_without_invalidating_anything() {
    let d = Dirs::new("keyfile");
    let kf = generate_keyfile();
    let s = d.suite();
    assert!(matches!(
        s.create(options("pw-123456", None, &[(VaultKind::Pass, "Privat")]), None, None, None),
        Err(GhulError::InvalidInput { .. })
    ));
    let setup =
        s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let p = s.protection().unwrap();
    assert_eq!((p.security_keys, p.keyfiles, p.recovery_codes), (0, 1, 1));
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat.clone(), login("A", "b", "c")).unwrap();
    assert!(matches!(s.add_keyfile(kf.clone(), "again".into()), Err(GhulError::AlreadyExists)));
    s.lock();

    assert!(matches!(s.unlock("pw-123456".into(), None, None, None, None), Err(GhulError::KeyfileRequired)));
    assert!(matches!(
        s.unlock("pw-123456".into(), None, None, None, Some(generate_keyfile())),
        Err(GhulError::WrongKeyfile)
    ));
    assert!(matches!(s.unlock("falsch".into(), None, None, None, Some(kf.clone())), Err(GhulError::WrongPassword)));
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    let file_before = d.storage().read(vault_file(&s, &privat)).unwrap().unwrap();

    // Add a security key: key file, recovery code and older copies keep working.
    let k = key();
    s.add_security_key(k.clone(), listener(), "1234".into(), "Key".into()).unwrap();
    assert_eq!(s.protection().unwrap().security_keys, 1);
    s.lock();
    s.unlock("pw-123456".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    s.lock();
    s.unlock_with_recovery_code(setup.recovery_code.clone().unwrap(), "pw-654321".into()).unwrap();
    let copy = Dirs::new("copy");
    let f = copy.suite();
    f.create_from_vault(
        file_before,
        "Kopie".into(),
        "pw-123456".into(),
        None,
        None,
        None,
        Some(kf.clone()),
        "C".into(),
    )
    .unwrap();
    assert_eq!(f.list_entries(VaultKind::Pass, None).unwrap().len(), 1);

    // Retire the key file; the last second factor cannot be removed.
    let kf_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::Keyfile).unwrap();
    s.remove_slot(kf_slot.slot_id).unwrap();
    let key_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::SecurityKey).unwrap();
    assert!(matches!(s.remove_slot(key_slot.slot_id), Err(GhulError::LastFactor)));
    let code_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::RecoveryCode).unwrap();
    assert!(matches!(s.remove_slot(code_slot.slot_id), Err(GhulError::InvalidInput { .. })));
    s.lock();
    assert!(matches!(s.unlock("pw-654321".into(), None, None, None, Some(kf.clone())), Err(GhulError::WrongKeyfile)));
    s.unlock("pw-654321".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);
    s.lock();
    s.unlock_with_recovery_code(setup.recovery_code.unwrap(), "anders-123".into()).unwrap();
}

fn vault_file(s: &GhulSuite, id: &str) -> String {
    s.list_vaults().unwrap().into_iter().find(|v| v.id == id).unwrap().file
}

#[test]
fn vault_management_and_suite_from_vault() {
    let d = Dirs::new("manage");
    let kf = generate_keyfile();
    let s = d.suite();
    let setup =
        s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let work = s.create_vault(VaultKind::Pass, "Arbeit & Co".into()).unwrap();
    let file = s.list_vaults().unwrap().into_iter().find(|v| v.id == work).unwrap().file;
    // The file name says nothing about the vault (F23).
    assert_eq!(file, format!("{}.ghlpss", &work[..16]));
    s.save_entry(work.clone(), login("Jira", "w", "x")).unwrap();
    s.rename_vault(work.clone(), "Firma".into()).unwrap();
    let exported = s.export_vault(work.clone()).unwrap();

    // The name lives inside the file.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.list_vaults().unwrap().iter().any(|v| v.name == "Firma"));

    // Delete (file removed) and add it again from the export.
    s.remove_vault(work.clone()).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 1);
    assert!(d.storage().read(file.clone()).unwrap().is_none());
    let again = s.add_vault_file(exported.clone(), "".into()).unwrap();
    assert_eq!(again, work);
    assert!(matches!(s.add_vault_file(exported.clone(), "x".into()), Err(GhulError::AlreadyExists)));
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, Some(work.clone())).unwrap()[0].title, "Jira");

    // A vault file from another suite has a different protection.
    let other = Dirs::new("other");
    let o = other.suite();
    o.create(options("anderes-passwort", Some(generate_keyfile()), &[(VaultKind::Totp, "X")]), None, None, None)
        .unwrap();
    let x = vault_id(&o, "X");
    assert!(matches!(s.add_vault_file(o.export_vault(x).unwrap(), "X".into()), Err(GhulError::DifferentProtection)));

    // Opening a single vault without its suite builds a new suite around it.
    let fresh = Dirs::new("fresh");
    let f = fresh.suite();
    assert!(matches!(
        f.create_from_vault(
            exported.clone(),
            "Firma".into(),
            "pw-123456".into(),
            None,
            None,
            None,
            None,
            "Code".into()
        ),
        Err(GhulError::KeyfileRequired)
    ));
    assert!(matches!(
        f.create_from_vault(
            exported.clone(),
            "Firma".into(),
            "falsch".into(),
            None,
            None,
            None,
            Some(kf.clone()),
            "C".into()
        ),
        Err(GhulError::WrongPassword)
    ));
    let r = f
        .create_from_vault(exported, "Firma".into(), "pw-123456".into(), None, None, None, Some(kf.clone()), "C".into())
        .unwrap();
    assert_eq!(r.recovery_code, None, "the vault already carries the suite's code");
    let vaults = f.list_vaults().unwrap();
    assert_eq!((vaults.len(), vaults[0].kind, vaults[0].entries), (1, VaultKind::Pass, 1));
    f.lock();
    // The recovery code of the original suite opens the single vault, too.
    f.unlock_with_recovery_code(setup.recovery_code.unwrap(), "neu-neu-neu".into()).unwrap();
    assert_eq!(f.list_entries(VaultKind::Pass, None).unwrap()[0].title, "Jira");
}

/// N1: switching quick unlock on again (a new key) and then off leaves nothing of it on the
/// device, also not the previous version that writing keeps as ".bak".
#[test]
fn quick_unlock_off_leaves_no_copy_behind() {
    let d = Dirs::new("quick-bak");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.quick_unlock_enable("bio".into(), None).unwrap();
    s.quick_unlock_enable("bio".into(), None).unwrap();
    let quick_files = || -> Vec<String> {
        std::fs::read_dir(d.local())
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.contains(".ghlq"))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(quick_files(), vec!["quick-bio.ghlq".to_owned()], "no older copy next to the file");
    s.quick_unlock_disable("bio".into()).unwrap();
    assert!(quick_files().is_empty(), "{:?}", quick_files());
}

#[test]
fn quick_unlock_fingerprint_and_pin() {
    let d = Dirs::new("quick");
    let k = key();
    let s = d.suite();
    s.create(options("pw-1", None, &[(VaultKind::Pass, "P")]), Some(k.clone()), Some(listener()), Some("1234".into()))
        .unwrap();
    s.save_entry(vault_id(&s, "P"), login("Example", "u", "p")).unwrap();

    let bio = s.quick_unlock_enable("bio".into(), None).unwrap();
    let pin = s.quick_unlock_enable("pin".into(), Some("246810".into())).unwrap();
    assert!(s.quick_unlock_exists("bio".into()) && s.quick_unlock_exists("pin".into()));
    s.lock();

    s.unlock_quick("bio".into(), bio.clone(), None, 3600).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);
    // A new vault can be created after a quick unlock (needs the password key).
    s.create_vault(VaultKind::Totp, "2FA".into()).unwrap();
    s.lock();
    assert!(matches!(s.unlock_quick("bio".into(), vec![0; 32], None, 3600), Err(GhulError::QuickUnlockInvalid)));
    assert!(matches!(
        s.unlock_quick("pin".into(), pin.clone(), Some("000000".into()), 3600),
        Err(GhulError::QuickUnlockInvalid)
    ));
    s.unlock_quick("pin".into(), pin.clone(), Some("246810".into()), 3600).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 2);
    s.lock();

    // A full unlock restarts the validity period; expiry deletes the file.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    s.unlock("pw-1".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    // 3 s of validity: the full unlock above (Argon2) may itself take over a second on a slow runner.
    s.unlock_quick("bio".into(), bio.clone(), None, 3).unwrap();
    s.lock();
    std::thread::sleep(std::time::Duration::from_millis(3100));
    assert!(matches!(s.unlock_quick("bio".into(), bio, None, 3), Err(GhulError::QuickUnlockExpired)));
    assert!(!s.quick_unlock_exists("bio".into()));

    // A password change invalidates all quick unlocks.
    s.unlock_quick("pin".into(), pin.clone(), Some("246810".into()), 3600).unwrap();
    s.change_password("pw-1".into(), "pw-2".into()).unwrap();
    assert!(!s.quick_unlock_exists("pin".into()));
    s.lock();
    assert!(matches!(
        s.unlock_quick("pin".into(), pin, Some("246810".into()), 3600),
        Err(GhulError::QuickUnlockUnavailable)
    ));
}

/// F92: locking never waits for a call that still holds the open suite (a slow or unreachable
/// server). The app locks on its main thread; waiting there froze it (ANR in the emulator).
#[test]
fn lock_does_not_wait_for_a_running_call() {
    let d = Dirs::new("lockwait");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&s, "Privat");
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let busy = {
        let s = s.clone();
        std::thread::spawn(move || {
            s.with_open(|_| {
                started_tx.send(()).unwrap();
                // A server answering slowly: the call holds the open suite until released.
                release_rx.recv_timeout(std::time::Duration::from_secs(20)).ok();
                Ok(())
            })
        })
    };
    started_rx.recv().unwrap();
    let t = std::time::Instant::now();
    s.lock();
    assert!(!s.is_unlocked(), "locked at once for the app");
    assert!(t.elapsed() < std::time::Duration::from_secs(1), "lock waited {:?}", t.elapsed());
    release_tx.send(()).unwrap();
    busy.join().unwrap().unwrap();
    s.finish_lock();
    // The keys are gone: nothing opens without unlocking again.
    assert!(!s.is_unlocked());
    assert!(matches!(s.save_entry(privat.clone(), login("A", "b", "c")), Err(GhulError::Locked)));
    assert!(matches!(s.list_vaults(), Err(GhulError::Locked)));

    // Unlocking again works, and a late finish_lock (from an earlier lock) leaves it open.
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    s.finish_lock();
    assert!(s.is_unlocked());
    s.save_entry(privat, login("A", "b", "c")).unwrap();
}

#[test]
fn the_folder_is_the_suite() {
    let d = Dirs::new("folder");
    let kf = generate_keyfile();
    let s = d.suite();
    let setup = s
        .create(
            options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
            None,
            None,
            None,
        )
        .unwrap();
    let code = setup.recovery_code.unwrap();
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat.clone(), login("A", "b", "c")).unwrap();
    let privat_file = vault_file(&s, &privat);
    let st = d.storage();

    // A vault file copied into the folder (e.g. created on another device) simply belongs to it.
    let other_device = Dirs::new("folder-device");
    let od = other_device.suite();
    od.create_from_vault(
        st.read(privat_file.clone()).unwrap().unwrap(),
        "".into(),
        "pw-123456".into(),
        None,
        None,
        None,
        Some(kf.clone()),
        "C".into(),
    )
    .unwrap();
    let arbeit = od.create_vault(VaultKind::Pass, "Arbeit".into()).unwrap();
    let arbeit_file = vault_file(&od, &arbeit);
    st.write(arbeit_file.clone(), other_device.storage().read(arbeit_file.clone()).unwrap().unwrap()).unwrap();
    assert!(s.find_new_vaults().unwrap().is_empty());
    assert!(s.list_vaults().unwrap().iter().any(|v| v.name == "Arbeit"));
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 3);

    // A copy of a vault under another name is not listed twice.
    st.write("privat (1).ghlpss".into(), st.read(privat_file.clone()).unwrap().unwrap()).unwrap();
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 3);
    st.delete("privat (1).ghlpss".into()).unwrap();

    // Damaged file: listed as unavailable, everything else opens.
    let totp_file = s.list_vaults().unwrap().into_iter().find(|v| v.kind == VaultKind::Totp).unwrap().file;
    let good_totp = st.read(totp_file.clone()).unwrap().unwrap();
    let mut bad = good_totp.clone();
    let n = bad.len();
    bad[n - 1] ^= 1;
    st.write(totp_file.clone(), bad).unwrap();
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().iter().filter(|v| !v.available).count(), 1);
    st.write(totp_file.clone(), good_totp).unwrap();

    // Without the settings file the folder still opens; the file is recreated.
    s.lock();
    st.delete("suite.ghlsuite".into()).unwrap();
    assert!(s.exists().unwrap());
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(st.read("suite.ghlsuite".into()).unwrap().is_some());
    assert_eq!(s.list_vaults().unwrap().len(), 3);
    s.lock();

    // The recovery code opens a folder that holds just one vault file.
    let single = Dirs::new("folder-single");
    single.storage().write(privat_file.clone(), st.read(privat_file.clone()).unwrap().unwrap()).unwrap();
    let one = single.suite();
    one.unlock_with_recovery_code(code.clone(), "ganz-neu-123".into()).unwrap();
    assert_eq!(one.list_entries(VaultKind::Pass, None).unwrap()[0].title, "A");

    // A damaged settings file is reported.
    let mut suite = st.read("suite.ghlsuite".into()).unwrap().unwrap();
    let n = suite.len();
    suite[n - 1] ^= 1;
    st.write("suite.ghlsuite".into(), suite).unwrap();
    assert!(matches!(s.unlock("pw-123456".into(), None, None, None, Some(kf)), Err(GhulError::Corrupted { .. })));
}

#[test]
fn foreign_and_outdated_vaults() {
    let d = Dirs::new("foreign");
    let kf = generate_keyfile();
    let s = d.suite();
    let setup =
        s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let code = setup.recovery_code.unwrap();
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat.clone(), login("A", "b", "c")).unwrap();
    let st = d.storage();
    let privat_file = vault_file(&s, &privat);
    let old_privat = st.read(privat_file.clone()).unwrap().unwrap();

    // A vault of another suite (other password and key file) in the folder is offered.
    let other = Dirs::new("foreign-other");
    let okf = generate_keyfile();
    let o = other.suite();
    o.create(options("anderes-pw-1", Some(okf.clone()), &[(VaultKind::Totp, "Verein")]), None, None, None).unwrap();
    let verein = vault_id(&o, "Verein");
    o.add_totp(verein.clone(), totp_input()).unwrap();
    st.write("verein.ghltotp".into(), o.export_vault(verein).unwrap()).unwrap();
    let found = s.find_new_vaults().unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].kind == VaultKind::Totp && found[0].protection.keyfiles == 1);
    let src = VaultSource::InFolder { file: "verein.ghltotp".into() };
    assert!(matches!(
        s.adopt_vault(src.clone(), "".into(), "falsch".into(), None, None, None, Some(okf.clone())),
        Err(GhulError::WrongPassword)
    ));
    // Taking it over switches it to this suite's protection and gives it a neutral name.
    s.adopt_vault(src, "".into(), "anderes-pw-1".into(), None, None, None, Some(okf.clone())).unwrap();
    assert!(s.find_new_vaults().unwrap().is_empty());
    let verein_file = s.list_vaults().unwrap().into_iter().find(|v| v.name == "Verein").unwrap().file;
    assert!(verein_file.ends_with(".ghltotp") && !verein_file.contains("verein"), "{verein_file}");
    assert!(st.read("verein.ghltotp".into()).unwrap().is_none());
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.totp_codes(59).unwrap().len(), 1);
    // ... and the suite's recovery code now opens it as well.
    let single = Dirs::new("foreign-single");
    single.storage().write("verein.ghltotp".into(), st.read(verein_file.clone()).unwrap().unwrap()).unwrap();
    single.suite().unlock_with_recovery_code(code.clone(), "neu-neu-123".into()).unwrap();

    // Ignoring: a dismissed file is not offered again.
    st.write("fremd2.ghltotp".into(), other.storage().read(vault_file(&o, &vault_id(&o, "Verein"))).unwrap().unwrap())
        .unwrap();
    assert_eq!(s.find_new_vaults().unwrap().len(), 1);
    s.ignore_found_vault("fremd2.ghltotp".into()).unwrap();
    assert!(s.find_new_vaults().unwrap().is_empty());
    st.delete("fremd2.ghltotp".into()).unwrap();

    // Password change: an old copy of this suite's own file (old password) is not taken in by
    // itself once the change is complete (H0: only the password proves who wrote a file, the
    // code alone does not). It is offered and taken over with the password it was made with.
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    st.delete(privat_file.clone()).unwrap();
    st.write("privat-alt.ghlpss".into(), old_privat.clone()).unwrap();
    assert_eq!(s.find_new_vaults().unwrap().len(), 1, "offered, not taken in");
    let src = VaultSource::InFolder { file: "privat-alt.ghlpss".into() };
    s.adopt_vault(src, "".into(), "pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    s.lock();
    s.unlock("pw-neu-7890".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);

    // A key added later reaches every file, also the taken-over one.
    let k = key();
    s.add_security_key(k.clone(), listener(), "1234".into(), "Key".into()).unwrap();
    s.lock();
    s.unlock("pw-neu-7890".into(), Some(k), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s.list_vaults().unwrap().iter().filter(|v| v.available).count(), 2);

    // Outdated: an older file of a known vault is flagged until accepted.
    // (The taken-over file got a neutral name, F23.)
    assert!(st.read("privat-alt.ghlpss".into()).unwrap().is_none());
    let privat_now = vault_file(&s, &privat);
    let before = st.read(privat_now.clone()).unwrap().unwrap();
    s.save_entry(privat.clone(), login("B", "c", "d")).unwrap();
    s.lock();
    st.write(privat_now, before).unwrap();
    s.unlock("pw-neu-7890".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.list_vaults().unwrap().into_iter().find(|v| v.id == privat).unwrap().outdated);
    s.accept_vault_state(privat.clone()).unwrap();
    s.lock();
    s.unlock("pw-neu-7890".into(), None, None, None, Some(kf)).unwrap();
    assert!(!s.list_vaults().unwrap().into_iter().find(|v| v.id == privat).unwrap().outdated);
}

/// H4: two notes or cards with the same title but different content are both imported;
/// importing the same export again still adds nothing twice.
#[test]
fn import_keeps_different_entries_with_the_same_title() {
    let d = Dirs::new("import-same-title");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let pass = vault_id(&s, "Privat");
    let bw = r#"{"encrypted":false,"items":[
      {"type":2,"name":"Wiederherstellungscodes","notes":"Konto A: 1111"},
      {"type":2,"name":"Wiederherstellungscodes","notes":"Konto B: 2222"},
      {"type":3,"name":"Visa","card":{"cardholderName":"A","number":"4111111111111111"}},
      {"type":3,"name":"Visa","card":{"cardholderName":"B","number":"4000056655665556"}}]}"#;
    let r = s.import_data(bw.as_bytes().to_vec(), "bw.json".into(), Some(pass.clone()), None, false, None).unwrap();
    assert_eq!((r.entries, r.duplicates), (4, 0), "all four are different");
    let r = s.import_data(bw.as_bytes().to_vec(), "bw.json".into(), Some(pass.clone()), None, false, None).unwrap();
    assert_eq!((r.entries, r.duplicates), (0, 4), "the same export again adds nothing");
    // The same entry in two folders is kept in both.
    let folders = r#"{"encrypted":false,"folders":[{"id":"f1","name":"Arbeit"},{"id":"f2","name":"Privat"}],
      "items":[
      {"type":1,"name":"Mail","folderId":"f1","login":{"username":"ich","password":"pw"}},
      {"type":1,"name":"Mail","folderId":"f2","login":{"username":"ich","password":"pw"}}]}"#;
    let r =
        s.import_data(folders.as_bytes().to_vec(), "bw.json".into(), Some(pass.clone()), None, false, None).unwrap();
    assert_eq!((r.entries, r.duplicates), (2, 0), "one in each folder");
    let r = s.import_data(folders.as_bytes().to_vec(), "bw.json".into(), Some(pass), None, false, None).unwrap();
    assert_eq!((r.entries, r.duplicates), (0, 2));
}

#[test]
fn import_and_export() {
    let d = Dirs::new("import");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(
        options("pw-123456", Some(kf), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
        None,
        None,
        None,
    )
    .unwrap();
    let pass = vault_id(&s, "Privat");
    let totp = vault_id(&s, "2FA");
    let bw = r#"{"encrypted":false,"items":[
      {"type":1,"name":"GitHub","login":{"username":"ghul","password":"pw1","uris":[{"uri":"https://github.com/login"}],
       "totp":"otpauth://totp/GitHub:ghul?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"}},
      {"type":1,"name":"Bank","login":{"username":"me","password":"pw2","uris":[{"uri":"androidapp://de.bank.app"}]}},
      {"type":2,"name":"Notiz","notes":"text"}]}"#;
    let preview = super::suite::preview_import(bw.as_bytes().to_vec(), "bitwarden.json".into()).unwrap();
    assert_eq!((preview.entries, preview.totps, preview.totps_in_logins), (3, 0, 1));
    let r = s
        .import_data(
            bw.as_bytes().to_vec(),
            "bitwarden.json".into(),
            Some(pass.clone()),
            Some(totp.clone()),
            true,
            None,
        )
        .unwrap();
    assert_eq!((r.entries, r.totps, r.duplicates), (3, 1, 0));
    assert_eq!(s.totp_codes(59).unwrap()[0].code, "287082");
    // Importing again finds only duplicates.
    let r = s
        .import_data(bw.as_bytes().to_vec(), "b.json".into(), Some(pass.clone()), Some(totp.clone()), true, None)
        .unwrap();
    assert_eq!((r.entries, r.totps, r.duplicates), (0, 0, 4));
    assert!(matches!(
        s.import_data(bw.as_bytes().to_vec(), "b.json".into(), Some(totp.clone()), None, false, None),
        Err(GhulError::InvalidInput { .. })
    ));
    assert!(matches!(
        s.import_data(b"random bytes".to_vec(), "x.bin".into(), Some(pass.clone()), None, false, None),
        Err(GhulError::ImportUnknownFormat)
    ));

    // Plain export needs the password and can be imported again.
    assert!(matches!(s.export_plain(pass.clone(), "falsch".into()), Err(GhulError::WrongPassword)));
    let csv = s.export_plain(pass.clone(), "pw-123456".into()).unwrap();
    assert!(String::from_utf8(csv.clone()).unwrap().contains("GitHub"));
    let links = s.export_plain(totp.clone(), "pw-123456".into()).unwrap();
    assert!(String::from_utf8(links.clone()).unwrap().starts_with("otpauth://totp/GitHub:ghul?"));
    let other = s.create_vault(VaultKind::Pass, "Kopie".into()).unwrap();
    let r = s.import_data(csv, "export.csv".into(), Some(other.clone()), None, false, None).unwrap();
    assert_eq!(r.entries, 3);
}

use super::servers::ServerSaved;

fn saved(r: ServerSaved) -> String {
    match r {
        ServerSaved::Saved { id } => id,
        other => panic!("expected Saved, got {other:?}"),
    }
}

fn titles(s: &GhulSuite) -> Vec<String> {
    let mut t: Vec<String> = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.title).collect();
    t.sort();
    t
}

/// Copies the server's suite folder into a new device's storage (what `fetch_from_server` does).
fn copy_suite(server: &std::path::Path, device: &Dirs) {
    let st = device.storage();
    for e in std::fs::read_dir(server.join("suite")).unwrap() {
        let e = e.unwrap();
        st.write(e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()).unwrap();
    }
}

/// A server that answers slowly (every call waits), like a weak connection.
struct SlowFs {
    inner: crate::net::remote::LocalFs,
    delay: std::time::Duration,
}

impl crate::net::remote::RemoteFs for SlowFs {
    fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
        std::thread::sleep(self.delay);
        self.inner.list(dir)
    }
    fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
        std::thread::sleep(self.delay);
        self.inner.read(path)
    }
    fn write(&self, path: &str, data: &[u8]) -> crate::net::NetResult<()> {
        std::thread::sleep(self.delay);
        self.inner.write(path, data)
    }
    fn delete(&self, path: &str) -> crate::net::NetResult<()> {
        std::thread::sleep(self.delay);
        self.inner.delete(path)
    }
    fn mkdir_all(&self, dir: &str) -> crate::net::NetResult<()> {
        self.inner.mkdir_all(dir)
    }
}

/// F147: a synchronisation with a slow server does not hold the suite while it reads the
/// server: reading and saving entries meanwhile answer at once.
#[test]
fn a_slow_server_does_not_hold_the_suite() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-slow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-slow-a"), Dirs::new("sync-slow-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&a, "Privat");
    a.save_entry(privat.clone(), login("Vorher", "u", "1")).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    a.save_entry(privat.clone(), login("Vor dem Lauf", "u", "2")).unwrap();

    let slow = SlowFs { inner: LocalFs::new(&server_dir), delay: std::time::Duration::from_millis(700) };
    let (a2, done) = (a.clone(), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let done2 = done.clone();
    let run = std::thread::spawn(move || {
        let r = a2.sync_folder(&slow);
        done2.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    });
    // The synchronisation is reading the server now (each call takes 0.7 s).
    std::thread::sleep(std::time::Duration::from_millis(300));
    let t = std::time::Instant::now();
    assert!(titles(&a).contains(&"Vorher".to_owned()));
    a.save_entry(privat.clone(), login("Während des Laufs", "u", "3")).unwrap();
    let waited = t.elapsed();
    assert!(!done.load(std::sync::atomic::Ordering::SeqCst), "the synchronisation still runs");
    assert!(waited < std::time::Duration::from_millis(500), "the app waited {waited:?} for the server");
    run.join().unwrap().unwrap();

    // Nothing is lost: the entry saved meanwhile goes up with the next synchronisation.
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&b), vec!["Vor dem Lauf", "Vorher", "Während des Laufs"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// A server that stops at its first write until the test lets it go on (the upload phase).
struct GateFs {
    inner: crate::net::remote::LocalFs,
    armed: std::sync::atomic::AtomicBool,
    entered: std::sync::mpsc::Sender<()>,
    go_on: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl GateFs {
    fn new(dir: &std::path::Path) -> (GateFs, std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (entered, at_gate) = std::sync::mpsc::channel();
        let (release, go_on) = std::sync::mpsc::channel();
        let fs = GateFs {
            inner: crate::net::remote::LocalFs::new(dir),
            armed: std::sync::atomic::AtomicBool::new(true),
            entered,
            go_on: std::sync::Mutex::new(go_on),
        };
        (fs, at_gate, release)
    }
}

impl crate::net::remote::RemoteFs for GateFs {
    fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
        self.inner.list(dir)
    }
    fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
        self.inner.read(path)
    }
    fn write(&self, path: &str, data: &[u8]) -> crate::net::NetResult<()> {
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.send(()).unwrap();
            self.go_on.lock().unwrap().recv().unwrap();
        }
        self.inner.write(path, data)
    }
    fn delete(&self, path: &str) -> crate::net::NetResult<()> {
        self.inner.delete(path)
    }
    fn mkdir_all(&self, dir: &str) -> crate::net::NetResult<()> {
        self.inner.mkdir_all(dir)
    }
}

/// F147: while a synchronisation uploads, the app's calls answer at once, and an entry saved
/// meanwhile goes up with the next run.
#[test]
fn uploading_does_not_hold_the_suite() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-gate-a"), Dirs::new("sync-gate-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&a, "Privat");
    a.save_entry(privat.clone(), login("Vorher", "u", "1")).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    a.save_entry(privat.clone(), login("Vor dem Lauf", "u", "2")).unwrap();

    let (gate, at_gate, release) = GateFs::new(&server_dir);
    let a2 = a.clone();
    let run = std::thread::spawn(move || a2.sync_folder(&gate));
    at_gate.recv_timeout(std::time::Duration::from_secs(20)).expect("the run reaches its upload");
    // The app's calls in a thread of their own: if they waited for the upload, the test would
    // otherwise wait forever (the upload goes on only when the test lets it).
    let (a3, p3) = (a.clone(), privat.clone());
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let seen = titles(&a3).contains(&"Vor dem Lauf".to_owned());
        a3.save_entry(p3, login("Während des Hochladens", "u", "3")).unwrap();
        let _ = answered.send(seen);
    });
    // Generous for a busy test machine; before, the calls waited until the test gave up.
    let calls = answer.recv_timeout(std::time::Duration::from_secs(2));
    release.send(()).unwrap();
    assert_eq!(calls, Ok(true), "the app's calls waited for the upload");
    run.join().unwrap().unwrap();

    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&b), vec!["Vor dem Lauf", "Vorher", "Während des Hochladens"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// F147: locking during the upload stops it. That run does not count as the last
/// synchronisation, and the next run still reports what it took over (so the app reloads).
#[test]
fn locking_stops_an_upload_and_the_next_run_reports_it() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-gate-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-lock-a"), Dirs::new("sync-lock-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&a, "Privat");
    a.save_entry(privat.clone(), login("Von A", "u", "1")).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    let last = a.sync_status().unwrap().last_sync;
    assert!(last > 0);
    // B adds an entry on the server; A changes one too, so A's run merges and uploads.
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "u", "2")).unwrap();
    b.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    a.save_entry(privat.clone(), login("Noch von A", "u", "3")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));

    let (gate, at_gate, release) = GateFs::new(&server_dir);
    let a2 = a.clone();
    let run = std::thread::spawn(move || a2.sync_folder(&gate));
    at_gate.recv_timeout(std::time::Duration::from_secs(20)).expect("the run reaches its upload");
    a.lock();
    release.send(()).unwrap();
    assert!(matches!(run.join().unwrap(), Err(GhulError::Locked)));

    a.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(a.sync_status().unwrap().last_sync, last, "the stopped run is not the last synchronisation");
    assert!(titles(&a).contains(&"Von B".to_owned()), "merged here before the upload");
    let rep = a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    assert!(rep.downloaded + rep.merged > 0, "the next run reports what the stopped one took over: {rep:?}");
    assert!(a.sync_status().unwrap().last_sync > last);
    b.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    assert_eq!(titles(&b), vec!["Noch von A", "Von A", "Von B"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// F147: a run whose server was changed or removed after it connected neither merges nor
/// uploads (the next run uses the new server).
#[test]
fn a_run_whose_server_changed_meanwhile_does_nothing() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-changed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-changed-a"), Dirs::new("sync-changed-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "u", "1")).unwrap();
    b.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Von A", "u", "2")).unwrap();
    let before = std::fs::read(server_dir.join("suite").join(super::suite::SUITE_FILE)).unwrap();

    // Connected to a server that is no longer the configured one (synchronisation is off here).
    let mut pending = super::sync::SyncReport::default();
    let rep = a.sync_unlocked(&LocalFs::new(&server_dir), Some([7; 32]), &mut pending).unwrap();
    assert_eq!(rep, super::sync::SyncReport::default());
    assert_eq!(titles(&a), vec!["Von A"], "nothing merged");
    assert_eq!(
        std::fs::read(server_dir.join("suite").join(super::suite::SUITE_FILE)).unwrap(),
        before,
        "nothing uploaded"
    );
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// A server whose folder listing leaves out the settings file (some FTP listings do).
struct HidingFs(crate::net::remote::LocalFs);

impl crate::net::remote::RemoteFs for HidingFs {
    fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
        Ok(self.0.list(dir)?.into_iter().filter(|e| e.name != super::suite::SUITE_FILE).collect())
    }
    fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
        self.0.read(path)
    }
    fn write(&self, path: &str, data: &[u8]) -> crate::net::NetResult<()> {
        self.0.write(path, data)
    }
    fn delete(&self, path: &str) -> crate::net::NetResult<()> {
        self.0.delete(path)
    }
    fn mkdir_all(&self, dir: &str) -> crate::net::NetResult<()> {
        self.0.mkdir_all(dir)
    }
}

/// F147: the settings file is read by name, not only when the listing shows it; else a
/// password changed on another device would go unnoticed and be overwritten.
#[test]
fn a_listing_without_the_settings_file_still_notices_a_new_password() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-hiding-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-hiding-a"), Dirs::new("sync-hiding-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    a.change_password("pw-123456".into(), "neues-pw-789".into()).unwrap();
    a.sync_folder(&LocalFs::new(&server_dir)).unwrap();
    let hiding = HidingFs(LocalFs::new(&server_dir));
    assert!(matches!(b.sync_folder(&hiding), Err(GhulError::PasswordChangedElsewhere)));
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn sync_between_two_devices() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-sync-server-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sync-a"), Dirs::new("sync-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&a, "Privat");
    let shared = a.save_entry(privat.clone(), login("Shared", "u", "1")).unwrap();
    let r = a.sync_folder(&server).unwrap();
    assert_eq!(r.uploaded, 1);

    // Second device gets the files and unlocks with the same password and key file.
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&b), vec!["Shared"]);
    assert_eq!(b.sync_folder(&server).unwrap(), super::sync::SyncReport::default());

    // Both change different entries at the same time; the same entry changes on both sides.
    a.save_entry(privat.clone(), login("From A", "a", "x")).unwrap();
    b.save_entry(privat.clone(), login("From B", "b", "y")).unwrap();
    let mut on_a = a.get_entry(privat.clone(), shared.clone()).unwrap();
    on_a.fields[2] = FieldData { name: "password".into(), value: FieldValue::Secret { value: "A1".into() } };
    a.save_entry(privat.clone(), on_a.clone()).unwrap();
    let mut on_b = b.get_entry(privat.clone(), shared.clone()).unwrap();
    on_b.fields[2] = FieldData { name: "password".into(), value: FieldValue::Secret { value: "B1".into() } };
    b.save_entry(privat.clone(), on_b.clone()).unwrap();
    on_b.fields[2] = FieldData { name: "password".into(), value: FieldValue::Secret { value: "B2".into() } };
    b.save_entry(privat.clone(), on_b).unwrap(); // B changed it twice: higher revision wins
    a.sync_folder(&server).unwrap();
    let r = b.sync_folder(&server).unwrap();
    assert_eq!(r.merged, 1);
    a.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        assert_eq!(titles(s), vec!["From A", "From B", "Shared"]);
        let e = s.get_entry(privat.clone(), shared.clone()).unwrap();
        assert!(e
            .fields
            .contains(&FieldData { name: "password".into(), value: FieldValue::Secret { value: "B2".into() } }));
    }

    // Deleting an entry and a vault travels; a new vault travels.
    b.delete_entry(privat.clone(), shared.clone()).unwrap();
    let work = b.create_vault(VaultKind::Totp, "Arbeit".into()).unwrap();
    b.add_totp(work.clone(), totp_input()).unwrap();
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    assert_eq!(titles(&a), vec!["From A", "From B"]);
    assert_eq!(a.totp_codes(59).unwrap().len(), 1);
    a.remove_vault(work.clone()).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    assert!(b.list_vaults().unwrap().iter().all(|v| v.id != work));

    // A key file added on A reaches B without asking.
    let kf2 = generate_keyfile();
    a.add_keyfile(kf2.clone(), "Zweite".into()).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    b.lock();
    b.unlock("pw-123456".into(), None, None, None, Some(kf2.clone())).unwrap();

    // A password change on A: B has to type the new password once.
    a.change_password("pw-123456".into(), "neues-pw-789".into()).unwrap();
    a.sync_folder(&server).unwrap();
    assert!(matches!(b.sync_folder(&server), Err(GhulError::PasswordChangedElsewhere)));
    assert!(matches!(b.adopt_password_folder(&server, "falsch"), Err(GhulError::WrongPassword)));
    b.adopt_password_folder(&server, "neues-pw-789").unwrap();
    b.lock();
    b.unlock("neues-pw-789".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&b), vec!["From A", "From B"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// Archiving in GhulDrive against Dovecot (IMAP) and the SFTP test server: old mails become
/// `.eml` files in `Mail-Archiv/<account>/<year>`, unchanged at the provider (also unread), a
/// second run skips them, an archived mail reads like the original, a renamed archive folder is
/// still found, and without GhulDrive nothing is fetched.
#[test]
fn mail_archive_to_drive_against_imap_and_sftp() {
    use super::mail::{MailAccountInput, MailSecurity, MailServers};
    use super::maildrive::MailArchived;
    use super::servers::{ServerInput, ServerKind};
    let (Ok(host), Ok(fp)) = (std::env::var("GHUL_TEST_MAIL"), std::env::var("GHUL_TEST_MAIL_TLSFP")) else { return };
    let Some((shost, sport, suser, sroot, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("archive-drive");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let email = format!("archiv{run}@test.local");
    let account = s
        .mail_save_account(
            None,
            mail_checked(
                &s,
                MailAccountInput {
                    name: "Privat".into(),
                    display_name: "Archiv".into(),
                    email: email.clone(),
                    username: String::new(),
                    password: "test-pw-123".into(),
                    servers: MailServers {
                        imap_host: host.clone(),
                        imap_port: 10993,
                        imap_security: MailSecurity::Tls,
                        smtp_host: host.clone(),
                        smtp_port: 10587,
                        smtp_security: MailSecurity::StartTls,
                    },
                    imap_cert: Some(fp.clone()),
                    smtp_cert: Some(fp.clone()),
                    signature: String::new(),
                },
            ),
        )
        .unwrap();
    let raw = |n: u32, date: &str| {
        format!(
            "From: Erika Musterfrau <erika@example.net>\r\nTo: {email}\r\nSubject: Archivbetreff {n}\r\nDate: {date}\r\n\
             Message-ID: <a{n}.{run}@example.net>\r\n\r\nArchivtext {n}\r\n"
        )
        .into_bytes()
    };
    let put = |m: Vec<u8>, date: &str| {
        let date = date.to_owned();
        s.with_imap(&account, move |sess| {
            let (date, m) = (date.clone(), m.clone());
            Box::pin(async move { crate::net::imap::append_dated(sess, "INBOX", &m, &date).await })
        })
        .unwrap();
    };
    put(raw(1, "Sun, 2 Mar 2025 10:00:00 +0000"), "02-Mar-2025 10:00:00 +0000");
    put(raw(2, "Mon, 3 Mar 2025 10:00:00 +0000"), "03-Mar-2025 10:00:00 +0000");
    put(raw(3, "Tue, 7 May 2024 10:00:00 +0000"), "07-May-2024 10:00:00 +0000");
    let today = crate::net::imap::search_date(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 / 86_400,
    );
    let today = if today.find('-') == Some(1) { format!("0{today}") } else { today };
    put(raw(4, "Wed, 1 Jan 2025 10:00:00 +0000"), &format!("{today} 08:00:00 +0000"));

    // Without GhulDrive: nothing is fetched, the app says what is missing.
    let all = s.mail_archive_uids(account.clone(), "INBOX".into(), None).unwrap();
    assert_eq!(all.len(), 4);
    assert!(matches!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), all[0]), Err(GhulError::NotConfigured)));
    assert_eq!(s.mail_archive_folder(None).unwrap(), None);

    crate::net::tests::authorize_test_key(&authkeys, &s.ssh_public_key().unwrap());
    let input = ServerInput {
        kind: ServerKind::Sftp,
        name: "Heimserver".into(),
        host: shost,
        port: sport,
        user: suser,
        password: None,
        base: format!("{sroot}/archive-drive-{run}"),
    };
    let id = saved(s.save_server(input.clone(), Some(hostkey.clone())).unwrap());
    s.drive_setup(id).unwrap();

    // "Older than a week": the three old ones (received in 2024 and 2025), not today's.
    let week_ago =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 / 86_400 - 7;
    let old = s.mail_archive_uids(account.clone(), "INBOX".into(), Some(week_ago)).unwrap();
    assert_eq!(old.len(), 3, "{old:?}");
    for uid in &old {
        assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), *uid).unwrap(), MailArchived::Saved);
    }
    // A second run stores nothing twice.
    for uid in &old {
        assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), *uid).unwrap(), MailArchived::Skipped);
    }
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), 999_999).unwrap(), MailArchived::Gone);

    // Mail-Archiv/Privat/2024 and /2025, the files named by date, sender and subject.
    let root = s.mail_archive_folder(None).unwrap().expect("archive folder");
    let top = s.drive_list(None, true).unwrap();
    assert!(top.iter().any(|i| i.id == root && i.name == "Mail-Archiv"), "{top:?}");
    let acc = s.mail_archive_folder(Some(account.clone())).unwrap().expect("account folder");
    let years: Vec<String> = s.drive_list(Some(acc.clone()), false).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(years, vec!["2024", "2025"]);
    let y2025 = s.drive_list(Some(acc.clone()), false).unwrap().into_iter().find(|i| i.name == "2025").unwrap().id;
    let files = s.drive_list(Some(y2025), false).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].name.starts_with("2025-03-02 Erika Musterfrau - Archivbetreff 1 ("), "{}", files[0].name);
    assert!(files[0].name.ends_with(").eml"));

    // The copy is the mail as it was at the provider, and reads like it.
    let eml = download(&s, &files[0].id);
    assert_eq!(eml, s.mail_raw(account.clone(), "INBOX".into(), old[0], false).unwrap());
    let m = s.mail_parse_eml(account.clone(), eml).unwrap();
    assert_eq!(m.subject, "Archivbetreff 1");
    assert!(m.text.contains("Archivtext 1"));
    assert!(matches!(
        s.mail_parse_eml(account.clone(), b"\x00\x01kein Mail".to_vec()),
        Err(GhulError::InvalidInput { .. }) | Ok(_)
    ));

    // At the provider nothing changed: all four there, the archived ones still unread.
    let page = s.mail_list(account.clone(), "INBOX".into(), 0, 50).unwrap();
    assert_eq!(page.total, 4);
    assert!(page.messages.iter().all(|m| !m.seen), "archiving must not mark mails as read");

    // Automatic archiving: off by default; on, a run gets the old mails once, the next run
    // (after "done") nothing more, until a newer mail is old enough.
    assert_eq!(s.mail_archive_auto_days().unwrap(), 0);
    assert!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids.is_empty());
    s.mail_archive_set_auto_days(7).unwrap();
    let batch = s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap();
    assert_eq!(batch.uids, old);
    s.mail_archive_auto_done(account.clone(), "INBOX".into(), batch.validity, *old.last().unwrap()).unwrap();
    assert!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids.is_empty());
    // An older mark does not move it back; another UIDVALIDITY starts over.
    s.mail_archive_auto_done(account.clone(), "INBOX".into(), batch.validity, old[0]).unwrap();
    assert!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids.is_empty());
    s.mail_archive_auto_done(account.clone(), "INBOX".into(), batch.validity + 1, 9_999).unwrap();
    assert_eq!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids, old);
    s.mail_archive_auto_done(account.clone(), "INBOX".into(), batch.validity, *old.last().unwrap()).unwrap();
    // A mail moved in later keeps its old date but gets a higher UID than today's mail: after
    // archiving it, the mark stays below today's mail, which is archived once it is old enough.
    put(raw(6, "Sun, 2 Mar 2025 12:00:00 +0000"), "02-Mar-2025 12:00:00 +0000");
    let moved = s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap();
    assert_eq!(moved.uids.len(), 1);
    s.mail_archive_auto_done(account.clone(), "INBOX".into(), moved.validity, moved.uids[0]).unwrap();
    assert_eq!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids, moved.uids, "mark held back");
    s.mail_archive_set_auto_days(0).unwrap();
    assert!(s.mail_archive_auto_uids(account.clone(), "INBOX".into()).unwrap().uids.is_empty());

    // Renamed and moved, the archive is still the archive: nothing is stored twice.
    s.drive_rename(root.clone(), "Alte Mails".into()).unwrap();
    let elsewhere = s.drive_create_folder(None, "Ablage".into()).unwrap();
    s.drive_move(root.clone(), Some(elsewhere)).unwrap();
    assert_eq!(s.mail_archive_folder(None).unwrap(), Some(root.clone()));
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), old[0]).unwrap(), MailArchived::Skipped);

    // Another mail that only claims the Message-ID of an archived one is not taken for it:
    // it is stored under a code of its own content (once).
    let fake = String::from_utf8(raw(1, "Sun, 2 Mar 2025 10:00:00 +0000"))
        .unwrap()
        .replace("Archivtext 1", "Bitte sofort hier klicken und bezahlen");
    put(fake.into_bytes(), "02-Mar-2025 10:00:00 +0000");
    let fake_uid = *s.mail_archive_uids(account.clone(), "INBOX".into(), None).unwrap().last().unwrap();
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), fake_uid).unwrap(), MailArchived::Saved);
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), fake_uid).unwrap(), MailArchived::Skipped);
    let y2025 = s.drive_list(Some(acc.clone()), false).unwrap().into_iter().find(|i| i.name == "2025").unwrap().id;
    let named: Vec<String> = s.drive_list(Some(y2025), false).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(named.len(), 3, "{named:?}");
    assert_eq!(named.iter().filter(|n| n.starts_with("2025-03-02 Erika Musterfrau - Archivbetreff 1 (")).count(), 2);
    // Even with the same size: a forged copy archived first does not stop the real mail.
    let real = raw(5, "Mon, 3 Mar 2025 11:00:00 +0000");
    let forged = String::from_utf8(real.clone()).unwrap().replace("Archivtext 5", "Gefaelscht 5").into_bytes();
    assert_eq!(forged.len(), real.len());
    put(forged, "03-Mar-2025 11:00:00 +0000");
    put(real, "03-Mar-2025 11:00:00 +0000");
    let uids = s.mail_archive_uids(account.clone(), "INBOX".into(), None).unwrap();
    let (forged_uid, real_uid) = (uids[uids.len() - 2], uids[uids.len() - 1]);
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), forged_uid).unwrap(), MailArchived::Saved);
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), real_uid).unwrap(), MailArchived::Saved);
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), real_uid).unwrap(), MailArchived::Skipped);

    // In the trash, it no longer counts: the next mail starts a new archive folder.
    s.drive_delete(root.clone()).unwrap();
    assert_eq!(s.mail_archive_folder(None).unwrap(), None);
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), old[0]).unwrap(), MailArchived::Saved);
    assert_ne!(s.mail_archive_folder(None).unwrap(), Some(root));

    // The archive's own storage location: mails go there while another one is shown.
    let second = saved(
        s.drive_save_server(ServerInput { base: format!("{sroot}/archive-drive-{run}-b"), ..input }, Some(hostkey))
            .unwrap(),
    );
    let first = s.drive_storage().unwrap().server.unwrap().id;
    s.drive_set_archive_location(second.clone()).unwrap();
    let before = s.drive_list(None, true).unwrap();
    assert_eq!(s.mail_archive_to_drive(account.clone(), "INBOX".into(), old[1]).unwrap(), MailArchived::Saved);
    assert_eq!(s.drive_storage().unwrap().server.unwrap().id, first, "the shown location stays");
    assert_eq!(s.drive_list(None, true).unwrap(), before, "nothing new where the archive does not go");
    assert!(s.mail_archive_folder(None).unwrap().is_some(), "the archive is found in its location");
    s.drive_switch_location(second).unwrap();
    let top: Vec<String> = s.drive_list(None, true).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(top, vec!["Mail-Archiv"]);
    let _ = std::fs::remove_dir_all(format!("{sroot}/archive-drive-{run}"));
    let _ = std::fs::remove_dir_all(format!("{sroot}/archive-drive-{run}-b"));
}

#[test]
fn sync_over_sftp() {
    use super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let authorize = |public: &str| {
        crate::net::tests::authorize_test_key(&authkeys, public);
    };
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("sftp-a"), Dirs::new("sftp-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    a.save_entry(vault_id(&a, "Privat"), login("Über SFTP", "u", "p")).unwrap();
    authorize(&a.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let base = format!("{root}/sync-{run}");
    let input = ServerInput {
        kind: super::servers::ServerKind::Sftp,
        name: "Test".into(),
        host: host.clone(),
        port,
        user: user.clone(),
        password: None,
        base: base.clone(),
    };
    // "Check connection" stores nothing: first the fingerprint, then (confirmed) it works.
    use super::servers::ServerTest;
    assert_eq!(a.server_test(input.clone(), None).unwrap(), ServerTest::Confirm { fingerprint: hostkey.clone() });
    assert_eq!(a.server_test(input.clone(), Some(hostkey.clone())).unwrap(), ServerTest::Ok);
    assert!(a.list_servers().unwrap().is_empty(), "checking stores nothing");
    let mut wrong_user = input.clone();
    wrong_user.user = "niemand-hier".into();
    assert!(a.server_test(wrong_user, Some(hostkey.clone())).is_err());
    // First the fingerprint is shown; then the server is stored with the confirmed fingerprint.
    assert_eq!(a.save_server(input.clone(), None).unwrap(), ServerSaved::Confirm { fingerprint: hostkey.clone() });
    let id = saved(a.save_server(input.clone(), Some(hostkey.clone())).unwrap());
    assert_eq!(a.list_servers().unwrap()[0].id, id);
    a.set_sync_server(Some(id.clone())).unwrap();
    let r = a.sync_now().unwrap();
    assert_eq!(r.uploaded, 1);
    assert!(a.sync_status().unwrap().last_sync > 0);

    // New device: fetch with its own key, unlock, synchronise with the suite's settings.
    let b = db.suite();
    let device = super::servers::generate_ssh_key().unwrap();
    authorize(&device.public_key);
    assert_eq!(b.fetch_from_server(input.clone(), None, Some(device.private_key.clone())).unwrap(), hostkey);
    assert_eq!(b.fetch_from_server(input, Some(hostkey), Some(device.private_key)).unwrap(), "");
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&b), vec!["Über SFTP"]);
    b.save_entry(vault_id(&b, "Privat"), login("Von B", "b", "q")).unwrap();
    b.sync_now().unwrap();
    a.sync_now().unwrap();
    assert_eq!(titles(&a), vec!["Von B", "Über SFTP"]);
}

fn upload(s: &GhulSuite, folder: Option<String>, name: &str, data: &[u8]) -> String {
    let up = s.drive_upload_begin(folder, name.into()).unwrap();
    for chunk in data.chunks(super::drive::BLOCK_SIZE as usize) {
        s.drive_upload_chunk(up.clone(), chunk.to_vec()).unwrap();
    }
    s.drive_upload_finish(up).unwrap()
}

fn download(s: &GhulSuite, id: &str) -> Vec<u8> {
    let n = s.drive_file_blocks(id.into()).unwrap();
    (0..n).flat_map(|i| s.drive_download_block(id.into(), i).unwrap()).collect()
}

fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// An upload that stopped half-way continues with the same file key: the blocks already on the
/// server are not written again, and a finished upload forgets its key (the next content of the
/// same file gets a new one). The device's note of unfinished uploads is encrypted.
#[test]
fn drive_upload_continues_with_its_key() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-drive-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let d = Dirs::new("drive-resume");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let fs: std::sync::Arc<dyn crate::net::remote::RemoteFs> = std::sync::Arc::new(LocalFs::new(&server_dir));
    s.drive_use_folder(fs).unwrap();
    let block = super::drive::BLOCK_SIZE as usize;
    let data: Vec<u8> = (0..3 * block as u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
    // Two of three parts, then the app is gone (no finish, no cancel).
    let up = s.drive_upload_begin(None, "Film Geheimname.mp4".into()).unwrap();
    s.drive_upload_chunk(up.clone(), data[..block].to_vec()).unwrap();
    s.drive_upload_chunk(up, data[block..2 * block].to_vec()).unwrap();
    let written: Vec<_> = files_under(&server_dir.join("drive/blocks"))
        .iter()
        .map(|p| (p.clone(), std::fs::metadata(p).unwrap().modified().unwrap()))
        .collect();
    assert_eq!(written.len(), 2);
    let note = std::fs::read(std::path::Path::new(&d.local()).join("drive-pending")).expect("unfinished upload noted");
    assert!(!note.windows(11).any(|w| w == b"Geheimname."), "the note is encrypted");
    // The same file again: the first two blocks are there and stay untouched.
    let id = upload(&s, None, "Film Geheimname.mp4", &data);
    for (p, at) in &written {
        assert_eq!(std::fs::metadata(p).unwrap().modified().unwrap(), *at, "{p:?} written again");
    }
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 3);
    assert_eq!(download(&s, &id), data);
    assert!(!std::path::Path::new(&d.local()).join("drive-pending").exists(), "nothing unfinished any more");
    // New content under the same name: a new key, so all of its blocks are new.
    let mut edited = data.clone();
    edited[0] ^= 1;
    upload(&s, None, "Film Geheimname.mp4", &edited);
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 6, "3 for the version, 3 new");

    // A finished upload whose note stayed behind (the app was gone right after finishing): its
    // key belongs to that content now and is not used for the next upload of the same name.
    let pending = std::path::Path::new(&d.local()).join("drive-pending");
    let up = s.drive_upload_begin(None, "Notiz.txt".into()).unwrap();
    s.drive_upload_chunk(up.clone(), b"erste Fassung".to_vec()).unwrap();
    let stale = std::fs::read(&pending).unwrap();
    s.drive_upload_finish(up).unwrap();
    std::fs::write(&pending, stale).unwrap();
    let before = files_under(&server_dir.join("drive/blocks")).len();
    upload(&s, None, "Notiz.txt", b"erste Fassung");
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), before + 1, "a key of its own: a new block");
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// When the list of shares on the server cannot be read, GhulDrive deletes no block at all
/// (it cannot know which ones a link still hands out).
#[test]
fn drive_deletes_nothing_when_the_shares_cannot_be_read() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-drive-pinned-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let d = Dirs::new("drive-pinned");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let fs: std::sync::Arc<dyn crate::net::remote::RemoteFs> = std::sync::Arc::new(LocalFs::new(&server_dir));
    s.drive_use_folder(fs).unwrap();
    let id = upload(&s, None, "weg.txt", b"bald geloescht");
    s.drive_delete(id).unwrap();
    // `shares` is not a folder that can be listed: emptying the trash fails, the block stays.
    std::fs::write(server_dir.join("shares"), b"kein Ordner").unwrap();
    assert!(s.drive_empty_trash(0).is_err());
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 1);
    // A cancelled upload keeps its blocks as well while that is so.
    let up = s.drive_upload_begin(None, "abbruch.bin".into()).unwrap();
    s.drive_upload_chunk(up.clone(), vec![1, 2, 3]).unwrap();
    assert!(s.drive_upload_cancel(up.clone()).is_err());
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 2);
    // Readable again: everything is cleaned up.
    std::fs::remove_file(server_dir.join("shares")).unwrap();
    s.drive_empty_trash(0).unwrap();
    s.drive_upload_cancel(up).unwrap();
    assert!(files_under(&server_dir.join("drive/blocks")).is_empty());

    // A description without expiry or limit and with odd entries (the module still serves
    // it) pins its blocks all the same.
    let kept = upload(&s, None, "geteilt.txt", b"per Hand geteilt");
    let block = files_under(&server_dir.join("drive/blocks"))[0].file_name().unwrap().to_string_lossy().into_owned();
    std::fs::create_dir_all(server_dir.join("shares")).unwrap();
    std::fs::write(
        server_dir.join("shares/0123456789abcdef0123456789abcdef.json"),
        format!(r#"{{"package":"x","files":[["{block}", 7, "../x"]]}}"#),
    )
    .unwrap();
    s.drive_delete(kept).unwrap();
    s.drive_empty_trash(0).unwrap();
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 1, "still handed out by the link");
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn drive_files_and_folders() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-drive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let d = Dirs::new("drive");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let fs: std::sync::Arc<dyn crate::net::remote::RemoteFs> = std::sync::Arc::new(LocalFs::new(&server_dir));
    s.drive_use_folder(fs.clone()).unwrap();
    assert!(s.drive_list(None, true).unwrap().is_empty());

    let photos = s.drive_create_folder(None, "Fotos".into()).unwrap();
    let big: Vec<u8> = (0..9_500_000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
    let id = upload(&s, Some(photos.clone()), "urlaub.jpg", &big);
    assert_eq!(s.drive_file_blocks(id.clone()).unwrap(), 3);
    assert_eq!(download(&s, &id), big);
    // Same content again under another name: its own key and its own blocks, so the server
    // cannot see that two files are equal (and a share of one reveals nothing of the other).
    let first_blocks = files_under(&server_dir.join("drive/blocks"));
    let copy = upload(&s, Some(photos.clone()), "urlaub kopie.jpg", &big);
    let blocks_after = files_under(&server_dir.join("drive/blocks"));
    assert_eq!(blocks_after.len(), first_blocks.len() + 3);
    assert!(first_blocks.iter().all(|b| blocks_after.contains(b)));
    assert_eq!(download(&s, &copy), big);
    // Blocks are AES-256-GCM (browsers can open them with the file's key alone).
    let first = std::fs::read(&blocks_after[0]).unwrap();
    assert_eq!(&first[..8], b"GHULBLK2");
    // Same name again with new content: a new version, the old one is kept.
    let edited: Vec<u8> = big.iter().rev().copied().collect();
    assert_eq!(upload(&s, Some(photos.clone()), "urlaub.jpg", &edited), id);
    let names: Vec<String> = s.drive_list(Some(photos.clone()), false).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["urlaub kopie.jpg", "urlaub.jpg"]);
    assert_eq!(s.drive_versions(id.clone()).unwrap().len(), 1);
    assert_eq!(download(&s, &id), edited);
    s.drive_restore_version(id.clone(), 0).unwrap();
    assert_eq!(download(&s, &id), big);
    let blocks_before = files_under(&server_dir.join("drive/blocks")).len();

    // Nothing readable on the server: no names, no content.
    let secret = b"Mein geheimer Text, der nicht auf dem Server lesbar sein darf.";
    let note = upload(&s, None, "notiz.txt", secret);
    for f in files_under(&server_dir) {
        let raw = std::fs::read(&f).unwrap();
        assert!(!raw.windows(secret.len()).any(|w| w == secret), "{f:?} contains plaintext");
        assert!(!raw.windows(6).any(|w| w == b"urlaub"), "{f:?} contains a file name");
    }

    // Rename, move, search, delete with clean-up of unused blocks.
    s.drive_rename(note.clone(), "Notiz.md".into()).unwrap();
    let docs = s.drive_create_folder(None, "Dokumente".into()).unwrap();
    s.drive_move(note.clone(), Some(docs.clone())).unwrap();
    assert!(matches!(s.drive_move(docs.clone(), Some(docs.clone())), Err(GhulError::InvalidInput { .. })));
    assert_eq!(s.drive_search("notiz".into()).unwrap()[0].parent.as_deref(), Some(docs.as_str()));
    // Delete = trash: restorable, hidden from lists, still on the server.
    s.drive_delete(copy.clone()).unwrap();
    assert_eq!(s.drive_trash().unwrap()[0].id, copy);
    assert_eq!(s.drive_list(Some(photos.clone()), false).unwrap().len(), 1);
    s.drive_restore(copy.clone()).unwrap();
    assert_eq!(s.drive_list(Some(photos.clone()), false).unwrap().len(), 2);
    s.drive_delete(copy.clone()).unwrap();
    assert_eq!(s.drive_empty_trash(30).unwrap(), 0, "younger than 30 days stays");
    assert_eq!(s.drive_empty_trash(0).unwrap(), 1);
    let left = files_under(&server_dir.join("drive/blocks"));
    assert_eq!(left.len(), blocks_before - 3 + 1, "the original, its version and the note; the copy's blocks are gone");
    let copy_blocks: Vec<&std::path::PathBuf> = blocks_after.iter().filter(|b| !first_blocks.contains(b)).collect();
    assert!(copy_blocks.iter().all(|b| !left.contains(b)), "the copy's own blocks are deleted");
    // Favourites and usage.
    s.drive_set_favorite(note.clone(), true).unwrap();
    assert_eq!(s.drive_favorites().unwrap()[0].id, note);
    let u = s.drive_usage().unwrap();
    assert_eq!((u.images, u.documents), (big.len() as u64, secret.len() as u64));
    assert_eq!(u.versions, edited.len() as u64);
    // Offline copy on this device: still encrypted, opens even when the server copy is gone.
    s.drive_set_offline(note.clone(), true).unwrap();
    assert!(s.drive_list(Some(docs.clone()), false).unwrap()[0].offline);
    // Encrypted ZIP with a password.
    let z = s.drive_zip(vec![note.clone()], "Zip-Passwort-1".into()).unwrap();
    let mut a = zip::ZipArchive::new(std::io::Cursor::new(z)).unwrap();
    {
        use std::io::Read;
        let mut f = a.by_index_decrypt(0, b"Zip-Passwort-1").unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, secret);
    }
    assert!(a.by_index_decrypt(0, b"falsch").is_err());
    assert!(s.drive_zip(vec![note.clone()], "kurz".into()).is_err());
    s.drive_delete(photos.clone()).unwrap();
    s.drive_empty_trash(0).unwrap();
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 1, "only the note's block is left");
    let status = s.drive_status().unwrap();
    assert_eq!((status.files, status.bytes), (1, secret.len() as u64));

    // A cancelled upload leaves nothing behind.
    let up = s.drive_upload_begin(None, "abbruch.bin".into()).unwrap();
    s.drive_upload_chunk(up.clone(), vec![1, 2, 3]).unwrap();
    s.drive_upload_cancel(up).unwrap();
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), 1);

    // After locking, the index is read again from the server.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, None).err();
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn drive_over_sftp_two_devices() {
    use super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let authorize = |public: &str| {
        crate::net::tests::authorize_test_key(&authkeys, public);
    };
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("drive-a"), Dirs::new("drive-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    authorize(&a.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let input = ServerInput {
        kind: super::servers::ServerKind::Sftp,
        name: "S".into(),
        host,
        port,
        user,
        password: None,
        base: format!("{root}/drive-{run}"),
    };
    let id = saved(a.save_server(input.clone(), Some(hostkey.clone())).unwrap());
    a.drive_setup(id.clone()).unwrap();
    a.set_sync_server(Some(id)).unwrap();
    let file = upload(&a, None, "bericht.pdf", b"%PDF-1.7 test");
    a.sync_now().unwrap();

    // Second device: the Drive key comes with the suite settings.
    let b = db.suite();
    let device = super::servers::generate_ssh_key().unwrap();
    authorize(&device.public_key);
    b.fetch_from_server(input, Some(hostkey), Some(device.private_key)).unwrap();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(b.drive_list(None, true).unwrap()[0].name, "bericht.pdf");
    assert_eq!(download(&b, &file), b"%PDF-1.7 test");
}

/// The order of the emulator flow: GhulDrive on its own location, then the synchronisation is
/// set up and runs, then a second location is added and the synchronisation runs again. The
/// new location must still be there (bug: it vanished after the next synchronisation).
#[test]
fn a_location_added_after_the_first_sync_stays() {
    use super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("location-after-sync");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let mut keys = std::fs::read_to_string(&authkeys).unwrap_or_default();
    keys.push_str(&s.ssh_public_key().unwrap());
    keys.push('\n');
    std::fs::write(&authkeys, keys).unwrap();
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let input = |base: String| ServerInput {
        kind: super::servers::ServerKind::Sftp,
        name: String::new(),
        host: host.clone(),
        port,
        user: user.clone(),
        password: None,
        base,
    };
    let first = saved(s.drive_save_server(input(format!("{root}/loc-a-{run}")), Some(hostkey.clone())).unwrap());
    upload(&s, None, "a.txt", b"A");
    let sync = saved(s.save_server(input(format!("{root}/loc-sync-{run}")), Some(hostkey.clone())).unwrap());
    s.set_sync_server(Some(sync)).unwrap();
    s.sync_now().unwrap();
    let second = saved(s.drive_save_server(input(format!("{root}/loc-b-{run}")), Some(hostkey.clone())).unwrap());
    let ids = |s: &GhulSuite| s.drive_locations().unwrap().into_iter().map(|l| l.server.id).collect::<Vec<_>>();
    assert_eq!(ids(&s), vec![first.clone(), second.clone()]);
    s.sync_now().unwrap();
    assert_eq!(ids(&s), vec![first.clone(), second.clone()], "kept after the synchronisation");
    s.sync_now().unwrap();
    assert_eq!(ids(&s), vec![first, second], "and after the next one");
}

#[test]
fn mail_with_imap_and_smtp() {
    use super::mail::{
        mail_poll, mail_suggest_servers, MailAccountInput, MailCheck, MailSecurity, MailServers, OutgoingAttachment,
        OutgoingMail,
    };
    let (Ok(host), Ok(fp)) = (std::env::var("GHUL_TEST_MAIL"), std::env::var("GHUL_TEST_MAIL_TLSFP")) else { return };
    assert_eq!(mail_suggest_servers("max@posteo.de".into()).imap_host, "posteo.de");
    assert_eq!(mail_suggest_servers("max@example.org".into()).smtp_host, "smtp.example.org");

    let d = Dirs::new("mail");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let email = format!("alice{run}@test.local");
    let mut input = MailAccountInput {
        name: "Test".into(),
        display_name: "Alice Ä.".into(),
        email: email.clone(),
        username: email.clone(),
        password: "test-pw-123".into(),
        servers: MailServers {
            imap_host: host.clone(),
            imap_port: 10993,
            imap_security: MailSecurity::Tls,
            smtp_host: host.clone(),
            smtp_port: 10587,
            smtp_security: MailSecurity::StartTls,
        },
        imap_cert: None,
        smtp_cert: None,
        signature: "Gruß, Alice".into(),
    };
    // "Check connection", step by step. Self-signed certificates are shown with their
    // fingerprint until confirmed; nothing is saved by checking.
    let steps = |t: &super::mail::MailTest| -> Vec<(String, bool, String)> {
        t.steps.iter().map(|s| (s.step.clone(), s.ok, s.detail.clone())).collect()
    };
    let t = s.mail_test_account(None, input.clone()).unwrap();
    assert!(!t.ok);
    assert_eq!(t.confirm, Some(MailCheck::ImapCertificate { fingerprint: fp.clone() }));
    assert_eq!(steps(&t)[0], ("imap_server".into(), false, super::mail::STEP_CERTIFICATE.into()));
    assert_eq!(steps(&t)[1], ("imap_login".into(), false, super::mail::STEP_SKIPPED.into()));
    assert_eq!(
        steps(&t)[2],
        ("smtp_server".into(), false, super::mail::STEP_CERTIFICATE.into()),
        "SMTP is checked too"
    );
    input.imap_cert = Some(fp.clone());
    let t = s.mail_test_account(None, input.clone()).unwrap();
    assert_eq!(t.confirm, Some(MailCheck::SmtpCertificate { fingerprint: fp.clone() }));
    assert!(t.steps[0].ok && t.steps[1].ok && !t.steps[2].ok);
    input.smtp_cert = Some(fp.clone());
    // A wrong password: both servers answer, both logins fail, and it says so.
    let mut wrong = input.clone();
    wrong.password = "falsch".into();
    let t = s.mail_test_account(None, wrong.clone()).unwrap();
    assert_eq!(t.steps.iter().map(|s| s.ok).collect::<Vec<_>>(), vec![true, false, true, false], "{:?}", t.steps);
    assert!(t.steps[1].detail.contains("login rejected"));
    // A server that is not there: its login is not tried.
    let mut nowhere = input.clone();
    nowhere.servers.smtp_port = 1;
    let t = s.mail_test_account(None, nowhere.clone()).unwrap();
    assert!(t.steps[0].ok && t.steps[1].ok && !t.steps[2].ok);
    assert_eq!(t.steps[2].detail, super::mail::STEP_REFUSED, "said in words, not the system's text");
    assert_eq!(t.steps[3].detail, super::mail::STEP_SKIPPED);
    let mut unknown = input.clone();
    unknown.servers.imap_host = "imap.nirgendwo.invalid".into();
    assert_eq!(s.mail_test_account(None, unknown).unwrap().steps[0].detail, super::mail::STEP_NOT_FOUND);
    // Not checked, or not checked successfully: not saved.
    assert!(matches!(
        s.mail_save_account(None, nowhere),
        Err(GhulError::InvalidInput { msg }) if msg == super::mail::MAIL_NOT_TESTED
    ));
    assert!(s.mail_save_account(None, wrong).is_err());
    // IMAP only with TLS from the start: STARTTLS is refused before connecting.
    let mut starttls = input.clone();
    starttls.servers.imap_port = 10143;
    starttls.servers.imap_security = MailSecurity::StartTls;
    assert!(matches!(s.mail_test_account(None, starttls.clone()), Err(GhulError::InvalidInput { .. })));
    assert!(s.mail_save_account(None, starttls).is_err());
    // B21: the field "Login name" usually stays empty; the check then logs in with the address,
    // like the saved account does (before, it logged in with an empty name and always failed).
    let mut no_login = input.clone();
    no_login.username = String::new();
    no_login.email = format!(" {email} ");
    assert!(s.mail_test_account(None, no_login).unwrap().ok);
    // A fingerprint confirmed for another server does not count: asked again for the real one.
    let mut other_pin = input.clone();
    other_pin.imap_cert = Some("00".repeat(32));
    let t = s.mail_test_account(None, other_pin).unwrap();
    assert_eq!(t.confirm, Some(MailCheck::ImapCertificate { fingerprint: fp.clone() }));
    // A check does not outlive locking.
    assert!(s.mail_test_account(None, input.clone()).unwrap().ok);
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.mail_save_account(None, input.clone()).is_err(), "checked before locking");
    // Checked successfully, then changed: the change is not checked, so not saved.
    assert!(s.mail_test_account(None, input.clone()).unwrap().ok);
    let mut changed = input.clone();
    changed.servers.imap_port = 10994;
    assert!(s.mail_save_account(None, changed).is_err());

    let id = s.mail_save_account(None, input.clone()).unwrap();
    assert_eq!(s.mail_accounts().unwrap()[0].email, email);
    let roles: Vec<String> = s.mail_folders(id.clone()).unwrap().into_iter().map(|f| f.role).collect();
    assert_eq!(&roles[..1], &["inbox".to_string()]);
    assert!(roles.contains(&"sent".to_string()) && roles.contains(&"trash".to_string()));

    let poll = s.mail_poll_config(id.clone()).unwrap();
    let before = mail_poll(poll.clone(), 0).unwrap();
    assert_eq!(before.new_messages, 0);

    // Push: a waiting check (IMAP IDLE) returns as soon as the mail is there, not after its 5 minutes.
    let started = std::time::Instant::now();
    let waiting = {
        let (poll, since) = (poll.clone(), before.next_uid);
        std::thread::spawn(move || super::mail::mail_wait(poll, since, 300).unwrap())
    };
    std::thread::sleep(std::time::Duration::from_millis(500));

    let attachment: Vec<u8> = (0..70_000u32).map(|i| (i % 253) as u8).collect();
    s.mail_send(OutgoingMail {
        account_id: id.clone(),
        to: vec![email.clone()],
        cc: vec![],
        bcc: vec![],
        subject: "Grüße aus GhulMail".into(),
        text: "Hallo!\n.\nZeile mit Punkt.".into(),
        in_reply_to: String::new(),
        attachments: vec![OutgoingAttachment {
            name: "daten.bin".into(),
            mime: "application/octet-stream".into(),
            data: attachment.clone(),
        }],
        pgp: true,
    })
    .unwrap();

    let pushed = waiting.join().unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "{:?}", started.elapsed());
    assert_eq!(pushed.new_messages, 1);
    assert!(pushed.newest_uid > 0 && pushed.newest_uid < pushed.next_uid);
    // Nothing new: waits for the given time and reports nothing.
    let quiet = std::time::Instant::now();
    assert_eq!(super::mail::mail_wait(poll.clone(), pushed.next_uid, 2).unwrap().new_messages, 0);
    assert!(quiet.elapsed() >= std::time::Duration::from_secs(2));

    let after = mail_poll(poll.clone(), before.next_uid).unwrap();
    assert_eq!(after.new_messages, 1);
    assert_eq!(after.unseen, 1);
    // A mail to an own address is encrypted (the key is known): outside it reads "...".
    assert_eq!(after.newest_subject, "...");
    assert_eq!(after.newest_from, "Alice Ä.");

    let page = s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap();
    assert_eq!(page.total, 1);
    let m = &page.messages[0];
    // Encrypted: from outside nobody (not even the list) sees that there is an attachment.
    assert!(!m.seen && !m.has_attachments);
    assert_eq!(m.from_address, email);
    let sent = s.mail_list(id.clone(), "Sent".into(), 0, 50).unwrap();
    assert_eq!(sent.total, 1);
    assert!(sent.messages[0].seen);

    let full = s.mail_get(id.clone(), "INBOX".into(), m.uid).unwrap();
    assert!(full.encrypted);
    assert_eq!(full.subject, "Grüße aus GhulMail");
    assert!(full.text.contains("Hallo!\n.\nZeile mit Punkt."), "{}", full.text);
    assert!(full.text.contains("Gruß, Alice"));
    assert_eq!(full.to, vec![email.clone()]);
    assert_eq!(full.attachments.len(), 1);
    assert_eq!(full.attachments[0].name, "daten.bin");
    assert_eq!(s.mail_attachment(id.clone(), "INBOX".into(), m.uid, 0).unwrap(), attachment);
    assert!(s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap().messages[0].seen);
    assert_eq!(mail_poll(poll.clone(), 0).unwrap().unseen, 0);

    // Reply with threading header.
    s.mail_send(OutgoingMail {
        account_id: id.clone(),
        to: vec![format!("Alice <{email}>")],
        cc: vec![],
        bcc: vec![],
        subject: "Re: Grüße aus GhulMail".into(),
        text: "Antwort".into(),
        in_reply_to: full.message_id.clone(),
        attachments: vec![],
        pgp: true,
    })
    .unwrap();
    let page = s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap();
    assert_eq!(page.total, 2);
    // Both are encrypted: the server only sees "...", the newest is the reply.
    let reply = page.messages.iter().map(|x| x.uid).max().unwrap();
    let raw_reply = s.mail_get(id.clone(), "INBOX".into(), reply).unwrap();
    assert_eq!(raw_reply.subject, "Re: Grüße aus GhulMail");
    assert_eq!(raw_reply.text.lines().next(), Some("Antwort"));

    // The server cannot search inside encrypted mail, only in what stays outside (sender).
    assert_eq!(s.mail_search(id.clone(), "INBOX".into(), "Antwort".into()).unwrap().len(), 0);
    assert_eq!(s.mail_search(id.clone(), "INBOX".into(), "Alice".into()).unwrap().len(), 2);

    s.mail_set_flagged(id.clone(), "INBOX".into(), m.uid, true).unwrap();
    s.mail_set_seen(id.clone(), "INBOX".into(), reply, false).unwrap();
    let page = s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap();
    assert!(page.messages.iter().find(|x| x.uid == m.uid).unwrap().flagged);
    assert!(!page.messages.iter().find(|x| x.uid == reply).unwrap().seen);

    // Flagged view and labels (IMAP keywords).
    assert_eq!(
        s.mail_flagged(id.clone(), "INBOX".into()).unwrap().iter().map(|x| x.uid).collect::<Vec<_>>(),
        vec![m.uid]
    );
    s.mail_set_label(id.clone(), "INBOX".into(), m.uid, "Rechnungen 2026".into(), true).unwrap();
    let page = s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap();
    let tagged = page.messages.iter().find(|x| x.uid == m.uid).unwrap();
    assert_eq!(tagged.labels, vec!["Rechnungen 2026".to_string()]);
    assert!(tagged.encrypted && tagged.preview.is_empty(), "encrypted mail has no preview");
    assert_eq!(
        tagged.thread_key,
        page.messages.iter().find(|x| x.uid == reply).unwrap().thread_key,
        "reply is in the same conversation"
    );
    s.mail_set_label(id.clone(), "INBOX".into(), m.uid, "Rechnungen 2026".into(), false).unwrap();

    // Original (headers) and .eml: encrypted as stored, or decrypted on request.
    let headers = s.mail_headers(id.clone(), "INBOX".into(), m.uid).unwrap();
    assert!(headers.contains("Subject: ...") && !headers.contains("Grüße"), "{headers}");
    let eml = s.mail_raw(id.clone(), "INBOX".into(), m.uid, false).unwrap();
    assert!(String::from_utf8_lossy(&eml).contains("BEGIN PGP MESSAGE"));
    let plain = s.mail_raw(id.clone(), "INBOX".into(), m.uid, true).unwrap();
    assert!(String::from_utf8_lossy(&plain).contains("Zeile mit Punkt"));

    // Drafts (in the suite, encrypted) and address suggestions from sent mail.
    let draft = s
        .mail_save_draft(super::mailx::MailDraftData {
            account_id: id.clone(),
            to: "bob@example.org".into(),
            subject: "Entwurf".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(s.mail_drafts().unwrap()[0].subject, "Entwurf");
    // Attachments stay with the draft; too large ones are refused.
    let pdf = OutgoingAttachment { name: "a.pdf".into(), mime: "application/pdf".into(), data: vec![7u8; 2048] };
    let mut with_file = s.mail_drafts().unwrap()[0].clone();
    with_file.attachments = vec![pdf.clone()];
    s.mail_save_draft(with_file.clone()).unwrap();
    assert_eq!(s.mail_drafts().unwrap()[0].attachments, vec![pdf]);
    with_file.attachments = vec![OutgoingAttachment {
        name: "big.bin".into(),
        mime: "application/octet-stream".into(),
        data: vec![0u8; 10 * 1024 * 1024 + 1],
    }];
    assert!(s.mail_save_draft(with_file).is_err());
    s.mail_delete_draft(draft).unwrap();
    assert!(s.mail_drafts().unwrap().is_empty());
    assert_eq!(s.mail_contacts("".into(), 5).unwrap()[0].email, email.to_lowercase());

    // Send later: nothing before the time, sent once when due.
    let later = OutgoingMail {
        account_id: id.clone(),
        to: vec![email.clone()],
        cc: vec![],
        bcc: vec![],
        subject: "Später".into(),
        text: "Geplant".into(),
        in_reply_to: String::new(),
        attachments: vec![],
        pgp: true,
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    s.mail_schedule(later, now + 3_600_000).unwrap();
    assert_eq!(s.mail_send_due(now).unwrap(), 0);
    assert_eq!(s.mail_scheduled().unwrap().len(), 1);
    assert_eq!(s.mail_send_due(now + 3_600_001).unwrap(), 1);
    assert!(s.mail_scheduled().unwrap().is_empty());
    let after_send = s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap().total;
    assert_eq!(after_send, 3);

    // Offline reading: list and opened mail come from the encrypted copy on the device.
    s.mail_set_offline(true).unwrap();
    s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap();
    s.mail_get(id.clone(), "INBOX".into(), m.uid).unwrap();
    let cached = s.mail_cached_list(id.clone(), "INBOX".into()).unwrap();
    assert_eq!(cached.total, 3);
    s.mail_set_offline(false).unwrap();
    assert!(s.mail_cached_list(id.clone(), "INBOX".into()).is_none());

    // Delete: first into the trash, there for good.
    s.mail_delete(id.clone(), "INBOX".into(), reply).unwrap();
    let trash = s.mail_list(id.clone(), "Trash".into(), 0, 50).unwrap();
    assert_eq!(trash.total, 1);
    assert_eq!(s.mail_list(id.clone(), "INBOX".into(), 0, 50).unwrap().total, 2);
    s.mail_delete(id.clone(), "Trash".into(), trash.messages[0].uid).unwrap();
    assert_eq!(s.mail_list(id.clone(), "Trash".into(), 0, 50).unwrap().total, 0);
    s.mail_move(id.clone(), "INBOX".into(), m.uid, "Drafts".into()).unwrap();
    assert_eq!(s.mail_list(id.clone(), "Drafts".into(), 0, 50).unwrap().total, 1);

    // The account survives lock and unlock; the password is kept when left empty on update.
    s.lock();
    assert!(s.mail_accounts().is_err());
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    // Only the name changes (the form carries neither password nor certificates): no new check
    // needed, and the confirmed certificates stay.
    let mut edit = input.clone();
    edit.password = String::new();
    edit.name = "Umbenannt".into();
    edit.imap_cert = None;
    edit.smtp_cert = None;
    s.mail_save_account(Some(id.clone()), edit.clone()).unwrap();
    assert_eq!(s.mail_accounts().unwrap()[0].name, "Umbenannt");
    assert_eq!(s.mail_folders(id.clone()).unwrap()[0].role, "inbox");
    // Another server needs a check; with the stored password it works and can be saved.
    let mut moved = edit;
    moved.servers.smtp_port = 1;
    assert!(s.mail_save_account(Some(id.clone()), moved.clone()).is_err());
    moved.servers.smtp_port = 10587;
    moved.servers.imap_host = "localhost".into();
    let t = s.mail_test_account(Some(id.clone()), moved.clone()).unwrap();
    assert!(t.confirm.is_some() || t.ok, "{:?}", t.steps);
    s.mail_remove_account(id).unwrap();
    assert!(s.mail_accounts().unwrap().is_empty());
}

/// `input` after "check connection" passed for it (an account is saved only then).
fn mail_checked(s: &GhulSuite, input: super::mail::MailAccountInput) -> super::mail::MailAccountInput {
    let t = s.mail_test_account(None, input.clone()).unwrap();
    assert!(t.ok, "{:?}", t.steps);
    input
}

/// A plain local web server for a folder (stands in for Apache/Nginx in share tests).
struct WebServer {
    child: std::process::Child,
    port: u16,
}

impl Drop for WebServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn web_server(dir: &std::path::Path) -> WebServer {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let child = std::process::Command::new("python3")
        .args(["-m", "http.server", &port.to_string(), "--bind", "127.0.0.1", "--directory"])
        .arg(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    WebServer { child, port }
}

/// The server module installed over the server's own login into `web_dir` (as the app does)
/// and served from `web_abs` (the same folder on this machine) by `php -S`. `None` without PHP.
fn serve_module(
    s: &GhulSuite,
    server_id: &str,
    web_dir: &str,
    web_abs: &std::path::Path,
) -> Option<(std::process::Child, String)> {
    std::process::Command::new("php").arg("-v").output().ok()?;
    std::fs::create_dir_all(web_abs).unwrap();
    let (child, port) = crate::net::tests::spawn_php_ci(web_abs)?;
    let url = format!("http://127.0.0.1:{port}/ghulsuite.php");
    let done = s.server_module_install(server_id.into(), web_dir.into(), url.clone()).unwrap();
    assert_eq!(done.url.as_deref(), Some(url.as_str()), "{:?}", done.checks);
    assert!(done.checks.iter().all(|c| c.ok), "{:?}", done.checks);
    Some((child, url))
}

fn http_get(url: &str) -> crate::net::http::Response {
    let u = crate::net::http::Url::parse(url).unwrap();
    crate::net::block_on(crate::net::http::request("GET", &u, &[], b"", None), crate::net::TIMEOUT).unwrap()
}

/// Opens a share link like the recipient's browser: the package from the module, then every
/// block of every file with the ticket, decrypted with the file keys from the package. `None`
/// when the module refuses the link.
/// What a recipient gets: the share's description and its files (path, content).
type Opened = (crate::share::ShareMeta, Vec<(String, Vec<u8>)>);

fn open_link(link: &str, password: Option<&str>) -> Option<Opened> {
    let (page, fragment) = link.split_once('#').unwrap();
    let (id, key) = fragment.split_once('.').unwrap();
    let key: [u8; 32] = crate::share::b64url_decode(key).unwrap().try_into().unwrap();
    let r = http_get(&format!("{page}?s={id}"));
    if r.status != 200 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
    let b64 = v["package"].as_str().unwrap().replace('+', "-").replace('/', "_").replace('=', "");
    let package = crate::share::b64url_decode(&b64).unwrap();
    let (meta, content) = crate::share::open(&package, &key, password).ok()?;
    assert_eq!(meta.mime, super::shares::ENVELOPE_TYPE);
    let env: super::shares::Envelope = serde_json::from_slice(&content).unwrap();
    let ticket = v["ticket"].as_str().unwrap();
    let mut files = Vec::new();
    for (i, f) in env.files.iter().enumerate() {
        let fk = super::drive::FileKey::of(&crate::share::b64url_decode(&f.key).unwrap()).unwrap();
        let mut data = Vec::new();
        // The block names come sealed in the package; the server's list must say the same.
        let served: Vec<&str> = v["blocks"][i].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
        assert_eq!(served, f.blocks.iter().map(String::as_str).collect::<Vec<_>>());
        for (n, block) in f.blocks.iter().enumerate() {
            let r = http_get(&format!("{page}?s={id}&t={ticket}&f={i}&b={n}"));
            if r.status != 200 {
                return None;
            }
            data.extend_from_slice(&fk.open_block(block, &r.body).unwrap());
        }
        assert_eq!(data.len() as u64, f.size);
        files.push((f.name.clone(), data));
    }
    Some((meta, files))
}

fn share_opts() -> super::shares::ShareOptions {
    super::shares::ShareOptions { password: None, expires_days: 0, max_downloads: 0 }
}

/// Share links through the server module, the whole way: without the module nothing is
/// written; installed over SFTP it is checked; a file, a folder (every file on its own), a
/// password, a one-time link, an expired link, content that is not in GhulDrive, a withdrawn
/// link; a shared file that is deleted keeps its link working (the content as shared); nothing
/// is copied.
fn share_through_module(
    s: &GhulSuite,
    server_id: &str,
    web_dir: &str,
    web_abs: &std::path::Path,
    base_abs: &std::path::Path,
) {
    let a = upload(s, None, "Steuer 2025.pdf", b"%PDF-1.7 Steuer");
    let folder = s.drive_create_folder(None, "Urlaub".into()).unwrap();
    let beach: Vec<u8> = (0..9_000_000u32).map(|i| (i % 249) as u8).collect();
    upload(s, Some(folder.clone()), "Strand.jpg", &beach);
    upload(s, Some(folder.clone()), "Möwe.jpg", &[8u8; 1000]);

    // Without the module: refused, nothing on the server.
    assert_eq!(s.share_missing_module().unwrap(), Some(server_id.to_owned()));
    let r = s.share_drive(vec![a.clone()], None, share_opts());
    assert!(
        matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "share links need the server module"),
        "{:?}",
        r.err()
    );
    assert!(matches!(s.share_bytes("x.txt".into(), b"x".to_vec(), share_opts()), Err(GhulError::InvalidInput { .. })));
    assert!(!base_abs.join("shares").exists() || std::fs::read_dir(base_abs.join("shares")).unwrap().count() == 0);

    // A wrong address: the module is written but not remembered; the right one works.
    let wrong =
        s.server_module_install(server_id.into(), web_dir.into(), "http://127.0.0.1:1/ghulsuite.php".into()).unwrap();
    assert_eq!(wrong.url, None);
    assert!(!wrong.checks.iter().find(|c| c.step == "https").unwrap().ok);
    assert_eq!(wrong.checks.iter().find(|c| c.step == "folder").unwrap().detail, super::shares::SHARE_CHECK_SKIPPED);
    assert!(s.server_module_install(server_id.into(), web_dir.into(), "ftp://example.org/x".into()).is_err());
    let Some((mut php, url)) = serve_module(s, server_id, web_dir, web_abs) else { return };
    assert_eq!(s.share_missing_module().unwrap(), None);
    let info = s.list_servers().unwrap().into_iter().chain(std::iter::empty()).find(|x| x.id == server_id);
    if let Some(info) = info {
        assert_eq!((info.module_url.as_str(), info.module_version), (url.as_str(), crate::net::module::MODULE_VERSION));
    }
    let blocks_before = files_under(&base_abs.join("drive/blocks")).len();

    // One file: its name, its content.
    let one = s.share_drive(vec![a.clone()], None, share_opts()).unwrap();
    assert!(one.url.starts_with(&format!("{url}#")), "{}", one.url);
    assert_eq!(one.name, "Steuer 2025.pdf");
    let (meta, files) = open_link(&one.url, None).unwrap();
    assert_eq!(meta.name, "Steuer 2025.pdf");
    assert_eq!(files, vec![("Steuer 2025.pdf".to_owned(), b"%PDF-1.7 Steuer".to_vec())]);
    // Nothing was copied: no new blocks, and the server never saw a key or a name in clear.
    assert_eq!(files_under(&base_abs.join("drive/blocks")).len(), blocks_before);
    for f in files_under(&base_abs.join("shares")) {
        let raw = std::fs::read(&f).unwrap();
        assert!(!raw.windows(6).any(|w| w == b"Steuer"), "{f:?} names a file");
    }

    // A folder and a file with a password: every file on its own, with its path.
    let mut opts = share_opts();
    opts.password = Some("Geheim 42".into());
    let several = s.share_drive(vec![folder.clone(), a.clone()], Some("Urlaub und Steuer".into()), opts).unwrap();
    assert!(several.password);
    assert!(open_link(&several.url, Some("falsch")).is_none());
    let (meta, files) = open_link(&several.url, Some("Geheim 42")).unwrap();
    assert_eq!(meta.name, "Urlaub und Steuer");
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["Steuer 2025.pdf", "Urlaub/Möwe.jpg", "Urlaub/Strand.jpg"]);
    assert_eq!(files[2].1, beach, "a large file over several blocks");

    // One-time link: the second opening is refused; once its download had time to finish,
    // the share is gone from the list and from the server.
    let mut once = share_opts();
    once.max_downloads = 1;
    let link = s.share_drive(vec![a.clone()], None, once).unwrap();
    assert!(open_link(&link.url, None).is_some());
    assert!(open_link(&link.url, None).is_none());
    let listed = s.share_list().unwrap();
    assert_eq!(listed.iter().find(|x| x.id == link.id).unwrap().downloads, 1, "still finishing");
    let state = base_abs.join(format!("shares/{}.state", link.id));
    std::fs::write(&state, r#"{"count":1,"tickets":{}}"#).unwrap();
    assert!(!s.share_list().unwrap().iter().any(|x| x.id == link.id));
    assert!(!base_abs.join(format!("shares/{}.json", link.id)).exists());
    for leftover in ["state", "lock", "state.tmp"] {
        assert!(!base_abs.join(format!("shares/{}.{leftover}", link.id)).exists(), "{leftover} left behind");
    }

    // Expired (the module checks the time itself): refused, and cleaned up by the app.
    let mut week = share_opts();
    week.expires_days = 7;
    let soon = s.share_drive(vec![a.clone()], None, week).unwrap();
    assert!(soon.expires > 0);
    assert!(open_link(&soon.url, None).is_some());
    let described = base_abs.join(format!("shares/{}.json", soon.id));
    let mut d: serde_json::Value = serde_json::from_slice(&std::fs::read(&described).unwrap()).unwrap();
    d["exp"] = serde_json::json!(1000);
    std::fs::write(&described, serde_json::to_vec(&d).unwrap()).unwrap();
    assert!(open_link(&soon.url, None).is_none());
    assert!(!s.share_list().unwrap().iter().any(|x| x.id == soon.id), "an expired link leaves the list");
    assert!(!described.exists(), "and the server");

    // Text that is not in GhulDrive (e.g. a login): blocks of its own, gone with the share.
    let blocks_now = files_under(&base_abs.join("drive/blocks")).len();
    let text = s.share_bytes("Zugang WLAN.txt".into(), b"SSID: Gast\nPasswort: x".to_vec(), share_opts()).unwrap();
    assert_eq!(files_under(&base_abs.join("drive/blocks")).len(), blocks_now + 1);
    assert_eq!(open_link(&text.url, None).unwrap().1[0].1, b"SSID: Gast\nPasswort: x");
    assert_eq!(s.share_list().unwrap().iter().find(|x| x.id == text.id).unwrap().downloads, 1);
    s.share_delete(text.id.clone()).unwrap();
    assert!(open_link(&text.url, None).is_none());
    assert_eq!(files_under(&base_abs.join("drive/blocks")).len(), blocks_now);

    // Which links contain an item (the app asks before deleting it).
    assert_eq!(s.share_links_of(vec![folder.clone()]).unwrap().len(), 1);
    let whole: Vec<String> = s.share_links_of(vec![a.clone()]).unwrap().into_iter().map(|x| x.id).collect();
    assert!(whole.contains(&one.id) && whole.contains(&several.id));

    // Deleted for good, the shared file still opens through its link (its blocks are kept);
    // ending the link removes them.
    s.drive_delete(a.clone()).unwrap();
    s.drive_empty_trash(0).unwrap();
    assert_eq!(open_link(&one.url, None).unwrap().1[0].1, b"%PDF-1.7 Steuer");
    s.share_delete(one.id.clone()).unwrap();
    s.share_delete(several.id.clone()).unwrap();
    assert!(open_link(&one.url, None).is_none());
    assert!(s.share_list().unwrap().iter().all(|x| x.id != one.id && x.id != several.id));
    assert_eq!(
        files_under(&base_abs.join("drive/blocks")).len(),
        blocks_before - 1,
        "the deleted file's block went with its last link"
    );

    // The module can be taken off the server again.
    s.server_module_remove(server_id.into()).unwrap();
    assert!(!web_abs.join("ghulsuite.php").exists());
    assert_eq!(s.share_missing_module().unwrap(), Some(server_id.to_owned()));
    let _ = php.kill();
    let _ = php.wait();
}

#[test]
fn share_links_over_sftp_through_the_module() {
    use super::servers::{ServerInput, ServerKind};
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("share-sftp");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    crate::net::tests::authorize_test_key(&authkeys, &s.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let input = ServerInput {
        kind: ServerKind::Sftp,
        name: "Heimserver".into(),
        host,
        port,
        user,
        password: None,
        base: format!("{root}/suite-{run}"),
    };
    let id = saved(s.save_server(input, Some(hostkey)).unwrap());
    s.drive_setup(id.clone()).unwrap();
    s.set_sync_server(Some(id.clone())).unwrap();
    let loc = s.drive_locations().unwrap()[0].server.id.clone();
    let root = std::path::PathBuf::from(&root);
    let web = format!("{}/web-{run}/public_html", root.display());
    share_through_module(&s, &loc, &web, std::path::Path::new(&web), &root.join(format!("suite-{run}")));
    let _ = std::fs::remove_dir_all(root.join(format!("web-{run}")));
    let _ = std::fs::remove_dir_all(root.join(format!("suite-{run}")));

    // The GhulSuite folder next to the website: fine. Later the web server hands out the
    // whole folder above it (another site root): updating the module finds the GhulSuite
    // folder reachable, takes the module down and forgets it, so the app offers no links.
    let site = root.join(format!("site-{run}"));
    let www = site.join("www");
    let info = s.list_servers().unwrap().into_iter().find(|x| x.id == id).unwrap();
    let open = ServerInput {
        kind: ServerKind::Sftp,
        name: "Offen".into(),
        host: info.host.clone(),
        port,
        user: info.user.clone(),
        password: None,
        base: format!("{}/ghulsuite", site.display()),
    };
    let open_id = saved(s.save_server(open, Some(info.host_key.clone())).unwrap());
    std::fs::create_dir_all(site.join("ghulsuite/drive")).unwrap();
    std::fs::create_dir_all(&www).unwrap();
    let module_of = |s: &GhulSuite| s.list_servers().unwrap().into_iter().find(|x| x.id == open_id).unwrap().module_url;
    if let Some((mut php, port)) = crate::net::tests::spawn_php_ci(&www) {
        let url = format!("http://127.0.0.1:{port}/ghulsuite.php");
        let done = s.server_module_install(open_id.clone(), www.display().to_string(), url.clone()).unwrap();
        let _ = php.kill();
        let _ = php.wait();
        assert_eq!(done.url.as_deref(), Some(url.as_str()), "{:?}", done.checks);
        assert!(done.checks.iter().any(|c| c.step == "private" && c.ok), "{:?}", done.checks);
        assert_eq!(module_of(&s), url);
    }
    if let Some((mut php, port)) = crate::net::tests::spawn_php_ci(&site) {
        let url = format!("http://127.0.0.1:{port}/www/ghulsuite.php");
        let done = s.server_module_install(open_id.clone(), www.display().to_string(), url).unwrap();
        let _ = php.kill();
        let _ = php.wait();
        assert_eq!(done.url, None, "{:?}", done.checks);
        let private = done.checks.iter().find(|c| c.step == "private").expect("checked");
        assert!(!private.ok && private.detail.contains("/ghulsuite/shares/check-"), "{private:?}");
        assert!(!www.join("ghulsuite.php").exists(), "the module is taken down again");
        assert!(module_of(&s).is_empty(), "and no longer offered");
    }
    let _ = std::fs::remove_dir_all(&site);
}

#[test]
fn ftps_storage_drive_and_share_links() {
    use super::servers::{ServerInput, ServerKind};
    let Some(cfg) = crate::net::tests::ftps_test_config("app") else { return };
    let home = std::env::var("GHUL_TEST_DIR").map(|d| std::path::PathBuf::from(d).join("ftp-home")).unwrap();
    let d = Dirs::new("ftps");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let input = ServerInput {
        kind: ServerKind::Ftps,
        name: "Webhosting".into(),
        host: cfg.host.clone(),
        port: cfg.port,
        user: cfg.user.clone(),
        password: Some(cfg.password.to_string()),
        base: cfg.base.clone(),
    };
    // Self-signed certificate: confirm its fingerprint first.
    let fp = match s.save_server(input.clone(), None).unwrap() {
        ServerSaved::Confirm { fingerprint } => fingerprint,
        other => panic!("{other:?}"),
    };
    assert_eq!(Some(fp.clone()), cfg.cert);
    let id = saved(s.save_server(input, Some(fp)).unwrap());
    // Sync and Drive over FTPS.
    s.set_sync_server(Some(id.clone())).unwrap();
    assert_eq!(s.sync_now().unwrap().uploaded, 1);
    s.drive_setup(id.clone()).unwrap();
    let big: Vec<u8> = (0..5_000_000u32).map(|i| (i % 251) as u8).collect();
    let f = upload(&s, None, "Video.mp4", &big);
    assert_eq!(download(&s, &f), big);
    // Folder browsing for the module's folder; the module over FTPS, then share links.
    assert!(s.server_browse(id.clone(), String::new()).unwrap().contains(&cfg.base));
    let rel = format!("{}-web", cfg.base);
    let loc = s.drive_locations().unwrap()[0].server.id.clone();
    share_through_module(&s, &loc, &rel, &home.join(&rel), &home.join(&cfg.base));
}

#[test]
fn drive_locations_are_separate_from_the_synchronisation() {
    use super::drive::DriveMode;
    use super::servers::{ServerFolders, ServerInput, ServerKind};
    let Some(cfg) = crate::net::tests::ftps_test_config("drive-modes") else { return };
    let home = std::env::var("GHUL_TEST_DIR").map(|d| std::path::PathBuf::from(d).join("ftp-home")).unwrap();
    let d = Dirs::new("drive-modes");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let input = |base: &str| ServerInput {
        kind: ServerKind::Ftps,
        name: String::new(),
        host: cfg.host.clone(),
        port: cfg.port,
        user: cfg.user.clone(),
        password: Some(cfg.password.to_string()),
        base: base.to_owned(),
    };
    // Folder picker before anything is stored: confirm the certificate, then list.
    let fp = match s.server_browse_input(input(""), None, String::new()).unwrap() {
        ServerFolders::Confirm { fingerprint } => fingerprint,
        other => panic!("{other:?}"),
    };
    let sync_id = saved(s.save_server(input(&cfg.base), Some(fp.clone())).unwrap());
    match s.server_browse_input(input(""), Some(fp.clone()), String::new()).unwrap() {
        ServerFolders::Folders { folders } => assert!(folders.contains(&cfg.base)),
        other => panic!("{other:?}"),
    }
    let st = s.drive_storage().unwrap();
    assert_eq!((st.mode, st.sync_available), (DriveMode::None, false));
    assert!(matches!(s.drive_use_sync(), Err(GhulError::NotConfigured)));

    // An older Drive that used the synchronisation's server: that server becomes a storage
    // location of Drive's own (a copy in the Drive file). The synchronisation keeps its server
    // and can drop it; Drive keeps working.
    s.set_sync_server(Some(sync_id.clone())).unwrap();
    s.drive_use_sync().unwrap();
    // As an older app left it: only a reference to the synchronisation, no location yet.
    s.drive_back_to_use_sync_for_tests();
    // The synchronisation is switched off before Drive is opened again: Drive keeps its server
    // all the same (it becomes a location before the synchronisation changes).
    s.set_sync_server(None).unwrap();
    let first = s.drive_locations().unwrap();
    assert_eq!(first.len(), 1);
    assert!(first[0].active && first[0].archive);
    let first_id = first[0].server.id.clone();
    assert_ne!(first_id, sync_id);
    assert_eq!(s.drive_storage().unwrap().mode, DriveMode::Own);
    let plan = upload(&s, None, "Plan.txt", b"Plan A");
    // What another device still has of the synchronisation's server (used again below).
    let sync_servers: Vec<ciborium::Value> =
        s.with_open(|o| Ok(super::suite::setting(&o.suite, super::servers::SERVERS_KEY))).unwrap();
    s.remove_server(sync_id.clone()).unwrap();
    assert!(s.list_servers().unwrap().is_empty(), "Drive's locations are not offered for synchronisation");
    assert!(s.remove_server(first_id.clone()).is_err(), "a location is removed in GhulDrive");
    assert_eq!(download(&s, &plan), b"Plan A");

    // A second location is added next to it; the active one stays.
    let other = format!("{}-b", cfg.base);
    let second_id = saved(s.drive_save_server(input(&other), Some(fp.clone())).unwrap());
    let locs = s.drive_locations().unwrap();
    assert_eq!(
        locs.iter().map(|l| (l.server.id.clone(), l.active)).collect::<Vec<_>>(),
        vec![(first_id.clone(), true), (second_id.clone(), false)]
    );
    assert!(home.join(&other).join("drive/index.ghldrv").exists(), "checked when added");
    // The same folder twice would share its files between two locations.
    assert!(matches!(s.drive_save_server(input(&other), Some(fp.clone())), Err(GhulError::AlreadyExists)));
    assert!(matches!(
        s.drive_save_server(input(&format!("{other}/")), Some(fp.clone())),
        Err(GhulError::AlreadyExists)
    ));
    assert_eq!(s.drive_locations().unwrap().len(), 2);
    // With a second location the archive stays on the first one, also after a switch.
    let archive: Vec<bool> = s.drive_locations().unwrap().iter().map(|l| l.archive).collect();
    assert_eq!(archive, vec![true, false]);

    // An upload started on the first location ends there, also when the user switches meanwhile.
    let up = s.drive_upload_begin(None, "Lang.bin".into()).unwrap();
    s.drive_upload_chunk(up.clone(), b"Teil 1 ".to_vec()).unwrap();
    s.drive_switch_location(second_id.clone()).unwrap();
    s.drive_upload_chunk(up.clone(), b"Teil 2".to_vec()).unwrap();
    let long = s.drive_upload_finish(up).unwrap();
    assert!(s.drive_list(None, true).unwrap().is_empty(), "nothing of it on the second location");
    let archive: Vec<bool> = s.drive_locations().unwrap().iter().map(|l| l.archive).collect();
    assert_eq!(archive, vec![true, false], "the archive did not follow the switch");
    s.drive_switch_location(first_id.clone()).unwrap();
    assert_eq!(download(&s, &long), b"Teil 1 Teil 2");
    s.drive_delete(long).unwrap();

    // Switching shows the other location's files; switching back the first ones.
    s.drive_switch_location(second_id.clone()).unwrap();
    assert!(s.drive_list(None, true).unwrap().is_empty(), "a fresh Drive on the other folder");
    let memo = upload(&s, None, "Notiz.txt", b"Zweiter Ort");
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.drive_storage().unwrap().server.map(|x| x.id), Some(second_id.clone()), "survives locking");
    s.drive_switch_location(first_id.clone()).unwrap();
    let names: Vec<String> = s.drive_list(None, true).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["Plan.txt"]);
    assert!(s.drive_switch_location("unbekannt".into()).is_err());

    // The mail archive's location is chosen on its own.
    s.drive_set_archive_location(second_id.clone()).unwrap();
    let archive: Vec<bool> = s.drive_locations().unwrap().iter().map(|l| l.archive).collect();
    assert_eq!(archive, vec![false, true]);

    // Removing with the files: everything of GhulDrive on that server goes, nothing else.
    std::fs::write(home.join(&other).join("drive/eigene-notiz.txt"), b"mine").unwrap();
    std::fs::create_dir_all(home.join(&other).join("shares")).unwrap();
    // Writable for the FTP user, as GhulDrive's own share folder would be.
    #[cfg(unix)]
    std::fs::set_permissions(home.join(&other).join("shares"), std::os::unix::fs::PermissionsExt::from_mode(0o777))
        .unwrap();
    std::fs::write(home.join(&other).join("shares/0a1b.json"), b"{}").unwrap();
    std::fs::write(home.join(&other).join("shares/liste.txt"), b"mine").unwrap();
    s.drive_remove_location(second_id.clone(), true, false).unwrap();
    assert_eq!(
        files_under(&home.join(&other).join("drive")).len(),
        1,
        "only the user's own file is left: {:?}",
        files_under(&home.join(&other).join("drive"))
    );
    assert!(home.join(&other).join("drive/eigene-notiz.txt").exists());
    assert!(home.join(&other).join("shares/liste.txt").exists());
    assert!(!home.join(&other).join("shares/0a1b.json").exists(), "GhulDrive's share files go");
    assert!(!home.join(&other).join("drive/index.ghldrv").exists());
    assert_eq!(s.drive_locations().unwrap().len(), 1);
    assert!(s.drive_locations().unwrap()[0].archive, "the archive moves to the remaining location");
    let _ = memo;

    // Removing the active one without the files: Drive has no location any more, the files
    // stay on the server and come back when the location is added again.
    let other_device = s.drive_settings_for_tests();
    s.drive_remove_location(first_id.clone(), false, false).unwrap();
    assert!(s.drive_locations().unwrap().is_empty());
    assert_eq!(s.drive_storage().unwrap().mode, DriveMode::None);
    // S4: another device's Drive file from before the removal (newer, so its active location
    // wins the merge) does not bring the location back.
    s.drive_merge_newer_settings_for_tests(&other_device);
    assert!(s.drive_locations().unwrap().is_empty(), "removed stays removed");
    assert_eq!(s.drive_storage().unwrap().mode, DriveMode::None);
    // Nor a Drive file of an older app that still uses the synchronisation's server: the
    // location made from it has the same id on every device and was removed on purpose.
    s.with_open(|o| super::suite::put_setting(&mut o.suite, super::servers::SERVERS_KEY, &sync_servers)).unwrap();
    s.set_sync_server(Some(sync_id.clone())).unwrap();
    s.drive_back_to_use_sync_for_tests();
    assert!(s.drive_locations().unwrap().is_empty(), "the moved location does not come back");
    assert_eq!(s.drive_storage().unwrap().mode, DriveMode::None);
    s.set_sync_server(None).unwrap();
    s.remove_server(sync_id.clone()).unwrap();
    assert!(home.join(&cfg.base).join("drive/index.ghldrv").exists(), "files stay on the server");
    let again = saved(s.drive_save_server(input(&cfg.base), Some(cfg.cert.clone().unwrap())).unwrap());
    assert_eq!(s.drive_storage().unwrap().server.map(|x| x.id), Some(again.clone()));
    let names: Vec<String> = s.drive_list(None, true).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["Plan.txt"]);

    // A location in the folder the synchronisation uses: removing it never deletes files there.
    let sync_again = saved(s.save_server(input(&cfg.base), Some(fp.clone())).unwrap());
    assert!(s.drive_remove_location(again.clone(), true, false).is_err());
    assert_eq!(s.drive_locations().unwrap().len(), 1, "nothing removed");
    assert!(home.join(&cfg.base).join("drive/index.ghldrv").exists());
    s.remove_server(sync_again).unwrap();
}

#[test]
fn module_addresses_are_guessed_from_the_folder() {
    let urls = super::servers::module_urls;
    assert_eq!(urls("ftp.example.org", "public_html/ghul")[0], "https://example.org/ghul/ghulsuite.php");
    assert_eq!(urls("w0123.webhost.test", "example.org/tools")[0], "https://example.org/tools/ghulsuite.php");
    assert_eq!(urls("ftp.example.org", "example.net/htdocs")[0], "https://example.net/ghulsuite.php");
    assert_eq!(
        urls("ftp.example.org", ""),
        vec!["https://example.org/ghulsuite.php", "https://www.example.org/ghulsuite.php"]
    );
    assert_eq!(urls("sftp.example.org", "public_html")[0], "https://example.org/ghulsuite.php");
}

/// A suite with GhulDrive on the SFTP test server and the server module installed there,
/// served by `php -S`. `None` without the test server or PHP.
struct ModuleSuite {
    _dirs: Dirs,
    s: Arc<GhulSuite>,
    php: std::process::Child,
    places: Vec<std::path::PathBuf>,
}

impl Drop for ModuleSuite {
    fn drop(&mut self) {
        let _ = self.php.kill();
        let _ = self.php.wait();
        for p in &self.places {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn module_suite(name: &str) -> Option<ModuleSuite> {
    use super::servers::{ServerInput, ServerKind};
    let (host, port, user, root, authkeys, hostkey) = crate::net::tests::sftp_env()?;
    std::process::Command::new("php").arg("-v").output().ok()?;
    let d = Dirs::new(name);
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    crate::net::tests::authorize_test_key(&authkeys, &s.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let base = format!("{root}/{name}-suite-{run}");
    let input = ServerInput {
        kind: ServerKind::Sftp,
        name: String::new(),
        host,
        port,
        user,
        password: None,
        base: base.clone(),
    };
    let id = saved(s.save_server(input, Some(hostkey)).unwrap());
    s.drive_setup(id.clone()).unwrap();
    // GhulDrive works on a storage location of its own (a copy of that server): the module there.
    let id = s.drive_locations().unwrap()[0].server.id.clone();
    let web = format!("{root}/{name}-web-{run}");
    let (php, _) = serve_module(&s, &id, &web, std::path::Path::new(&web))?;
    Some(ModuleSuite { _dirs: d, s, php, places: vec![base.into(), web.into()] })
}

/// Removing a storage location ends its share links first (also without deleting its files).
#[test]
fn removing_a_location_ends_its_share_links() {
    let Some(m) = module_suite("remove-location") else { return };
    let s = &m.s;
    let file = upload(s, None, "Vertrag.pdf", b"%PDF-1.7 Vertrag");
    let link = s.share_drive(vec![file], None, share_opts()).unwrap();
    assert!(open_link(&link.url, None).is_some());
    let loc = s.drive_locations().unwrap()[0].server.id.clone();
    s.drive_remove_location(loc, false, false).unwrap();
    assert!(s.share_list().unwrap().is_empty());
    assert!(open_link(&link.url, None).is_none(), "the link has ended");
    let base = &m.places[0];
    assert!(base.join("drive/index.ghldrv").exists(), "the files stay on the server");
    assert!(!base.join(format!("shares/{}.json", link.id)).exists());
}

/// A link that cannot be ended (its server fails) keeps the location, unless the user removes
/// it anyway: then the link is only forgotten here and nothing is deleted on the server.
#[test]
fn removing_a_location_anyway_forgets_links_it_cannot_end() {
    let Some(m) = module_suite("remove-location-anyway") else { return };
    let s = &m.s;
    let file = upload(s, None, "Vertrag.pdf", b"%PDF-1.7 Vertrag");
    let link = s.share_drive(vec![file], None, share_opts()).unwrap();
    let loc = s.drive_locations().unwrap()[0].server.id.clone();
    // The share's description cannot be read any more (a folder where the file was).
    let json = m.places[0].join(format!("shares/{}.json", link.id));
    std::fs::remove_file(&json).unwrap();
    std::fs::create_dir(&json).unwrap();
    assert!(s.drive_remove_location(loc.clone(), true, false).is_err());
    assert_eq!(s.drive_locations().unwrap().len(), 1, "nothing removed");
    assert_eq!(s.share_list().unwrap().len(), 1, "the link is still listed");
    s.drive_remove_location(loc, true, true).unwrap();
    assert!(s.drive_locations().unwrap().is_empty());
    assert!(s.share_list().unwrap().is_empty(), "forgotten here");
    assert!(m.places[0].join("drive/index.ghldrv").exists(), "nothing deleted on the server");
}

/// The way the app puts Drive files into a mail: decrypted file as attachment and a share link in
/// the text. Sent over SMTP, read back over IMAP, and the link from the received mail is opened.
#[test]
fn drive_file_and_share_link_into_mail() {
    use super::mail::{MailAccountInput, MailSecurity, MailServers, OutgoingAttachment, OutgoingMail};

    let (Ok(host), Ok(fp)) = (std::env::var("GHUL_TEST_MAIL"), std::env::var("GHUL_TEST_MAIL_TLSFP")) else { return };
    // GhulDrive with share links; no mail account exists yet, Drive and sharing work without it.
    let Some(m) = module_suite("drive-mail") else { return };
    let s = &m.s;
    let content: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let file = upload(s, None, "Rechnung März.pdf", &content);
    let link = s.share_drive(vec![file.clone()], None, share_opts()).unwrap();
    assert!(s.mail_accounts().unwrap().is_empty());

    // Now the mail module gets an account.
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let email = format!("drive{run}@test.local");
    let account = s
        .mail_save_account(
            None,
            mail_checked(
                s,
                MailAccountInput {
                    name: "Test".into(),
                    display_name: "Drive".into(),
                    email: email.clone(),
                    username: email.clone(),
                    password: "test-pw-123".into(),
                    servers: MailServers {
                        imap_host: host.clone(),
                        imap_port: 10993,
                        imap_security: MailSecurity::Tls,
                        smtp_host: host,
                        smtp_port: 10587,
                        smtp_security: MailSecurity::StartTls,
                    },
                    imap_cert: Some(fp.clone()),
                    smtp_cert: Some(fp),
                    signature: String::new(),
                },
            ),
        )
        .unwrap();
    // Same as the app: decrypt the Drive file on the device, attach it, put the link in the text.
    let attachment = download(s, &file);
    assert_eq!(attachment, content);
    s.mail_send(OutgoingMail {
        account_id: account.clone(),
        to: vec![email],
        cc: vec![],
        bcc: vec![],
        subject: "Rechnung März.pdf".into(),
        text: format!("Hallo,\n\nhier ist Rechnung März.pdf für dich:\n{}\n", link.url),
        in_reply_to: String::new(),
        attachments: vec![OutgoingAttachment {
            name: "Rechnung März.pdf".into(),
            mime: "application/pdf".into(),
            data: attachment,
        }],
        pgp: true,
    })
    .unwrap();

    let page = s.mail_list(account.clone(), "INBOX".into(), 0, 10).unwrap();
    assert_eq!(page.total, 1);
    let got = s.mail_get(account.clone(), "INBOX".into(), page.messages[0].uid).unwrap();
    assert_eq!(got.attachments.len(), 1);
    assert_eq!(got.attachments[0].name, "Rechnung März.pdf");
    assert_eq!(s.mail_attachment(account, "INBOX".into(), got.uid, 0).unwrap(), content);
    // The link survives the mail transport unchanged (no line breaks, no encoding) and opens.
    let received = got.text.lines().find(|l| l.starts_with("http")).unwrap().trim().to_string();
    assert_eq!(received, link.url);
    let (meta, files) = open_link(&received, None).unwrap();
    assert_eq!(meta.name, "Rechnung März.pdf");
    assert_eq!(files[0].1, content);
}

/// Fixture for the browser test of the PHP server module (tools/test-share-php.js): runs the
/// module under `php -S`, creates shares through the suite, writes their links and waits until
/// the browser test has finished (file `done`).
///
///   GHUL_PHP_FIXTURE=/tmp/x cargo test -p ghul-core php_browser_fixture & node tools/test-share-php.js /tmp/x
#[test]
fn php_browser_fixture() {
    let Ok(dir) = std::env::var("GHUL_PHP_FIXTURE") else { return };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let m = module_suite("php-browser").expect("SFTP test server and php needed");
    let s = &m.s;
    let content: Vec<u8> = (0..9_500_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("expected.bin"), &content).unwrap();
    let report = upload(s, None, "Bericht.pdf", &content);
    let folder = s.drive_create_folder(None, "Fotos".into()).unwrap();
    upload(s, Some(folder.clone()), "a.txt", b"Datei A");
    upload(s, Some(folder.clone()), "b.txt", b"Datei B");
    let mut links = String::new();
    let mut opts = share_opts();
    links += &format!("plain {}\n", s.share_drive(vec![report.clone()], None, opts.clone()).unwrap().url);
    links += &format!("folder {}\n", s.share_drive(vec![folder], None, opts.clone()).unwrap().url);
    opts.password = Some("Grüße 123".into());
    links += &format!("pw {}\n", s.share_drive(vec![report.clone()], None, opts.clone()).unwrap().url);
    opts.password = None;
    opts.max_downloads = 1;
    links += &format!("once {}\n", s.share_bytes("Einmal.txt".into(), b"WLAN: geheim".to_vec(), opts).unwrap().url);
    // Expired: the module refuses it although the description is still there.
    let mut week = share_opts();
    week.expires_days = 7;
    let soon = s.share_drive(vec![report], None, week).unwrap();
    let described = m.places[0].join(format!("shares/{}.json", soon.id));
    let mut d: serde_json::Value = serde_json::from_slice(&std::fs::read(&described).unwrap()).unwrap();
    d["exp"] = serde_json::json!(1000);
    std::fs::write(&described, serde_json::to_vec(&d).unwrap()).unwrap();
    links += &format!("expired {}\n", soon.url);
    std::fs::write(dir.join("links.txt"), links).unwrap();

    let done = dir.join("done");
    for _ in 0..1800 {
        if done.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(done.exists(), "browser test did not finish");
    // The downloads were counted.
    let list = s.share_list().unwrap();
    assert!(list.iter().find(|x| x.name == "Bericht.pdf" && !x.password).unwrap().downloads >= 1, "{list:?}");
    assert_eq!(list.iter().find(|x| x.name == "Einmal.txt").map(|x| x.downloads), Some(1));
}

/// OpenPGP between two GhulSuite users with separate suites, over real IMAP/SMTP servers:
/// first mail signed with the key in the Autocrypt header, the answer encrypted automatically,
/// subject protected, attachment inside the encryption, key lookup at the provider (WKD).
#[test]
fn pgp_mail_between_two_users() {
    use super::mail::{
        MailAccountInput, MailSecurity, MailServers, MailSignatureStatus, OutgoingAttachment, OutgoingMail,
    };
    let (Ok(host), Ok(fp)) = (std::env::var("GHUL_TEST_MAIL"), std::env::var("GHUL_TEST_MAIL_TLSFP")) else { return };
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let user = |who: &str, name: &str| {
        let d = Dirs::new(&format!("pgp-{who}"));
        let s = d.suite();
        s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None)
            .unwrap();
        let email = format!("{who}{run}@test.local");
        let id = s
            .mail_save_account(
                None,
                mail_checked(
                    &s,
                    MailAccountInput {
                        name: who.into(),
                        display_name: name.into(),
                        email: email.clone(),
                        username: email.clone(),
                        password: "test-pw-123".into(),
                        servers: MailServers {
                            imap_host: host.clone(),
                            imap_port: 10993,
                            imap_security: MailSecurity::Tls,
                            smtp_host: host.clone(),
                            smtp_port: 10587,
                            smtp_security: MailSecurity::StartTls,
                        },
                        imap_cert: Some(fp.clone()),
                        smtp_cert: Some(fp.clone()),
                        signature: format!("Viele Grüße\n{name}"),
                    },
                ),
            )
            .unwrap();
        (d, s, id, email)
    };
    let (_da, alice, alice_id, alice_mail) = user("alice", "Alice Ä.");
    let (_db, bob, bob_id, bob_mail) = user("bob", "Bob");
    // Every account has its own key from the start.
    assert_eq!(alice.mail_accounts().unwrap()[0].pgp_fingerprint.len(), 49);
    let mail = |to: &str, subject: &str, text: &str, attachments: Vec<OutgoingAttachment>, id: &str| OutgoingMail {
        account_id: id.to_owned(),
        to: vec![to.to_owned()],
        cc: vec![],
        bcc: vec![],
        subject: subject.to_owned(),
        text: text.to_owned(),
        in_reply_to: String::new(),
        attachments,
        pgp: true,
    };
    let newest = |s: &GhulSuite, id: &str| {
        let page = s.mail_list(id.to_owned(), "INBOX".into(), 0, 1).unwrap();
        let uid = page.messages[0].uid;
        (page.messages[0].subject.clone(), s.mail_get(id.to_owned(), "INBOX".into(), uid).unwrap())
    };

    // 1. Alice does not know Bob's key: the first mail is signed, not encrypted.
    let wkd_dir = std::env::temp_dir().join(format!("ghul-wkd-{run}"));
    std::fs::create_dir_all(&wkd_dir).unwrap();
    let wkd = web_server(&wkd_dir);
    std::env::set_var("GHUL_TEST_WKD", format!("http://127.0.0.1:{}", wkd.port));
    let status = alice.mail_recipient_keys(vec![format!("Bob <{bob_mail}>")]).unwrap();
    assert!(!status[0].has_key);
    alice.mail_send(mail(&bob_mail, "Hallo Bob", "Erste Nachricht.", vec![], &alice_id)).unwrap();
    let (list_subject, m) = newest(&bob, &bob_id);
    assert_eq!(list_subject, "Hallo Bob");
    assert!(!m.encrypted);
    // Signed with the key that came with this very mail: correct, but not yet confirmed (an
    // attacker could have sent both); shown like an unknown key the first time.
    assert_eq!(m.signature, MailSignatureStatus::UnknownKey);
    assert_eq!(m.signer, alice_mail.to_lowercase());
    // B12: opened a second time, it is still the mail the key came with: not confirmed either.
    let (_, again) = newest(&bob, &bob_id);
    assert_eq!(again.signature, MailSignatureStatus::UnknownKey);
    assert!(m.text.contains("Erste Nachricht.") && m.text.contains("Viele Grüße\nAlice Ä."), "{}", m.text);
    assert!(m.attachments.is_empty(), "the signature is no attachment");
    // Bob learned Alice's key from the Autocrypt header.
    let known = bob.mail_known_keys().unwrap();
    assert_eq!((known[0].email.as_str(), known[0].source.as_str()), (alice_mail.to_lowercase().as_str(), "autocrypt"));

    // 2. Bob's answer is encrypted automatically, with a protected subject and an attachment.
    assert!(bob.mail_recipient_keys(vec![alice_mail.clone()]).unwrap()[0].has_key);
    let contract: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    bob.mail_send(mail(
        &alice_mail,
        "Geheimes Angebot \u{2013} 5 % Rabatt",
        "Hallo Alice, siehe Anhang.",
        vec![OutgoingAttachment { name: "Angebot.pdf".into(), mime: "application/pdf".into(), data: contract.clone() }],
        &bob_id,
    ))
    .unwrap();
    let (list_subject, m) = newest(&alice, &alice_id);
    assert_eq!(list_subject, "...", "the server only sees a placeholder subject");
    assert!(m.encrypted);
    assert_eq!(m.subject, "Geheimes Angebot \u{2013} 5 % Rabatt");
    // Alice had no key of Bob before reading: it came with this very mail (Autocrypt), so the
    // signature counts as "not yet confirmed".
    assert_eq!(m.signature, MailSignatureStatus::UnknownKey);
    assert!(m.text.contains("Hallo Alice, siehe Anhang."));
    assert_eq!(m.attachments.len(), 1);
    assert_eq!(m.attachments[0].name, "Angebot.pdf");
    let uid = m.uid;
    assert_eq!(alice.mail_attachment(alice_id.clone(), "INBOX".into(), uid, 0).unwrap(), contract);
    // Bob can read his own copy in "Sent".
    let sent = bob.mail_list(bob_id.clone(), "Sent".into(), 0, 1).unwrap();
    let copy = bob.mail_get(bob_id.clone(), "Sent".into(), sent.messages[0].uid).unwrap();
    assert!(copy.encrypted && copy.subject.starts_with("Geheimes Angebot"));

    // 3. Key lookup at the provider (WKD) for someone who never wrote.
    let (carol_sk, carol_fp) = super::mailcrypt::generate("Carol", "carol@test.local").unwrap();
    let carol = super::mailcrypt::public_of(&super::mailcrypt::secret_key(&carol_sk).unwrap());
    let hu = wkd_dir.join(".well-known/openpgpkey/hu");
    std::fs::create_dir_all(&hu).unwrap();
    std::fs::write(hu.join(super::mailcrypt::wkd_hash("carol")), super::mailcrypt::public_bytes(&carol).unwrap())
        .unwrap();
    let status = alice.mail_recipient_keys(vec!["carol@test.local".into()]).unwrap();
    assert!(status[0].has_key && status[0].source == "wkd", "{status:?}");
    assert!(alice.mail_known_keys().unwrap().iter().any(|k| k.fingerprint.replace(' ', "") == carol_fp));
    // A key served for another address is not taken.
    std::fs::write(hu.join(super::mailcrypt::wkd_hash("dave")), super::mailcrypt::public_bytes(&carol).unwrap())
        .unwrap();
    assert!(!alice.mail_recipient_keys(vec!["dave@test.local".into()]).unwrap()[0].has_key);

    // 4. Keys can be exported, imported and forgotten.
    let armored = alice.mail_public_key(alice_id.clone()).unwrap();
    assert!(armored.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    let (_dc, eve, _, _) = user("eve", "Eve");
    assert_eq!(eve.mail_import_key(armored.into_bytes()).unwrap(), vec![alice_mail.to_lowercase()]);
    eve.mail_forget_key(alice_mail.clone()).unwrap();
    assert!(eve.mail_known_keys().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&wkd_dir);
}

#[test]
fn pass_extras_history_attachments_move_and_security_check() {
    let d = Dirs::new("pass-extras");
    let s = d.suite();
    s.create(
        options(
            "pw-123456",
            Some(generate_keyfile()),
            &[(VaultKind::Pass, "Privat"), (VaultKind::Pass, "Arbeit"), (VaultKind::Totp, "Codes")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    let (privat, arbeit, codes) = (vault_id(&s, "Privat"), vault_id(&s, "Arbeit"), vault_id(&s, "Codes"));
    let id = s.save_entry(privat.clone(), login("Bank", "ich", "Kurz1")).unwrap();
    let extras = s.entry_extras(privat.clone(), id.clone()).unwrap();
    assert!(extras.created > 0 && !extras.favorite && extras.history.is_empty());

    // Exporting single entries needs the master password and contains only those entries.
    let forum = s.save_entry(privat.clone(), login("Forum", "du", "Anders2")).unwrap();
    assert!(matches!(
        s.export_entries(privat.clone(), vec![id.clone()], "falsch".into()),
        Err(GhulError::WrongPassword)
    ));
    let one =
        String::from_utf8(s.export_entries(privat.clone(), vec![id.clone()], "pw-123456".into()).unwrap()).unwrap();
    assert!(one.contains("Bank") && one.contains("Kurz1") && !one.contains("Forum"));
    s.delete_entry(privat.clone(), forum).unwrap();

    // Password history: the old password is kept when it changes, newest first.
    let mut e = s.get_entry(privat.clone(), id.clone()).unwrap();
    for f in e.fields.iter_mut().filter(|f| f.name == "password") {
        f.value = FieldValue::Secret { value: "Neu-und-viel-l4nger!".into() };
    }
    s.save_entry(privat.clone(), e.clone()).unwrap();
    s.save_entry(privat.clone(), e).unwrap(); // unchanged: no new history item
    let h = s.entry_extras(privat.clone(), id.clone()).unwrap().history;
    assert_eq!(h.iter().map(|i| i.password.as_str()).collect::<Vec<_>>(), vec!["Kurz1"]);

    // Favourite and attachments survive saving the entry.
    s.set_entry_favorite(privat.clone(), id.clone(), true).unwrap();
    s.add_entry_attachment(privat.clone(), id.clone(), "Vertrag.pdf".into(), b"%PDF".to_vec()).unwrap();
    s.add_entry_attachment(privat.clone(), id.clone(), "Vertrag.pdf".into(), b"zwei".to_vec()).unwrap();
    assert!(s.add_entry_attachment(privat.clone(), id.clone(), "..".into(), vec![1]).is_err());
    let e = s.get_entry(privat.clone(), id.clone()).unwrap();
    assert!(e.fields.iter().all(|f| !f.name.starts_with("attachment:")));
    s.save_entry(privat.clone(), e).unwrap();
    let extras = s.entry_extras(privat.clone(), id.clone()).unwrap();
    assert!(extras.favorite);
    let mut names: Vec<String> = extras.attachments.iter().map(|a| a.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["Vertrag (2).pdf", "Vertrag.pdf"]);
    assert_eq!(s.entry_attachment(privat.clone(), id.clone(), "Vertrag (2).pdf".into()).unwrap(), b"zwei");
    s.remove_entry_attachment(privat.clone(), id.clone(), "Vertrag (2).pdf".into()).unwrap();
    assert!(s.list_entries(VaultKind::Pass, None).unwrap()[0].favorite);

    // Duplicate and move.
    let copy = s.duplicate_entry(privat.clone(), id.clone(), " (Kopie)".into()).unwrap();
    let moved = s.move_entry(privat.clone(), copy, arbeit.clone()).unwrap();
    let list = s.list_entries(VaultKind::Pass, None).unwrap();
    assert_eq!(
        list.iter().map(|e| (e.title.as_str(), e.vault_name.as_str())).collect::<Vec<_>>(),
        vec![("Bank", "Privat"), ("Bank (Kopie)", "Arbeit")]
    );
    assert_eq!(s.entry_extras(arbeit.clone(), moved.clone()).unwrap().attachments.len(), 1);
    assert!(s.move_entry(arbeit.clone(), moved.clone(), codes.clone()).is_err());

    // Security check: both use the same password; a weak one; 2FA only for one site in GhulAuth.
    s.save_entry(privat.clone(), login("Forum", "ich", "123456")).unwrap();
    let r = s.security_report().unwrap();
    let kinds = |title: &str| -> Vec<super::passx::SecurityIssueKind> {
        r.issues.iter().filter(|i| i.title == title).map(|i| i.kind).collect()
    };
    use super::passx::SecurityIssueKind::*;
    assert_eq!(kinds("Bank"), vec![Reused, NoSecondFactor]);
    assert_eq!(kinds("Forum"), vec![Weak, NoSecondFactor]);
    assert_eq!(r.checked, 3);
    assert!(r.score < 100);
    let mut t = totp_input();
    t.issuer = "Forum".into();
    s.add_totp(codes.clone(), t).unwrap();
    let r = s.security_report().unwrap();
    assert!(!r.issues.iter().any(|i| i.title == "Forum" && i.kind == NoSecondFactor));
    // The GhulAuth favourite flag uses the same call.
    let code = s.totp_codes(59).unwrap()[0].clone();
    s.set_entry_favorite(code.vault_id.clone(), code.id.clone(), true).unwrap();
    assert!(s.totp_codes(59).unwrap()[0].favorite);
}

#[test]
fn totp_export_link_round_trip() {
    let d = Dirs::new("totp-uri");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Totp, "Codes")]), None, None, None).unwrap();
    let v = vault_id(&s, "Codes");
    let mut t = totp_input();
    t.issuer = "Mein Dienst".into();
    s.add_totp(v.clone(), t).unwrap();
    let c = s.totp_codes(59).unwrap()[0].clone();
    let uri = s.totp_uri(v, c.id).unwrap();
    assert!(
        uri.starts_with("otpauth://totp/Mein%20Dienst:a%40example.org?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
        "{uri}"
    );
    let back = crate::app::tools::parse_otpauth_uri(uri).unwrap();
    assert_eq!((back.issuer.as_str(), back.digits), ("Mein Dienst", 8));
    // No own web server: no clock check.
    assert_eq!(s.clock_offset(0).unwrap(), None);
}

#[test]
fn backup_and_restore_into_an_empty_folder() {
    let d = Dirs::new("backup-a");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat, login("Bank", "ich", "geheim-1234")).unwrap();
    let zip = s.suite_backup().unwrap();
    assert!(!zip.windows(11).any(|w| w == b"geheim-1234"), "backup holds only encrypted files");
    s.lock();
    assert!(s.suite_backup().is_err(), "only while unlocked");

    let d2 = Dirs::new("backup-b");
    let t = d2.suite();
    assert!(t.restore_backup(b"kein zip".to_vec()).is_err());
    assert!(t.restore_backup(zip.clone()).unwrap() >= 2);
    assert!(t.restore_backup(zip).is_err(), "not over an existing suite");
    t.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(t.list_entries(VaultKind::Pass, None).unwrap()[0].title, "Bank");
}

#[test]
fn calendar_vault_events_series_reminders_and_sync() {
    use super::cal::{CalCalendar, CalDeleteMode, CalEvent, CalLink, CalRepeat};
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-cal-server-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("cal-a"), Dirs::new("cal-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    let vault = vault_id(&a, "Kalender");
    let file = vault_file(&a, &vault);
    assert!(file.ends_with(".ghlcal"), "{file}");
    assert_eq!(a.list_vaults().unwrap()[0].kind, VaultKind::Cal);

    let cal = |name: &str, color: u32| CalCalendar {
        id: String::new(),
        vault_id: vault.clone(),
        name: name.into(),
        color,
        default_reminder: 15,
        events: 0,
    };
    let privat = a.cal_save_calendar(cal("Privat", 0xd6336c)).unwrap();
    let work = a.cal_save_calendar(cal("Arbeit", 0x2f74e8)).unwrap();
    let dentist = a
        .cal_save_event(CalEvent {
            calendar_id: privat.clone(),
            title: " Zahnarzt Dr. Weber ".into(),
            start: "2026-11-13T10:00".into(),
            end: "2026-11-13T10:45".into(),
            reminders: vec![1440, 60],
            location: "Praxis am Markt".into(),
            links: vec![CalLink { kind: "drive".into(), label: "Überweisung.pdf".into(), target: "x".into() }],
            ..Default::default()
        })
        .unwrap();
    let standup = a
        .cal_save_event(CalEvent {
            calendar_id: work.clone(),
            title: "Team-Standup".into(),
            start: "2026-11-09T09:30".into(),
            end: "2026-11-09T09:45".into(),
            repeat: CalRepeat::Weekdays,
            reminders: vec![5],
            ..Default::default()
        })
        .unwrap();
    a.cal_save_event(CalEvent {
        calendar_id: privat.clone(),
        title: "Urlaub".into(),
        all_day: true,
        start: "2026-12-06".into(),
        end: "2026-12-13".into(),
        reminders: vec![1440],
        ..Default::default()
    })
    .unwrap();
    // Invalid input is refused.
    assert!(a
        .cal_save_event(CalEvent {
            calendar_id: privat.clone(),
            start: "2026-11-13T10:00".into(),
            ..Default::default()
        })
        .is_err());
    assert!(a
        .cal_save_event(CalEvent {
            calendar_id: "00".into(),
            title: "x".into(),
            start: "2026-11-13".into(),
            ..Default::default()
        })
        .is_err());

    let e = a.cal_get_event(dentist.clone()).unwrap();
    assert_eq!(e.title, "Zahnarzt Dr. Weber");
    assert_eq!(e.reminders, vec![60, 1440]);
    assert_eq!(e.links[0].label, "Überweisung.pdf");

    let cals = a.cal_calendars().unwrap();
    assert_eq!(cals.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Privat", "Arbeit"]);
    assert_eq!(cals[0].events, 2);

    // A week with holidays: the standup on five days, the dentist, all-day first on its day.
    let week = a.cal_occurrences("2026-11-09".into(), "2026-11-15".into(), "DE-NW".into(), "de".into()).unwrap();
    assert_eq!(week.iter().filter(|o| o.event_id == standup).count(), 5);
    let fri: Vec<&str> = week.iter().filter(|o| o.start.starts_with("2026-11-13")).map(|o| o.title.as_str()).collect();
    assert_eq!(fri, vec!["Team-Standup", "Zahnarzt Dr. Weber"]);
    let nov1 = a.cal_occurrences("2026-11-01".into(), "2026-11-01".into(), "DE-NW".into(), "de".into()).unwrap();
    assert_eq!(nov1.len(), 1);
    assert_eq!((nov1[0].calendar_id.as_str(), nov1[0].title.as_str()), ("holidays", "Allerheiligen"));
    let mid = a.cal_occurrences("2026-12-10".into(), "2026-12-10".into(), "".into(), "de".into()).unwrap();
    assert_eq!(mid.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), vec!["Urlaub", "Team-Standup"]);

    // Reminders over the next days, in time order (all-day relative to 9:00).
    let rem = a.cal_reminders("2026-11-12T08:00".into(), 2).unwrap();
    let at: Vec<(&str, &str)> = rem.iter().map(|r| (r.at.as_str(), r.title.as_str())).collect();
    assert_eq!(
        at,
        vec![
            ("2026-11-12T09:25", "Team-Standup"),
            ("2026-11-12T10:00", "Zahnarzt Dr. Weber"),
            ("2026-11-13T09:00", "Zahnarzt Dr. Weber"),
            ("2026-11-13T09:25", "Team-Standup"),
        ]
    );
    let dec = a.cal_reminders("2026-12-05T00:00".into(), 1).unwrap();
    assert!(dec.iter().any(|r| r.title == "Urlaub" && r.at == "2026-12-05T09:00"));

    // Deleting one occurrence and the rest of a series.
    a.cal_delete_event(standup.clone(), "2026-11-11".into(), CalDeleteMode::One).unwrap();
    a.cal_delete_event(standup.clone(), "2026-11-13".into(), CalDeleteMode::Following).unwrap();
    let week = a.cal_occurrences("2026-11-09".into(), "2026-11-30".into(), "".into(), "de".into()).unwrap();
    let days: Vec<&str> = week.iter().filter(|o| o.event_id == standup).map(|o| &o.start[..10]).collect();
    assert_eq!(days, vec!["2026-11-09", "2026-11-10", "2026-11-12"]);

    // Moving to another calendar keeps the id.
    let mut e = a.cal_get_event(dentist.clone()).unwrap();
    e.calendar_id = work.clone();
    assert_eq!(a.cal_save_event(e).unwrap(), dentist);

    // .ics out and back in: duplicates are skipped.
    let ics = a.cal_export_ics(vec![]).unwrap();
    assert!(ics.contains("SUMMARY:Zahnarzt Dr. Weber"));
    let fresh = a.cal_save_calendar(cal("Import", 0x0e9f80)).unwrap();
    let n =
        a.cal_import_ics(fresh.clone(), ics.clone().into_bytes(), Arc::new(super::caltext::FixedOffset(60))).unwrap();
    assert_eq!(n, 3);
    assert_eq!(
        a.cal_import_ics(fresh.clone(), ics.into_bytes(), Arc::new(super::caltext::FixedOffset(60))).unwrap(),
        0
    );
    a.cal_delete_calendar(fresh).unwrap();
    assert_eq!(a.cal_calendars().unwrap().len(), 2);

    // Sync to a second device, change on both, merge.
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(b.cal_calendars().unwrap().len(), 2);
    let mut on_b = b.cal_get_event(dentist.clone()).unwrap();
    on_b.location = "Praxis Süd".into();
    b.cal_save_event(on_b).unwrap();
    a.cal_save_event(CalEvent {
        calendar_id: privat,
        title: "Abendessen".into(),
        start: "2026-11-12T19:00".into(),
        end: "2026-11-12T21:00".into(),
        ..Default::default()
    })
    .unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        let day = s.cal_occurrences("2026-11-12".into(), "2026-11-13".into(), "".into(), "de".into()).unwrap();
        assert!(day.iter().any(|o| o.title == "Abendessen"));
        assert!(day.iter().any(|o| o.location == "Praxis Süd"));
    }
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn old_suites_move_drive_and_mail_into_their_own_files() {
    use ciborium::value::Value;
    let d = Dirs::new("areas");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    // A suite from before: GhulMail and GhulDrive keep their settings in the suite file.
    s.with_open(|o| {
        let st = &mut o.suite.payload.settings;
        st.insert(
            "mail_accounts".into(),
            Value::Array(vec![Value::Map(vec![(Value::Text("id".into()), Value::Text("a1".into()))])]),
        );
        st.insert("mail_drafts".into(), Value::Array(vec![]));
        st.insert("drive".into(), Value::Map(vec![(Value::Text("server".into()), Value::Text("s1".into()))]));
        st.insert("servers".into(), Value::Array(vec![]));
        Ok(())
    })
    .unwrap();
    // Any call moves them; the suite keeps only its own settings.
    let kinds: Vec<VaultKind> = s.list_vaults().unwrap().iter().map(|v| v.kind).collect();
    assert!(kinds.contains(&VaultKind::Mail) && kinds.contains(&VaultKind::Drive), "{kinds:?}");
    s.with_open(|o| {
        let st = &o.suite.payload.settings;
        assert!(!st.contains_key("mail_accounts") && !st.contains_key("drive") && st.contains_key("servers"));
        let mail = o.area_id(VaultKind::Mail).unwrap();
        assert!(o.vault_mut(&mail)?.1.payload.settings.contains_key("mail_accounts"));
        Ok(())
    })
    .unwrap();
    let files: Vec<String> = s.list_vaults().unwrap().into_iter().map(|v| v.file).collect();
    assert!(files.iter().any(|f| f.ends_with(".ghlmail")) && files.iter().any(|f| f.ends_with(".ghldrv")), "{files:?}");
    // The files are written: after locking, everything is still where it moved to.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    s.with_open(|o| {
        assert!(!o.suite.payload.settings.contains_key("mail_accounts"));
        let id = o.area_id(VaultKind::Mail).unwrap();
        assert!(o.vault_mut(&id)?.1.payload.settings.contains_key("mail_accounts"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn recovery_code_counts_only_when_a_group_was_typed_back() {
    let d = Dirs::new("code-confirm");
    let s = d.suite();
    let kf = generate_keyfile();
    let code = s
        .create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None)
        .unwrap()
        .recovery_code
        .unwrap();
    // Closed before confirming (e.g. the app crashed in the last setup step).
    assert!(!s.recovery_confirmed().unwrap());
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(!s.recovery_confirmed().unwrap(), "still asked for after unlocking");
    let group = |c: &str, i: usize| c.split('-').nth(i).unwrap().to_owned();
    assert!(!s.confirm_recovery_code(2, group(&code, 3)).unwrap());
    assert!(!s.recovery_confirmed().unwrap());
    assert!(s.confirm_recovery_code(2, format!(" {} ", group(&code, 2).to_lowercase())).unwrap());
    assert!(s.recovery_confirmed().unwrap());
    assert!(s.confirm_recovery_code(99, "x".into()).is_err());
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert!(s.recovery_confirmed().unwrap());
    // A new code needs its own confirmation.
    let new = s.new_recovery_code("Neu".into()).unwrap();
    assert!(!s.recovery_confirmed().unwrap());
    assert!(s.confirm_recovery_code(0, group(&new, 0)).unwrap());
    assert!(s.recovery_confirmed().unwrap());
}

#[test]
fn a_setup_that_stopped_halfway_is_completed_by_unlocking() {
    let d = Dirs::new("setup-abort");
    let kf = generate_keyfile();
    let storage = d.storage();
    // Stopped before anything was written (e.g. while touching the key): nothing to clean up.
    assert_eq!(d.suite().setup_state().unwrap(), super::setup::SetupState::Empty);
    let s = d.suite();
    s.create(
        options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(s.setup_state().unwrap(), super::setup::SetupState::Complete);
    s.lock();
    // Stopped after the first vault file: the second vault and the suite file are missing.
    let files = storage.list().unwrap();
    let totp = files.iter().find(|f| f.ends_with(".ghltotp")).unwrap().clone();
    storage.delete(totp).unwrap();
    storage.delete(super::suite::SUITE_FILE.to_owned()).unwrap();
    let s = d.suite();
    assert_eq!(s.setup_state().unwrap(), super::setup::SetupState::Incomplete);
    assert!(s.exists().unwrap(), "the setup is not offered again over the existing files");
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.setup_state().unwrap(), super::setup::SetupState::Complete);
    let vaults = s.list_vaults().unwrap();
    assert_eq!(vaults.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), vec!["Privat"]);
    // The code was never confirmed: the app asks for a new one; a missing area can be added.
    assert!(!s.recovery_confirmed().unwrap());
    s.add_area(VaultKind::Totp, String::new()).unwrap();
    s.lock();
    let s = d.suite();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 2);
}

#[test]
fn area_options_live_in_the_area_file_and_areas_can_be_removed_everywhere() {
    use crate::net::remote::LocalFs;
    let server_dir = std::env::temp_dir().join(format!("ghul-areas-server-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("areas-a"), Dirs::new("areas-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    // No file, no options: Drive was not chosen.
    assert!(a.area_options(VaultKind::Drive).unwrap().is_empty());
    let set = |s: &GhulSuite, k: &str, v: &str| {
        s.set_area_options(VaultKind::Cal, [(k.to_owned(), v.to_owned())].into_iter().collect()).unwrap()
    };
    assert!(matches!(
        a.set_area_options(VaultKind::Drive, [("trash_days".into(), "30".into())].into_iter().collect()),
        Err(GhulError::NotConfigured)
    ));
    set(&a, "holiday_region", "DE-NW");
    set(&a, "week_start", "0");
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(b.area_options(VaultKind::Cal).unwrap()["holiday_region"], "DE-NW");
    // Both change options: the later change of each option wins, on both devices.
    set(&a, "week_start", "6");
    std::thread::sleep(std::time::Duration::from_millis(5));
    set(&b, "holiday_region", "AT");
    std::thread::sleep(std::time::Duration::from_millis(5));
    set(&a, "holiday_region", "CH");
    b.sync_folder(&server).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    for s in [&a, &b] {
        let o = s.area_options(VaultKind::Cal).unwrap();
        assert_eq!((o["holiday_region"].as_str(), o["week_start"].as_str()), ("CH", "6"));
    }
    // Removing an area deletes its file here, on the server and on the other device.
    a.add_area(VaultKind::Drive, String::new()).unwrap();
    a.remove_area(VaultKind::Cal).unwrap();
    assert!(a.area_options(VaultKind::Cal).unwrap().is_empty());
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    let kinds = |s: &GhulSuite| s.list_vaults().unwrap().into_iter().map(|v| v.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&a), vec![VaultKind::Drive]);
    assert_eq!(kinds(&b), vec![VaultKind::Drive]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
fn files_of_a_newer_app_ask_for_an_update() {
    let mut data = b"GHULFUTR".to_vec();
    data.extend_from_slice(&[1, 0, 0, 0, 0, 0]);
    data.extend_from_slice(&[0u8; 64]);
    let e: GhulError = crate::vault::LockedContainer::parse(&data).err().unwrap().into();
    assert!(matches!(e, GhulError::UpdateRequired));
    let e: GhulError =
        crate::vault::LockedContainer::parse(b"PK\x03\x04 not ours at all, really").err().unwrap().into();
    assert!(matches!(e, GhulError::Corrupted { .. }));
}

#[test]
fn import_keeps_what_it_cannot_convert_and_names_it() {
    use super::import::SkipReason;
    use super::suite::KeepSkipped;
    let d = Dirs::new("import-keep");
    let s = d.suite();
    s.create(
        options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
        None,
        None,
        None,
    )
    .unwrap();
    let (pass, totp) = (vault_id(&s, "Privat"), vault_id(&s, "2FA"));
    // Proton Authenticator: a normal code, a link without a label (the name comes separately),
    // a Steam code and a counter-based code (HOTP).
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    let export = format!(
        r#"{{"version":1,"entries":[
          {{"content":{{"uri":"otpauth://totp/Proton:me?secret={secret}&issuer=Proton","entry_type":"Totp","name":"Proton"}}}},
          {{"content":{{"uri":"otpauth://totp/?secret=JBSWY3DPEHPK3PXP","entry_type":"Totp","name":"Sparkasse"}}}},
          {{"content":{{"uri":"steam://MFRGGZDFMZTWQ2LK","entry_type":"Steam","name":"Steam"}}}},
          {{"content":{{"uri":"otpauth://hotp/Alt:konto?secret={secret}&counter=3","entry_type":"Totp","name":"Alt"}}}}]}}"#
    );
    let preview = super::suite::preview_import(export.as_bytes().to_vec(), "proton.json".into()).unwrap();
    assert_eq!((preview.format.as_str(), preview.totps, preview.skipped), ("Proton Authenticator", 3, 1));
    let item = &preview.skipped_items[0];
    assert_eq!((item.name.as_str(), item.reason, item.in_entry), ("Alt", SkipReason::Hotp, false));

    let keep = KeepSkipped {
        folder: "Import (nicht umgewandelt)".into(),
        reason_label: "Grund".into(),
        data_label: "Originaldaten".into(),
        reason_hotp: "Zählerbasierter Code (HOTP)".into(),
        reason_unsupported: "Eintragsart unbekannt".into(),
        reason_invalid: "Code-Daten unvollständig".into(),
    };
    let r = s
        .import_data(
            export.as_bytes().to_vec(),
            "proton.json".into(),
            Some(pass.clone()),
            Some(totp.clone()),
            false,
            Some(keep.clone()),
        )
        .unwrap();
    assert_eq!((r.totps, r.skipped, r.kept), (3, 1, 1));
    let codes = s.totp_codes(59).unwrap();
    assert!(codes.iter().any(|c| c.issuer == "Sparkasse"), "label filled in from Proton's name");
    let steam = codes.iter().find(|c| c.issuer == "Steam").unwrap();
    assert_eq!(steam.code.len(), 5, "Steam Guard code: {}", steam.code);
    // The HOTP account is kept as a note with its original data.
    let notes = s.list_entries(VaultKind::Pass, Some(pass.clone())).unwrap();
    let note = notes.iter().find(|e| e.title == "Alt").unwrap();
    let full = s.get_entry(pass.clone(), note.id.clone()).unwrap();
    let field = |n: &str| match full.fields.iter().find(|f| f.name == n).map(|f| f.value.clone()) {
        Some(FieldValue::Text { value }) | Some(FieldValue::Secret { value }) | Some(FieldValue::Url { value }) => {
            value
        }
        other => format!("{other:?}"),
    };
    assert!(field("Originaldaten").contains("otpauth://hotp/Alt:konto"));
    assert!(field("Grund").contains("HOTP"));
    // A second import keeps nothing twice.
    let r =
        s.import_data(export.into_bytes(), "proton.json".into(), Some(pass), Some(totp), false, Some(keep)).unwrap();
    assert_eq!((r.totps, r.kept), (0, 0));
}

mod absurd;
mod attacks;
mod chaos;
mod collections;
mod crazy_user;
mod everyday;
mod gnupg;
mod properties;
mod rekey;
mod security;

/// "Change storage location": every file is copied and checked, a target holding a suite is
/// refused, and the suite opens from the new place with all entries.
#[test]
fn copy_to_other_storage() {
    let from = Dirs::new("copy-from");
    let to = Dirs::new("copy-to");
    let kf = generate_keyfile();
    let s = from.suite();
    s.create(
        options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "Codes")]),
        None,
        None,
        None,
    )
    .unwrap();
    let pass = s.list_vaults().unwrap().into_iter().find(|v| v.kind == VaultKind::Pass).unwrap();
    s.save_entry(pass.id.clone(), login("A", "anna", "geheim-1")).unwrap();

    // Copied while unlocked: settings file plus two vault files.
    assert_eq!(s.copy_to(to.storage()).unwrap(), 3);
    let mut a = from.storage().list().unwrap();
    let mut b = to.storage().list().unwrap();
    a.retain(|f| !f.ends_with(".bak") && !f.ends_with(".tmp"));
    b.retain(|f| !f.ends_with(".bak") && !f.ends_with(".tmp"));
    a.sort();
    b.sort();
    assert_eq!(a, b);
    for f in &a {
        assert_eq!(from.storage().read(f.clone()).unwrap(), to.storage().read(f.clone()).unwrap(), "{f}");
    }
    // The old place keeps its files.
    assert!(from.suite().exists().unwrap());

    // A second copy into the same place is refused, nothing is overwritten.
    assert!(matches!(s.copy_to(to.storage()), Err(GhulError::AlreadyExists)));

    // The new place opens on its own.
    s.lock();
    let moved = GhulSuite::new(to.storage(), from.local());
    moved.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(moved.list_entries(VaultKind::Pass, None).unwrap()[0].title, "A");

    // An empty source has nothing to copy.
    let empty = Dirs::new("copy-empty");
    assert!(matches!(empty.suite().copy_to(Dirs::new("copy-empty-to").storage()), Err(GhulError::NoVault)));
}

/// A vault file renamed by hand (other name, even the extension of another kind) is recognised
/// by its content and gets its neutral name back at the next unlock. A file renamed to something
/// that is no vault file at all (`.txt`) is simply not seen, and is back once its name is.
#[test]
fn renamed_vault_files_are_found_by_content() {
    let d = Dirs::new("renamed");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let v = s.list_vaults().unwrap().into_iter().find(|v| v.kind == VaultKind::Pass).unwrap();
    s.save_entry(v.id.clone(), login("A", "b", "c")).unwrap();
    s.lock();
    let st = d.storage();
    let data = st.read(v.file.clone()).unwrap().unwrap();

    for other in ["Urlaub 2026.ghlpss", "x.ghltotp"] {
        st.delete(v.file.clone()).unwrap();
        st.write(other.into(), data.clone()).unwrap();
        s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
        let now = s.list_vaults().unwrap().into_iter().find(|x| x.id == v.id).unwrap();
        assert_eq!(now.file, v.file, "back to its neutral name");
        assert_eq!(now.kind, VaultKind::Pass, "the kind comes from the content, not the extension");
        assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap()[0].title, "A");
        assert!(st.read(other.into()).unwrap().is_none());
        s.lock();
    }

    st.delete(v.file.clone()).unwrap();
    st.write("tresor.txt".into(), data.clone()).unwrap();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.list_entries(VaultKind::Pass, None).unwrap().is_empty());
    s.lock();
    st.delete("tresor.txt".into()).unwrap();
    st.write("tresor.ghlpss".into(), data).unwrap();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap()[0].title, "A");
}

/// A server name that does not resolve and a port nobody listens on give fixed texts the app
/// translates, not the system's message ("failed to lookup address information: ...").
#[test]
fn unknown_server_name_and_refused_port_have_fixed_texts() {
    use crate::net::NetError;
    let msg = |e: std::io::Error| match GhulError::from(NetError::Connect(e.to_string())) {
        GhulError::Network { msg } => msg,
        other => panic!("{other:?}"),
    };
    let lookup = std::net::TcpStream::connect(("nirgendwo.invalid", 22)).unwrap_err();
    assert_eq!(msg(lookup), "server address not found");
    // A port that was just free on this machine: nobody listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let refused = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap_err();
    assert_eq!(msg(refused), "connection refused");
    // Other failures keep their text.
    let other = GhulError::from(NetError::Connect("connection closed".into()));
    assert!(matches!(other, GhulError::Network { msg } if msg == "connection failed: connection closed"));
}

/// Review of S4: an item counts as deleted only while it is gone from its list. One renewed
/// after its deletion mark (it stays in the list) is not.
#[test]
fn an_item_renewed_after_its_deletion_mark_is_not_deleted() {
    use ciborium::Value;
    let d = Dirs::new("marked-renewed");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    s.add_area(VaultKind::Drive, String::new()).unwrap();
    let state = s
        .with_open(|o| {
            super::areas::mark_area_deleted(o, VaultKind::Drive, "servers", "x")?;
            let deleted = super::areas::area_marked_deleted(o, VaultKind::Drive, "servers", "x");
            let item = Value::Map(vec![
                (Value::Text("id".into()), Value::Text("x".into())),
                (Value::Text("updated".into()), Value::Integer(i64::MAX.into())),
            ]);
            super::areas::put_area_setting(o, VaultKind::Drive, "servers", &vec![item])?;
            Ok((deleted, super::areas::area_marked_deleted(o, VaultKind::Drive, "servers", "x")))
        })
        .unwrap();
    assert_eq!(state, (true, false));
}

/// After a change of the protection GhulDrive gets a new key: whoever kept the old one (an old
/// copy and an old recovery code) reads no index written from then on, and an index sealed with
/// the old key is no longer accepted where the new one has written.
#[test]
fn drive_gets_a_new_key_with_a_new_recovery_code() {
    let d = Dirs::new("drive-dk");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-drive-dk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    s.drive_use_folder(Arc::new(crate::net::remote::LocalFs::new(&server))).unwrap();
    let old_dk = [7u8; 32];
    let alt = upload(&s, None, "Alt.txt", b"vorher");
    let index = server.join("drive/index.ghldrv");
    assert!(GhulSuite::drive_index_opens_with(&old_dk, &std::fs::read(&index).unwrap()));

    s.new_recovery_code("Neu".into()).unwrap();
    // The next access writes the index with the new key, before anything else changes.
    s.drive_list(None, true).unwrap();
    assert!(!GhulSuite::drive_index_opens_with(&old_dk, &std::fs::read(&index).unwrap()), "new key");
    upload(&s, None, "Neu.txt", b"nachher");
    assert_eq!(download(&s, &alt), b"vorher", "older files stay readable");
    let names: Vec<String> = s.drive_list(None, true).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["Alt.txt", "Neu.txt"]);

    // An index sealed with the old key is not accepted any more.
    // (Also what a device that had not yet seen the new key writes in the same moment as one
    // that had: taken only when the user accepts it, then written with the new key at once.)
    let good = std::fs::read(&index).unwrap();
    std::fs::write(&index, s.drive_index_sealed_with(&old_dk, true)).unwrap();
    assert!(matches!(s.drive_list(None, true), Err(GhulError::DriveRolledBack)), "an index made with the old key");
    s.drive_accept_server_state().unwrap();
    assert!(!GhulSuite::drive_index_opens_with(&old_dk, &std::fs::read(&index).unwrap()), "written again");
    std::fs::write(&index, &good).unwrap();
    let _ = s.drive_accept_server_state();

    // H0: whoever has the Drive key (any recovery code opens the Drive file, also the current
    // one) but not the password cannot plant an index: not accepted, also not when asked to.
    let current_dk = s.drive_key_for_tests();
    std::fs::write(&index, s.drive_index_sealed_with(&current_dk, false)).unwrap();
    assert!(matches!(s.drive_list(None, true), Err(GhulError::Corrupted { .. })), "made without the password");
    assert!(s.drive_accept_server_state().is_err(), "not even when accepted");
    std::fs::write(&index, &good).unwrap();
    let names: Vec<String> = s.drive_list(None, true).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["Alt.txt", "Neu.txt"]);
    let _ = std::fs::remove_dir_all(&server);
}
