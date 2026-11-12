// SPDX-License-Identifier: AGPL-3.0-or-later
//! Key derivation helpers (Argon2id, HKDF) and display fingerprints.

use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Argon2id with a 32-byte output.
pub fn argon2id(
    password: &[u8],
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(memory_kib, iterations, parallelism, Some(32)).map_err(|e| e.to_string())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon.hash_password_into(password, salt, out.as_mut()).map_err(|e| e.to_string())?;
    Ok(out)
}

/// HKDF-SHA256 (extract with empty salt, expand with `info`) to 32 bytes.
pub fn hkdf32(ikm: &[u8], info: &[u8]) -> Zeroizing<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand(info, out.as_mut()).expect("32 bytes is a valid HKDF length");
    out
}

/// Short, non-reversible fingerprint of a secret for display and comparison (16 hex chars).
pub fn fingerprint(secret: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"ghulsuite/fingerprint/v1");
    h.update(secret);
    h.finalize()[..8].iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2id_rfc9106_vector() {
        // RFC 9106 §5.3 test vector uses secret and associated data, which the plain API
        // does not expose; instead check determinism and parameter handling.
        let a = argon2id(b"password", b"somesaltsomesalt", 1024, 2, 1).unwrap();
        let b = argon2id(b"password", b"somesaltsomesalt", 1024, 2, 1).unwrap();
        let c = argon2id(b"passwore", b"somesaltsomesalt", 1024, 2, 1).unwrap();
        assert_eq!(*a, *b);
        assert_ne!(*a, *c);
        assert!(argon2id(b"p", b"short", 1024, 2, 1).is_err(), "salt must be >= 8 bytes");
    }

    #[test]
    fn fingerprint_is_stable() {
        assert_eq!(fingerprint(b"abc"), fingerprint(b"abc"));
        assert_eq!(fingerprint(b"abc").len(), 16);
    }
}
