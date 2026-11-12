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
}

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
        OtpAlgorithm::Sha1 => run!(Sha1),
        OtpAlgorithm::Sha256 => run!(Sha256),
        OtpAlgorithm::Sha512 => run!(Sha512),
    }
}

pub fn hotp(secret: &[u8], counter: u64, digits: u32, algorithm: OtpAlgorithm) -> Result<String, OtpError> {
    if !(6..=8).contains(&digits) {
        return Err(OtpError::InvalidDigits);
    }
    if secret.is_empty() {
        return Err(OtpError::EmptySecret);
    }
    let h = mac(algorithm, secret, &counter.to_be_bytes());
    let offset = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[offset] & 0x7f, h[offset + 1], h[offset + 2], h[offset + 3]]);
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
