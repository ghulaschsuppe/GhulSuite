// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quick unlock (fingerprint or app PIN) after a full unlock.
//!
//! A full unlock yields the password key `PK` and the factor secret `S`. For quick unlock they
//! are stored in `quick-<name>.ghlq` (device-local, never in a shared folder), encrypted with a
//! key derived from a random 32-byte "quick key" `QK`: and, for PIN mode, additionally from
//! Argon2id(PIN). The app keeps `QK` only wrapped by a hardware-backed platform key (Android
//! Keystore: fingerprint-bound or device-bound).
//!
//! ```text
//! "GHULQUIK" | version u16 = 3 | suite_id[16] | salt[32] | has_pin u8 | created_ms i64
//!   | max_age_ms i64 | nonce[24] | AEAD(PK[32] || S[32])
//! key = HKDF(QK [|| Argon2id(NFC(PIN), salt)], "ghul/v1/quick" || suite_id), AAD = everything before the nonce
//! ```
//!
//! The validity (`max_age_ms`) is chosen when quick unlock is enabled (with both factors) and is
//! part of the AEAD's associated data: the app cannot extend it without enabling it again.
//! Files of version 2 (validity given by the app at every unlock) are no longer opened.
//! The validity is counted from the last *full* unlock: every full unlock writes
//! `quick-<name>.stamp` = `time_ms i64 || HMAC-SHA256(HKDF(PK, "ghul/v2/quick-stamp" || name), time_ms)`.
//! The stamp can only be verified (and forged) with the password key.

use std::fs;
use std::path::{Path, PathBuf};

use rand_core::{OsRng, RngCore};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use crate::vault::keys::{hkdf, open, seal};

const MAGIC: &[u8; 8] = b"GHULQUIK";
const VERSION: u16 = 3;
const PREFIX_LEN: usize = 8 + 2 + 16 + 32 + 1 + 8 + 8;
const PIN_MEMORY_KIB: u32 = 64 * 1024;
const PIN_ITERATIONS: u32 = 3;
/// The shortest app PIN (M42: the same rule as the app's).
pub const MIN_PIN_LEN: usize = 6;

/// The app PIN: at least [`MIN_PIN_LEN`] characters.
pub fn check_pin(pin: &str) -> Result<()> {
    if pin.chars().count() < MIN_PIN_LEN {
        return Err(GhulError::InvalidInput { msg: format!("PIN must have at least {MIN_PIN_LEN} characters") });
    }
    Ok(())
}

pub fn path(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(GhulError::InvalidInput { msg: "invalid quick unlock name".into() });
    }
    Ok(dir.join(format!("quick-{name}.ghlq")))
}

fn pin_key(pin: &str, salt: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
    let pin: Zeroizing<String> = Zeroizing::new(pin.nfc().collect());
    crate::kdf::argon2id(pin.as_bytes(), salt, PIN_MEMORY_KIB, PIN_ITERATIONS, 1)
        .map_err(|msg| GhulError::InvalidInput { msg })
}

fn file_key(qk: &[u8], pin: Option<&str>, salt: &[u8; 32], suite_id: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>> {
    if qk.len() != 32 {
        return Err(GhulError::QuickUnlockInvalid);
    }
    let mut ikm = Zeroizing::new(qk.to_vec());
    if let Some(pin) = pin {
        ikm.extend_from_slice(pin_key(pin, salt)?.as_ref());
    }
    Ok(hkdf(&ikm, &[b"ghul/v1/quick", suite_id]))
}

/// Creates the quick unlock file and returns the quick key `QK` (32 bytes) for the platform to wrap.
const SECRET_LEN: usize = 32 + 32;

pub struct QuickSecret {
    pub password_key: Zeroizing<[u8; 32]>,
    /// The factor secret `S`.
    pub factor_secret: Zeroizing<[u8; 32]>,
}

/// Writes the quick unlock file and returns the new quick key. `max_age_ms`: how long after a
/// full unlock it opens (stored in the file).
#[allow(clippy::too_many_arguments)]
pub fn create(
    dir: &Path,
    name: &str,
    suite_id: &[u8; 16],
    password_key: &[u8; 32],
    factor_secret: &[u8; 32],
    pin: Option<&str>,
    now_ms: i64,
    max_age_ms: i64,
) -> Result<Zeroizing<Vec<u8>>> {
    if let Some(p) = pin {
        check_pin(p)?;
    }
    if max_age_ms <= 0 {
        return Err(GhulError::InvalidInput { msg: "quick unlock needs a validity".into() });
    }
    let mut qk = Zeroizing::new(vec![0u8; 32]);
    OsRng.fill_bytes(&mut qk);
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    let key = file_key(&qk, pin, &salt, suite_id)?;

    let mut out = Vec::with_capacity(PREFIX_LEN + 24 + 80);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(suite_id);
    out.extend_from_slice(&salt);
    out.push(u8::from(pin.is_some()));
    out.extend_from_slice(&now_ms.to_le_bytes());
    out.extend_from_slice(&max_age_ms.to_le_bytes());
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let mut secret = Zeroizing::new([0u8; SECRET_LEN]);
    secret[..32].copy_from_slice(password_key);
    secret[32..64].copy_from_slice(factor_secret);
    let ct = seal(&key, &nonce, secret.as_ref(), &out);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    fs::create_dir_all(dir)?;
    super::storage::write_atomic(dir, &format!("quick-{name}.ghlq"), &out)?;
    Ok(qk)
}

/// Opens the quick unlock file, within the validity stored in it. An expired one is refused and
/// kept (a full unlock makes it valid again).
pub fn open_file(
    dir: &Path,
    name: &str,
    suite_id: &[u8; 16],
    qk: &[u8],
    pin: Option<&str>,
    now_ms: i64,
) -> Result<QuickSecret> {
    let p = path(dir, name)?;
    let data = fs::read(&p).map_err(|_| GhulError::QuickUnlockUnavailable)?;
    // An older file (version 2: the app gave the validity at every unlock): never used again,
    // and the app can say why (H6).
    if data.len() >= 10 && &data[..8] == MAGIC && u16::from_le_bytes([data[8], data[9]]) < VERSION {
        let _ = wipe(&p);
        let _ = fs::remove_file(stamp_path(dir, name));
        return Err(GhulError::QuickUnlockOutdated);
    }
    if data.len() != PREFIX_LEN + 24 + SECRET_LEN + 16 || &data[..8] != MAGIC {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockUnavailable);
    }
    let version = u16::from_le_bytes([data[8], data[9]]);
    if version != VERSION || data[10..26] != suite_id[..] {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockUnavailable);
    }
    let salt: [u8; 32] = data[26..58].try_into().expect("32 bytes");
    let has_pin = data[58] == 1;
    let created = i64::from_le_bytes(data[59..67].try_into().expect("8 bytes"));
    // Checked by the AEAD below (associated data): only trusted once it opened.
    let max_age_ms = i64::from_le_bytes(data[67..75].try_into().expect("8 bytes"));
    if has_pin != pin.is_some() {
        return Err(GhulError::InvalidInput { msg: "PIN required".into() });
    }
    let key = file_key(qk, pin, &salt, suite_id)?;
    let nonce: [u8; 24] = data[PREFIX_LEN..PREFIX_LEN + 24].try_into().expect("24 bytes");
    let plain =
        open(&key, &nonce, &data[PREFIX_LEN + 24..], &data[..PREFIX_LEN]).ok_or(GhulError::QuickUnlockInvalid)?;
    let mut pk = Zeroizing::new([0u8; 32]);
    let mut secret = Zeroizing::new([0u8; 32]);
    pk.copy_from_slice(&plain[..32]);
    secret.copy_from_slice(&plain[32..64]);
    let last_full_unlock = read_stamp(dir, name, &pk).map_or(created, |t| t.max(created));
    // M30: expired is refused, not deleted: the next full unlock writes a new stamp and quick
    // unlock works again (it stays enabled until it is turned off or the protection changes).
    if now_ms < last_full_unlock || now_ms - last_full_unlock > max_age_ms {
        return Err(GhulError::QuickUnlockExpired);
    }
    Ok(QuickSecret { password_key: pk, factor_secret: secret })
}

fn stamp_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("quick-{name}.stamp"))
}

/// `info` = "ghul/v2/quick-stamp" with the password key.
fn stamp_mac(key: &[u8; 32], info: &[u8], name: &str, time_ms: i64) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let key = hkdf(key, &[info, name.as_bytes()]);
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key.as_ref()).expect("any key size");
    mac.update(&time_ms.to_le_bytes());
    mac.finalize().into_bytes().into()
}

fn read_stamp(dir: &Path, name: &str, password_key: &[u8; 32]) -> Option<i64> {
    use subtle::ConstantTimeEq;
    let data = fs::read(stamp_path(dir, name)).ok()?;
    if data.len() != 40 {
        return None;
    }
    let t = i64::from_le_bytes(data[..8].try_into().ok()?);
    bool::from(stamp_mac(password_key, STAMP_INFO, name, t).ct_eq(&data[8..])).then_some(t)
}

const STAMP_INFO: &[u8] = b"ghul/v2/quick-stamp";

/// Records a full unlock for every existing quick unlock file.
pub fn refresh_stamps(dir: &Path, password_key: &[u8; 32], now_ms: i64) {
    for name in ["bio", "pin"] {
        if let Ok(p) = path(dir, name) {
            if p.exists() {
                let mut data = now_ms.to_le_bytes().to_vec();
                data.extend_from_slice(&stamp_mac(password_key, STAMP_INFO, name, now_ms));
                let _ = fs::write(stamp_path(dir, name), data);
            }
        }
    }
}

/// Overwrites and deletes a file. The zeros go in pieces (a copy of a large vault must not need
/// its size in memory).
pub fn wipe(p: &Path) -> std::io::Result<()> {
    if let Ok(meta) = fs::metadata(p) {
        let zero = |len: u64| -> std::io::Result<()> {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().write(true).open(p)?;
            let piece = [0u8; 64 * 1024];
            let mut left = len;
            while left > 0 {
                let n = left.min(piece.len() as u64) as usize;
                f.write_all(&piece[..n])?;
                left -= n as u64;
            }
            f.sync_all()
        };
        let _ = zero(meta.len());
    }
    fs::remove_file(p)
}

pub fn delete_all(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("quick-")
                && (name.ends_with(".ghlq") || name.ends_with(".ghlq.bak") || name.ends_with(".ghlq.tmp"))
            {
                let _ = wipe(&e.path());
            }
        }
    }
}
