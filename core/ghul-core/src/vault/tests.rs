// SPDX-License-Identifier: AGPL-3.0-or-later
//! Container tests: lifecycle, security key and key file slots, recovery, real CTAP2 flow
//! against the software authenticator, mutation fuzzing and the published test vectors.

use std::sync::Arc;

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use serde_json::{json, Value as Json};

use super::container::{Fido2Registration, KdfCost, LockedContainer, SecondFactor, SuiteKeys, UnlockedContainer};
use super::error::VaultError;
use super::format::{ContainerKind, Slot};
use super::keyfile::Keyfile;
use super::keys::{
    fido2_slot_key, hkdf, kek, keyfile_slot_key, payload_key, recovery_slot_key, PasswordKey, INFO_X25519_FIDO2,
    INFO_X25519_KEYFILE, INFO_X25519_WRAP,
};
use super::payload::{Entry, Field, SlotMeta};
use super::recovery::RecoveryCode;
use crate::kdf::hex;

const COST: KdfCost = KdfCost { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 };
const RP: &str = "ghulsuite.invalid";
const NOW: i64 = 1_762_800_000_000;

fn rng(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

fn login(id: u8, title: &str, password: &str) -> Entry {
    let mut e = Entry::new([id; 16], "login", NOW);
    e.fields.insert("title".into(), Field::Text(title.into()));
    e.fields.insert("username".into(), Field::Text("ghul".into()));
    e.fields.insert("password".into(), Field::Secret(password.into()));
    e.fields.insert("urls".into(), Field::List(vec!["https://example.org".into()]));
    e
}

struct Setup {
    keys: SuiteKeys,
    fido_slot: Slot,
    hmac_output: [u8; 32],
}

fn setup(r: &mut ChaCha20Rng, password: &str) -> Setup {
    let keys = SuiteKeys::create(password, COST, r).unwrap();
    let prf_input = SuiteKeys::new_prf_input(r);
    let mut hmac_output = [0u8; 32];
    r.fill_bytes(&mut hmac_output);
    let reg = Fido2Registration { rp_id: RP, credential_id: &[0xC1; 48], prf_input, hmac_output: &hmac_output };
    let fido_slot = keys.fido2_slot(&reg, r);
    Setup { keys, fido_slot, hmac_output }
}

fn fido_factor(s: &Setup) -> SecondFactor<'_> {
    SecondFactor::Fido2 { slot_id: *s.fido_slot.id(), hmac_output: &s.hmac_output }
}

fn open(data: &[u8], password: &str, factor: &SecondFactor<'_>) -> Result<UnlockedContainer, VaultError> {
    let locked = LockedContainer::parse(data)?;
    let s = locked.factor_secret(factor)?;
    let pk = PasswordKey::derive(password, &locked.header().kdf)?;
    locked.unlock(&pk, &s)
}

fn recover(data: &[u8], code: &RecoveryCode) -> Result<UnlockedContainer, VaultError> {
    LockedContainer::parse(data)?.recover(code)
}

#[test]
fn suite_lifecycle_with_security_key() {
    let mut r = rng(1);
    let s = setup(&mut r, "Grüße, GhulSuite!");
    let mut suite = s.keys.create_container(ContainerKind::Suite, vec![s.fido_slot.clone()], &mut r).unwrap();
    let (_, code) = suite.add_recovery_slot("Notfallblatt", NOW, &mut r).unwrap();
    let mut pass = s.keys.create_container(ContainerKind::Pass, vec![s.fido_slot.clone()], &mut r).unwrap();
    let mut totp = s.keys.create_container(ContainerKind::Totp, vec![s.fido_slot.clone()], &mut r).unwrap();
    pass.payload.entries.push(login(1, "Example", "hunter2"));
    let mut t = Entry::new([2; 16], "totp", NOW);
    t.fields.insert("secret".into(), Field::Bytes(b"12345678901234567890".to_vec()));
    totp.payload.entries.push(t);
    let pass_vmk = *pass.vmk();

    let suite_file = suite.seal(&mut r).unwrap();
    let pass_file = pass.seal(&mut r).unwrap();
    let totp_file = totp.seal(&mut r).unwrap();
    assert_eq!(&pass_file[..8], b"GHULPASS");
    assert_eq!(pass_file.len(), 14 + pass.header().encode().unwrap().len() + 24 + 4096 + 16);

    // One key touch and one Argon2 run open all containers.
    let lp = LockedContainer::parse(&pass_file).unwrap();
    let lt = LockedContainer::parse(&totp_file).unwrap();
    assert_eq!(lp.fido2_requests(), lt.fido2_requests());
    assert!(!lp.has_recovery() && lp.keyfile_slots() == 0);
    let sp = lp.factor_secret(&fido_factor(&s)).unwrap();
    let pk = PasswordKey::derive("Grüße, GhulSuite!", &lp.header().kdf).unwrap();
    let up = lp.unlock(&pk, &sp).unwrap();
    let ut = lt.unlock(&pk, &sp).unwrap();
    assert_eq!(up.payload.entries, pass.payload.entries);
    assert_eq!(ut.payload.entries, totp.payload.entries);
    assert_ne!(up.vmk(), ut.vmk());

    // NFC: decomposed umlaut gives the same key.
    assert!(open(&pass_file, "Gru\u{308}ße, GhulSuite!", &fido_factor(&s)).is_ok());

    // The recovery code alone opens the suite and restores the factor secret.
    let rs = recover(&suite_file, &code).unwrap();
    assert_eq!(rs.factor_secret().expose(), s.keys.factor_secret.expose());
    let code_text = code.to_display().to_lowercase();
    assert!(recover(&suite_file, &RecoveryCode::parse(&code_text).unwrap()).is_ok());
    // Vault containers have no recovery slot; they open with their key from the keyring.
    assert_eq!(recover(&pass_file, &code).err(), Some(VaultError::WrongSecondFactor));
    let via_keyring = LockedContainer::parse(&pass_file).unwrap().unlock_with_vmk(&pass_vmk).unwrap();
    assert_eq!(via_keyring.payload.entries, pass.payload.entries);
    assert_eq!(via_keyring.factor_secret().expose(), s.keys.factor_secret.expose());
}

#[test]
fn keyfile_slot_is_a_second_factor() {
    let mut r = rng(2);
    let kf = Keyfile::generate(&mut r);
    let keys = SuiteKeys::create("pw", COST, &mut r).unwrap();
    let slot = keys.keyfile_slot(&kf, &mut r);
    let mut c = keys.create_container(ContainerKind::Suite, vec![slot], &mut r).unwrap();
    let (_, code) = c.add_recovery_slot("code", NOW, &mut r).unwrap();
    c.payload.settings.insert("theme".into(), ciborium::value::Value::Text("dark".into()));
    let file = c.seal(&mut r).unwrap();

    let locked = LockedContainer::parse(&file).unwrap();
    assert!(locked.fido2_requests().is_empty() && locked.keyfile_slots() == 1);
    let reparsed = Keyfile::parse(&kf.encode()).unwrap();
    let u = open(&file, "pw", &SecondFactor::Keyfile(&reparsed)).unwrap();
    assert_eq!(u.payload.settings, c.payload.settings);
    assert_eq!(open(&file, "wrong", &SecondFactor::Keyfile(&kf)).err(), Some(VaultError::WrongPassword));
    let other = Keyfile::generate(&mut r);
    assert_eq!(open(&file, "pw", &SecondFactor::Keyfile(&other)).err(), Some(VaultError::WrongKeyfile));
    // The recovery code does not need the key file.
    assert_eq!(recover(&file, &code).unwrap().payload.settings, c.payload.settings);
    // A container without key file slot reports that.
    let s = setup(&mut r, "pw");
    let mut k = s.keys.create_container(ContainerKind::Pass, vec![s.fido_slot.clone()], &mut r).unwrap();
    let kfile = k.seal(&mut r).unwrap();
    assert_eq!(open(&kfile, "pw", &SecondFactor::Keyfile(&kf)).err(), Some(VaultError::NoKeyfileSlot));
    // Without any second factor slot the header is invalid.
    assert!(keys.create_container(ContainerKind::Pass, vec![], &mut r).is_err());
}

#[test]
fn wrong_factors_are_rejected() {
    let mut r = rng(3);
    let s = setup(&mut r, "password-1");
    let mut c = s.keys.create_container(ContainerKind::Suite, vec![s.fido_slot.clone()], &mut r).unwrap();
    let (_, code) = c.add_recovery_slot("code", NOW, &mut r).unwrap();
    let file = c.seal(&mut r).unwrap();

    assert_eq!(open(&file, "password-2", &fido_factor(&s)).err(), Some(VaultError::WrongPassword));
    let wrong = [0u8; 32];
    let f = SecondFactor::Fido2 { slot_id: *s.fido_slot.id(), hmac_output: &wrong };
    assert_eq!(open(&file, "password-1", &f).err(), Some(VaultError::WrongSecondFactor));
    let f = SecondFactor::Fido2 { slot_id: [9; 16], hmac_output: &s.hmac_output };
    assert_eq!(open(&file, "password-1", &f).err(), Some(VaultError::SlotNotFound));
    let other_code = RecoveryCode::generate(&mut r);
    assert_eq!(recover(&file, &other_code).err(), Some(VaultError::WrongSecondFactor));
    assert!(recover(&file, &code).is_ok());
}

#[test]
fn password_change_and_swapping_factors_keep_everything_valid() {
    let mut r = rng(5);
    let kf = Keyfile::generate(&mut r);
    let keys = SuiteKeys::create("old password", COST, &mut r).unwrap();
    let kf_slot = keys.keyfile_slot(&kf, &mut r);
    let mut c = keys.create_container(ContainerKind::Suite, vec![kf_slot.clone()], &mut r).unwrap();
    let (_, code) = c.add_recovery_slot("code", NOW, &mut r).unwrap();
    c.payload.entries.push(login(1, "A", "secret"));
    let first = c.seal(&mut r).unwrap();

    // New password with new salt; container key and recovery code stay the same.
    let mut u = open(&first, "old password", &SecondFactor::Keyfile(&kf)).unwrap();
    let vmk = *u.vmk();
    let mut new_kdf = u.header().kdf.clone();
    r.fill_bytes(&mut new_kdf.salt);
    let new_pk = PasswordKey::derive("new password", &new_kdf).unwrap();
    u.rekey(&new_pk, new_kdf.clone(), &mut r).unwrap();
    let second = u.seal(&mut r).unwrap();
    assert_eq!(open(&second, "old password", &SecondFactor::Keyfile(&kf)).err(), Some(VaultError::WrongPassword));
    assert!(open(&second, "new password", &SecondFactor::Keyfile(&kf)).is_ok());
    assert!(recover(&second, &code).is_ok());

    // Add a security key later: nothing is re-encrypted, the key file and the code keep working.
    let mut out = [0u8; 32];
    r.fill_bytes(&mut out);
    let reg = Fido2Registration { rp_id: RP, credential_id: &[7; 32], prf_input: [8; 32], hmac_output: &out };
    let key_slot = u.add_fido2_slot(&reg, "Key", NOW, &mut r).unwrap();
    assert_eq!(u.vmk(), &vmk);
    let third = u.seal(&mut r).unwrap();
    let f = SecondFactor::Fido2 { slot_id: *key_slot.id(), hmac_output: &out };
    assert!(open(&third, "new password", &f).is_ok());
    assert!(open(&third, "new password", &SecondFactor::Keyfile(&kf)).is_ok());
    assert!(recover(&third, &code).is_ok());
    assert!(LockedContainer::parse(&third).unwrap().unlock_with_vmk(&vmk).is_ok());

    // Then retire the key file: only that slot goes away.
    let mut u3 = open(&third, "new password", &f).unwrap();
    u3.remove_slot(kf_slot.id()).unwrap();
    let fourth = u3.seal(&mut r).unwrap();
    assert_eq!(open(&fourth, "new password", &SecondFactor::Keyfile(&kf)).err(), Some(VaultError::NoKeyfileSlot));
    let u4 = open(&fourth, "new password", &f).unwrap();
    assert_eq!(u4.payload.entries[0].fields["password"], Field::Secret("secret".into()));
    assert!(recover(&fourth, &code).is_ok());

    // The last second factor cannot be removed.
    assert_eq!(u3.remove_slot(key_slot.id()).err(), Some(VaultError::LastSlot));
    let mut out2 = [0u8; 32];
    r.fill_bytes(&mut out2);
    let reg2 = Fido2Registration { rp_id: RP, credential_id: &[9; 32], prf_input: [1; 32], hmac_output: &out2 };
    let second_key = u3.add_fido2_slot(&reg2, "Ersatzkey", NOW, &mut r).unwrap();
    u3.remove_slot(key_slot.id()).unwrap();
    let fifth = u3.seal(&mut r).unwrap();
    let f2 = SecondFactor::Fido2 { slot_id: *second_key.id(), hmac_output: &out2 };
    assert!(open(&fifth, "new password", &f2).is_ok());
    assert_eq!(open(&fifth, "new password", &f).err(), Some(VaultError::SlotNotFound));
}

#[test]
fn rejects_tampered_kind_and_versions() {
    let mut r = rng(6);
    let s = setup(&mut r, "pw");
    let mut c = s.keys.create_container(ContainerKind::Pass, vec![s.fido_slot.clone()], &mut r).unwrap();
    let file = c.seal(&mut r).unwrap();

    let mut as_totp = file.clone();
    as_totp[..8].copy_from_slice(b"GHULTOTP");
    assert_eq!(open(&as_totp, "pw", &fido_factor(&s)).err(), Some(VaultError::Corrupted));
    let mut v3 = file.clone();
    v3[8] = 3;
    assert_eq!(LockedContainer::parse(&v3).err(), Some(VaultError::UnsupportedVersion(3)));
    let mut v0 = file.clone();
    v0[8] = 0;
    assert_eq!(LockedContainer::parse(&v0).err(), Some(VaultError::UnsupportedVersion(0)));
    // Public-key slots (types 4 and 5) do not exist in version 1.
    let mut as_v1 = file.clone();
    as_v1[8] = 1;
    assert!(LockedContainer::parse(&as_v1).is_err());
    assert!(c.seal_version(1, &mut r).is_err());
    assert!(LockedContainer::parse(b"GHULPASS").is_err());
    assert!(LockedContainer::parse(b"").is_err());
}

/// Every modification of a sealed file must be rejected, and nothing may panic.
#[test]
fn mutation_fuzzing() {
    let mut r = rng(7);
    let s = setup(&mut r, "pw");
    let mut c = s.keys.create_container(ContainerKind::Suite, vec![s.fido_slot.clone()], &mut r).unwrap();
    let (_, code) = c.add_recovery_slot("code", NOW, &mut r).unwrap();
    c.payload.entries.push(login(1, "A", "B"));
    let file = c.seal(&mut r).unwrap();
    let original = LockedContainer::parse(&file).unwrap();
    let pk = PasswordKey::derive("pw", &original.header().kdf).unwrap();
    let sec = original.factor_secret(&fido_factor(&s)).unwrap();
    let vmk = *c.vmk();

    let mut m = rng(70);
    for i in 0..4000 {
        let mut data = file.clone();
        match i % 5 {
            0 | 1 => {
                let pos = (m.next_u32() as usize) % data.len();
                data[pos] ^= 1 << (m.next_u32() % 8);
            }
            2 => data.truncate((m.next_u32() as usize) % data.len()),
            3 => {
                let pos = (m.next_u32() as usize) % (data.len() + 1);
                data.insert(pos, m.next_u32() as u8);
            }
            _ => {
                let end = 14 + 600.min(data.len() - 14);
                let pos = 14 + (m.next_u32() as usize) % (end - 14);
                data[pos] = m.next_u32() as u8;
            }
        }
        if data == file {
            continue;
        }
        let Ok(locked) = LockedContainer::parse(&data) else { continue };
        let _ = locked.fido2_requests();
        assert!(LockedContainer::parse(&data).unwrap().recover(&code).is_err(), "mutation {i} accepted via code");
        assert!(LockedContainer::parse(&data).unwrap().unlock_with_vmk(&vmk).is_err(), "mutation {i} accepted via vmk");
        if locked.header().kdf != original.header().kdf {
            continue;
        }
        let via_slot = locked.factor_secret(&fido_factor(&s));
        assert!(locked.unlock(&pk, &sec).is_err(), "mutation {i} was accepted");
        if let Ok(sec2) = via_slot {
            assert!(
                LockedContainer::parse(&data).unwrap().unlock(&pk, &sec2).is_err(),
                "mutation {i} accepted via slot"
            );
        }
    }

    for len in 0..600 {
        let mut data = b"GHULPASS\x01\x00".to_vec();
        data.resize(10 + len, 0);
        m.fill_bytes(&mut data[10..]);
        let _ = LockedContainer::parse(&data);
    }
}

/// Real flow: register with the software authenticator via CTAP2, then unlock with it.
#[test]
fn end_to_end_with_ctap2_authenticator() {
    use crate::ctap2::{prf_salt, Authenticator};
    use crate::ctaphid::CtapHid;
    use crate::mock::{MockAuthenticator, MockConfig};

    let key = MockAuthenticator::new(MockConfig { protocols: vec![2, 1], fido_2_1: true, pin: Some("1234".into()) });
    let mut r = rng(8);
    let keys = SuiteKeys::create("pw", COST, &mut r).unwrap();

    let auth = Authenticator::new(CtapHid::open(key.as_ref(), None).unwrap()).unwrap();
    let cred = auth.make_credential(RP, "GhulSuite", "1234").unwrap();
    let prf_input = SuiteKeys::new_prf_input(&mut r);
    let out = auth.hmac_secret(RP, &cred.id, &prf_salt(&prf_input), "1234").unwrap();
    let first: [u8; 32] = out.output.as_slice().try_into().unwrap();
    let slot = keys
        .fido2_slot(&Fido2Registration { rp_id: RP, credential_id: &cred.id, prf_input, hmac_output: &first }, &mut r);
    let mut c = keys.create_container(ContainerKind::Pass, vec![slot], &mut r).unwrap();
    c.payload.entries.push(login(1, "Mock", "pw"));
    let file = c.seal(&mut r).unwrap();

    let locked = LockedContainer::parse(&file).unwrap();
    let req = &locked.fido2_requests()[0];
    let key2 = Arc::clone(&key);
    let auth = Authenticator::new(CtapHid::open(key2.as_ref(), None).unwrap()).unwrap();
    let out = auth.hmac_secret(&req.rp_id, &req.credential_id, &req.hmac_salt, "1234").unwrap();
    assert!(out.user_verified);
    let output: [u8; 32] = out.output.as_slice().try_into().unwrap();
    let sec = locked.factor_secret(&SecondFactor::Fido2 { slot_id: req.slot_id, hmac_output: &output }).unwrap();
    let pk = PasswordKey::derive("pw", &locked.header().kdf).unwrap();
    let u = locked.unlock(&pk, &sec).unwrap();
    assert_eq!(u.payload.entries[0].fields["title"], Field::Text("Mock".into()));
}

// --- Test vectors (docs/testvectors/container-v1.json and -v2.json) ----------------------

const VECTOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/testvectors");

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// The intermediate values of a public-key slot (type 4 or 5) for the vectors.
fn public_slot_json(ikm: &[u8; 32], info: &[u8], slot: &Slot) -> Json {
    let (slot_id, public) = match slot {
        Slot::Fido2(f) => (f.slot_id, f.public.as_ref().unwrap()),
        Slot::Keyfile(k) => (k.slot_id, k.public.as_ref().unwrap()),
        Slot::Recovery(_) => unreachable!(),
    };
    let seed = hkdf(ikm, &[info, &slot_id]);
    let private = x25519_dalek::StaticSecret::from(*seed);
    let shared = private.diffie_hellman(&x25519_dalek::PublicKey::from(public.ephemeral));
    let mut wrap_ikm = shared.as_bytes().to_vec();
    wrap_ikm.extend_from_slice(&public.ephemeral);
    wrap_ikm.extend_from_slice(&public.public_key);
    let wrap_key = hkdf(&wrap_ikm, &[INFO_X25519_WRAP, &slot_id]);
    json!({
        "private_key": hex(seed.as_ref()),
        "public_key": hex(&public.public_key),
        "ephemeral_public_key": hex(&public.ephemeral),
        "shared_secret": hex(shared.as_bytes()),
        "slot_key": hex(wrap_key.as_ref()),
    })
}

/// `version` 1: slots of type 1 and 3 (files of older app versions, still read); 2: types 4 and 5.
#[allow(clippy::type_complexity)]
fn build_vectors(version: u16) -> Json {
    let mut r = rng(2026);
    let password = "Grüße, GhulSuite!";
    let keyfile = Keyfile::generate(&mut r);
    let mut out = Vec::new();

    // 1: suite container, security key + recovery code
    // 2: GhulPass vault of the same suite (security key, same recovery code as the suite)
    // 3: GhulAuth container of another suite: password + key file slot, with recovery code
    let keys = SuiteKeys::create(password, COST, &mut r).unwrap();
    let prf_input = SuiteKeys::new_prf_input(&mut r);
    let mut hmac_output = [0u8; 32];
    r.fill_bytes(&mut hmac_output);
    let credential_id: Vec<u8> = (0..48).map(|i| i as u8).collect();
    let reg = Fido2Registration { rp_id: RP, credential_id: &credential_id, prf_input, hmac_output: &hmac_output };
    let fido = if version == 1 {
        super::container::legacy_fido2_slot(&keys.suite_id, &keys.factor_secret, &reg, &mut r)
    } else {
        keys.fido2_slot(&reg, &mut r)
    };
    let keys_kf = SuiteKeys::create(password, COST, &mut r).unwrap();
    let kf_slot = if version == 1 {
        super::container::legacy_keyfile_slot(&keys_kf.suite_id, &keys_kf.factor_secret, &keyfile, &mut r)
    } else {
        keys_kf.keyfile_slot(&keyfile, &mut r)
    };

    let cases: Vec<(&SuiteKeys, ContainerKind, Slot, bool, Option<&Keyfile>, &str)> = vec![
        (&keys, ContainerKind::Suite, fido.clone(), true, None, "suite container: security key slot + recovery slot"),
        (
            &keys,
            ContainerKind::Pass,
            fido.clone(),
            true,
            None,
            "GhulPass vault of the same suite: security key slot + the suite's recovery code",
        ),
        (
            &keys_kf,
            ContainerKind::Totp,
            kf_slot,
            true,
            Some(&keyfile),
            "GhulAuth container of another suite: key file slot + recovery slot",
        ),
    ];
    let suite_code = RecoveryCode::generate(&mut r);
    for (k, kind, slot, with_recovery, kf, description) in cases {
        let mut c = k.create_container(kind, vec![slot.clone()], &mut r).unwrap();
        c.payload
            .slots
            .insert(hex(slot.id()), SlotMeta { label: "Factor".into(), created: NOW, extra: Default::default() });
        let recovery = match (with_recovery, kf) {
            (false, _) => None,
            // the suite and its vault share one code; the other suite has its own
            (true, None) => Some((
                c.set_recovery_code(&suite_code, "Notfallblatt", NOW, &mut r).unwrap(),
                RecoveryCode::from_bytes(*suite_code.expose()),
            )),
            (true, Some(_)) => Some(c.add_recovery_slot("Notfallblatt", NOW, &mut r).unwrap()),
        };
        match kind {
            ContainerKind::Totp => {
                let mut e = Entry::new([2; 16], "totp", NOW);
                e.fields.insert("issuer".into(), Field::Text("Example".into()));
                e.fields.insert("secret".into(), Field::Bytes(b"12345678901234567890".to_vec()));
                e.fields.insert("digits".into(), Field::Int(6));
                e.fields.insert("period".into(), Field::Int(30));
                e.fields.insert("algorithm".into(), Field::Text("SHA1".into()));
                c.payload.entries.push(e);
            }
            ContainerKind::Pass => c.payload.entries.push(login(1, "Example", "correct horse battery staple")),
            ContainerKind::Suite => {
                c.payload.settings.insert("note".into(), ciborium::value::Value::Text("keyring lives here".into()));
            }
            ContainerKind::Drive | ContainerKind::Mail => {
                c.payload.settings.insert("note".into(), ciborium::value::Value::Text("area settings".into()));
            }
            ContainerKind::Cal => {
                let mut e = Entry::new([3; 16], "event", NOW);
                e.fields.insert("title".into(), Field::Text("Dentist".into()));
                e.fields.insert("start".into(), Field::Text("2026-11-13T10:00".into()));
                c.payload.entries.push(e);
            }
        }
        let file = c.seal_version(version, &mut r).unwrap();
        let cid = c.header().container_id;
        let kek_v = kek(&k.password_key, &k.factor_secret, &cid);
        let vmk = *c.vmk();
        let pkey = payload_key(&zeroize::Zeroizing::new(vmk), &cid);
        let factor = match &slot {
            Slot::Fido2(f) if version == 1 => json!({
                "type": "fido2", "slot_id": hex(&f.slot_id), "rp_id": f.rp_id, "credential_id": hex(&f.credential_id),
                "prf_input": hex(&f.prf_input), "hmac_salt": hex(&crate::ctap2::prf_salt(&f.prf_input)),
                "hmac_output": hex(&hmac_output), "slot_key": hex(fido2_slot_key(&hmac_output, &f.slot_id).as_ref()),
            }),
            Slot::Fido2(f) => json!({
                "type": "fido2-x25519", "slot_id": hex(&f.slot_id), "rp_id": f.rp_id,
                "credential_id": hex(&f.credential_id), "prf_input": hex(&f.prf_input),
                "hmac_salt": hex(&crate::ctap2::prf_salt(&f.prf_input)), "hmac_output": hex(&hmac_output),
                "x25519": public_slot_json(&hmac_output, INFO_X25519_FIDO2, &slot),
            }),
            Slot::Keyfile(f) if version == 1 => json!({
                "type": "keyfile", "slot_id": hex(&f.slot_id), "keyfile_id": hex(&f.keyfile_id),
                "slot_key": hex(keyfile_slot_key(kf.unwrap().expose(), &f.slot_id).as_ref()),
            }),
            Slot::Keyfile(f) => json!({
                "type": "keyfile-x25519", "slot_id": hex(&f.slot_id), "keyfile_id": hex(&f.keyfile_id),
                "x25519": public_slot_json(kf.unwrap().expose(), INFO_X25519_KEYFILE, &slot),
            }),
            Slot::Recovery(_) => unreachable!(),
        };
        out.push(json!({
            "description": description,
            "kind": kind.name(),
            "password": password,
            "kdf": { "algorithm": "argon2id-v19", "memory_kib": k.kdf.memory_kib, "iterations": k.kdf.iterations,
                     "parallelism": k.kdf.parallelism, "salt": hex(&k.kdf.salt) },
            "keyfile": kf.map(|x| hex(&x.encode())),
            "suite_id": hex(&k.suite_id),
            "password_key": hex(k.password_key.expose()),
            "factor_secret": hex(k.factor_secret.expose()),
            "factor": factor,
            "recovery": recovery.as_ref().map(|(rs, code)| json!({
                "slot_id": hex(rs.id()), "code": code.to_display().as_str(), "code_secret": hex(code.expose()),
                "slot_key": hex(recovery_slot_key(code.expose(), rs.id(), &cid).as_ref()),
            })),
            "container_id": hex(&cid),
            "kek": hex(kek_v.as_ref()),
            "vmk": hex(&vmk),
            "payload_key": hex(pkey.as_ref()),
            "payload_cbor": hex(&c.payload.to_cbor().unwrap()),
            "file": hex(&file),
        }));
    }
    json!({
        "format": format!("GhulSuite container v{version}"),
        "note": "Generated by ghul-core (vault::tests::test_vectors). Argon2id: 64 MiB, 3 iterations, 1 lane.",
        "vectors": out,
    })
}

#[test]
fn test_vectors() {
    for version in [1u16, 2] {
        check_vectors(version);
    }
}

fn check_vectors(version: u16) {
    let path = format!("{VECTOR_DIR}/container-v{version}.json");
    let generated = build_vectors(version);
    if std::env::var("GHUL_WRITE_VECTORS").is_ok() {
        std::fs::create_dir_all(VECTOR_DIR).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&generated).unwrap() + "\n").unwrap();
    }
    let published: Json = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} missing; run with GHUL_WRITE_VECTORS=1")),
    )
    .unwrap();
    assert_eq!(generated, published, "implementation no longer matches the published test vectors v{version}");

    // Independent check: open each published file from its inputs only.
    for v in published["vectors"].as_array().unwrap() {
        let file = unhex(v["file"].as_str().unwrap());
        let keyfile = v["keyfile"].as_str().map(|k| Keyfile::parse(&unhex(k)).unwrap());
        let pw = v["password"].as_str().unwrap();
        let u = if v["factor"]["type"].as_str().unwrap().starts_with("fido2") {
            let out: [u8; 32] = unhex(v["factor"]["hmac_output"].as_str().unwrap()).try_into().unwrap();
            let slot_id: [u8; 16] = unhex(v["factor"]["slot_id"].as_str().unwrap()).try_into().unwrap();
            open(&file, pw, &SecondFactor::Fido2 { slot_id, hmac_output: &out }).unwrap()
        } else {
            open(&file, pw, &SecondFactor::Keyfile(keyfile.as_ref().unwrap())).unwrap()
        };
        assert_eq!(hex(&u.payload.to_cbor().unwrap()), v["payload_cbor"].as_str().unwrap());
        if let Some(code) = v["recovery"]["code"].as_str() {
            let code = RecoveryCode::parse(code).unwrap();
            assert_eq!(
                hex(&recover(&file, &code).unwrap().payload.to_cbor().unwrap()),
                v["payload_cbor"].as_str().unwrap()
            );
        }
        let vmk: [u8; 32] = unhex(v["vmk"].as_str().unwrap()).try_into().unwrap();
        assert!(LockedContainer::parse(&file).unwrap().unlock_with_vmk(&vmk).is_ok());
    }
}
