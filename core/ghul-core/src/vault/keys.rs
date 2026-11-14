// SPDX-License-Identifier: AGPL-3.0-or-later
//! Key hierarchy (see `docs/FORMAT.md`, section "Kryptografie").
//!
//! ```text
//! PK   = Argon2id(NFC(password), kdf.salt, m, t, p)                       password key
//! S    = random 256 bit, wrapped by every security key and key file slot    factor secret
//! KEK  = HKDF(PK || S, "ghul/v1/kek" || container_id)
//! VMK  = random 256 bit, AEAD(KEK, aad = "ghul/v1/vmk" || container_id)
//! PKEY = HKDF(VMK, "ghul/v1/payload" || container_id)
//! ```
//!
//! Slots of format version 2 hold an X25519 public key per security key or key file: the
//! private key is derived from the key's hmac-secret output (or the key file), so the app can
//! wrap a new `S` for every slot without touching the keys (`wrap_to_public`).

use chacha20poly1305::aead::{Aead, KeyInit, Payload as AeadPayload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand_core::CryptoRngCore;
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::error::{Result, VaultError};
use super::format::{KdfParams, Wrapped, NONCE_LEN, TAG_LEN};

pub const INFO_KEK: &[u8] = b"ghul/v1/kek";
pub const INFO_PAYLOAD: &[u8] = b"ghul/v1/payload";
pub const INFO_SLOT_FIDO2: &[u8] = b"ghul/v1/slot/fido2";
pub const INFO_SLOT_KEYFILE: &[u8] = b"ghul/v1/slot/keyfile";
pub const INFO_SLOT_RECOVERY: &[u8] = b"ghul/v1/slot/recovery";
pub const AAD_VMK: &[u8] = b"ghul/v1/vmk";
pub const AAD_FACTOR: &[u8] = b"ghul/v1/factor";
pub const AAD_RECOVERY: &[u8] = b"ghul/v1/recovery";
pub const INFO_X25519_FIDO2: &[u8] = b"ghul/v2/slot/fido2-x25519";
pub const INFO_X25519_KEYFILE: &[u8] = b"ghul/v2/slot/keyfile-x25519";
pub const INFO_X25519_WRAP: &[u8] = b"ghul/v2/slot/x25519-wrap";

pub type Key32 = Zeroizing<[u8; 32]>;

/// Argon2id output of the master password.
pub struct PasswordKey(pub(crate) Key32);

impl PasswordKey {
    /// Derives the password key. The password is normalized to Unicode NFC so that the same
    /// password typed on different platforms yields the same key.
    pub fn derive(password: &str, kdf: &KdfParams) -> Result<PasswordKey> {
        if password.is_empty() {
            return Err(VaultError::InvalidInput("password must not be empty".into()));
        }
        kdf.validate()?;
        let normalized: Zeroizing<String> = Zeroizing::new(password.nfc().collect());
        let key =
            crate::kdf::argon2id(normalized.as_bytes(), &kdf.salt, kdf.memory_kib, kdf.iterations, kdf.parallelism)
                .map_err(VaultError::InvalidInput)?;
        Ok(PasswordKey(key))
    }

    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        PasswordKey(Zeroizing::new(bytes))
    }
}

/// The factor secret `S` shared by all slots of a suite.
pub struct FactorSecret(pub(crate) Key32);

impl FactorSecret {
    pub fn generate(rng: &mut impl CryptoRngCore) -> Self {
        let mut s = Zeroizing::new([0u8; 32]);
        rng.fill_bytes(s.as_mut());
        FactorSecret(s)
    }

    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        FactorSecret(Zeroizing::new(bytes))
    }
}

/// HKDF-SHA256 with empty salt, info = concatenation of `info`.
pub fn hkdf(ikm: &[u8], info: &[&[u8]]) -> Key32 {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand_multi_info(info, out.as_mut()).expect("32 bytes is a valid HKDF length");
    out
}

pub fn kek(pk: &PasswordKey, s: &FactorSecret, container_id: &[u8; 16]) -> Key32 {
    let mut ikm = Zeroizing::new([0u8; 64]);
    ikm[..32].copy_from_slice(pk.0.as_ref());
    ikm[32..].copy_from_slice(s.0.as_ref());
    hkdf(ikm.as_ref(), &[INFO_KEK, container_id])
}

pub fn payload_key(vmk: &Key32, container_id: &[u8; 16]) -> Key32 {
    hkdf(vmk.as_ref(), &[INFO_PAYLOAD, container_id])
}

pub fn fido2_slot_key(hmac_output: &[u8; 32], slot_id: &[u8; 16]) -> Key32 {
    hkdf(hmac_output, &[INFO_SLOT_FIDO2, slot_id])
}

pub fn recovery_slot_key(code: &[u8; 32], slot_id: &[u8; 16], container_id: &[u8; 16]) -> Key32 {
    hkdf(code, &[INFO_SLOT_RECOVERY, slot_id, container_id])
}

pub fn recovery_aad(container_id: &[u8; 16], slot_id: &[u8; 16]) -> Vec<u8> {
    [AAD_RECOVERY, container_id, slot_id].concat()
}

pub fn keyfile_slot_key(keyfile_secret: &[u8; 32], slot_id: &[u8; 16]) -> Key32 {
    hkdf(keyfile_secret, &[INFO_SLOT_KEYFILE, slot_id])
}

/// X25519 private key of a version-2 slot, derived from the hmac-secret output or key file
/// secret (`info` = [`INFO_X25519_FIDO2`] or [`INFO_X25519_KEYFILE`]). Clamped as in RFC 7748.
pub fn slot_private_key(ikm: &[u8; 32], info: &[u8], slot_id: &[u8; 16]) -> x25519_dalek::StaticSecret {
    x25519_dalek::StaticSecret::from(*hkdf(ikm, &[info, slot_id]))
}

pub fn public_key(private: &x25519_dalek::StaticSecret) -> [u8; 32] {
    x25519_dalek::PublicKey::from(private).to_bytes()
}

fn x25519_wrap_key(shared: &[u8; 32], ephemeral: &[u8; 32], public: &[u8; 32], slot_id: &[u8; 16]) -> Key32 {
    let mut ikm = Zeroizing::new([0u8; 96]);
    ikm[..32].copy_from_slice(shared);
    ikm[32..64].copy_from_slice(ephemeral);
    ikm[64..].copy_from_slice(public);
    hkdf(ikm.as_ref(), &[INFO_X25519_WRAP, slot_id])
}

/// Wraps `secret` for the holder of `public` (X25519 with a fresh ephemeral key). Returns the
/// ephemeral public key and the wrapped secret; `None` for a public key of small order.
pub fn wrap_to_public(
    public: &[u8; 32],
    slot_id: &[u8; 16],
    secret: &[u8; 32],
    aad: &[u8],
    rng: &mut impl CryptoRngCore,
) -> Option<([u8; 32], Wrapped)> {
    let eph = x25519_dalek::StaticSecret::random_from_rng(&mut *rng);
    let eph_pub = public_key(&eph);
    let shared = eph.diffie_hellman(&x25519_dalek::PublicKey::from(*public));
    if !shared.was_contributory() {
        return None;
    }
    let key = x25519_wrap_key(shared.as_bytes(), &eph_pub, public, slot_id);
    Some((eph_pub, wrap32(&key, secret, aad, rng)))
}

/// Opens a secret wrapped by [`wrap_to_public`] with the slot's private key.
pub fn unwrap_with_private(
    private: &x25519_dalek::StaticSecret,
    public: &[u8; 32],
    ephemeral: &[u8; 32],
    slot_id: &[u8; 16],
    wrapped: &Wrapped,
    aad: &[u8],
) -> Option<Key32> {
    use subtle::ConstantTimeEq;
    if !bool::from(public_key(private).ct_eq(public)) {
        return None;
    }
    let shared = private.diffie_hellman(&x25519_dalek::PublicKey::from(*ephemeral));
    if !shared.was_contributory() {
        return None;
    }
    let key = x25519_wrap_key(shared.as_bytes(), ephemeral, public, slot_id);
    unwrap32(&key, wrapped, aad)
}

pub fn factor_aad(suite_id: &[u8; 16], slot_id: &[u8; 16]) -> Vec<u8> {
    [AAD_FACTOR, suite_id, slot_id].concat()
}

pub fn vmk_aad(container_id: &[u8; 16]) -> Vec<u8> {
    [AAD_VMK, container_id].concat()
}

/// XChaCha20-Poly1305 encryption.
pub fn seal(key: &[u8; 32], nonce: &[u8; NONCE_LEN], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(nonce), AeadPayload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 encryption cannot fail for valid lengths")
}

/// XChaCha20-Poly1305 decryption; `None` if authentication fails.
pub fn open(key: &[u8; 32], nonce: &[u8; NONCE_LEN], ciphertext: &[u8], aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), AeadPayload { msg: ciphertext, aad })
        .ok()
        .map(Zeroizing::new)
}

pub fn wrap32(key: &[u8; 32], secret: &[u8; 32], aad: &[u8], rng: &mut impl CryptoRngCore) -> Wrapped {
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut nonce);
    let ct = seal(key, &nonce, secret, aad);
    let mut ciphertext = [0u8; 32 + TAG_LEN];
    ciphertext.copy_from_slice(&ct);
    Wrapped { nonce, ciphertext }
}

pub fn unwrap32(key: &[u8; 32], wrapped: &Wrapped, aad: &[u8]) -> Option<Key32> {
    let pt = open(key, &wrapped.nonce, &wrapped.ciphertext, aad)?;
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&pt);
    Some(out)
}

pub fn random32(rng: &mut impl CryptoRngCore) -> Key32 {
    let mut k = Zeroizing::new([0u8; 32]);
    rng.fill_bytes(k.as_mut());
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_normalization() {
        let kdf = KdfParams { memory_kib: 64 * 1024, iterations: 3, parallelism: 1, salt: [9; 32] };
        // "é" precomposed vs. "e" + combining acute accent
        let a = PasswordKey::derive("caf\u{e9}", &kdf).unwrap();
        let b = PasswordKey::derive("cafe\u{301}", &kdf).unwrap();
        assert_eq!(a.expose(), b.expose());
        assert!(PasswordKey::derive("", &kdf).is_err());
    }

    #[test]
    fn rejects_weak_kdf() {
        let kdf = KdfParams { memory_kib: 1024, iterations: 3, parallelism: 1, salt: [9; 32] };
        assert!(PasswordKey::derive("pw", &kdf).is_err());
    }

    #[test]
    fn public_key_wrap_roundtrip() {
        use rand_core::SeedableRng;
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(2);
        let slot_id = [5u8; 16];
        let private = slot_private_key(&[7u8; 32], INFO_X25519_FIDO2, &slot_id);
        let public = public_key(&private);
        let (eph, w) = wrap_to_public(&public, &slot_id, &[8u8; 32], b"aad", &mut rng).unwrap();
        assert_eq!(*unwrap_with_private(&private, &public, &eph, &slot_id, &w, b"aad").unwrap(), [8u8; 32]);
        // Other key material, other slot, other AAD: nothing opens.
        let other = slot_private_key(&[6u8; 32], INFO_X25519_FIDO2, &slot_id);
        assert!(unwrap_with_private(&other, &public, &eph, &slot_id, &w, b"aad").is_none());
        assert!(unwrap_with_private(&private, &public, &eph, &[4u8; 16], &w, b"aad").is_none());
        assert!(unwrap_with_private(&private, &public, &eph, &slot_id, &w, b"other").is_none());
        // Same key file or key, other kind of slot: another key pair.
        let keyfile = slot_private_key(&[7u8; 32], INFO_X25519_KEYFILE, &slot_id);
        assert_ne!(public_key(&keyfile), public);
        // Public keys of small order are refused (the shared secret would be known).
        assert!(wrap_to_public(&[0u8; 32], &slot_id, &[8u8; 32], b"aad", &mut rng).is_none());
        let mut one = [0u8; 32];
        one[0] = 1;
        assert!(wrap_to_public(&one, &slot_id, &[8u8; 32], b"aad", &mut rng).is_none());
    }

    #[test]
    fn wrap_roundtrip_and_aad_binding() {
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(1);
        use rand_core::SeedableRng;
        let key = [1u8; 32];
        let w = wrap32(&key, &[2u8; 32], b"aad", &mut rng);
        assert_eq!(*unwrap32(&key, &w, b"aad").unwrap(), [2u8; 32]);
        assert!(unwrap32(&key, &w, b"other").is_none());
        assert!(unwrap32(&[3u8; 32], &w, b"aad").is_none());
    }
}
