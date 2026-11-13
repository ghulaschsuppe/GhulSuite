// SPDX-License-Identifier: AGPL-3.0-or-later
//! HOTP (RFC 4226) and TOTP (RFC 6238).

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
    /// Steam Guard: HMAC-SHA1 like TOTP, but five characters from Steam's own alphabet.
    Steam,
}

/// Characters of Steam Guard codes.
const STEAM_CHARS: &[u8; 26] = b"23456789BCDFGHJKMNPQRTVWXY";

/// Number of characters of a Steam Guard code.
pub const STEAM_DIGITS: u32 = 5;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OtpError {
    #[error("digits must be between 6 and 8")]
    InvalidDigits,
    #[error("period must be between 1 and 3600 seconds")]
    InvalidPeriod,
    #[error("secret must not be empty")]
    EmptySecret,
}

fn mac(algorithm: OtpAlgorithm, secret: &[u8], msg: &[u8]) -> Vec<u8> {
    macro_rules! run {
        ($h:ty) => {{
            let mut m = <Hmac<$h> as Mac>::new_from_slice(secret).expect("HMAC accepts any key size");
            m.update(msg);
            m.finalize().into_bytes().to_vec()
        }};
    }
    match algorithm {
        OtpAlgorithm::Sha1 | OtpAlgorithm::Steam => run!(Sha1),
        OtpAlgorithm::Sha256 => run!(Sha256),
        OtpAlgorithm::Sha512 => run!(Sha512),
    }
}

pub fn hotp(secret: &[u8], counter: u64, digits: u32, algorithm: OtpAlgorithm) -> Result<String, OtpError> {
    if algorithm != OtpAlgorithm::Steam && !(6..=8).contains(&digits) {
        return Err(OtpError::InvalidDigits);
    }
    if secret.is_empty() {
        return Err(OtpError::EmptySecret);
    }
    let h = mac(algorithm, secret, &counter.to_be_bytes());
    let offset = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[offset] & 0x7f, h[offset + 1], h[offset + 2], h[offset + 3]]);
    if algorithm == OtpAlgorithm::Steam {
        let mut rest = bin as usize;
        let mut code = String::with_capacity(STEAM_DIGITS as usize);
        for _ in 0..STEAM_DIGITS {
            code.push(STEAM_CHARS[rest % STEAM_CHARS.len()] as char);
            rest /= STEAM_CHARS.len();
        }
        return Ok(code);
    }
    let code = bin % 10u32.pow(digits);
    Ok(format!("{code:0width$}", width = digits as usize))
}

pub fn totp(
    secret: &[u8],
    unix_time: u64,
    period: u64,
    digits: u32,
    algorithm: OtpAlgorithm,
) -> Result<String, OtpError> {
    if !(1..=3600).contains(&period) {
        return Err(OtpError::InvalidPeriod);
    }
    hotp(secret, unix_time / period, digits, algorithm)
}

/// Seconds until the current TOTP code changes.
pub fn totp_remaining(unix_time: u64, period: u64) -> u64 {
    period - unix_time % period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_guard_codes() {
        // Same secret and time as other Steam Guard implementations (e.g. the one in andOTP/Aegis).
        let secret = crate::app::tools::base32_decode("JBSWY3DPEHPK3PXP").unwrap();
        let code = totp(&secret, 1_700_000_000, 30, STEAM_DIGITS, OtpAlgorithm::Steam).unwrap();
        assert_eq!(code.len(), 5);
        assert!(code.bytes().all(|c| STEAM_CHARS.contains(&c)));
        // Five characters from the 26-letter alphabet, derived from the same HMAC-SHA1 value as TOTP.
        let h = mac(OtpAlgorithm::Sha1, &secret, &(1_700_000_000u64 / 30).to_be_bytes());
        let o = (h[19] & 0x0f) as usize;
        let mut v = u32::from_be_bytes([h[o] & 0x7f, h[o + 1], h[o + 2], h[o + 3]]) as usize;
        let expected: String = (0..5)
            .map(|_| {
                let c = STEAM_CHARS[v % 26] as char;
                v /= 26;
                c
            })
            .collect();
        assert_eq!(code, expected);
    }

    #[test]
    fn rfc4226_hotp_vectors() {
        let expected =
            ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"];
        for (c, e) in expected.iter().enumerate() {
            assert_eq!(hotp(b"12345678901234567890", c as u64, 6, OtpAlgorithm::Sha1).unwrap(), *e);
        }
    }

    #[test]
    fn rfc6238_totp_vectors() {
        let s1 = b"12345678901234567890".as_slice();
        let s256 = b"12345678901234567890123456789012".as_slice();
        let s512 = b"1234567890123456789012345678901234567890123456789012345678901234".as_slice();
        let cases: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (1111111111, "14050471", "67062674", "99943326"),
            (1234567890, "89005924", "91819424", "93441116"),
            (2000000000, "69279037", "90698825", "38618901"),
            (20000000000, "65353130", "77737706", "47863826"),
        ];
        for (t, e1, e256, e512) in cases {
            assert_eq!(totp(s1, t, 30, 8, OtpAlgorithm::Sha1).unwrap(), e1);
            assert_eq!(totp(s256, t, 30, 8, OtpAlgorithm::Sha256).unwrap(), e256);
            assert_eq!(totp(s512, t, 30, 8, OtpAlgorithm::Sha512).unwrap(), e512);
        }
    }

    #[test]
    fn parameter_checks() {
        assert_eq!(hotp(b"x", 0, 5, OtpAlgorithm::Sha1), Err(OtpError::InvalidDigits));
        assert_eq!(hotp(b"", 0, 6, OtpAlgorithm::Sha1), Err(OtpError::EmptySecret));
        assert_eq!(totp(b"x", 0, 0, 6, OtpAlgorithm::Sha1), Err(OtpError::InvalidPeriod));
        assert_eq!(totp_remaining(59, 30), 1);
        assert_eq!(totp_remaining(60, 30), 30);
    }
}
