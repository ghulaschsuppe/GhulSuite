// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quick unlock (fingerprint or app PIN) after a full unlock.
//!
//! A full unlock yields the password key `PK` and the suite's container key `VMK`. For quick unlock they are stored in `quick-<name>.ghlq` (device-local, never in a shared
//! folder), encrypted with a key derived from a random 32-byte "quick key" `QK` – and, for PIN
//! mode, additionally from Argon2id(PIN). The app keeps `QK` only wrapped by a
//! hardware-backed platform key (Android Keystore: fingerprint-bound or device-bound).
//!
//! ```text
//! "GHULQUIK" | version u16 = 1 | suite_id[16] | salt[32] | has_pin u8 | created_ms i64 | nonce[24]
//!   | AEAD(PK[32] || VMK[32])
//! key = HKDF(QK [|| Argon2id(NFC(PIN), salt)], "ghul/v1/quick" || suite_id), AAD = everything before the nonce
//! ```
//!
//! The validity is counted from the last *full* unlock: every full unlock writes
//! `quick-<name>.stamp` = `time_ms i64 || HMAC-SHA256(HKDF(VMK, "ghul/v1/quick-stamp" || name), time_ms)`.
//! The stamp can only be verified (and forged) with the suite key.

use std::fs;
use std::path::{Path, PathBuf};

use rand_core::{OsRng, RngCore};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use crate::vault::keys::{hkdf, open, seal};

const MAGIC: &[u8; 8] = b"GHULQUIK";
const VERSION: u16 = 1;
const PREFIX_LEN: usize = 8 + 2 + 16 + 32 + 1 + 8;
const PIN_MEMORY_KIB: u32 = 64 * 1024;
const PIN_ITERATIONS: u32 = 3;
pub const MIN_PIN_LEN: usize = 4;

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
    pub vmk: Zeroizing<[u8; 32]>,
}

pub fn create(
    dir: &Path,
    name: &str,
    suite_id: &[u8; 16],
    password_key: &[u8; 32],
    vmk: &[u8; 32],
    pin: Option<&str>,
    now_ms: i64,
) -> Result<Zeroizing<Vec<u8>>> {
    if let Some(p) = pin {
        if p.chars().count() < MIN_PIN_LEN {
            return Err(GhulError::InvalidInput { msg: format!("PIN must have at least {MIN_PIN_LEN} characters") });
        }
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
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let mut secret = Zeroizing::new([0u8; SECRET_LEN]);
    secret[..32].copy_from_slice(password_key);
    secret[32..64].copy_from_slice(vmk);
    let ct = seal(&key, &nonce, secret.as_ref(), &out);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    fs::create_dir_all(dir)?;
    super::storage::write_atomic(dir, &format!("quick-{name}.ghlq"), &out)?;
    Ok(qk)
}

/// Opens the quick unlock file. Expired files are deleted.
pub fn open_file(
    dir: &Path,
    name: &str,
    suite_id: &[u8; 16],
    qk: &[u8],
    pin: Option<&str>,
    max_age_ms: i64,
    now_ms: i64,
) -> Result<QuickSecret> {
    let p = path(dir, name)?;
    let data = fs::read(&p).map_err(|_| GhulError::QuickUnlockUnavailable)?;
    if data.len() != PREFIX_LEN + 24 + SECRET_LEN + 16 || &data[..8] != MAGIC {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockUnavailable);
    }
    if u16::from_le_bytes([data[8], data[9]]) != VERSION || data[10..26] != suite_id[..] {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockUnavailable);
    }
    let salt: [u8; 32] = data[26..58].try_into().expect("32 bytes");
    let has_pin = data[58] == 1;
    let created = i64::from_le_bytes(data[59..67].try_into().expect("8 bytes"));
    if has_pin != pin.is_some() {
        return Err(GhulError::InvalidInput { msg: "PIN required".into() });
    }
    let key = file_key(qk, pin, &salt, suite_id)?;
    let nonce: [u8; 24] = data[PREFIX_LEN..PREFIX_LEN + 24].try_into().expect("24 bytes");
    let plain =
        open(&key, &nonce, &data[PREFIX_LEN + 24..], &data[..PREFIX_LEN]).ok_or(GhulError::QuickUnlockInvalid)?;
    let mut pk = Zeroizing::new([0u8; 32]);
    let mut vmk = Zeroizing::new([0u8; 32]);
    pk.copy_from_slice(&plain[..32]);
    vmk.copy_from_slice(&plain[32..64]);
    let last_full_unlock = read_stamp(dir, name, &vmk).map_or(created, |t| t.max(created));
    if now_ms < last_full_unlock || now_ms - last_full_unlock > max_age_ms {
        let _ = wipe(&p);
        let _ = fs::remove_file(stamp_path(dir, name));
        return Err(GhulError::QuickUnlockExpired);
    }
    Ok(QuickSecret { password_key: pk, vmk })
}

fn stamp_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("quick-{name}.stamp"))
}

fn stamp_mac(vmk: &[u8; 32], name: &str, time_ms: i64) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let key = hkdf(vmk, &[b"ghul/v1/quick-stamp", name.as_bytes()]);
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key.as_ref()).expect("any key size");
    mac.update(&time_ms.to_le_bytes());
    mac.finalize().into_bytes().into()
}

fn read_stamp(dir: &Path, name: &str, vmk: &[u8; 32]) -> Option<i64> {
    use subtle::ConstantTimeEq;
    let data = fs::read(stamp_path(dir, name)).ok()?;
    if data.len() != 40 {
        return None;
    }
    let t = i64::from_le_bytes(data[..8].try_into().ok()?);
    bool::from(stamp_mac(vmk, name, t).ct_eq(&data[8..])).then_some(t)
}

/// Records a full unlock for every existing quick unlock file.
pub fn refresh_stamps(dir: &Path, vmk: &[u8; 32], now_ms: i64) {
    for name in ["bio", "pin"] {
        if let Ok(p) = path(dir, name) {
            if p.exists() {
                let mut data = now_ms.to_le_bytes().to_vec();
                data.extend_from_slice(&stamp_mac(vmk, name, now_ms));
                let _ = fs::write(stamp_path(dir, name), data);
            }
        }
    }
}

/// Overwrites and deletes a file.
pub fn wipe(p: &Path) -> std::io::Result<()> {
    if let Ok(meta) = fs::metadata(p) {
        let _ = fs::write(p, vec![0u8; meta.len() as usize]);
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
