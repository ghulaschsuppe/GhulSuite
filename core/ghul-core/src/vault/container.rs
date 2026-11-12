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
//! Alternatives: [`LockedContainer::recover`] with the recovery code (opens the container key
//! directly) and [`LockedContainer::unlock_with_vmk`] with a key from the suite's keyring.

use rand_core::CryptoRngCore;
use zeroize::Zeroizing;

use super::error::{Result, VaultError};
use super::format::{
    encode_prefix, split_file, ContainerKind, Fido2Slot, Header, KdfParams, KeyfileSlot, RecoverySlot, Slot, MAX_SLOTS,
    NONCE_LEN,
};
use super::keyfile::Keyfile;
use super::keys::{
    factor_aad, fido2_slot_key, kek, keyfile_slot_key, open, payload_key, random32, recovery_aad, recovery_slot_key,
    seal, unwrap32, vmk_aad, wrap32, FactorSecret, Key32, PasswordKey,
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
        })
    }
}

fn new_fido2_slot(
    suite_id: &[u8; 16],
    s: &FactorSecret,
    reg: &Fido2Registration<'_>,
    rng: &mut impl CryptoRngCore,
) -> Slot {
    let mut slot_id = [0u8; 16];
    rng.fill_bytes(&mut slot_id);
    let key = fido2_slot_key(reg.hmac_output, &slot_id);
    let wrapped = wrap32(&key, s.expose(), &factor_aad(suite_id, &slot_id), rng);
    Slot::Fido2(Fido2Slot {
        slot_id,
        rp_id: reg.rp_id.to_owned(),
        credential_id: reg.credential_id.to_vec(),
        prf_input: reg.prf_input,
        uv_required: true,
        wrapped,
    })
}

fn new_keyfile_slot(suite_id: &[u8; 16], s: &FactorSecret, keyfile: &Keyfile, rng: &mut impl CryptoRngCore) -> Slot {
    let mut slot_id = [0u8; 16];
    rng.fill_bytes(&mut slot_id);
    let key = keyfile_slot_key(keyfile.expose(), &slot_id);
    let wrapped = wrap32(&key, s.expose(), &factor_aad(suite_id, &slot_id), rng);
    Slot::Keyfile(KeyfileSlot { slot_id, keyfile_id: keyfile.id(), wrapped })
}

/// A parsed container whose header is readable but whose content is still encrypted.
pub struct LockedContainer {
    kind: ContainerKind,
    header: Header,
    data: Vec<u8>,
}

impl LockedContainer {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let parts = split_file(data)?;
        let header = Header::parse(parts.header)?;
        Ok(LockedContainer { kind: parts.kind, header, data: data.to_vec() })
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
                let wrapped = self
                    .header
                    .slots
                    .iter()
                    .find_map(|s| match s {
                        Slot::Fido2(f) if &f.slot_id == slot_id => Some(&f.wrapped),
                        _ => None,
                    })
                    .ok_or(VaultError::SlotNotFound)?;
                let key = fido2_slot_key(hmac_output, slot_id);
                unwrap32(&key, wrapped, &factor_aad(suite_id, slot_id))
                    .map(FactorSecret)
                    .ok_or(VaultError::WrongSecondFactor)
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
                        let key = keyfile_slot_key(keyfile.expose(), &k.slot_id);
                        if let Some(sec) = unwrap32(&key, &k.wrapped, &factor_aad(suite_id, &k.slot_id)) {
                            return Ok(FactorSecret(sec));
                        }
                    }
                }
                Err(if any { VaultError::WrongKeyfile } else { VaultError::NoKeyfileSlot })
            }
        }
    }

    /// Decrypts the container. `password_key` must be derived with this container's KDF parameters.
    pub fn unlock(self, password_key: &PasswordKey, factor_secret: &FactorSecret) -> Result<UnlockedContainer> {
        let cid = self.header.container_id;
        let k = kek(password_key, factor_secret, &cid);
        let vmk = unwrap32(&k, &self.header.vmk, &vmk_aad(&cid)).ok_or(VaultError::WrongPassword)?;
        self.open_payload(vmk, Some(FactorSecret::from_bytes(*factor_secret.expose())))
    }

    /// Opens the container with its key from the suite's keyring.
    pub fn unlock_with_vmk(self, vmk: &[u8; 32]) -> Result<UnlockedContainer> {
        self.open_payload(Zeroizing::new(*vmk), None)
    }

    /// Opens the container with the recovery code alone (no password, key or key file).
    pub fn recover(self, code: &RecoveryCode) -> Result<UnlockedContainer> {
        let cid = self.header.container_id;
        let vmk = self
            .header
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
            .ok_or(VaultError::WrongSecondFactor)?;
        self.open_payload(vmk, None)
    }

    fn open_payload(self, vmk: Key32, factor_secret: Option<FactorSecret>) -> Result<UnlockedContainer> {
        let cid = self.header.container_id;
        let parts = split_file(&self.data)?;
        let pkey = payload_key(&vmk, &cid);
        let plaintext = open(&pkey, &parts.nonce, parts.ciphertext, parts.aad).ok_or(VaultError::Corrupted)?;
        let payload = Payload::from_cbor(unpad(&plaintext)?, self.kind)?;
        let factor_secret = match factor_secret {
            Some(s) => s,
            None => {
                let s = payload.factor_secret.as_deref().ok_or(VaultError::Corrupted)?;
                FactorSecret::from_bytes(s.try_into().map_err(|_| VaultError::Corrupted)?)
            }
        };
        Ok(UnlockedContainer { kind: self.kind, header: self.header, vmk, factor_secret, payload })
    }
}

/// A decrypted container. Secrets are wiped from memory when it is dropped.
pub struct UnlockedContainer {
    kind: ContainerKind,
    header: Header,
    vmk: Key32,
    factor_secret: FactorSecret,
    pub payload: Payload,
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

    /// The container key, for the suite's keyring.
    pub fn vmk(&self) -> &[u8; 32] {
        &self.vmk
    }

    /// Encrypts and serializes the container. Increments the payload revision.
    pub fn seal(&mut self, rng: &mut impl CryptoRngCore) -> Result<Vec<u8>> {
        self.payload.revision =
            self.payload.revision.checked_add(1).ok_or_else(|| VaultError::Payload("revision overflow".into()))?;
        self.payload.factor_secret = Some(self.factor_secret.expose().to_vec());
        let header = self.header.encode()?;
        let cbor = self.payload.to_cbor()?;
        let plaintext = pad(&cbor)?;
        let mut out = Vec::with_capacity(14 + header.len() + NONCE_LEN + plaintext.len() + 16);
        encode_prefix(self.kind, header.len(), &mut out);
        out.extend_from_slice(&header);
        let mut nonce = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce);
        let pkey = payload_key(&self.vmk, &self.header.container_id);
        let ct = seal(&pkey, &nonce, &plaintext, &out);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
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
