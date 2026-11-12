// SPDX-License-Identifier: AGPL-3.0-or-later
//! CTAP2 PIN/UV auth protocols 1 and 2 (CTAP 2.1, §6.5.6 and §6.5.7).

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use ciborium::value::Value;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::cbor::{as_bytes, map_get_int, Cbor};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinProtocol {
    V1,
    V2,
}

impl PinProtocol {
    pub fn id(self) -> i64 {
        match self {
            PinProtocol::V1 => 1,
            PinProtocol::V2 => 2,
        }
    }

    /// `authenticate(key, message)` as defined by the protocol.
    pub fn authenticate(self, key: &[u8], message: &[u8]) -> Vec<u8> {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key size");
        mac.update(message);
        let tag = mac.finalize().into_bytes();
        match self {
            PinProtocol::V1 => tag[..16].to_vec(),
            PinProtocol::V2 => tag.to_vec(),
        }
    }
}

/// Shared secret established with the authenticator via ECDH.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SharedSecret {
    #[zeroize(skip)]
    pub protocol: PinProtocol,
    hmac_key: Vec<u8>,
    aes_key: Vec<u8>,
}

impl SharedSecret {
    /// Derives the shared secret from the raw ECDH output `z` (x coordinate).
    pub fn from_z(protocol: PinProtocol, z: &[u8]) -> Self {
        match protocol {
            PinProtocol::V1 => {
                let k = Sha256::digest(z).to_vec();
                SharedSecret { protocol, hmac_key: k.clone(), aes_key: k }
            }
            PinProtocol::V2 => {
                let hk = Hkdf::<Sha256>::new(Some(&[0u8; 32]), z);
                let mut hmac_key = vec![0u8; 32];
                let mut aes_key = vec![0u8; 32];
                hk.expand(b"CTAP2 HMAC key", &mut hmac_key).expect("32 bytes is a valid length");
                hk.expand(b"CTAP2 AES key", &mut aes_key).expect("32 bytes is a valid length");
                SharedSecret { protocol, hmac_key, aes_key }
            }
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        if plaintext.len() % 16 != 0 {
            return Err("plaintext length must be a multiple of 16".into());
        }
        match self.protocol {
            PinProtocol::V1 => {
                let enc = Aes256CbcEnc::new_from_slices(&self.aes_key, &[0u8; 16]).map_err(|e| e.to_string())?;
                Ok(enc.encrypt_padded_vec_mut::<NoPadding>(plaintext))
            }
            PinProtocol::V2 => {
                let mut iv = [0u8; 16];
                OsRng.fill_bytes(&mut iv);
                let enc = Aes256CbcEnc::new_from_slices(&self.aes_key, &iv).map_err(|e| e.to_string())?;
                let mut out = iv.to_vec();
                out.extend_from_slice(&enc.encrypt_padded_vec_mut::<NoPadding>(plaintext));
                Ok(out)
            }
        }
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let (iv, ct): ([u8; 16], &[u8]) = match self.protocol {
            PinProtocol::V1 => ([0u8; 16], ciphertext),
            PinProtocol::V2 => {
                if ciphertext.len() < 16 {
                    return Err("ciphertext too short".into());
                }
                let mut iv = [0u8; 16];
                iv.copy_from_slice(&ciphertext[..16]);
                (iv, &ciphertext[16..])
            }
        };
        if ct.len() % 16 != 0 {
            return Err("ciphertext length must be a multiple of 16".into());
        }
        let dec = Aes256CbcDec::new_from_slices(&self.aes_key, &iv).map_err(|e| e.to_string())?;
        dec.decrypt_padded_vec_mut::<NoPadding>(ct).map(Zeroizing::new).map_err(|e| e.to_string())
    }

    /// `authenticate` with the shared secret as key.
    pub fn authenticate(&self, message: &[u8]) -> Vec<u8> {
        self.protocol.authenticate(&self.hmac_key, message)
    }
}

/// Performs the platform side of the key agreement: generates an ephemeral P-256 key,
/// computes ECDH with the authenticator's key and returns the platform COSE key together
/// with the derived shared secret.
pub fn encapsulate(protocol: PinProtocol, peer_cose: &Value) -> Result<(Cbor, SharedSecret), String> {
    let peer = cose_to_public_key(peer_cose)?;
    let secret = random_secret_key();
    let shared = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), peer.as_affine());
    let z = Zeroizing::new(shared.raw_secret_bytes().to_vec());
    let platform_cose = public_key_to_cose(&secret.public_key());
    Ok((platform_cose, SharedSecret::from_z(protocol, &z)))
}

/// A random P-256 secret key from the operating system's generator (rejection sampling: a
/// candidate of zero or at/above the group order is drawn again, which practically never happens).
pub fn random_secret_key() -> SecretKey {
    loop {
        let mut bytes = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(bytes.as_mut());
        if let Ok(key) = SecretKey::from_slice(bytes.as_ref()) {
            return key;
        }
    }
}

pub fn cose_to_public_key(cose: &Value) -> Result<PublicKey, String> {
    let kty = map_get_int(cose, 1).and_then(crate::cbor::as_u64);
    if kty != Some(2) {
        return Err("key agreement key is not an EC2 key".into());
    }
    let x = map_get_int(cose, -2).and_then(as_bytes).ok_or("missing x coordinate")?;
    let y = map_get_int(cose, -3).and_then(as_bytes).ok_or("missing y coordinate")?;
    if x.len() != 32 || y.len() != 32 {
        return Err("invalid coordinate length".into());
    }
    // Uncompressed SEC1 point: 0x04 ‖ x ‖ y. Points not on the curve are rejected.
    let mut point = [0u8; 65];
    point[0] = 4;
    point[1..33].copy_from_slice(x);
    point[33..].copy_from_slice(y);
    PublicKey::from_sec1_bytes(&point).map_err(|_| "invalid P-256 point".into())
}

pub fn public_key_to_cose(key: &PublicKey) -> Cbor {
    let point = key.as_affine().to_sec1_point(false);
    Cbor::Map(vec![
        (Cbor::Int(1), Cbor::Int(2)),   // kty: EC2
        (Cbor::Int(3), Cbor::Int(-25)), // alg: ECDH-ES+HKDF-256
        (Cbor::Int(-1), Cbor::Int(1)),  // crv: P-256
        (Cbor::Int(-2), Cbor::bytes(point.x().expect("uncompressed point"))),
        (Cbor::Int(-3), Cbor::bytes(point.y().expect("uncompressed point"))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_both_protocols() {
        for p in [PinProtocol::V1, PinProtocol::V2] {
            let s = SharedSecret::from_z(p, &[7u8; 32]);
            let pt = [0x42u8; 32];
            let ct = s.encrypt(&pt).unwrap();
            assert_eq!(ct.len(), if p == PinProtocol::V1 { 32 } else { 48 });
            assert_eq!(s.decrypt(&ct).unwrap().as_slice(), &pt);
            assert_eq!(s.authenticate(b"x").len(), if p == PinProtocol::V1 { 16 } else { 32 });
        }
    }

    /// Known answers from the P-256 definition (SEC 2): scalar 1 is the base point G and
    /// 2·G is its double. Checks the COSE encoding (x, y big-endian, 32 bytes each), parsing and
    /// ECDH bit for bit, independent of the library version.
    #[test]
    fn known_answer_p256() {
        let hex = |s: &str| {
            (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<u8>>()
        };
        let gx = hex("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296");
        let gy = hex("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5");
        let g2x = hex("7cf27b188d034f7e8a52380304b51ac3c08969e277f21b35a60b48fc47669978");
        let mut one = [0u8; 32];
        one[31] = 1;
        let k1 = SecretKey::from_slice(&one).unwrap();
        let cose = crate::cbor::decode(&public_key_to_cose(&k1.public_key()).encode()).unwrap();
        assert_eq!(map_get_int(&cose, -2).and_then(as_bytes).unwrap(), &gx[..]);
        assert_eq!(map_get_int(&cose, -3).and_then(as_bytes).unwrap(), &gy[..]);
        // Parsing the COSE key gives back G; ECDH(2, G) = x(2·G).
        let g = cose_to_public_key(&cose).unwrap();
        let mut two = [0u8; 32];
        two[31] = 2;
        let k2 = SecretKey::from_slice(&two).unwrap();
        let z = p256::ecdh::diffie_hellman(k2.to_nonzero_scalar(), g.as_affine());
        assert_eq!(z.raw_secret_bytes().as_slice(), &g2x[..]);
        // A point that is not on the curve is refused.
        let mut bad = cose.clone();
        if let Value::Map(entries) = &mut bad {
            for (k, v) in entries.iter_mut() {
                if k.as_integer().map(i128::from) == Some(-3) {
                    *v = Value::Bytes(vec![1u8; 32]);
                }
            }
        }
        assert!(cose_to_public_key(&bad).is_err());
        // Zero and the group order are no valid secret keys.
        assert!(SecretKey::from_slice(&[0u8; 32]).is_err());
        let n = hex("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551");
        assert!(SecretKey::from_slice(&n).is_err());
    }

    #[test]
    fn ecdh_agreement_matches_on_both_sides() {
        let auth_secret = random_secret_key();
        let auth_cose = crate::cbor::decode(&public_key_to_cose(&auth_secret.public_key()).encode()).unwrap();
        let (platform_cose, platform_ss) = encapsulate(PinProtocol::V2, &auth_cose).unwrap();
        let platform_pub = cose_to_public_key(&crate::cbor::decode(&platform_cose.encode()).unwrap()).unwrap();
        let z = p256::ecdh::diffie_hellman(auth_secret.to_nonzero_scalar(), platform_pub.as_affine());
        let auth_ss = SharedSecret::from_z(PinProtocol::V2, z.raw_secret_bytes());
        let ct = platform_ss.encrypt(&[1u8; 16]).unwrap();
        assert_eq!(auth_ss.decrypt(&ct).unwrap().as_slice(), &[1u8; 16]);
    }
}
