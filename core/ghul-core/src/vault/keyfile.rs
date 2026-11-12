// SPDX-License-Identifier: AGPL-3.0-or-later
//! Key file `.ghlkey`: `"GHULKEY\0" | version u16 = 1 | secret[32] | check[4]` (46 bytes),
//! where `check = SHA-256(magic || version || secret)[0..4]`.

use rand_core::CryptoRngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::error::{Result, VaultError};

pub const KEYFILE_MAGIC: &[u8; 8] = b"GHULKEY\0";
pub const KEYFILE_VERSION: u16 = 1;
pub const KEYFILE_LEN: usize = 46;
const ID_INFO: &[u8] = b"ghul/v1/keyfile-id";

pub struct Keyfile {
    secret: Zeroizing<[u8; 32]>,
}

impl Keyfile {
    pub fn generate(rng: &mut impl CryptoRngCore) -> Self {
        let mut secret = Zeroizing::new([0u8; 32]);
        rng.fill_bytes(secret.as_mut());
        Keyfile { secret }
    }

    pub fn from_secret(secret: [u8; 32]) -> Self {
        Keyfile { secret: Zeroizing::new(secret) }
    }

    fn check(secret: &[u8; 32]) -> [u8; 4] {
        let mut h = Sha256::new();
        h.update(KEYFILE_MAGIC);
        h.update(KEYFILE_VERSION.to_le_bytes());
        h.update(secret);
        let d = h.finalize();
        [d[0], d[1], d[2], d[3]]
    }

    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(KEYFILE_LEN));
        out.extend_from_slice(KEYFILE_MAGIC);
        out.extend_from_slice(&KEYFILE_VERSION.to_le_bytes());
        out.extend_from_slice(self.secret.as_ref());
        out.extend_from_slice(&Self::check(&self.secret));
        out
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != KEYFILE_LEN || &data[..8] != KEYFILE_MAGIC {
            return Err(VaultError::InvalidKeyfile);
        }
        if u16::from_le_bytes([data[8], data[9]]) != KEYFILE_VERSION {
            return Err(VaultError::InvalidKeyfile);
        }
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(&data[10..42]);
        if Self::check(&secret) != data[42..46] {
            return Err(VaultError::InvalidKeyfile);
        }
        Ok(Keyfile { secret })
    }

    /// Public identifier stored in the key file slot to recognize the right key file.
    pub fn id(&self) -> [u8; 8] {
        let mut h = Sha256::new();
        h.update(ID_INFO);
        h.update(self.secret.as_ref());
        let d = h.finalize();
        d[..8].try_into().expect("8 bytes")
    }

    pub fn expose(&self) -> &[u8; 32] {
        &self.secret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    #[test]
    fn roundtrip_and_corruption() {
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(3);
        let kf = Keyfile::generate(&mut rng);
        let bytes = kf.encode();
        assert_eq!(bytes.len(), KEYFILE_LEN);
        let parsed = Keyfile::parse(&bytes).unwrap();
        assert_eq!(parsed.id(), kf.id());
        for i in 0..bytes.len() {
            let mut bad = bytes.to_vec();
            bad[i] ^= 0x01;
            assert!(Keyfile::parse(&bad).is_err(), "byte {i}");
        }
        assert!(Keyfile::parse(&bytes[..45]).is_err());
    }
}
