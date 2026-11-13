// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stateless helpers for the app: password generator and strength, otpauth URIs, Base32,
//! Argon2id calibration.

use std::time::Instant;

use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use crate::totp::OtpAlgorithm;
use crate::vault::format::{MAX_MEMORY_KIB, MIN_ITERATIONS, MIN_MEMORY_KIB};

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtpAlgo {
    Sha1,
    Sha256,
    Sha512,
    /// Steam Guard (five characters, `digits` is 5).
    Steam,
}

impl From<OtpAlgo> for OtpAlgorithm {
    fn from(a: OtpAlgo) -> Self {
        match a {
            OtpAlgo::Sha1 => OtpAlgorithm::Sha1,
            OtpAlgo::Sha256 => OtpAlgorithm::Sha256,
            OtpAlgo::Sha512 => OtpAlgorithm::Sha512,
            OtpAlgo::Steam => OtpAlgorithm::Steam,
        }
    }
}

impl OtpAlgo {
    pub fn name(self) -> &'static str {
        match self {
            OtpAlgo::Sha1 => "SHA1",
            OtpAlgo::Sha256 => "SHA256",
            OtpAlgo::Sha512 => "SHA512",
            OtpAlgo::Steam => "STEAM",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().replace('-', "").as_str() {
            "SHA1" => Some(OtpAlgo::Sha1),
            "SHA256" => Some(OtpAlgo::Sha256),
            "SHA512" => Some(OtpAlgo::Sha512),
            "STEAM" => Some(OtpAlgo::Steam),
            _ => None,
        }
    }
}

/// A TOTP account as entered or scanned by the user.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct TotpInput {
    pub issuer: String,
    pub account: String,
    /// Base32 (RFC 4648), spaces and case are ignored.
    pub secret_base32: String,
    pub algorithm: OtpAlgo,
    pub digits: u32,
    pub period: u32,
}

/// Decodes RFC 4648 Base32 (case-insensitive; spaces, dashes and `=` padding ignored).
pub fn base32_decode(input: &str) -> Option<Zeroizing<Vec<u8>>> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = Zeroizing::new(Vec::with_capacity(input.len() * 5 / 8));
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in input.chars() {
        if c == ' ' || c == '-' || c == '=' {
            continue;
        }
        let v = ALPHABET.iter().position(|&a| a as char == c.to_ascii_uppercase())? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
        acc &= (1 << bits) - 1;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(crate) fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Validates a TOTP input (secret decodes, digits and period in range).
pub fn validate_totp(input: &TotpInput) -> Result<Zeroizing<Vec<u8>>> {
    let secret = base32_decode(&input.secret_base32)
        .ok_or_else(|| GhulError::InvalidInput { msg: "secret is not valid Base32".into() })?;
    if input.algorithm == OtpAlgo::Steam {
        if input.digits != crate::totp::STEAM_DIGITS {
            return Err(GhulError::InvalidInput { msg: "Steam codes have 5 characters".into() });
        }
    } else if !(6..=8).contains(&input.digits) {
        return Err(GhulError::InvalidInput { msg: "digits must be 6 to 8".into() });
    }
    if !(1..=3600).contains(&input.period) {
        return Err(GhulError::InvalidInput { msg: "period must be 1 to 3600 seconds".into() });
    }
    if input.issuer.trim().is_empty() && input.account.trim().is_empty() {
        return Err(GhulError::InvalidInput { msg: "issuer or account is required".into() });
    }
    Ok(secret)
}

/// A Steam Guard account.
pub(crate) fn steam_input(issuer: &str, account: &str, secret: &str) -> TotpInput {
    TotpInput {
        issuer: issuer.to_owned(),
        account: account.to_owned(),
        secret_base32: secret.to_owned(),
        algorithm: OtpAlgo::Steam,
        digits: crate::totp::STEAM_DIGITS,
        period: 30,
    }
}

/// Parses an `otpauth://totp/...` URI (Key Uri Format); also `steam://<secret>` links.
#[uniffi::export]
pub fn parse_otpauth_uri(uri: String) -> Result<TotpInput> {
    let invalid = |m: &str| GhulError::InvalidInput { msg: m.to_owned() };
    // Steam Guard as Proton Authenticator and others export it: steam://<Base32 secret>
    if let Some(secret) = uri.trim().strip_prefix("steam://") {
        let input = steam_input("Steam", "", secret.trim());
        validate_totp(&input)?;
        return Ok(input);
    }
    let rest = uri.trim().strip_prefix("otpauth://").ok_or_else(|| invalid("not an otpauth:// URI"))?;
    let (otp_type, rest) = rest.split_once('/').ok_or_else(|| invalid("missing type"))?;
    if !otp_type.eq_ignore_ascii_case("totp") {
        return Err(invalid("only TOTP is supported"));
    }
    let (label, query) = rest.split_once('?').unwrap_or((rest, ""));
    let label = percent_decode(label).ok_or_else(|| invalid("invalid label"))?;
    let (label_issuer, account) = match label.split_once(':') {
        Some((i, a)) => (i.trim().to_owned(), a.trim().to_owned()),
        None => (String::new(), label.trim().to_owned()),
    };
    let mut input = TotpInput {
        issuer: label_issuer,
        account,
        secret_base32: String::new(),
        algorithm: OtpAlgo::Sha1,
        digits: 6,
        period: 30,
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v).ok_or_else(|| invalid("invalid parameter"))?;
        match k.to_ascii_lowercase().as_str() {
            "secret" => input.secret_base32 = v,
            "issuer" if !v.is_empty() => input.issuer = v,
            "algorithm" => input.algorithm = OtpAlgo::from_name(&v).ok_or_else(|| invalid("unknown algorithm"))?,
            "digits" => input.digits = v.parse().map_err(|_| invalid("invalid digits"))?,
            "period" => input.period = v.parse().map_err(|_| invalid("invalid period"))?,
            // Steam Guard as some apps write it: otpauth://totp/Steam:name?secret=…&encoder=steam
            "encoder" if v.eq_ignore_ascii_case("steam") => {
                input.algorithm = OtpAlgo::Steam;
                input.digits = crate::totp::STEAM_DIGITS;
            }
            _ => {}
        }
    }
    if input.algorithm == OtpAlgo::Steam {
        input.digits = crate::totp::STEAM_DIGITS;
    }
    validate_totp(&input)?;
    Ok(input)
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct PasswordOptions {
    pub length: u32,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Leaves out characters that are easy to confuse (0/O, 1/l/I, …).
    pub avoid_ambiguous: bool,
}

/// Generates a random password; every selected character class occurs at least once.
#[uniffi::export]
pub fn generate_password(options: PasswordOptions) -> Result<String> {
    let ambiguous = "0O1lI|`'\"";
    let filter = |set: &str| -> Vec<char> {
        set.chars().filter(|c| !options.avoid_ambiguous || !ambiguous.contains(*c)).collect()
    };
    let mut classes = Vec::new();
    if options.lowercase {
        classes.push(filter("abcdefghijklmnopqrstuvwxyz"));
    }
    if options.uppercase {
        classes.push(filter("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
    }
    if options.digits {
        classes.push(filter("0123456789"));
    }
    if options.symbols {
        classes.push(filter("!#$%&()*+,-./:;<=>?@[]^_{}~"));
    }
    if classes.is_empty() {
        return Err(GhulError::InvalidInput { msg: "select at least one character class".into() });
    }
    if !(8..=128).contains(&options.length) || (options.length as usize) < classes.len() {
        return Err(GhulError::InvalidInput { msg: "length must be 8 to 128".into() });
    }
    let all: Vec<char> = classes.iter().flatten().copied().collect();
    let pick = |set: &[char]| set[uniform(set.len() as u32) as usize];
    loop {
        let pw: Vec<char> = (0..options.length).map(|_| pick(&all)).collect();
        if classes.iter().all(|cls| pw.iter().any(|c| cls.contains(c))) {
            return Ok(pw.into_iter().collect());
        }
    }
}

/// Uniform random number in `0..n` without modulo bias.
pub(crate) fn uniform(n: u32) -> u32 {
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let v = OsRng.next_u32();
        if v < zone {
            return v % n;
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct PasswordStrength {
    /// 0 (very weak) to 4 (very strong), zxcvbn scale.
    pub score: u8,
    /// Estimated log10 of guesses needed.
    pub guesses_log10: f64,
}

/// Estimates password strength with zxcvbn (offline, nothing leaves the device).
#[uniffi::export]
pub fn password_strength(password: String) -> PasswordStrength {
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return PasswordStrength { score: 0, guesses_log10: 0.0 };
    }
    let e = zxcvbn::zxcvbn(&password, &[]);
    PasswordStrength { score: u8::from(e.score()), guesses_log10: e.guesses_log10() }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct KdfChoice {
    pub memory_kib: u32,
    pub iterations: u32,
    /// Estimated duration on this device in milliseconds.
    pub estimated_ms: u64,
}

/// Picks Argon2id parameters for about `target_ms` on this device: 3 iterations, memory as large
/// as possible (power of two MiB) up to `max_memory_mib`, at least 64 MiB.
#[uniffi::export]
pub fn calibrate_kdf(target_ms: u32, max_memory_mib: u32) -> Result<KdfChoice> {
    let t = Instant::now();
    crate::kdf::argon2id(b"calibration", b"ghulsuite-calibration", MIN_MEMORY_KIB, MIN_ITERATIONS, 1)
        .map_err(|msg| GhulError::InvalidInput { msg })?;
    let base_ms = (t.elapsed().as_millis() as u64).max(1);
    let max_kib = (max_memory_mib.saturating_mul(1024)).clamp(MIN_MEMORY_KIB, MAX_MEMORY_KIB);
    let mut memory_kib = MIN_MEMORY_KIB;
    let mut estimate = base_ms;
    while memory_kib * 2 <= max_kib && estimate * 2 <= target_ms as u64 {
        memory_kib *= 2;
        estimate *= 2;
    }
    Ok(KdfChoice { memory_kib, iterations: MIN_ITERATIONS, estimated_ms: estimate })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_rfc4648() {
        assert_eq!(base32_decode("MZXW6YTBOI======").unwrap().as_slice(), b"foobar");
        assert_eq!(base32_decode("mzxw 6ytb oi").unwrap().as_slice(), b"foobar");
        assert!(base32_decode("MZXW1").is_none());
        assert!(base32_decode("").is_none());
    }

    #[test]
    fn otpauth_uri() {
        let i = parse_otpauth_uri(
            "otpauth://totp/ACME%20Co:john@example.com?secret=HXDMVJECJJWSRB3HWIZR4IFUGFTMXBOZ&issuer=ACME%20Co&algorithm=SHA1&digits=6&period=30".into(),
        )
        .unwrap();
        assert_eq!(i.issuer, "ACME Co");
        assert_eq!(i.account, "john@example.com");
        assert_eq!(i.digits, 6);
        let j = parse_otpauth_uri("otpauth://totp/alice?secret=JBSWY3DPEHPK3PXP".into()).unwrap();
        assert_eq!((j.issuer.as_str(), j.account.as_str(), j.period), ("", "alice", 30));
        assert!(parse_otpauth_uri("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1".into()).is_err());
        assert!(parse_otpauth_uri("https://example.org".into()).is_err());
        assert!(parse_otpauth_uri("otpauth://totp/x?secret=!!!".into()).is_err());
        assert!(parse_otpauth_uri("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=12".into()).is_err());
    }

    #[test]
    fn generator() {
        let opts = PasswordOptions {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            avoid_ambiguous: true,
        };
        for _ in 0..100 {
            let pw = generate_password(opts.clone()).unwrap();
            assert_eq!(pw.chars().count(), 20);
            assert!(pw.chars().any(|c| c.is_ascii_digit()));
            assert!(!pw.contains('0') && !pw.contains('l'));
        }
        assert!(generate_password(PasswordOptions { length: 4, ..opts.clone() }).is_err());
        assert!(generate_password(PasswordOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..opts
        })
        .is_err());
    }

    #[test]
    fn strength() {
        assert!(password_strength("password".into()).score <= 1);
        assert!(password_strength("Kater-Ufer-Mondlicht-Brezel-73".into()).score >= 3);
    }
}
