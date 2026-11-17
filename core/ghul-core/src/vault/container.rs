// SPDX-License-Identifier: AGPL-3.0-or-later
//! Creating, opening, modifying and writing containers.
//!
//! Typical flow:
//! 1. [`LockedContainer::parse`] the file.
//! 2. Ask the security key for the hmac-secret of a FIDO2 slot ([`LockedContainer::fido2_requests`]),
//!    or use a key file.
//! 3. [`LockedContainer::factor_secret`] → `S`, [`PasswordKey::derive`] → `PK`.
//! 4. [`LockedContainer::unlock`] → [`UnlockedContainer`]; change the payload; [`UnlockedContainer::seal`].
//!
//! Alternative: [`LockedContainer::recover`] with the recovery code (opens the container key
//! directly). It only reads: the write tag of such a container is not checked and it cannot be
//! sealed. Whoever has the password uses [`LockedContainer::recover_with_password`] (checked,
//! can be sealed).

use rand_core::CryptoRngCore;
use zeroize::Zeroizing;

use super::error::{Result, VaultError};
use super::format::{
    encode_prefix, split_file, ContainerKind, Fido2Slot, Header, KdfParams, KeyfileSlot, PublicWrap, RecoverySlot,
    Slot, FORMAT_VERSION, MAX_SLOTS, NONCE_LEN,
};
use super::keyfile::Keyfile;
use super::keys::{auth_key, file_tag, tag_matches};
use super::keys::{
    factor_aad, kek, open, payload_key, public_key, random32, recovery_aad, recovery_slot_key, seal, slot_private_key,
    unwrap32, unwrap_with_private, vmk_aad, wrap32, wrap_to_public, FactorSecret, Key32, PasswordKey,
    INFO_X25519_FIDO2, INFO_X25519_KEYFILE,
};
use super::payload::{pad, unpad, Payload, SlotMeta};
use super::recovery::RecoveryCode;
use crate::ctap2::prf_salt;

/// What the app needs to ask a security key for one FIDO2 slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2Request {
    pub slot_id: [u8; 16],
    pub rp_id: String,
    pub credential_id: Vec<u8>,
    /// Salt for the `hmac-secret` extension (already PRF-transformed).
    pub hmac_salt: [u8; 32],
    /// Raw PRF input, for platforms that use the WebAuthn PRF extension (e.g. Windows).
    pub prf_input: [u8; 32],
}

/// A freshly registered security key credential and its first hmac-secret output.
pub struct Fido2Registration<'a> {
    pub rp_id: &'a str,
    pub credential_id: &'a [u8],
    pub prf_input: [u8; 32],
    pub hmac_output: &'a [u8; 32],
}

/// The second factor presented when unlocking with the password.
pub enum SecondFactor<'a> {
    /// Output of `hmac-secret` (with PIN verification) for the given slot.
    Fido2 { slot_id: [u8; 16], hmac_output: &'a [u8; 32] },
    /// A key file matching one of the key file slots.
    Keyfile(&'a Keyfile),
}

/// Suite-wide secrets for creating new containers and slots.
pub struct SuiteKeys {
    pub suite_id: [u8; 16],
    pub kdf: KdfParams,
    pub password_key: PasswordKey,
    pub factor_secret: FactorSecret,
}

/// Argon2id cost (salt is generated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfCost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl SuiteKeys {
    /// Starts a new suite: random suite ID, salt and factor secret; derives the password key.
    pub fn create(password: &str, cost: KdfCost, rng: &mut impl CryptoRngCore) -> Result<SuiteKeys> {
        let mut suite_id = [0u8; 16];
        rng.fill_bytes(&mut suite_id);
        let mut salt = [0u8; 32];
        rng.fill_bytes(&mut salt);
        let kdf =
            KdfParams { memory_kib: cost.memory_kib, iterations: cost.iterations, parallelism: cost.parallelism, salt };
        let password_key = PasswordKey::derive(password, &kdf)?;
        Ok(SuiteKeys::from_parts(suite_id, kdf, password_key, FactorSecret::generate(rng)))
    }

    /// Reuses existing key material (e.g. to build a suite around an existing vault).
    pub fn from_parts(
        suite_id: [u8; 16],
        kdf: KdfParams,
        password_key: PasswordKey,
        factor_secret: FactorSecret,
    ) -> SuiteKeys {
        SuiteKeys { suite_id, kdf, password_key, factor_secret }
    }

    /// Generates a fresh PRF input for registering a security key.
    pub fn new_prf_input(rng: &mut impl CryptoRngCore) -> [u8; 32] {
        let mut p = [0u8; 32];
        rng.fill_bytes(&mut p);
        p
    }

    /// Creates a FIDO2 slot from the hmac-secret output of a freshly registered credential.
    pub fn fido2_slot(&self, reg: &Fido2Registration<'_>, rng: &mut impl CryptoRngCore) -> Slot {
        new_fido2_slot(&self.suite_id, &self.factor_secret, reg, rng)
    }

    /// Creates a key file slot (the key file as second factor instead of a security key).
    pub fn keyfile_slot(&self, keyfile: &Keyfile, rng: &mut impl CryptoRngCore) -> Slot {
        new_keyfile_slot(&self.suite_id, &self.factor_secret, keyfile, rng)
    }

    /// Creates an empty container of `kind` with the given factor slots.
    pub fn create_container(
        &self,
        kind: ContainerKind,
        slots: Vec<Slot>,
        rng: &mut impl CryptoRngCore,
    ) -> Result<UnlockedContainer> {
        let mut container_id = [0u8; 16];
        rng.fill_bytes(&mut container_id);
        let vmk = random32(rng);
        let k = kek(&self.password_key, &self.factor_secret, &container_id);
        let vmk_wrapped = wrap32(&k, &vmk, &vmk_aad(&container_id), rng);
        let header = Header { container_id, suite_id: self.suite_id, kdf: self.kdf.clone(), slots, vmk: vmk_wrapped };
        header.validate()?;
        Ok(UnlockedContainer {
            kind,
            header,
            vmk,
            factor_secret: FactorSecret::from_bytes(*self.factor_secret.expose()),
            payload: Payload::new(kind),
            password_key: Some(PasswordKey::from_bytes(*self.password_key.expose())),
        })
    }
}

fn random_slot_id(rng: &mut impl CryptoRngCore) -> [u8; 16] {
    let mut slot_id = [0u8; 16];
    rng.fill_bytes(&mut slot_id);
    slot_id
}

/// Wraps `s` to a public key; the key comes from a key pair derived by us, so it never has small order.
fn public_wrap(
    public_key: [u8; 32],
    suite_id: &[u8; 16],
    slot_id: &[u8; 16],
    s: &FactorSecret,
    rng: &mut impl CryptoRngCore,
) -> Result<(PublicWrap, super::format::Wrapped)> {
    let (ephemeral, wrapped) = wrap_to_public(&public_key, slot_id, s.expose(), &factor_aad(suite_id, slot_id), rng)
        .ok_or_else(|| VaultError::format("invalid public key in slot"))?;
    Ok((PublicWrap { public_key, ephemeral }, wrapped))
}

/// A security key slot (type 4) for `slot_id`.
fn fido2_slot_with_id(
    slot_id: [u8; 16],
    suite_id: &[u8; 16],
    s: &FactorSecret,
    reg: &Fido2Registration<'_>,
    rng: &mut impl CryptoRngCore,
) -> Slot {
    let public = public_key(&slot_private_key(reg.hmac_output, INFO_X25519_FIDO2, &slot_id));
    let (public, wrapped) = public_wrap(public, suite_id, &slot_id, s, rng).expect("own public key");
    Slot::Fido2(Fido2Slot {
        slot_id,
        rp_id: reg.rp_id.to_owned(),
        credential_id: reg.credential_id.to_vec(),
        prf_input: reg.prf_input,
        uv_required: true,
        public,
        wrapped,
    })
}

/// A key file slot (type 5) for `slot_id`.
fn keyfile_slot_with_id(
    slot_id: [u8; 16],
    suite_id: &[u8; 16],
    s: &FactorSecret,
    keyfile: &Keyfile,
    rng: &mut impl CryptoRngCore,
) -> Slot {
    let public = public_key(&slot_private_key(keyfile.expose(), INFO_X25519_KEYFILE, &slot_id));
    let (public, wrapped) = public_wrap(public, suite_id, &slot_id, s, rng).expect("own public key");
    Slot::Keyfile(KeyfileSlot { slot_id, keyfile_id: keyfile.id(), public, wrapped })
}

fn new_fido2_slot(
    suite_id: &[u8; 16],
    s: &FactorSecret,
    reg: &Fido2Registration<'_>,
    rng: &mut impl CryptoRngCore,
) -> Slot {
    let slot_id = random_slot_id(rng);
    fido2_slot_with_id(slot_id, suite_id, s, reg, rng)
}

fn new_keyfile_slot(suite_id: &[u8; 16], s: &FactorSecret, keyfile: &Keyfile, rng: &mut impl CryptoRngCore) -> Slot {
    let slot_id = random_slot_id(rng);
    keyfile_slot_with_id(slot_id, suite_id, s, keyfile, rng)
}

/// Opens the factor secret of one security key slot with the key's hmac-secret output.
fn fido2_factor(suite_id: &[u8; 16], f: &Fido2Slot, hmac_output: &[u8; 32]) -> Option<Key32> {
    let aad = factor_aad(suite_id, &f.slot_id);
    let private = slot_private_key(hmac_output, INFO_X25519_FIDO2, &f.slot_id);
    unwrap_with_private(&private, &f.public.public_key, &f.public.ephemeral, &f.slot_id, &f.wrapped, &aad)
}

/// Opens the factor secret of one key file slot with the key file.
fn keyfile_factor(suite_id: &[u8; 16], k: &KeyfileSlot, keyfile: &Keyfile) -> Option<Key32> {
    let aad = factor_aad(suite_id, &k.slot_id);
    let private = slot_private_key(keyfile.expose(), INFO_X25519_KEYFILE, &k.slot_id);
    unwrap_with_private(&private, &k.public.public_key, &k.public.ephemeral, &k.slot_id, &k.wrapped, &aad)
}

/// A parsed container whose header is readable but whose content is still encrypted.
pub struct LockedContainer {
    kind: ContainerKind,
    version: u16,
    header: Header,
    data: Vec<u8>,
}

impl LockedContainer {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let parts = split_file(data)?;
        let header = Header::parse(parts.header)?;
        Ok(LockedContainer { kind: parts.kind, version: parts.version, header, data: data.to_vec() })
    }

    /// The format version of the file.
    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn kind(&self) -> ContainerKind {
        self.kind
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Number of key file slots (key files that can be used instead of a security key).
    pub fn keyfile_slots(&self) -> usize {
        self.header.slots.iter().filter(|s| matches!(s, Slot::Keyfile(_))).count()
    }

    pub fn has_recovery(&self) -> bool {
        self.header.slots.iter().any(|s| matches!(s, Slot::Recovery(_)))
    }

    pub fn fido2_requests(&self) -> Vec<Fido2Request> {
        self.header
            .slots
            .iter()
            .filter_map(|s| match s {
                Slot::Fido2(f) => Some(Fido2Request {
                    slot_id: f.slot_id,
                    rp_id: f.rp_id.clone(),
                    credential_id: f.credential_id.clone(),
                    hmac_salt: prf_salt(&f.prf_input),
                    prf_input: f.prf_input,
                }),
                _ => None,
            })
            .collect()
    }

    /// Recovers the factor secret `S` from a second factor.
    pub fn factor_secret(&self, factor: &SecondFactor<'_>) -> Result<FactorSecret> {
        let suite_id = &self.header.suite_id;
        match factor {
            SecondFactor::Fido2 { slot_id, hmac_output } => {
                let slot = self
                    .header
                    .slots
                    .iter()
                    .find_map(|s| match s {
                        Slot::Fido2(f) if &f.slot_id == slot_id => Some(f),
                        _ => None,
                    })
                    .ok_or(VaultError::SlotNotFound)?;
                fido2_factor(suite_id, slot, hmac_output).map(FactorSecret).ok_or(VaultError::WrongSecondFactor)
            }
            SecondFactor::Keyfile(keyfile) => {
                let id = keyfile.id();
                let mut any = false;
                for s in &self.header.slots {
                    if let Slot::Keyfile(k) = s {
                        any = true;
                        if k.keyfile_id != id {
                            continue;
                        }
                        if let Some(sec) = keyfile_factor(suite_id, k, keyfile) {
                            return Ok(FactorSecret(sec));
                        }
                    }
                }
                Err(if any { VaultError::WrongKeyfile } else { VaultError::NoKeyfileSlot })
            }
        }
    }

    /// Decrypts the container. `password_key` must be derived with this container's KDF parameters.
    /// Checks the write tag before the content is read: `Tampered` when the password and `S` open
    /// the container key but the file was not written with them.
    pub fn unlock(self, password_key: &PasswordKey, factor_secret: &FactorSecret) -> Result<UnlockedContainer> {
        let cid = self.header.container_id;
        let k = kek(password_key, factor_secret, &cid);
        let vmk = unwrap32(&k, &self.header.vmk, &vmk_aad(&cid)).ok_or(VaultError::WrongPassword)?;
        self.check_tag(&auth_key(password_key, factor_secret, &cid), &vmk)?;
        let payload = self.decrypt(&vmk)?;
        let pk = Some(PasswordKey::from_bytes(*password_key.expose()));
        Ok(self.into_unlocked(vmk, FactorSecret::from_bytes(*factor_secret.expose()), payload, pk))
    }

    /// Opens the container with its container key (read only; tests check which keys still open
    /// a file).
    #[cfg(test)]
    pub fn unlock_with_vmk(self, vmk: &[u8; 32]) -> Result<UnlockedContainer> {
        let vmk = Zeroizing::new(*vmk);
        let payload = self.decrypt(&vmk)?;
        let s = payload_factor_secret(&payload)?;
        Ok(self.into_unlocked(vmk, s, payload, None))
    }

    /// Opens the container with the recovery code alone (no password, key or key file). Read
    /// only: nothing shows that the file was written by someone with the password.
    pub fn recover(self, code: &RecoveryCode) -> Result<UnlockedContainer> {
        let vmk = self.recovery_vmk(code)?;
        let payload = self.decrypt(&vmk)?;
        let s = payload_factor_secret(&payload)?;
        Ok(self.into_unlocked(vmk, s, payload, None))
    }

    /// Opens the container with the recovery code and checks it with the password: the file must
    /// carry the write tag of the password and the factor secret it holds (e.g. after another
    /// device removed a key and renewed `S`). `Tampered` otherwise.
    pub fn recover_with_password(self, code: &RecoveryCode, password_key: &PasswordKey) -> Result<UnlockedContainer> {
        let vmk = self.recovery_vmk(code)?;
        let payload = self.decrypt(&vmk)?;
        let s = payload_factor_secret(&payload)?;
        let cid = self.header.container_id;
        // The file's container key must also be the one its password slot holds for this `S`.
        let k = kek(password_key, &s, &cid);
        match unwrap32(&k, &self.header.vmk, &vmk_aad(&cid)) {
            Some(v) if *v == *vmk => {}
            _ => return Err(VaultError::WrongPassword),
        }
        self.check_tag(&auth_key(password_key, &s, &cid), &vmk)?;
        Ok(self.into_unlocked(vmk, s, payload, Some(PasswordKey::from_bytes(*password_key.expose()))))
    }

    fn recovery_vmk(&self, code: &RecoveryCode) -> Result<Key32> {
        let cid = self.header.container_id;
        self.header
            .slots
            .iter()
            .filter_map(|s| match s {
                Slot::Recovery(r) => Some(r),
                _ => None,
            })
            .find_map(|r| {
                let key = recovery_slot_key(code.expose(), &r.slot_id, &cid);
                unwrap32(&key, &r.wrapped, &recovery_aad(&cid, &r.slot_id))
            })
            .ok_or(VaultError::WrongSecondFactor)
    }

    /// Checks the write tag. A wrong one on content that still decrypts is `Tampered` (made
    /// without the password); content that does not decrypt either is just damaged (a cut-off
    /// or broken copy), `Corrupted`. The content is not parsed here.
    fn check_tag(&self, ak: &Key32, vmk: &Key32) -> Result<()> {
        let parts = split_file(&self.data)?;
        if tag_matches(ak, parts.signed, &parts.auth) {
            return Ok(());
        }
        let pkey = payload_key(vmk, &self.header.container_id);
        match open(&pkey, &parts.nonce, parts.ciphertext, parts.aad) {
            Some(_) => Err(VaultError::Tampered),
            None => Err(VaultError::Corrupted),
        }
    }

    fn decrypt(&self, vmk: &Key32) -> Result<Payload> {
        let cid = self.header.container_id;
        let parts = split_file(&self.data)?;
        let pkey = payload_key(vmk, &cid);
        let plaintext = open(&pkey, &parts.nonce, parts.ciphertext, parts.aad).ok_or(VaultError::Corrupted)?;
        Payload::from_cbor(unpad(&plaintext)?, self.kind)
    }

    fn into_unlocked(
        self,
        vmk: Key32,
        factor_secret: FactorSecret,
        payload: Payload,
        password_key: Option<PasswordKey>,
    ) -> UnlockedContainer {
        UnlockedContainer { kind: self.kind, header: self.header, vmk, factor_secret, payload, password_key }
    }
}

/// The factor secret a container keeps in its content.
fn payload_factor_secret(payload: &Payload) -> Result<FactorSecret> {
    let s = payload.factor_secret.as_deref().ok_or(VaultError::Corrupted)?;
    Ok(FactorSecret::from_bytes(s.try_into().map_err(|_| VaultError::Corrupted)?))
}

/// A decrypted container. Secrets are wiped from memory when it is dropped.
pub struct UnlockedContainer {
    kind: ContainerKind,
    header: Header,
    vmk: Key32,
    factor_secret: FactorSecret,
    pub payload: Payload,
    /// The password key, to make the write tag when sealing (see `keys::auth_key`); `None` when
    /// opened without the password (read only).
    password_key: Option<PasswordKey>,
}

impl UnlockedContainer {
    pub fn kind(&self) -> ContainerKind {
        self.kind
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn container_id(&self) -> [u8; 16] {
        self.header.container_id
    }

    /// The container key.
    pub fn vmk(&self) -> &[u8; 32] {
        &self.vmk
    }

    /// Encrypts and serializes the container with its write tag. Increments the payload revision.
    /// A container opened without the password (recovery code) cannot be sealed until it
    /// gets a password ([`Self::rekey`]).
    pub fn seal(&mut self, rng: &mut impl CryptoRngCore) -> Result<Vec<u8>> {
        let pk = self
            .password_key
            .as_ref()
            .ok_or_else(|| VaultError::InvalidInput("opened without the password: read only".into()))?;
        // Only a consistent protection is written: the password slot must hold this container
        // key for this password and factor secret (otherwise other devices could not open it).
        let cid = self.header.container_id;
        match unwrap32(&kek(pk, &self.factor_secret, &cid), &self.header.vmk, &vmk_aad(&cid)) {
            Some(v) if *v == *self.vmk => {}
            _ => return Err(VaultError::InvalidInput("inconsistent protection".into())),
        }
        let ak = auth_key(pk, &self.factor_secret, &cid);
        self.payload.revision =
            self.payload.revision.checked_add(1).ok_or_else(|| VaultError::Payload("revision overflow".into()))?;
        self.payload.factor_secret = Some(self.factor_secret.expose().to_vec());
        let header = self.header.encode()?;
        let cbor = self.payload.to_cbor()?;
        let plaintext = pad(&cbor)?;
        let mut out = Vec::with_capacity(14 + header.len() + NONCE_LEN + plaintext.len() + 16 + 32);
        encode_prefix(self.kind, FORMAT_VERSION, header.len(), &mut out);
        out.extend_from_slice(&header);
        let mut nonce = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce);
        let pkey = payload_key(&self.vmk, &self.header.container_id);
        let ct = seal(&pkey, &nonce, &plaintext, &out);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        let tag = file_tag(&ak, &out);
        out.extend_from_slice(&tag);
        Ok(out)
    }

    /// Tests: what someone without the password can make from a container opened with an old
    /// code (the container key is known, the write key is not): a file with a made-up write tag.
    #[cfg(test)]
    pub(crate) fn seal_without_password(&mut self, rng: &mut impl CryptoRngCore) -> Vec<u8> {
        self.payload.factor_secret = Some(self.factor_secret.expose().to_vec());
        let header = self.header.encode().expect("valid header");
        let cbor = self.payload.to_cbor().expect("valid payload");
        let plaintext = pad(&cbor).expect("fits");
        let mut out = Vec::new();
        encode_prefix(self.kind, FORMAT_VERSION, header.len(), &mut out);
        out.extend_from_slice(&header);
        let mut nonce = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce);
        let pkey = payload_key(&self.vmk, &self.header.container_id);
        let ct = seal(&pkey, &nonce, &plaintext, &out);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        let mut tag = [0u8; 32];
        rng.fill_bytes(&mut tag);
        out.extend_from_slice(&tag);
        out
    }

    pub fn slots(&self) -> &[Slot] {
        &self.header.slots
    }

    /// Adds a FIDO2 slot for a newly registered key and records its label.
    pub fn add_fido2_slot(
        &mut self,
        reg: &Fido2Registration<'_>,
        label: &str,
        now_ms: i64,
        rng: &mut impl CryptoRngCore,
    ) -> Result<Slot> {
        let slot = new_fido2_slot(&self.header.suite_id, &self.factor_secret, reg, rng);
        self.insert_slot(slot.clone(), label, now_ms)?;
        Ok(slot)
    }

    /// Adds a key file slot and records its label.
    pub fn add_keyfile_slot(
        &mut self,
        keyfile: &Keyfile,
        label: &str,
        now_ms: i64,
        rng: &mut impl CryptoRngCore,
    ) -> Result<Slot> {
        let slot = new_keyfile_slot(&self.header.suite_id, &self.factor_secret, keyfile, rng);
        self.insert_slot(slot.clone(), label, now_ms)?;
        Ok(slot)
    }

    /// Adds a recovery slot with a new code and returns the code to print.
    pub fn add_recovery_slot(
        &mut self,
        label: &str,
        now_ms: i64,
        rng: &mut impl CryptoRngCore,
    ) -> Result<(Slot, RecoveryCode)> {
        let code = RecoveryCode::generate(rng);
        let slot = self.set_recovery_code(&code, label, now_ms, rng)?;
        Ok((slot, code))
    }

    /// Replaces all recovery slots by one for `code` (code → container key) and keeps the code in
    /// the encrypted content, so that further containers of the suite can get the same slot.
    pub fn set_recovery_code(
        &mut self,
        code: &RecoveryCode,
        label: &str,
        now_ms: i64,
        rng: &mut impl CryptoRngCore,
    ) -> Result<Slot> {
        let old: Vec<[u8; 16]> =
            self.header.slots.iter().filter(|s| matches!(s, Slot::Recovery(_))).map(|s| *s.id()).collect();
        let mut slot_id = [0u8; 16];
        rng.fill_bytes(&mut slot_id);
        let cid = self.header.container_id;
        let key = recovery_slot_key(code.expose(), &slot_id, &cid);
        let wrapped = wrap32(&key, &self.vmk, &recovery_aad(&cid, &slot_id), rng);
        let slot = Slot::Recovery(RecoverySlot { slot_id, wrapped });
        self.insert_slot(slot.clone(), label, now_ms)?;
        for id in &old {
            self.remove_slot(id)?;
        }
        self.payload.recovery_secret = Some(code.expose().to_vec());
        Ok(slot)
    }

    /// The suite's recovery code kept in this container, if any.
    pub fn recovery_code(&self) -> Option<RecoveryCode> {
        let b: [u8; 32] = self.payload.recovery_secret.as_deref()?.try_into().ok()?;
        Some(RecoveryCode::from_bytes(b))
    }

    /// Switches the container to another suite's protection: its factor secret, factor slots
    /// and password. The container key and the content stay; old slots (also recovery slots)
    /// are dropped and must be set again with [`Self::set_recovery_code`].
    pub fn reprotect(
        &mut self,
        suite_id: [u8; 16],
        factor_secret: FactorSecret,
        factor_slots: Vec<Slot>,
        password_key: &PasswordKey,
        kdf: KdfParams,
        rng: &mut impl CryptoRngCore,
    ) -> Result<()> {
        if factor_slots.iter().any(|s| !s.provides_factor()) {
            return Err(VaultError::InvalidInput("only key and key file slots".into()));
        }
        let candidate = Header { suite_id, slots: factor_slots, ..self.header.clone() };
        candidate.validate()?;
        self.header = candidate;
        self.factor_secret = factor_secret;
        self.payload.recovery_secret = None;
        self.payload.slots.retain(|id, _| self.header.slots.iter().any(|s| &crate::kdf::hex(s.id()) == id));
        self.rekey(password_key, kdf, rng)
    }

    /// Inserts a slot created for the same suite (e.g. by another container of the suite), so that
    /// all containers share identical factor slots and one key touch opens all of them.
    pub fn insert_slot(&mut self, slot: Slot, label: &str, now_ms: i64) -> Result<()> {
        if self.header.slots.len() >= MAX_SLOTS {
            return Err(VaultError::InvalidInput("too many slots".into()));
        }
        if self.header.slots.iter().any(|s| s.id() == slot.id()) {
            return Err(VaultError::InvalidInput("slot already present".into()));
        }
        let mut slots = self.header.slots.clone();
        slots.push(slot.clone());
        let candidate = Header { slots, ..self.header.clone() };
        candidate.validate()?;
        self.header = candidate;
        self.payload.slots.insert(
            crate::kdf::hex(slot.id()),
            SlotMeta { label: label.to_owned(), created: now_ms, extra: Default::default() },
        );
        Ok(())
    }

    pub fn remove_slot(&mut self, slot_id: &[u8; 16]) -> Result<()> {
        let pos = self.header.slots.iter().position(|s| s.id() == slot_id).ok_or(VaultError::SlotNotFound)?;
        let mut slots = self.header.slots.clone();
        slots.remove(pos);
        let candidate = Header { slots, ..self.header.clone() };
        if candidate.validate().is_err() {
            return Err(VaultError::LastSlot);
        }
        self.header = candidate;
        self.payload.slots.remove(&crate::kdf::hex(slot_id));
        Ok(())
    }

    /// Re-wraps the container key for a new password (and/or new KDF parameters). The container
    /// key, the slots and the recovery code stay valid.
    pub fn rekey(
        &mut self,
        new_password_key: &PasswordKey,
        new_kdf: KdfParams,
        rng: &mut impl CryptoRngCore,
    ) -> Result<()> {
        new_kdf.validate()?;
        let cid = self.header.container_id;
        let k = kek(new_password_key, &self.factor_secret, &cid);
        self.header.vmk = wrap32(&k, &self.vmk, &vmk_aad(&cid), rng);
        self.header.kdf = new_kdf;
        self.password_key = Some(PasswordKey::from_bytes(*new_password_key.expose()));
        Ok(())
    }

    /// A new factor secret wrapped for every security key and key file of this container,
    /// without touching them (their slots carry public keys). Nothing changes here: give the
    /// result to [`Self::replace_factor`] of every container of the suite.
    pub fn renewed_factor(&self, rng: &mut impl CryptoRngCore) -> Result<(FactorSecret, Vec<Slot>)> {
        let factor: Vec<&Slot> = self.header.slots.iter().filter(|s| s.provides_factor()).collect();
        let new_s = FactorSecret::generate(rng);
        let suite_id = self.header.suite_id;
        let mut slots = Vec::with_capacity(factor.len());
        for slot in factor {
            let mut slot = slot.clone();
            match &mut slot {
                Slot::Fido2(Fido2Slot { slot_id, public, wrapped, .. })
                | Slot::Keyfile(KeyfileSlot { slot_id, public, wrapped, .. }) => {
                    let (np, nw) = public_wrap(public.public_key, &suite_id, slot_id, &new_s, rng)?;
                    *public = np;
                    *wrapped = nw;
                }
                Slot::Recovery(_) => unreachable!("only factor slots"),
            }
            slots.push(slot);
        }
        Ok((new_s, slots))
    }

    /// Switches to another factor secret and its slots (from [`Self::renewed_factor`]); the
    /// recovery slot stays. Call [`Self::renew_container_key`] afterwards.
    pub fn replace_factor(
        &mut self,
        factor_secret: FactorSecret,
        factor_slots: Vec<Slot>,
        password_key: &PasswordKey,
        rng: &mut impl CryptoRngCore,
    ) -> Result<()> {
        if factor_slots.iter().any(|s| !s.provides_factor()) {
            return Err(VaultError::InvalidInput("only key and key file slots".into()));
        }
        let mut slots = factor_slots;
        slots.extend(self.header.slots.iter().filter(|s| !s.provides_factor()).cloned());
        let candidate = Header { slots, ..self.header.clone() };
        candidate.validate()?;
        self.header = candidate;
        self.factor_secret = factor_secret;
        self.payload.slots.retain(|id, _| self.header.slots.iter().any(|s| &crate::kdf::hex(s.id()) == id));
        let kdf = self.header.kdf.clone();
        self.rekey(password_key, kdf, rng)
    }

    /// Gives the container a new random container key: copies of the file from before (with
    /// an old recovery code, a removed key or an old password) no longer open the content
    /// written from now on. The recovery code (if any) gets a new slot for the new key.
    pub fn renew_container_key(&mut self, password_key: &PasswordKey, rng: &mut impl CryptoRngCore) -> Result<()> {
        let cid = self.header.container_id;
        self.vmk = random32(rng);
        let k = kek(password_key, &self.factor_secret, &cid);
        self.header.vmk = wrap32(&k, &self.vmk, &vmk_aad(&cid), rng);
        self.password_key = Some(PasswordKey::from_bytes(*password_key.expose()));
        if let Some(code) = self.recovery_code() {
            let label = self
                .header
                .slots
                .iter()
                .find(|s| matches!(s, Slot::Recovery(_)))
                .and_then(|s| self.payload.slots.get(&crate::kdf::hex(s.id())))
                .map(|m| (m.label.clone(), m.created))
                .unwrap_or_default();
            self.set_recovery_code(&code, &label.0, label.1, rng)?;
        } else {
            // Without the code a recovery slot would still open the old key only: drop it.
            let old: Vec<[u8; 16]> =
                self.header.slots.iter().filter(|s| matches!(s, Slot::Recovery(_))).map(|s| *s.id()).collect();
            for id in &old {
                self.remove_slot(id)?;
            }
        }
        Ok(())
    }

    /// Checks a password against this container without changing anything.
    pub fn verify_password_key(&self, password_key: &PasswordKey) -> bool {
        use subtle::ConstantTimeEq;
        let cid = self.header.container_id;
        let k = kek(password_key, &self.factor_secret, &cid);
        match unwrap32(&k, &self.header.vmk, &vmk_aad(&cid)) {
            Some(v) => bool::from(v.as_ref().ct_eq(self.vmk.as_ref())),
            None => false,
        }
    }

    pub fn factor_secret(&self) -> &FactorSecret {
        &self.factor_secret
    }
}

/// Zeroized wrapper for a container file (it may be held in memory for a while).
pub type ContainerBytes = Zeroizing<Vec<u8>>;
