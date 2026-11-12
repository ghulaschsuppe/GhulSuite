// SPDX-License-Identifier: AGPL-3.0-or-later
//! Printable recovery code: 256 random bits plus a 16-bit checksum, encoded as 55 characters of
//! Crockford Base32 in 11 groups of 5 (e.g. `7K2QF-…`).

use rand_core::CryptoRngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::error::{Result, VaultError};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECK_INFO: &[u8] = b"ghul/v1/recovery-check";
const CODE_CHARS: usize = 55;

pub struct RecoveryCode(pub(crate) Zeroizing<[u8; 32]>);

impl RecoveryCode {
    pub fn generate(rng: &mut impl CryptoRngCore) -> Self {
        let mut s = Zeroizing::new([0u8; 32]);
        rng.fill_bytes(s.as_mut());
        RecoveryCode(s)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        RecoveryCode(Zeroizing::new(bytes))
    }

    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    fn checksum(secret: &[u8; 32]) -> [u8; 2] {
        let mut h = Sha256::new();
        h.update(CHECK_INFO);
        h.update(secret);
        let d = h.finalize();
        [d[0], d[1]]
    }

    /// Display form: 11 groups of 5 characters separated by `-`.
    pub fn to_display(&self) -> Zeroizing<String> {
        let mut data = Zeroizing::new([0u8; 34]);
        data[..32].copy_from_slice(self.0.as_ref());
        data[32..].copy_from_slice(&Self::checksum(&self.0));
        let mut chars = Zeroizing::new(String::with_capacity(CODE_CHARS + 10));
        let (mut acc, mut bits, mut count) = (0u32, 0u32, 0usize);
        let push = |c: u8, chars: &mut String, count: &mut usize| {
            if *count > 0 && *count % 5 == 0 {
                chars.push('-');
            }
            chars.push(c as char);
            *count += 1;
        };
        for &b in data.iter() {
            acc = (acc << 8) | b as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                push(ALPHABET[((acc >> bits) & 31) as usize], &mut chars, &mut count);
            }
        }
        if bits > 0 {
            push(ALPHABET[((acc << (5 - bits)) & 31) as usize], &mut chars, &mut count);
        }
        chars
    }

    /// Parses a code typed by a human: case-insensitive, `-` and whitespace ignored,
    /// `O` read as `0`, `I`/`L` read as `1`.
    pub fn parse(input: &str) -> Result<Self> {
        let mut values = Zeroizing::new(Vec::with_capacity(CODE_CHARS));
        for c in input.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            let v = ALPHABET.iter().position(|&a| a as char == c).ok_or(VaultError::InvalidRecoveryCode)?;
            values.push(v as u8);
        }
        if values.len() != CODE_CHARS {
            return Err(VaultError::InvalidRecoveryCode);
        }
        let mut data = Zeroizing::new([0u8; 34]);
        let (mut acc, mut bits, mut idx) = (0u32, 0u32, 0usize);
        for &v in values.iter() {
            acc = (acc << 5) | v as u32;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                if idx < 34 {
                    data[idx] = (acc >> bits) as u8;
                }
                idx += 1;
            }
            acc &= (1 << bits) - 1;
        }
        // 55 * 5 = 275 bits = 34 bytes + 3 padding bits which must be zero.
        if idx != 34 || bits != 3 || acc != 0 {
            return Err(VaultError::InvalidRecoveryCode);
        }
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(&data[..32]);
        if Self::checksum(&secret) != data[32..34] {
            return Err(VaultError::InvalidRecoveryCode);
        }
        Ok(RecoveryCode(secret))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    #[test]
    fn roundtrip_and_typo_detection() {
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(7);
        for _ in 0..200 {
            let code = RecoveryCode::generate(&mut rng);
            let text = code.to_display();
            assert_eq!(text.len(), 55 + 10);
            assert_eq!(text.split('-').count(), 11);
            let parsed = RecoveryCode::parse(&text).unwrap();
            assert_eq!(parsed.expose(), code.expose());
            // lower case, spaces instead of dashes
            let sloppy = text.to_lowercase().replace('-', " ");
            assert_eq!(RecoveryCode::parse(&sloppy).unwrap().expose(), code.expose());
        }
        let code = RecoveryCode::generate(&mut rng);
        let text = code.to_display().to_string();
        // change one character
        let mut chars: Vec<char> = text.chars().collect();
        chars[3] = if chars[3] == 'A' { 'B' } else { 'A' };
        let typo: String = chars.into_iter().collect();
        assert_eq!(RecoveryCode::parse(&typo).err(), Some(VaultError::InvalidRecoveryCode));
        assert!(RecoveryCode::parse("too-short").is_err());
        assert!(RecoveryCode::parse(&format!("{text}X")).is_err());
    }

    #[test]
    fn confusable_characters() {
        let code = RecoveryCode::from_bytes([0u8; 32]);
        let text = code.to_display().to_string();
        assert!(text.starts_with("00000-"));
        let with_o = text.replacen("00000", "OoOoO", 1);
        assert_eq!(RecoveryCode::parse(&with_o).unwrap().expose(), &[0u8; 32]);
    }
}
