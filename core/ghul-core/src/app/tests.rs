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

    // Password change: an old copy (old salt) needs the old password once and is brought up to date.
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    st.delete(privat_file.clone()).unwrap();
    st.write("privat-alt.ghlpss".into(), old_privat.clone()).unwrap();
    let f = s.find_new_vaults().unwrap();
    assert_eq!(f.len(), 1);
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

#[test]
fn import_export_and_autofill() {
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

    // Autofill: websites by base domain; apps only directly when linked with their certificate.
    let web = s.autofill_logins(Some("gist.github.com".into()), None, None).unwrap();
    assert_eq!((web.len(), web[0].password.as_str(), web[0].direct), (1, "pw1", true));
    assert!(s.autofill_logins(Some("github.com.evil.net".into()), None, None).unwrap().is_empty());
    let app = s.autofill_logins(None, Some("com.github.android".into()), Some("c1".into())).unwrap();
    assert!(app.len() == 1 && !app[0].direct, "a package name alone is only a suggestion");
    let bank = s.autofill_logins(None, Some("de.bank.app".into()), Some("c2".into())).unwrap();
    assert!(bank[0].title == "Bank" && !bank[0].direct, "imported link without certificate");
    assert!(s.autofill_logins(None, Some("org.other.app".into()), None).unwrap().is_empty());
    let gh = &web[0];
    s.link_app_to_login(gh.vault_id.clone(), gh.id.clone(), "org.other.app".into(), Some("C3".into())).unwrap();
    assert!(s.autofill_logins(None, Some("org.other.app".into()), Some("c3".into())).unwrap()[0].direct);
    assert!(
        s.autofill_logins(None, Some("org.other.app".into()), Some("ff".into())).unwrap().is_empty(),
        "other certificate"
    );
    assert_eq!(s.autofill_logins(None, None, None).unwrap().len(), 2);

    // Saving from autofill: new login, then a password change of the same login.
    use super::suite::AutofillSaved;
    let save = |d: Option<&str>, p: Option<&str>, u: &str, pw: &str| {
        s.autofill_save(d.map(str::to_owned), p.map(str::to_owned), Some("c9".into()), u.into(), pw.into()).unwrap()
    };
    assert_eq!(save(Some("shop.example.org"), None, "kunde", "a1"), AutofillSaved::Created);
    assert_eq!(save(Some("www.example.org"), None, "kunde", "a1"), AutofillSaved::Unchanged);
    assert_eq!(save(Some("example.org"), None, "kunde", "a2"), AutofillSaved::Updated);
    assert_eq!(s.autofill_logins(Some("example.org".into()), None, None).unwrap()[0].password, "a2");
    assert_eq!(save(None, Some("com.chat.app"), "", "x"), AutofillSaved::Created);
    let chat = s.autofill_logins(None, Some("com.chat.app".into()), Some("c9".into())).unwrap();
    assert!(chat[0].title == "chat.com" && chat[0].direct);
    assert_eq!(save(None, Some("com.chat.app"), "", "y"), AutofillSaved::Updated);

    // Plain export needs the password and can be imported again.
    assert!(matches!(s.export_plain(pass.clone(), "falsch".into()), Err(GhulError::WrongPassword)));
    let csv = s.export_plain(pass.clone(), "pw-123456".into()).unwrap();
    assert!(String::from_utf8(csv.clone()).unwrap().contains("GitHub"));
    let links = s.export_plain(totp.clone(), "pw-123456".into()).unwrap();
    assert!(String::from_utf8(links.clone()).unwrap().starts_with("otpauth://totp/GitHub:ghul?"));
    let other = s.create_vault(VaultKind::Pass, "Kopie".into()).unwrap();
    let r = s.import_data(csv, "export.csv".into(), Some(other.clone()), None, false, None).unwrap();
    assert_eq!(r.entries, 5);
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

#[test]
fn sync_over_sftp() {
    use super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let authorize = |public: &str| {
        let mut keys = std::fs::read_to_string(&authkeys).unwrap_or_default();
        keys.push_str(public);
        keys.push('\n');
        std::fs::write(&authkeys, keys).unwrap();
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
        url: String::new(),
        upload: None,
    };
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
    // Same content again under another name: blocks are stored once.
    let blocks_before = files_under(&server_dir.join("drive/blocks")).len();
    let copy = upload(&s, Some(photos.clone()), "urlaub kopie.jpg", &big);
    assert_eq!(files_under(&server_dir.join("drive/blocks")).len(), blocks_before);
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
    assert_eq!(
        files_under(&server_dir.join("drive/blocks")).len(),
        blocks_before + 1,
        "blocks still used by the original and its version (+ the note)"
    );
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
        let mut keys = std::fs::read_to_string(&authkeys).unwrap_or_default();
        keys.push_str(public);
        keys.push('\n');
        std::fs::write(&authkeys, keys).unwrap();
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
        url: String::new(),
        upload: None,
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
    // Self-signed certificates are shown with their fingerprint until confirmed.
    assert_eq!(s.mail_check_account(input.clone()).unwrap(), MailCheck::ImapCertificate { fingerprint: fp.clone() });
    input.imap_cert = Some(fp.clone());
    assert_eq!(s.mail_check_account(input.clone()).unwrap(), MailCheck::SmtpCertificate { fingerprint: fp.clone() });
    input.smtp_cert = Some(fp.clone());
    let mut wrong = input.clone();
    wrong.password = "falsch".into();
    assert!(matches!(s.mail_check_account(wrong), Err(GhulError::ServerAuthFailed)));
    // IMAP only with TLS from the start: STARTTLS is refused before connecting.
    let mut starttls = input.clone();
    starttls.servers.imap_port = 10143;
    starttls.servers.imap_security = MailSecurity::StartTls;
    assert!(matches!(s.mail_check_account(starttls.clone()), Err(GhulError::InvalidInput { .. })));
    assert!(s.mail_save_account(None, starttls).is_err());
    assert_eq!(s.mail_check_account(input.clone()).unwrap(), MailCheck::Ok);
    // B21: the field "Login name" usually stays empty; the check then logs in with the address,
    // like the saved account does (before, it logged in with an empty name and always failed).
    let mut no_login = input.clone();
    no_login.username = String::new();
    no_login.email = format!(" {email} ");
    assert_eq!(s.mail_check_account(no_login).unwrap(), MailCheck::Ok);

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
    let mut edit = input;
    edit.password = String::new();
    edit.name = "Umbenannt".into();
    s.mail_save_account(Some(id.clone()), edit).unwrap();
    assert_eq!(s.mail_accounts().unwrap()[0].name, "Umbenannt");
    assert_eq!(s.mail_folders(id.clone()).unwrap()[0].role, "inbox");
    s.mail_remove_account(id).unwrap();
    assert!(s.mail_accounts().unwrap().is_empty());
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

/// Fetches a share link like the recipient's browser and decrypts it.
fn open_link(link: &str, password: Option<&str>) -> Option<(crate::share::ShareMeta, Vec<u8>)> {
    let (page, fragment) = link.split_once('#').unwrap();
    let (id, key) = fragment.split_once('.').unwrap();
    let key: [u8; 32] = crate::share::b64url_decode(key).unwrap().try_into().unwrap();
    let data_url = if page.ends_with(".php") { format!("{page}?s={id}") } else { format!("{page}s/{id}") };
    let url = crate::net::http::Url::parse(&data_url).unwrap();
    let r = crate::net::block_on(crate::net::http::request("GET", &url, &[], b"", None), crate::net::TIMEOUT).unwrap();
    if r.status != 200 {
        return None;
    }
    crate::share::open(&r.body, &key, password).ok()
}

fn share_opts(server: &str) -> super::shares::ShareOptions {
    super::shares::ShareOptions { server_id: server.into(), password: None, expires_days: 7, max_downloads: 0 }
}

/// Shares through a share folder: set up with self-check, single file, several files as ZIP,
/// password, end. Works the same for SFTP and FTPS.
fn share_via_folder(s: &GhulSuite, server_id: &str, dir: &str, web_root: &std::path::Path, rel: &str) {
    let web = web_server(web_root);
    let public = format!("http://127.0.0.1:{}/{rel}", web.port);
    // A wrong address fails the https check and stores nothing.
    let bad = s.share_setup(server_id.into(), dir.into(), format!("http://127.0.0.1:{}/falsch", web.port)).unwrap();
    assert!(!bad.iter().find(|c| c.step == "https").unwrap().ok, "{bad:?}");
    assert!(s.list_servers().unwrap().iter().find(|x| x.id == server_id).unwrap().share_url.is_empty());
    let checks = s.share_setup(server_id.into(), dir.into(), public.clone()).unwrap();
    assert!(checks.iter().all(|c| c.ok), "{checks:?}");
    assert_eq!(s.list_servers().unwrap().iter().find(|x| x.id == server_id).unwrap().share_url, public);

    let a = upload(s, None, "Steuer 2025.pdf", b"%PDF-1.7 Steuer");
    let folder = s.drive_create_folder(None, "Urlaub".into()).unwrap();
    upload(s, Some(folder.clone()), "Strand.jpg", &[9u8; 300_000]);
    upload(s, Some(folder.clone()), "Möwe.jpg", &[8u8; 1000]);

    let one = s.share_drive(vec![a.clone()], None, share_opts(server_id)).unwrap();
    assert!(one.url.starts_with(&format!("{public}/#")), "{}", one.url);
    assert_eq!(one.downloads, -1);
    let (meta, data) = open_link(&one.url, None).unwrap();
    assert_eq!((meta.name.as_str(), data.as_slice()), ("Steuer 2025.pdf", &b"%PDF-1.7 Steuer"[..]));

    let mut opts = share_opts(server_id);
    opts.password = Some("Geheim 42".into());
    let several = s.share_drive(vec![folder, a], Some("Urlaub und Steuer".into()), opts).unwrap();
    assert!(open_link(&several.url, Some("falsch")).is_none());
    let (meta, zipped) = open_link(&several.url, Some("Geheim 42")).unwrap();
    assert_eq!(meta.name, "Urlaub und Steuer.zip");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
    let mut names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["Steuer 2025.pdf", "Urlaub/Möwe.jpg", "Urlaub/Strand.jpg"]);

    // Download limits need the server module.
    let mut limited = share_opts(server_id);
    limited.max_downloads = 1;
    assert!(matches!(
        s.share_drive(vec![one.id.clone()], None, limited),
        Err(GhulError::InvalidInput { .. }) | Err(GhulError::NotFound)
    ));

    assert_eq!(s.share_list().unwrap().len(), 2);
    // With sync: the server copy still lists both shares when one is withdrawn.
    let syncing = s.sync_status().unwrap().server_id.is_some();
    if syncing {
        s.sync_now().unwrap();
    }
    s.share_delete(one.id.clone()).unwrap();
    assert!(open_link(&one.url, None).is_none());
    assert_eq!(s.share_list().unwrap().len(), 1);
    if syncing {
        // A withdrawn share must not come back through the older server copy.
        s.sync_now().unwrap();
        s.sync_now().unwrap();
        assert_eq!(s.share_list().unwrap().len(), 1);
    }
    // The share folder holds no plain names.
    let stored: Vec<String> = std::fs::read_dir(web_root.join(rel).join("s"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(stored.iter().all(|n| !n.contains("Steuer") && !n.contains("Urlaub")), "{stored:?}");
    // B14: once its server is gone, withdrawing a link says that its package stays.
    let left = s.share_list().unwrap();
    if let Some(last) = left.first() {
        if s.remove_server(server_id.into()).is_ok() {
            assert!(matches!(s.share_delete(last.id.clone()), Err(GhulError::ShareNotDeleted)));
            assert!(s.share_list().unwrap().is_empty());
        }
    }
}

#[test]
fn share_over_sftp_with_web_folder() {
    use super::servers::{ServerInput, ServerKind};
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("share-sftp");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let mut keys = std::fs::read_to_string(&authkeys).unwrap_or_default();
    keys.push_str(&s.ssh_public_key().unwrap());
    keys.push('\n');
    std::fs::write(&authkeys, keys).unwrap();
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let input = ServerInput {
        kind: ServerKind::Sftp,
        name: "Heimserver".into(),
        host,
        port,
        user,
        password: None,
        base: format!("{root}/suite-{run}"),
        url: String::new(),
        upload: None,
    };
    let id = saved(s.save_server(input, Some(hostkey)).unwrap());
    s.drive_setup(id.clone()).unwrap();
    s.set_sync_server(Some(id.clone())).unwrap();
    // The suite folder itself cannot become the share folder.
    assert!(s.share_setup(id.clone(), format!("{root}/suite-{run}"), "http://127.0.0.1:1/x".into()).is_err());
    let web_rel = format!("web-{run}/teilen");
    share_via_folder(&s, &id, &format!("{root}/{web_rel}"), std::path::Path::new(&root), &web_rel);
}

#[test]
fn ftps_storage_drive_and_sharing() {
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
        url: String::new(),
        upload: None,
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
    // Folder browsing for the share setup.
    assert!(s.server_browse(id.clone(), String::new()).unwrap().contains(&cfg.base));
    let rel = format!("{}-web", cfg.base);
    share_via_folder(&s, &id, &rel, &home, &rel);
}

#[test]
fn php_module_storage_drive_and_limited_shares() {
    use super::servers::{ServerInput, ServerKind};
    let Some(php) = crate::net::tests::php_server() else { return };
    let d = Dirs::new("php");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let mut input = ServerInput {
        kind: ServerKind::Php,
        name: String::new(),
        host: String::new(),
        port: 0,
        user: String::new(),
        password: Some(crate::kdf::hex(&[3u8; 32])),
        base: String::new(),
        url: php.url.clone(),
        upload: None,
    };
    // A wrong key is refused before anything is stored.
    assert!(matches!(s.save_server(input.clone(), None), Err(GhulError::ServerAuthFailed)));
    input.password = Some(crate::kdf::hex(&php.key));
    let fetch_input = input.clone();
    let id = saved(s.save_server(input, None).unwrap());
    let info = s.list_servers().unwrap().into_iter().find(|x| x.id == id).unwrap();
    assert_eq!((info.kind, info.share_url.as_str()), (ServerKind::Php, php.url.as_str()));
    s.set_sync_server(Some(id.clone())).unwrap();
    s.sync_now().unwrap();
    // U7: a new device fetches the suite through the module (not only over SFTP).
    let new_device = Dirs::new("php-fetch");
    let b = new_device.suite();
    assert_eq!(b.fetch_from_server(fetch_input, None, None).unwrap(), "");
    b.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(b.list_vaults().unwrap().len(), s.list_vaults().unwrap().len());
    s.drive_setup(id.clone()).unwrap();
    let f = upload(&s, None, "Vertrag.pdf", b"%PDF Vertrag");
    assert_eq!(download(&s, &f), b"%PDF Vertrag");
    // B19: after locking and unlocking, Drive connects again by itself (from the stored server).
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(download(&s, &f), b"%PDF Vertrag");

    // One-time link: the second download fails, and the share disappears from the list.
    let mut once = share_opts(&id);
    once.max_downloads = 1;
    let link = s.share_drive(vec![f.clone()], None, once).unwrap();
    assert!(link.url.starts_with(&format!("{}#", php.url)));
    assert_eq!(s.share_list().unwrap()[0].downloads, 0);
    assert_eq!(open_link(&link.url, None).unwrap().1, b"%PDF Vertrag");
    assert!(open_link(&link.url, None).is_none());
    assert!(s.share_list().unwrap().is_empty());
    // Text share (e.g. a login), counted downloads, ended by the owner.
    let mut three = share_opts(&id);
    three.max_downloads = 3;
    let text = s.share_bytes("Zugang WLAN.txt".into(), b"SSID: Gast\nPasswort: x".to_vec(), three).unwrap();
    open_link(&text.url, None).unwrap();
    assert_eq!(s.share_list().unwrap()[0].downloads, 1);
    s.share_delete(text.id).unwrap();
    assert!(open_link(&text.url, None).is_none());
}

#[test]
fn drive_storage_follows_sync_or_uses_its_own_server() {
    use super::drive::DriveMode;
    use super::servers::{ServerFolders, ServerInput, ServerKind};
    let Some(cfg) = crate::net::tests::ftps_test_config("drive-modes") else { return };
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
        url: String::new(),
        upload: None,
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
    // "Like synchronisation" needs synchronisation.
    let st = s.drive_storage().unwrap();
    assert_eq!((st.mode, st.sync_available), (DriveMode::None, false));
    assert!(matches!(s.drive_use_sync(), Err(GhulError::NotConfigured)));
    s.set_sync_server(Some(sync_id.clone())).unwrap();
    s.drive_use_sync().unwrap();
    let st = s.drive_storage().unwrap();
    assert_eq!((st.mode, st.server.map(|x| x.id)), (DriveMode::Sync, Some(sync_id.clone())));
    let f = upload(&s, None, "Plan.txt", b"Plan A");

    // Before the sync server goes: Drive keeps that server as its own, in the Drive file.
    s.drive_detach_from_sync().unwrap();
    s.remove_server(sync_id.clone()).unwrap();
    let st = s.drive_storage().unwrap();
    assert_eq!(st.mode, DriveMode::Own);
    let own = st.server.unwrap().id;
    assert_ne!(own, sync_id);
    assert_eq!(download(&s, &f), b"Plan A");
    assert!(s.list_servers().unwrap().is_empty(), "Drive's server is not offered for synchronisation");
    assert!(s.remove_server(own.clone()).is_err(), "Drive's server is changed in GhulDrive");

    // A different own server replaces the earlier one; it survives locking.
    let other = format!("{}-b", cfg.base);
    let new_id = saved(s.drive_save_server(input(&other), Some(fp)).unwrap());
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    let st = s.drive_storage().unwrap();
    assert_eq!((st.mode, st.server.map(|x| x.id)), (DriveMode::Own, Some(new_id)));
    assert!(s.drive_list(None, true).unwrap().is_empty(), "a fresh Drive on the other folder");
}

#[test]
fn php_module_is_installed_over_ftps_and_takes_large_files_in_parts() {
    use super::drive::DriveMode;
    use super::servers::{ServerInput, ServerKind};
    let Some(cfg) = crate::net::tests::ftps_test_config("php-install") else { return };
    if std::process::Command::new("php").arg("-v").output().is_err() {
        return;
    }
    // The module goes to the FTP server's folder; a copy of it is served from a folder this test
    // owns (the web server must be able to create the module's data folder next to it).
    let home = std::env::var("GHUL_TEST_DIR").map(|d| std::path::PathBuf::from(d).join("ftp-home")).unwrap();
    let web = home.join(&cfg.base).join("www");
    let served = std::env::temp_dir().join(format!("ghul-php-served-{}", cfg.base));
    std::fs::create_dir_all(&served).unwrap();
    let (mut php, port) = crate::net::tests::spawn_php(&served).unwrap();
    let d = Dirs::new("php-install");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let ftps = ServerInput {
        kind: ServerKind::Ftps,
        name: String::new(),
        host: cfg.host.clone(),
        port: cfg.port,
        user: cfg.user.clone(),
        password: Some(cfg.password.to_string()),
        base: String::new(),
        url: String::new(),
        upload: None,
    };
    let dir = format!("{}/www", cfg.base);
    let url = format!("http://127.0.0.1:{port}/ghulsuite.php");
    // Unknown certificate: nothing is uploaded until it is confirmed.
    let first = s.php_module_install(ftps.clone(), None, dir.clone(), url.clone()).unwrap();
    assert_eq!(first.confirm, cfg.cert);
    assert!(!web.join("ghulsuite.php").exists());
    let done = s.php_module_install(ftps, cfg.cert.clone(), dir, url.clone()).unwrap();
    // Uploaded over FTPS; the address does not answer yet (the copy is not served yet).
    let uploaded = std::fs::read_to_string(web.join("ghulsuite.php")).unwrap();
    assert!(uploaded.contains(&done.key));
    assert_eq!((done.url.as_deref(), done.tried.clone()), (None, vec![url.clone()]));
    std::fs::write(served.join("ghulsuite.php"), &uploaded).unwrap();
    let info = super::servers::php_module_probe(url.clone(), done.key.clone()).unwrap();
    assert_eq!(info.version, info.current);
    assert!(info.limit > 0);

    // Drive on the module, keeping the FTPS login for later updates.
    let id = saved(
        s.drive_save_server(
            ServerInput {
                kind: ServerKind::Php,
                name: String::new(),
                host: String::new(),
                port: 0,
                user: String::new(),
                password: Some(done.key.clone()),
                base: String::new(),
                url,
                upload: done.upload.clone(),
            },
            None,
        )
        .unwrap(),
    );
    assert_eq!(s.drive_storage().unwrap().mode, DriveMode::Own);
    // Drive blocks are larger than one part: they go up in parts.
    let big: Vec<u8> = (0..9_000_000u32).map(|i| (i % 253) as u8).collect();
    let f = upload(&s, None, "Film.mp4", &big);
    assert_eq!(download(&s, &f), big);
    // Updating the module uploads it again over the kept FTPS login, with the same key and data folder.
    assert_eq!(s.php_module_update(id).unwrap(), info.current);
    assert_eq!(std::fs::read_to_string(web.join("ghulsuite.php")).unwrap(), uploaded);
    assert_eq!(download(&s, &f), big);
    let _ = php.kill();
    let _ = php.wait();
    let _ = std::fs::remove_dir_all(&served);
}

#[test]
fn module_addresses_are_guessed_from_the_ftps_folder() {
    let urls = super::servers::module_urls;
    assert_eq!(urls("ftp.example.org", "public_html/ghul")[0], "https://example.org/ghul/ghulsuite.php");
    assert_eq!(urls("w0123.webhost.test", "example.org/tools")[0], "https://example.org/tools/ghulsuite.php");
    assert_eq!(urls("ftp.example.org", "example.net/htdocs")[0], "https://example.net/ghulsuite.php");
    assert_eq!(
        urls("ftp.example.org", ""),
        vec!["https://example.org/ghulsuite.php", "https://www.example.org/ghulsuite.php"]
    );
}

#[test]
fn php_module_file_is_generated_and_uploaded() {
    let m = super::servers::php_module_create();
    assert_eq!(m.file_name, "ghulsuite.php");
    assert_eq!(m.key.len(), 64);
    assert!(m.content.contains(&m.key) && !m.content.contains("__GHUL_"));
    assert_ne!(m.key, super::servers::php_module_create().key);
}

/// The way the app puts Drive files into a mail: decrypted file as attachment and a share link in
/// the text. Sent over SMTP, read back over IMAP, and the link from the received mail is opened.
#[test]
fn drive_file_and_share_link_into_mail() {
    use super::mail::{MailAccountInput, MailSecurity, MailServers, OutgoingAttachment, OutgoingMail};
    use super::servers::{ServerInput, ServerKind};
    let (Ok(host), Ok(fp)) = (std::env::var("GHUL_TEST_MAIL"), std::env::var("GHUL_TEST_MAIL_TLSFP")) else { return };
    let Some(php) = crate::net::tests::php_server() else { return };
    let d = Dirs::new("drive-mail");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();

    // GhulDrive on the PHP module; no mail account exists yet, Drive and sharing work without it.
    let srv = saved(
        s.save_server(
            ServerInput {
                kind: ServerKind::Php,
                name: String::new(),
                host: String::new(),
                port: 0,
                user: String::new(),
                password: Some(crate::kdf::hex(&php.key)),
                base: String::new(),
                url: php.url.clone(),
                upload: None,
            },
            None,
        )
        .unwrap(),
    );
    s.drive_setup(srv.clone()).unwrap();
    let content: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let file = upload(&s, None, "Rechnung März.pdf", &content);
    let link = s.share_drive(vec![file.clone()], None, share_opts(&srv)).unwrap();
    assert!(s.mail_accounts().unwrap().is_empty());

    // Now the mail module gets an account.
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let email = format!("drive{run}@test.local");
    let account = s
        .mail_save_account(
            None,
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
        )
        .unwrap();
    // Same as the app: decrypt the Drive file on the device, attach it, put the link in the text.
    let attachment = download(&s, &file);
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
    let (meta, data) = open_link(&received, None).unwrap();
    assert_eq!(meta.name, "Rechnung März.pdf");
    assert_eq!(data, content);
}

/// Fixture for the browser test of the PHP server module (tools/test-share-php.js): runs the
/// module under `php -S`, creates shares through the suite, writes their links and waits until
/// the browser test has finished (file `done`).
///
///   GHUL_PHP_FIXTURE=/tmp/x cargo test -p ghul-core php_browser_fixture & node tools/test-share-php.js /tmp/x
#[test]
fn php_browser_fixture() {
    use super::servers::{ServerInput, ServerKind};
    let Ok(dir) = std::env::var("GHUL_PHP_FIXTURE") else { return };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let Some(php) = crate::net::tests::php_server() else { panic!("php missing") };
    let d = Dirs::new("php-browser");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let id = saved(
        s.save_server(
            ServerInput {
                kind: ServerKind::Php,
                name: String::new(),
                host: String::new(),
                port: 0,
                user: String::new(),
                password: Some(crate::kdf::hex(&php.key)),
                base: String::new(),
                url: php.url.clone(),
                upload: None,
            },
            None,
        )
        .unwrap(),
    );
    let content: Vec<u8> = (0..2_500_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("expected.bin"), &content).unwrap();
    let mut links = String::new();
    let mut opts = share_opts(&id);
    links += &format!("plain {}\n", s.share_bytes("Bericht.pdf".into(), content.clone(), opts.clone()).unwrap().url);
    opts.password = Some("Grüße 123".into());
    links += &format!("pw {}\n", s.share_bytes("Geheim.pdf".into(), content.clone(), opts.clone()).unwrap().url);
    opts.password = None;
    opts.max_downloads = 1;
    links += &format!("once {}\n", s.share_bytes("Einmal.txt".into(), b"WLAN: geheim".to_vec(), opts).unwrap().url);
    // Already expired: the module must refuse it although the package exists.
    let fs = crate::net::phpfs::PhpFs::new(&crate::net::phpfs::PhpConfig {
        url: php.url.clone(),
        key: zeroize::Zeroizing::new(php.key),
        base: String::new(),
        cert: None,
    })
    .unwrap();
    let meta = crate::share::ShareMeta {
        name: "alt.txt".into(),
        mime: "text/plain".into(),
        size: 3,
        created: 0,
        expires: 1000,
    };
    let sealed = crate::share::seal_with(&meta, b"alt", None, 10_000, crate::share::CHUNK_SIZE);
    let sid = crate::kdf::hex(&rand_core::RngCore::next_u64(&mut rand_core::OsRng).to_be_bytes()).repeat(2);
    fs.share_put(&sid, &sealed.data, 1000, 0).unwrap();
    links += &format!("expired {}#{sid}.{}\n", php.url, crate::share::b64url(sealed.link_key.as_ref()));
    std::fs::write(dir.join("links.txt"), links).unwrap();

    let done = dir.join("done");
    for _ in 0..1800 {
        if done.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(done.exists(), "browser test did not finish");
    // The one-time link is gone from the list after its download; the others were counted.
    let list = s.share_list().unwrap();
    assert!(!list.iter().any(|x| x.name == "Einmal.txt"), "{list:?}");
    assert!(list.iter().find(|x| x.name == "Bericht.pdf").unwrap().downloads >= 1);
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
