// SPDX-License-Identifier: AGPL-3.0-or-later
//! App API tests against the software authenticator and temporary directories.

use std::sync::Arc;

use super::error::GhulError;
use super::storage::{local_storage, VaultStorage};
use super::suite::{
    generate_keyfile, EntryData, FieldData, FieldValue, GhulSuite, NewVault, SetupOptions, SlotKind, VaultKind,
};
use super::tools::{OtpAlgo, TotpInput};
use crate::mock::{MockAuthenticator, MockConfig};
use crate::{FidoListener, FidoStatus, HidTransport};

/// The master password most tests use.
const PW: &str = "pw-123456-Kq7wZ";

/// Both factors with a key file, for changes of the protection.
fn kf_proof(password: &str, kf: &[u8]) -> super::suite::FactorProof {
    super::suite::FactorProof { password: password.into(), pin: None, keyfile: Some(kf.to_vec()) }
}

/// The password alone (no key file, no PIN): refused for every change of the protection.
fn no_key_proof(password: &str) -> super::suite::FactorProof {
    super::suite::FactorProof { password: password.into(), pin: None, keyfile: None }
}

/// Both factors with a security key (PIN "1234", touched through the transport given with the call).
fn key_proof(password: &str) -> super::suite::FactorProof {
    super::suite::FactorProof { password: password.into(), pin: Some("1234".into()), keyfile: None }
}

/// Two new random keys for the app PIN's slots (what the app makes before it imports them).
fn pin_keys() -> Vec<Vec<u8>> {
    (0..2)
        .map(|_| {
            let mut k = vec![0u8; 32];
            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut k);
            k
        })
        .collect()
}

/// The Android Keystore as the app PIN uses it (M19): HMAC-SHA256 keys by generation and slot,
/// each usable once (`setMaxUsageCount(1)`), and the app's sequence around the core's calls.
#[derive(Default)]
pub(super) struct PinKeystore {
    pub(super) keys: std::collections::BTreeMap<(u32, u32), [u8; 32]>,
}

impl PinKeystore {
    pub(super) fn import(&mut self, generation: u32, keys: &[Vec<u8>]) {
        for (i, k) in keys.iter().enumerate() {
            self.keys.insert((generation, i as u32), k.as_slice().try_into().unwrap());
        }
    }

    /// One use of a key: it is gone afterwards.
    pub(super) fn sign(&mut self, generation: u32, slot: u32, message: &[u8]) -> Option<Vec<u8>> {
        use hmac::{Hmac, Mac};
        let k = self.keys.remove(&(generation, slot))?;
        let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(&k).unwrap();
        mac.update(message);
        Some(mac.finalize().into_bytes().to_vec())
    }

    pub(super) fn slots(&self, generation: u32) -> Vec<u32> {
        self.keys.keys().filter(|(g, _)| *g == generation).map(|(_, s)| *s).collect()
    }

    pub(super) fn keep_only(&mut self, generation: u32) {
        self.keys.retain(|(g, _), _| *g == generation);
    }

    /// Enabling: the core seals with the keys, then they go into the Keystore.
    pub(super) fn enable(
        &mut self,
        s: &GhulSuite,
        proof: super::suite::FactorProof,
        transport: Option<Arc<dyn HidTransport>>,
        pin: &str,
    ) -> Result<u32, GhulError> {
        let keys = pin_keys();
        let listener = transport.as_ref().map(|_| listener());
        let generation = s.quick_unlock_enable_pin(proof, transport, listener, pin.into(), 3600, keys.clone())?;
        self.keys.clear();
        self.import(generation, &keys);
        Ok(generation)
    }

    /// One attempt as the app makes it: keys of other generations go, the next generation's keys
    /// are imported first, the challenge, one use of the first slot left, the core's unlock.
    pub(super) fn attempt(&mut self, s: &GhulSuite, pin: &str) -> Result<u32, GhulError> {
        let generation = s.quick_pin_generation()?.ok_or(GhulError::QuickUnlockUnavailable)?;
        self.keep_only(generation);
        let Some(&slot) = self.slots(generation).first() else {
            // No key left: two wrong PINs. The app turns the PIN off.
            s.quick_unlock_disable("pin".into()).unwrap();
            return Err(GhulError::QuickUnlockUnavailable);
        };
        let next = generation.wrapping_add(1).max(1);
        let keys = pin_keys();
        self.import(next, &keys);
        let challenge = match s.quick_pin_challenge(pin.into()) {
            Ok(c) => c,
            Err(e) => {
                self.keep_only(generation);
                return Err(e);
            }
        };
        let tag = self.sign(generation, slot, &challenge.message).unwrap();
        match s.unlock_quick_pin(slot, tag, challenge.message, keys) {
            Ok(now) => {
                self.keep_only(now);
                Ok(now)
            }
            Err(e) => {
                self.keep_only(generation);
                Err(e)
            }
        }
    }
}

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
    MockAuthenticator::new(MockConfig {
        protocols: vec![2, 1],
        fido_2_1: true,
        pin: Some("1234".into()),
        cred_protect: true,
    })
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
        placement: None,
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

/// Every file below `dir`, in all subfolders.
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

fn titles(s: &GhulSuite) -> Vec<String> {
    let mut t: Vec<String> = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.title).collect();
    t.sort();
    t
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
                "Master Pw!-Kq7wZ",
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
    assert!(matches!(
        s2.unlock("Master Pw!-Kq7wZ".into(), None, None, None, None),
        Err(GhulError::SecurityKeyRequired)
    ));
    assert!(matches!(
        s2.unlock("wrong".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None),
        Err(GhulError::WrongPassword)
    ));
    s2.unlock("Master Pw!-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
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
        s.create(options("pw-123456-Kq7wZ", None, &[(VaultKind::Pass, "Privat")]), None, None, None),
        Err(GhulError::InvalidInput { .. })
    ));
    let setup = s
        .create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None)
        .unwrap();
    let p = s.protection().unwrap();
    assert_eq!((p.security_keys, p.keyfiles, p.recovery_codes), (0, 1, 1));
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat.clone(), login("A", "b", "c")).unwrap();
    assert!(matches!(
        s.add_keyfile(kf_proof(PW, &kf), None, None, kf.clone(), "again".into()),
        Err(GhulError::AlreadyExists)
    ));
    s.lock();

    assert!(matches!(s.unlock("pw-123456-Kq7wZ".into(), None, None, None, None), Err(GhulError::KeyfileRequired)));
    assert!(matches!(
        s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(generate_keyfile())),
        Err(GhulError::WrongKeyfile)
    ));
    assert!(matches!(s.unlock("falsch".into(), None, None, None, Some(kf.clone())), Err(GhulError::WrongPassword)));
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    let file_before = d.storage().read(vault_file(&s, &privat)).unwrap().unwrap();

    // Add a security key: key file, recovery code and older copies keep working.
    let k = key();
    s.add_security_key(kf_proof(PW, &kf), None, k.clone(), listener(), "1234".into(), "Key".into()).unwrap();
    assert_eq!(s.protection().unwrap().security_keys, 1);
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    s.lock();
    s.unlock_with_recovery_code(setup.recovery_code.clone().unwrap(), "pw-654321-Kq7wZ".into()).unwrap();
    let copy = Dirs::new("copy");
    let f = copy.suite();
    f.create_from_vault(
        file_before,
        "Kopie".into(),
        "pw-123456-Kq7wZ".into(),
        None,
        None,
        None,
        Some(kf.clone()),
        "C".into(),
    )
    .unwrap();
    assert_eq!(f.list_entries(VaultKind::Pass, None).unwrap().len(), 1);

    // The key file is the key's backup: it stays until a second key takes its place.
    let kf_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::Keyfile).unwrap();
    assert!(matches!(
        s.remove_slot(kf_proof("pw-654321-Kq7wZ", &kf), None, None, kf_slot.slot_id.clone()),
        Err(GhulError::BackupFactorRequired)
    ));
    let k2 = key();
    s.add_security_key(kf_proof("pw-654321-Kq7wZ", &kf), None, k2.clone(), listener(), "1234".into(), "Key 2".into())
        .unwrap();
    s.remove_slot(kf_proof("pw-654321-Kq7wZ", &kf), None, None, kf_slot.slot_id).unwrap();
    // Two keys: neither can go alone (the other one would be without a backup).
    let key_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::SecurityKey).unwrap();
    assert!(matches!(
        s.remove_slot(key_proof("pw-654321-Kq7wZ"), Some(k.clone()), Some(listener()), key_slot.slot_id),
        Err(GhulError::BackupFactorRequired)
    ));
    // The backup key opens just like the first one.
    s.lock();
    s.unlock("pw-654321-Kq7wZ".into(), Some(k2), Some(listener()), Some("1234".into()), None).unwrap();
    let code_slot = s.slots().unwrap().into_iter().find(|x| x.kind == SlotKind::RecoveryCode).unwrap();
    assert!(matches!(
        s.remove_slot(key_proof("pw-654321-Kq7wZ"), Some(k.clone()), Some(listener()), code_slot.slot_id),
        Err(GhulError::InvalidInput { .. })
    ));
    s.lock();
    assert!(matches!(
        s.unlock("pw-654321-Kq7wZ".into(), None, None, None, Some(kf.clone())),
        Err(GhulError::WrongKeyfile)
    ));
    s.unlock("pw-654321-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);
    s.lock();
    s.unlock_with_recovery_code(setup.recovery_code.unwrap(), "anders-123-Kq7wZ".into()).unwrap();
}

fn vault_file(s: &GhulSuite, id: &str) -> String {
    s.list_vaults().unwrap().into_iter().find(|v| v.id == id).unwrap().file
}

#[test]
fn vault_management_and_suite_from_vault() {
    let d = Dirs::new("manage");
    let kf = generate_keyfile();
    let s = d.suite();
    let setup = s
        .create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None)
        .unwrap();
    let work = s.create_vault(VaultKind::Pass, "Arbeit & Co".into()).unwrap();
    let file = s.list_vaults().unwrap().into_iter().find(|v| v.id == work).unwrap().file;
    // The file name says nothing about the vault (F23).
    assert_eq!(file, format!("{}.ghlpss", &work[..16]));
    s.save_entry(work.clone(), login("Jira", "w", "x")).unwrap();
    s.rename_vault(work.clone(), "Firma".into()).unwrap();
    let exported = s.export_vault(work.clone()).unwrap();

    // The name lives inside the file.
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.list_vaults().unwrap().iter().any(|v| v.name == "Firma"));

    // Delete (file removed) and add it again from the export.
    s.remove_vault(work.clone()).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 1);
    assert!(d.storage().read(file.clone()).unwrap().is_none());
    let again = s.add_vault_file(exported.clone(), "".into()).unwrap();
    assert_eq!(again, work);
    assert!(matches!(s.add_vault_file(exported.clone(), "x".into()), Err(GhulError::AlreadyExists)));
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
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
            "pw-123456-Kq7wZ".into(),
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
        .create_from_vault(
            exported,
            "Firma".into(),
            "pw-123456-Kq7wZ".into(),
            None,
            None,
            None,
            Some(kf.clone()),
            "C".into(),
        )
        .unwrap();
    // r9 M-D1: always a new code (the file may come from someone else, who knows the old one).
    let new_code = r.recovery_code.expect("a new code");
    let vaults = f.list_vaults().unwrap();
    assert_eq!((vaults.len(), vaults[0].kind, vaults[0].entries), (1, VaultKind::Pass, 1));
    f.lock();
    // The recovery code of the original suite no longer opens the new one; the new code does.
    assert!(f.unlock_with_recovery_code(setup.recovery_code.unwrap(), "neu-neu-neu-Kq7wZ".into()).is_err());
    f.unlock_with_recovery_code(new_code, "neu-neu-neu-Kq7wZ".into()).unwrap();
    assert_eq!(f.list_entries(VaultKind::Pass, None).unwrap()[0].title, "Jira");
}

#[test]
fn quick_unlock_fingerprint_and_pin() {
    let d = Dirs::new("quick");
    let k = key();
    let s = d.suite();
    s.create(
        options("pw-1-Kq7wZ", None, &[(VaultKind::Pass, "P")]),
        Some(k.clone()),
        Some(listener()),
        Some("1234".into()),
    )
    .unwrap();
    s.save_entry(vault_id(&s, "P"), login("Example", "u", "p")).unwrap();

    // H6: both factors, and the validity is chosen here (stored in the file). Locked and
    // unlocked first: the password alone right after creating is the backup's (see
    // `a_new_suite_with_a_key_takes_its_backup_with_the_password`).
    s.lock();
    s.unlock("pw-1-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    assert!(matches!(
        s.quick_unlock_enable(no_key_proof("pw-1-Kq7wZ"), None, None, "bio".into(), 3600),
        Err(GhulError::SecurityKeyRequired)
    ));
    let bio =
        s.quick_unlock_enable(key_proof("pw-1-Kq7wZ"), Some(k.clone()), Some(listener()), "bio".into(), 3600).unwrap();
    let mut ks = PinKeystore::default();
    ks.enable(&s, key_proof("pw-1-Kq7wZ"), Some(k.clone()), "24681357").unwrap();
    assert!(s.quick_unlock_exists("bio".into()) && s.quick_unlock_exists("pin".into()));
    s.lock();

    s.unlock_quick("bio".into(), bio.clone()).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);
    // A new vault can be created after a quick unlock (needs the password key).
    s.create_vault(VaultKind::Totp, "2FA".into()).unwrap();
    s.lock();
    assert!(matches!(s.unlock_quick("bio".into(), vec![0; 32]), Err(GhulError::QuickUnlockInvalid)));
    assert!(matches!(ks.attempt(&s, "00000000"), Err(GhulError::QuickUnlockInvalid)));
    ks.attempt(&s, "24681357").unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 2);

    // A full unlock restarts the validity period (one hour, chosen when it is enabled), on a
    // test clock: no waiting, nothing that depends on how fast the runner is.
    use super::suite::NOW_SHIFT_MS;
    let hour = 3_600_000;
    let bio =
        s.quick_unlock_enable(key_proof("pw-1-Kq7wZ"), Some(k.clone()), Some(listener()), "bio".into(), 3600).unwrap();
    s.lock();
    NOW_SHIFT_MS.with(|c| c.set(hour * 3 / 4));
    s.unlock("pw-1-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    // 1.5 hours after enabling, 45 minutes after the full unlock: still valid.
    NOW_SHIFT_MS.with(|c| c.set(hour * 3 / 2));
    s.unlock_quick("bio".into(), bio.clone()).unwrap();
    s.lock();
    // Over an hour after the last full unlock: expired.
    NOW_SHIFT_MS.with(|c| c.set(hour * 2));
    assert!(matches!(s.unlock_quick("bio".into(), bio.clone()), Err(GhulError::QuickUnlockExpired)));
    // M30: expired is not gone: after the next full unlock it works again.
    assert!(s.quick_unlock_exists("bio".into()));
    s.unlock("pw-1-Kq7wZ".into(), Some(k.clone()), Some(listener()), Some("1234".into()), None).unwrap();
    s.lock();
    s.unlock_quick("bio".into(), bio).unwrap();
    s.lock();

    // A password change invalidates all quick unlocks.
    ks.attempt(&s, "24681357").unwrap();
    s.change_password(key_proof("pw-1-Kq7wZ"), Some(k.clone()), Some(listener()), "pw-2-Kq7wZ".into()).unwrap();
    assert!(!s.quick_unlock_exists("pin".into()));
    s.lock();
    assert!(matches!(ks.attempt(&s, "24681357"), Err(GhulError::QuickUnlockUnavailable)));
    NOW_SHIFT_MS.with(|c| c.set(0));
}

/// F92: locking never waits for a call that still holds the open suite (a slow import or
/// update check). The app locks on its main thread; waiting there froze it (ANR in the emulator).
#[test]
fn lock_does_not_wait_for_a_running_call() {
    let d = Dirs::new("lockwait");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let privat = vault_id(&s, "Privat");
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let busy = {
        let s = s.clone();
        std::thread::spawn(move || {
            s.with_open(|_| {
                started_tx.send(()).unwrap();
                // A slow call: it holds the open suite until released.
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
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
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
            options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
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
        "pw-123456-Kq7wZ".into(),
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
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 3);
    assert!(s.list_vaults().unwrap().iter().any(|v| v.name == "Arbeit"));

    // A copy of a vault under another name is not listed twice.
    st.write("privat (1).ghlpss".into(), st.read(privat_file.clone()).unwrap().unwrap()).unwrap();
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
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
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().iter().filter(|v| !v.available).count(), 1);
    st.write(totp_file.clone(), good_totp).unwrap();

    // Without the settings file the folder still opens; the file is recreated.
    s.lock();
    st.delete("suite.ghlsuite".into()).unwrap();
    assert!(s.exists().unwrap());
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(st.read("suite.ghlsuite".into()).unwrap().is_some());
    assert_eq!(s.list_vaults().unwrap().len(), 3);
    s.lock();

    // The recovery code opens a folder that holds just one vault file.
    let single = Dirs::new("folder-single");
    single.storage().write(privat_file.clone(), st.read(privat_file.clone()).unwrap().unwrap()).unwrap();
    let one = single.suite();
    one.unlock_with_recovery_code(code.clone(), "ganz-neu-123".into()).unwrap();
    assert_eq!(one.list_entries(VaultKind::Pass, None).unwrap()[0].title, "A");

    // A damaged settings file is reported: its seal (the last bytes) or its content.
    let good = st.read("suite.ghlsuite".into()).unwrap().unwrap();
    let mut suite = good.clone();
    let n = suite.len();
    suite[n - 1] ^= 1;
    st.write("suite.ghlsuite".into(), suite).unwrap();
    assert!(matches!(s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())), Err(GhulError::Tampered)));
    let mut suite = good;
    suite[n - 40] ^= 1;
    st.write("suite.ghlsuite".into(), suite).unwrap();
    assert!(matches!(s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)), Err(GhulError::Corrupted { .. })));
}

#[test]
fn foreign_vaults() {
    let d = Dirs::new("foreign");
    let kf = generate_keyfile();
    let s = d.suite();
    let setup = s
        .create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None)
        .unwrap();
    let code = setup.recovery_code.unwrap();
    let privat = vault_id(&s, "Privat");
    s.save_entry(privat.clone(), login("A", "b", "c")).unwrap();
    let st = d.storage();
    let privat_file = vault_file(&s, &privat);
    let old_privat = st.read(privat_file.clone()).unwrap().unwrap();

    // A vault of another suite (other password and key file) lying in the folder is left alone.
    let other = Dirs::new("foreign-other");
    let okf = generate_keyfile();
    let o = other.suite();
    o.create(options("anderes-pw-1", Some(okf.clone()), &[(VaultKind::Totp, "Verein")]), None, None, None).unwrap();
    let verein = vault_id(&o, "Verein");
    o.add_totp(verein.clone(), totp_input()).unwrap();
    let verein_data = o.export_vault(verein).unwrap();
    st.write("verein.ghltotp".into(), verein_data.clone()).unwrap();
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 1);
    assert!(s.totp_codes(59).unwrap().is_empty());
    st.delete("verein.ghltotp".into()).unwrap();
    // Added on purpose, it needs its own password and key file once.
    assert!(matches!(s.add_vault_file(verein_data.clone(), "".into()), Err(GhulError::DifferentProtection)));
    assert!(matches!(
        s.adopt_vault(verein_data.clone(), "".into(), "falsch".into(), None, None, None, Some(okf.clone())),
        Err(GhulError::WrongPassword)
    ));
    // Taking it over switches it to this suite's protection and gives it a neutral name.
    s.adopt_vault(verein_data, "".into(), "anderes-pw-1".into(), None, None, None, Some(okf.clone())).unwrap();
    let verein_file = s.list_vaults().unwrap().into_iter().find(|v| v.name == "Verein").unwrap().file;
    assert!(verein_file.ends_with(".ghltotp") && !verein_file.contains("verein"), "{verein_file}");
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.totp_codes(59).unwrap().len(), 1);
    // ... and the suite's recovery code now opens it as well.
    let single = Dirs::new("foreign-single");
    single.storage().write("verein.ghltotp".into(), st.read(verein_file.clone()).unwrap().unwrap()).unwrap();
    single.suite().unlock_with_recovery_code(code.clone(), "neu-neu-123-Kq7wZ".into()).unwrap();

    // Password change: an old copy (old salt) needs the old password once and is brought up to date.
    s.change_password(kf_proof("pw-123456-Kq7wZ", &kf), None, None, "pw-neu-7890".into()).unwrap();
    s.remove_vault(privat.clone()).unwrap();
    s.adopt_vault(old_privat, "".into(), "pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    s.lock();
    s.unlock("pw-neu-7890".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.list_entries(VaultKind::Pass, None).unwrap().len(), 1);

    // A key added later reaches every file, also the taken-over one.
    let k = key();
    s.add_security_key(kf_proof("pw-neu-7890", &kf), None, k.clone(), listener(), "1234".into(), "Key".into()).unwrap();
    s.lock();
    s.unlock("pw-neu-7890".into(), Some(k), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(s.list_vaults().unwrap().iter().filter(|v| v.available).count(), 2);
}

#[test]
fn import_and_export() {
    let d = Dirs::new("import");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(
        options("pw-123456-Kq7wZ", Some(kf), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
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
    assert!(matches!(s.export_plain(pass.clone(), "falsch".into(), false), Err(GhulError::WrongPassword)));
    let csv = s.export_plain(pass.clone(), "pw-123456-Kq7wZ".into(), false).unwrap();
    assert!(String::from_utf8(csv.clone()).unwrap().contains("GitHub"));
    let links = s.export_plain(totp.clone(), "pw-123456-Kq7wZ".into(), false).unwrap();
    assert!(String::from_utf8(links.clone()).unwrap().starts_with("otpauth://totp/GitHub:ghul?"));
    let other = s.create_vault(VaultKind::Pass, "Kopie".into()).unwrap();
    let r = s.import_data(csv, "export.csv".into(), Some(other.clone()), None, false, None).unwrap();
    assert_eq!(r.entries, 3);
}

#[test]
fn pass_extras_history_attachments_move_and_security_check() {
    let d = Dirs::new("pass-extras");
    let s = d.suite();
    s.create(
        options(
            "pw-123456-Kq7wZ",
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
        s.export_entries(privat.clone(), vec![id.clone()], "falsch".into(), false),
        Err(GhulError::WrongPassword)
    ));
    let one =
        String::from_utf8(s.export_entries(privat.clone(), vec![id.clone()], "pw-123456-Kq7wZ".into(), false).unwrap())
            .unwrap();
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
    s.create(options("pw-123456-Kq7wZ", Some(generate_keyfile()), &[(VaultKind::Totp, "Codes")]), None, None, None)
        .unwrap();
    let v = vault_id(&s, "Codes");
    let mut t = totp_input();
    t.issuer = "Mein Dienst".into();
    s.add_totp(v.clone(), t).unwrap();
    let c = s.totp_codes(59).unwrap()[0].clone();
    // M13: the secret only with the master password.
    assert!(matches!(s.totp_uri(v.clone(), c.id.clone(), "falsch-falsch-1".into()), Err(GhulError::WrongPassword)));
    assert!(matches!(s.totp_uri(v.clone(), c.id.clone(), String::new()), Err(GhulError::WrongPassword)));
    let uri = s.totp_uri(v, c.id, "pw-123456-Kq7wZ".into()).unwrap();
    assert!(
        uri.starts_with("otpauth://totp/Mein%20Dienst:a%40example.org?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
        "{uri}"
    );
    let back = crate::app::tools::parse_otpauth_uri(uri).unwrap();
    assert_eq!((back.issuer.as_str(), back.digits), ("Mein Dienst", 8));
}

#[test]
fn calendar_vault_events_series_reminders_and_handing_over() {
    use super::cal::{CalCalendar, CalDeleteMode, CalEvent, CalLink, CalRepeat};
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("cal-a"), Dirs::new("cal-b"));
    let a = da.suite();
    a.create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
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
            links: vec![CalLink { kind: "pass".into(), label: "Überweisung.pdf".into(), target: "x".into() }],
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

    // The suite goes to a second device and is changed there, then comes back.
    a.cal_save_event(CalEvent {
        calendar_id: privat,
        title: "Abendessen".into(),
        start: "2026-11-12T19:00".into(),
        end: "2026-11-12T21:00".into(),
        ..Default::default()
    })
    .unwrap();
    a.lock();
    super::tests::everyday::copy_folder(&da, &db);
    let b = db.suite();
    b.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(b.cal_calendars().unwrap().len(), 2);
    let mut on_b = b.cal_get_event(dentist.clone()).unwrap();
    on_b.location = "Praxis Süd".into();
    b.cal_save_event(on_b).unwrap();
    b.lock();
    super::tests::everyday::copy_folder(&db, &da);
    a.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    b.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
    for s in [&a, &b] {
        let day = s.cal_occurrences("2026-11-12".into(), "2026-11-13".into(), "".into(), "de".into()).unwrap();
        assert!(day.iter().any(|o| o.title == "Abendessen"));
        assert!(day.iter().any(|o| o.location == "Praxis Süd"));
    }
}

#[test]
fn recovery_code_counts_only_when_a_group_was_typed_back() {
    let d = Dirs::new("code-confirm");
    let s = d.suite();
    let kf = generate_keyfile();
    let code = s
        .create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None)
        .unwrap()
        .recovery_code
        .unwrap();
    // Closed before confirming (e.g. the app crashed in the last setup step).
    assert!(!s.recovery_confirmed().unwrap());
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(!s.recovery_confirmed().unwrap(), "still asked for after unlocking");
    let group = |c: &str, i: usize| c.split('-').nth(i).unwrap().to_owned();
    assert!(!s.confirm_recovery_code(2, group(&code, 3)).unwrap());
    assert!(!s.recovery_confirmed().unwrap());
    assert!(s.confirm_recovery_code(2, format!(" {} ", group(&code, 2).to_lowercase())).unwrap());
    assert!(s.recovery_confirmed().unwrap());
    assert!(s.confirm_recovery_code(99, "x".into()).is_err());
    s.lock();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.recovery_confirmed().unwrap());
    // A new code needs its own confirmation.
    let new = s.new_recovery_code(kf_proof(PW, &kf), None, None, "Neu".into()).unwrap();
    assert!(!s.recovery_confirmed().unwrap());
    // r9 krypto M2: the whole code typed back: a slip in any group (here the last) is found,
    // the old code does not count, the new one (lower case, without dashes) does.
    let mut slip: Vec<char> = new.chars().collect();
    let last = slip.len() - 1;
    slip[last] = if slip[last] == '2' { '3' } else { '2' };
    assert!(!s.confirm_whole_recovery_code(slip.into_iter().collect()).unwrap());
    assert!(!s.confirm_whole_recovery_code(code.clone()).unwrap());
    assert!(!s.recovery_confirmed().unwrap());
    assert!(s.confirm_whole_recovery_code(new.replace('-', " ").to_lowercase()).unwrap());
    assert!(s.recovery_confirmed().unwrap());
    let new = s.new_recovery_code(kf_proof(PW, &kf), None, None, "Neu".into()).unwrap();
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
        options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
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
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(s.setup_state().unwrap(), super::setup::SetupState::Complete);
    let vaults = s.list_vaults().unwrap();
    assert_eq!(vaults.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), vec!["Privat"]);
    // The code was never confirmed: the app asks for a new one; a missing area can be added.
    assert!(!s.recovery_confirmed().unwrap());
    s.add_area(VaultKind::Totp, String::new()).unwrap();
    s.lock();
    let s = d.suite();
    s.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.list_vaults().unwrap().len(), 2);
}

#[test]
fn area_options_live_in_the_area_file_and_areas_can_be_removed() {
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("areas-a"), Dirs::new("areas-b"));
    let a = da.suite();
    a.create(options("pw-123456-Kq7wZ", Some(kf.clone()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    // No file, no options: GhulAuth was not chosen.
    assert!(a.area_options(VaultKind::Totp).unwrap().is_empty());
    let set = |s: &GhulSuite, k: &str, v: &str| {
        s.set_area_options(VaultKind::Cal, [(k.to_owned(), v.to_owned())].into_iter().collect()).unwrap()
    };
    assert!(matches!(
        a.set_area_options(VaultKind::Totp, [("sort".into(), "name".into())].into_iter().collect()),
        Err(GhulError::NotConfigured)
    ));
    set(&a, "holiday_region", "DE-NW");
    set(&a, "week_start", "0");
    set(&a, "week_start", "6");
    // The options travel with the area's file.
    a.lock();
    super::tests::everyday::copy_folder(&da, &db);
    let b = db.suite();
    b.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf.clone())).unwrap();
    let o = b.area_options(VaultKind::Cal).unwrap();
    assert_eq!((o["holiday_region"].as_str(), o["week_start"].as_str()), ("DE-NW", "6"));
    // Removing an area deletes its file.
    b.add_area(VaultKind::Totp, String::new()).unwrap();
    b.remove_area(VaultKind::Cal).unwrap();
    assert!(b.area_options(VaultKind::Cal).unwrap().is_empty());
    let kinds = |s: &GhulSuite| s.list_vaults().unwrap().into_iter().map(|v| v.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&b), vec![VaultKind::Totp]);
    b.lock();
    b.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(kinds(&b), vec![VaultKind::Totp]);
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
        options("pw-123456-Kq7wZ", Some(generate_keyfile()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
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
mod archive;
mod attacks;
mod collections;
mod crazy_user;
mod docs;
mod everyday;
mod factors;
mod format_freeze;
mod hardening;
mod properties;
mod quick_pin;
mod r9;
mod rekey;
mod single;

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

/// Two cards "Visa" with different numbers (or two notes without a title) are two entries; the
/// same login with other spelling of its title is one.
#[test]
fn import_duplicates_are_only_the_same_entry() {
    use super::suite::dedup_key;
    use crate::vault::Field;
    let card = |n: &str| {
        let mut f = std::collections::BTreeMap::new();
        f.insert("title".to_owned(), Field::Text("Visa".into()));
        f.insert("number".to_owned(), Field::Secret(n.into()));
        f
    };
    assert_ne!(dedup_key("card", &card("4111")), dedup_key("card", &card("5500")));
    let login = |t: &str, pw: &str| {
        let mut f = std::collections::BTreeMap::new();
        f.insert("title".to_owned(), Field::Text(t.into()));
        f.insert("password".to_owned(), Field::Secret(pw.into()));
        f
    };
    assert_eq!(dedup_key("login", &login("GitHub", "pw")), dedup_key("login", &login(" github ", "pw")));
    assert_ne!(
        dedup_key("login", &login("GitHub", "pw")),
        dedup_key("login", &login("GitHub", "PW")),
        "passwords are exact"
    );
}

/// Who finds the app unlocked cannot change its protection: a new recovery code, a key file, a
/// security key, removing a factor and quick unlock all need the master password; with a wrong
/// one nothing changes.
#[test]
fn protection_changes_need_the_master_password() {
    let d = Dirs::new("needs-pw");
    let s = d.suite();
    let kf = generate_keyfile();
    let setup = s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    // Locked and unlocked again: no confirmation from creating is left.
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let slots = s.slots().unwrap();
    let wrong = || kf_proof("falsch-123", &kf);
    // The password alone is not enough: the key file is asked for as well.
    let no_factor = || super::suite::FactorProof { password: PW.into(), pin: None, keyfile: None };
    let wrong_file = || kf_proof(PW, &generate_keyfile());
    assert!(matches!(s.new_recovery_code(wrong(), None, None, "x".into()), Err(GhulError::WrongPassword)));
    assert!(matches!(s.new_recovery_code(no_factor(), None, None, "x".into()), Err(GhulError::KeyfileRequired)));
    assert!(matches!(s.new_recovery_code(wrong_file(), None, None, "x".into()), Err(GhulError::WrongKeyfile)));
    assert!(matches!(
        s.add_keyfile(wrong(), None, None, generate_keyfile(), "x".into()),
        Err(GhulError::WrongPassword)
    ));
    assert!(matches!(
        s.add_security_key(wrong(), None, key(), listener(), "1234".into(), "x".into()),
        Err(GhulError::WrongPassword)
    ));
    assert!(matches!(s.remove_slot(wrong(), None, None, slots[0].slot_id.clone()), Err(GhulError::WrongPassword)));
    assert!(matches!(
        s.quick_unlock_enable_pin(wrong(), None, None, "13572468".into(), 3600, pin_keys()),
        Err(GhulError::WrongPassword)
    ));
    assert!(matches!(
        s.quick_unlock_enable_pin(no_factor(), None, None, "13572468".into(), 3600, pin_keys()),
        Err(GhulError::KeyfileRequired)
    ));
    assert_eq!(s.slots().unwrap().len(), slots.len(), "nothing changed");
    assert!(!s.quick_unlock_exists("pin".into()));
    s.lock();
    // The old recovery code still works: no new one was made.
    s.unlock_with_recovery_code(setup.recovery_code.unwrap(), "pw-654321-Kq7wZ".into()).unwrap();
}

#[test]
fn the_same_key_twice_is_no_backup() {
    let d = Dirs::new("same-key");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let k = key();
    s.add_security_key(kf_proof(PW, &kf), None, k.clone(), listener(), "1234".into(), "A".into()).unwrap();
    let n = s.slots().unwrap().len();
    // The key refuses a second credential for this suite (excludeList): nothing is added.
    let r = s.add_security_key(kf_proof(PW, &kf), None, k.clone(), listener(), "1234".into(), "A2".into());
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "{r:?}");
    assert_eq!(s.slots().unwrap().len(), n);
    // Another key is a backup.
    s.add_security_key(kf_proof(PW, &kf), None, key(), listener(), "1234".into(), "B".into()).unwrap();
    assert_eq!(s.slots().unwrap().len(), n + 1);
}

#[test]
fn confirmed_factors_count_once_and_not_past_locking() {
    let d = Dirs::new("confirm");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let pw_only = || super::suite::FactorProof { password: PW.into(), pin: None, keyfile: None };
    // Created with a key file (no backup follows): nothing is confirmed.
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K2".into()).is_err());
    // Confirmed: the next change needs the password alone, the one after both factors again.
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K2".into()).unwrap();
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K3".into()).is_err());
    // A confirmation is not carried on with the password alone.
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    assert!(s.confirm_factors(pw_only(), None, None).is_err());
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K3".into()).is_err());
    // A wrong password does not use a confirmation up.
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    let wrong = super::suite::FactorProof { password: "falsch-123".into(), pin: None, keyfile: None };
    assert!(matches!(s.add_keyfile(wrong, None, None, generate_keyfile(), "x".into()), Err(GhulError::WrongPassword)));
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K3".into()).is_err(), "taken by the try");
    // Locking ends a confirmation.
    s.confirm_factors(kf_proof(PW, &kf), None, None).unwrap();
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K4".into()).is_err());
    // Wrong factors confirm nothing.
    assert!(s.confirm_factors(kf_proof(PW, &generate_keyfile()), None, None).is_err());
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K5".into()).is_err());
}

#[test]
fn a_new_suite_with_a_key_takes_its_backup_with_the_password() {
    let d = Dirs::new("backup-after-create");
    let s = d.suite();
    let k = key();
    s.create(options(PW, None, &[(VaultKind::Pass, "Privat")]), Some(k.clone()), Some(listener()), Some("1234".into()))
        .unwrap();
    let pw_only = || super::suite::FactorProof { password: PW.into(), pin: None, keyfile: None };
    // Right after creating: the backup key with the password alone (the first key is not touched again).
    s.add_security_key(pw_only(), None, key(), listener(), "1234".into(), "Backup".into()).unwrap();
    assert_eq!(s.slots().unwrap().iter().filter(|x| x.kind == super::suite::SlotKind::SecurityKey).count(), 2);
    // Once only.
    assert!(s.add_keyfile(pw_only(), None, None, generate_keyfile(), "K".into()).is_err());
}
