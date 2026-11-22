// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quick unlock (fingerprint or app PIN) after a full unlock.
//!
//! A full unlock yields the password key `PK` and the factor secret `S`. For quick unlock they
//! are stored in `quick-<name>.ghlq` (device-local, never in a shared folder).
//!
//! **Fingerprint** (`bio`, version 3): encrypted with a key derived from a random 32-byte "quick
//! key" `QK`, which the app keeps only wrapped by a fingerprint-bound key of the Android Keystore.
//!
//! ```text
//! "GHULQUIK" | version u16 = 3 | suite_id[16] | salt[32] | has_pin u8 = 0 | created_ms i64
//!   | max_age_ms i64 | nonce[24] | AEAD(PK[32] || S[32])
//! key = HKDF(QK, "ghul/v1/quick" || suite_id), AAD = everything before the nonce
//! ```
//!
//! **App PIN** (`pin`, version 4, M19): every attempt needs a key of the Android Keystore that
//! works once (`setMaxUsageCount(1)`), and the PIN goes into what that key computes. Two such
//! keys `K_0`, `K_1` (32 bytes each, made at random by the app and imported after this core
//! saw them) give two attempts; a right PIN seals the file anew for two new keys. The file alone
//! (without a key) allows no check of a PIN, also not offline.
//!
//! ```text
//! "GHULQUIK" | version u16 = 4 | suite_id[16] | salt[32] | has_pin u8 = 1 | created_ms i64
//!   | max_age_ms i64 | generation u32 | { nonce[24] | AEAD_i(PK[32] || S[32]) } for i = 0, 1
//! P   = Argon2id(NFC(PIN), salt)                                  (the challenge)
//! T_i = HMAC-SHA256(K_i, "ghul/v4/quick-pin-tag" || P)             (in the Keystore)
//! key_i = HKDF(T_i, "ghul/v4/quick-pin" || suite_id || generation || i)
//! AAD_i = everything before the first slot || i
//! ```
//!
//! PIN files of version 3 (the PIN only in Argon2id, open to an offline attack with the quick
//! key) are refused as outdated and deleted.
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
/// The app PIN's file (see the module).
const PIN_VERSION: u16 = 4;
const PIN_PREFIX_LEN: usize = PREFIX_LEN + 4;
/// Attempts of the app PIN: one Keystore key each.
pub const PIN_SLOTS: usize = 2;
const PIN_SLOT_LEN: usize = 24 + 32 + 32 + 16;
/// The name of the app PIN's quick unlock.
pub const PIN_NAME: &str = "pin";
const PIN_MEMORY_KIB: u32 = 64 * 1024;
const PIN_ITERATIONS: u32 = 3;
/// The shortest app PIN: digits only (decision of the owner, 22.11.2026).
pub const MIN_PIN_LEN: usize = 8;

/// The message for an app PIN that is too short or not only digits (translated by the app).
pub const PIN_RULE: &str = "the app PIN needs at least 8 digits";

/// The message for an app PIN that is easy to guess (translated by the app).
pub const PIN_TOO_SIMPLE: &str = "the app PIN is too easy to guess: no repeated digits or blocks, no runs, no dates";

/// The app PIN: at least [`MIN_PIN_LEN`] digits, nothing else, and none of the PINs tried first
/// (r9 dieb M2: two attempts are enough for those): one digit or a short block repeated
/// (`11111111`, `12121212`, `12341234`), a run up or down (`12345678`, `87654321`), and for
/// eight digits a date of the years 1900 to 2099 as `DDMMYYYY`, `MMDDYYYY` or `YYYYMMDD`.
pub fn check_pin(pin: &str) -> Result<()> {
    if pin.chars().count() < MIN_PIN_LEN || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(GhulError::InvalidInput { msg: PIN_RULE.into() });
    }
    if pin_too_simple(pin.as_bytes()) {
        return Err(GhulError::InvalidInput { msg: PIN_TOO_SIMPLE.into() });
    }
    Ok(())
}

fn pin_too_simple(d: &[u8]) -> bool {
    let d: Vec<i16> = d.iter().map(|b| i16::from(b - b'0')).collect();
    let repeated = (1..=4).any(|n| d.len() % n == 0 && d.chunks(n).all(|c| c == &d[..n]));
    let run = [1, -1].iter().any(|step| d.windows(2).all(|w| w[1] - w[0] == *step));
    let num = |r: std::ops::Range<usize>| d[r].iter().fold(0i32, |a, x| a * 10 + i32::from(*x));
    let date = |day: i32, month: i32, year: i32| {
        (1..=31).contains(&day) && (1..=12).contains(&month) && (1900..=2099).contains(&year)
    };
    let dated = d.len() == 8
        && (date(num(0..2), num(2..4), num(4..8))
            || date(num(2..4), num(0..2), num(4..8))
            || date(num(6..8), num(4..6), num(0..4)));
    repeated || run || dated
}

pub fn path(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(GhulError::InvalidInput { msg: "invalid quick unlock name".into() });
    }
    Ok(dir.join(format!("quick-{name}.ghlq")))
}

/// `P = Argon2id(NFC(PIN), salt)`: the input of the app PIN's Keystore keys.
fn pin_key(pin: &str, salt: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
    let pin: Zeroizing<String> = Zeroizing::new(pin.nfc().collect());
    crate::kdf::argon2id(pin.as_bytes(), salt, PIN_MEMORY_KIB, PIN_ITERATIONS, 1)
        .map_err(|msg| GhulError::InvalidInput { msg })
}

fn file_key(qk: &[u8], suite_id: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>> {
    if qk.len() != 32 {
        return Err(GhulError::QuickUnlockInvalid);
    }
    Ok(hkdf(qk, &[b"ghul/v1/quick", suite_id]))
}

/// Creates the quick unlock file and returns the quick key `QK` (32 bytes) for the platform to wrap.
const SECRET_LEN: usize = 32 + 32;

pub struct QuickSecret {
    pub password_key: Zeroizing<[u8; 32]>,
    /// The factor secret `S`.
    pub factor_secret: Zeroizing<[u8; 32]>,
}

/// Writes the quick unlock file of the fingerprint (or another name than the app PIN's) and
/// returns the new quick key. `max_age_ms`: how long after a full unlock it opens (stored in the
/// file).
pub fn create(
    dir: &Path,
    name: &str,
    suite_id: &[u8; 16],
    password_key: &[u8; 32],
    factor_secret: &[u8; 32],
    now_ms: i64,
    max_age_ms: i64,
) -> Result<Zeroizing<Vec<u8>>> {
    if name == PIN_NAME {
        return Err(GhulError::InvalidInput { msg: PIN_HAS_SLOTS.into() });
    }
    if max_age_ms <= 0 {
        return Err(GhulError::InvalidInput { msg: "quick unlock needs a validity".into() });
    }
    let mut qk = Zeroizing::new(vec![0u8; 32]);
    OsRng.fill_bytes(&mut qk);
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    let key = file_key(&qk, suite_id)?;

    let mut out = Vec::with_capacity(PREFIX_LEN + 24 + 80);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(suite_id);
    out.extend_from_slice(&salt);
    out.push(0);
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

/// Opens the quick unlock file of the fingerprint, within the validity stored in it. An expired
/// one is refused and kept (a full unlock makes it valid again). A PIN file of version 3 is
/// outdated (and deleted).
pub fn open_file(dir: &Path, name: &str, suite_id: &[u8; 16], qk: &[u8], now_ms: i64) -> Result<QuickSecret> {
    if name == PIN_NAME {
        return Err(GhulError::InvalidInput { msg: PIN_HAS_SLOTS.into() });
    }
    let p = path(dir, name)?;
    let data = fs::read(&p).map_err(|_| GhulError::QuickUnlockUnavailable)?;
    if outdated_pin_file(&data) {
        let _ = wipe(&p);
        let _ = fs::remove_file(stamp_path(dir, name));
        return Err(GhulError::QuickUnlockOutdated);
    }
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
    let has_pin = data[58] == 1;
    let created = i64::from_le_bytes(data[59..67].try_into().expect("8 bytes"));
    // Checked by the AEAD below (associated data): only trusted once it opened.
    let max_age_ms = i64::from_le_bytes(data[67..75].try_into().expect("8 bytes"));
    if has_pin {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockOutdated);
    }
    let key = file_key(qk, suite_id)?;
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

/// A PIN file of before version 4: never used again (see the module).
fn outdated_pin_file(data: &[u8]) -> bool {
    data.len() > 58 && &data[..8] == MAGIC && u16::from_le_bytes([data[8], data[9]]) == VERSION && data[58] == 1
}

/// The message when the app PIN is set up or used through the functions of the fingerprint.
pub const PIN_HAS_SLOTS: &str = "the app PIN has functions of its own";

// --- app PIN (version 4) --------------------------------------------------------------------------

/// What the header of the app PIN's file says (read before an attempt; the AEAD checks it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinHeader {
    pub salt: [u8; 32],
    pub created_ms: i64,
    pub max_age_ms: i64,
    pub generation: u32,
}

/// `T_i` for the key `K_i` and the input `P` (what the Keystore computes; here for setting up and
/// sealing anew, and for tests).
pub fn pin_tag(slot_key: &[u8; 32], input: &[u8; 32]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(slot_key).expect("any key size");
    mac.update(PIN_TAG_INFO);
    mac.update(input);
    mac.finalize().into_bytes().into()
}

/// What the app's Keystore key signs: [`PIN_TAG_INFO`] followed by the challenge `P`.
pub const PIN_TAG_INFO: &[u8] = b"ghul/v4/quick-pin-tag";

fn pin_slot_key(tag: &[u8; 32], suite_id: &[u8; 16], generation: u32, slot: u8) -> Zeroizing<[u8; 32]> {
    hkdf(tag, &[b"ghul/v4/quick-pin", suite_id, &generation.to_le_bytes(), &[slot]])
}

/// The challenge `P` of an attempt: Argon2id of the PIN with the salt of the file.
pub fn pin_challenge(pin: &str, header: &PinHeader) -> Result<Zeroizing<[u8; 32]>> {
    pin_key(pin, &header.salt)
}

/// A new salt and `P` for setting up the app PIN.
pub fn new_pin_input(pin: &str) -> Result<([u8; 32], Zeroizing<[u8; 32]>)> {
    check_pin(pin)?;
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    let p = pin_key(pin, &salt)?;
    Ok((salt, p))
}

/// Writes the app PIN's file: `PK || S` sealed once for each of the two keys (their tags over
/// `input`). Used to set it up and to seal it anew after a right PIN (then with the salt,
/// `created_ms` and `max_age_ms` of the file before: the validity is never extended). The copy
/// of the file before (`.bak`, `.tmp`) goes at once: with it and a key left over, an attempt
/// on the old generation would come back. Once the new file is in place the write counts as
/// done: a copy that cannot be removed is only reported (no `Err`), since the caller would
/// otherwise keep the keys of the old generation for a file of the new one and a right PIN
/// would cost the whole PIN; nothing reads such a copy, and the next write or turning off
/// removes it.
#[allow(clippy::too_many_arguments)]
pub fn write_pin(
    dir: &Path,
    suite_id: &[u8; 16],
    password_key: &[u8; 32],
    factor_secret: &[u8; 32],
    input: &[u8; 32],
    header: &PinHeader,
    keys: &[[u8; 32]; PIN_SLOTS],
) -> Result<()> {
    if header.max_age_ms <= 0 {
        return Err(GhulError::InvalidInput { msg: "quick unlock needs a validity".into() });
    }
    let mut out = Vec::with_capacity(PIN_PREFIX_LEN + PIN_SLOTS * PIN_SLOT_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&PIN_VERSION.to_le_bytes());
    out.extend_from_slice(suite_id);
    out.extend_from_slice(&header.salt);
    out.push(1);
    out.extend_from_slice(&header.created_ms.to_le_bytes());
    out.extend_from_slice(&header.max_age_ms.to_le_bytes());
    out.extend_from_slice(&header.generation.to_le_bytes());
    let prefix = out.clone();
    let mut secret = Zeroizing::new([0u8; SECRET_LEN]);
    secret[..32].copy_from_slice(password_key);
    secret[32..64].copy_from_slice(factor_secret);
    for (i, k) in keys.iter().enumerate() {
        let tag = Zeroizing::new(pin_tag(k, input));
        let key = pin_slot_key(&tag, suite_id, header.generation, i as u8);
        let mut aad = prefix.clone();
        aad.push(i as u8);
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&seal(&key, &nonce, secret.as_ref(), &aad));
    }
    fs::create_dir_all(dir)?;
    let file = format!("quick-{PIN_NAME}.ghlq");
    super::storage::write_atomic(dir, &file, &out)?;
    for left in [format!("{file}.bak"), format!("{file}.tmp")] {
        let p = dir.join(left);
        if p.exists() {
            #[cfg(test)]
            let res = if PIN_CLEANUP_FAILS.with(std::cell::Cell::get) {
                Err(std::io::Error::other("test: cannot remove"))
            } else {
                wipe(&p)
            };
            #[cfg(not(test))]
            let res = wipe(&p);
            if let Err(e) = res {
                eprintln!("GhulSuite: an old copy of the app PIN's file could not be removed yet: {e}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    // Tests: removing the copy before (`.bak`, `.tmp`) after the new file is in place fails.
    pub(crate) static PIN_CLEANUP_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The header of the app PIN's file. A PIN file of version 3 is outdated (and deleted); no file
/// (or one of another suite, damaged): unavailable; any other read error: `Storage`.
pub fn read_pin_header(dir: &Path, suite_id: &[u8; 16]) -> Result<PinHeader> {
    read_pin_file(dir, suite_id).map(|(_, h)| h)
}

/// The generation of the app PIN's file: `None` only when there is no such file. A read error
/// is `Storage`, an outdated file `QuickUnlockOutdated` (deleted), a damaged one or one of
/// another suite `QuickUnlockUnavailable` (deleted).
pub fn pin_generation(dir: &Path, suite_id: &[u8; 16]) -> Result<Option<u32>> {
    match fs::symlink_metadata(path(dir, PIN_NAME)?) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    read_pin_file(dir, suite_id).map(|(_, h)| Some(h.generation))
}

/// The app PIN's file, read once, and its checked header (see [`read_pin_header`]).
fn read_pin_file(dir: &Path, suite_id: &[u8; 16]) -> Result<(Vec<u8>, PinHeader)> {
    let p = path(dir, PIN_NAME)?;
    let data = match fs::read(&p) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GhulError::QuickUnlockUnavailable),
        Err(e) => return Err(e.into()),
    };
    if outdated_pin_file(&data)
        || (data.len() >= 10 && &data[..8] == MAGIC && u16::from_le_bytes([data[8], data[9]]) < VERSION)
    {
        let _ = wipe(&p);
        let _ = fs::remove_file(stamp_path(dir, PIN_NAME));
        return Err(GhulError::QuickUnlockOutdated);
    }
    if data.len() != PIN_PREFIX_LEN + PIN_SLOTS * PIN_SLOT_LEN
        || &data[..8] != MAGIC
        || u16::from_le_bytes([data[8], data[9]]) != PIN_VERSION
        || data[10..26] != suite_id[..]
        || data[58] != 1
    {
        let _ = wipe(&p);
        return Err(GhulError::QuickUnlockUnavailable);
    }
    let header = PinHeader {
        salt: data[26..58].try_into().expect("32 bytes"),
        created_ms: i64::from_le_bytes(data[59..67].try_into().expect("8 bytes")),
        max_age_ms: i64::from_le_bytes(data[67..75].try_into().expect("8 bytes")),
        generation: u32::from_le_bytes(data[75..79].try_into().expect("4 bytes")),
    };
    Ok((data, header))
}

/// Whether the app PIN would be expired now by its header and the (unchecked) time of the last
/// full unlock: asked before an attempt, so an expired PIN never costs a Keystore key. The
/// check after opening ([`open_pin`]) is the one that counts.
pub fn pin_expired_by_header(dir: &Path, header: &PinHeader, now_ms: i64) -> bool {
    let stamp = fs::read(stamp_path(dir, PIN_NAME))
        .ok()
        .filter(|d| d.len() == 40)
        .and_then(|d| d[..8].try_into().ok().map(i64::from_le_bytes));
    let last = stamp.map_or(header.created_ms, |t| t.max(header.created_ms));
    now_ms < last || now_ms - last > header.max_age_ms
}

/// Opens slot `slot` of the app PIN's file with the tag the Keystore computed. A wrong PIN (or
/// tag) is `QuickUnlockInvalid`, an expired one `QuickUnlockExpired` (the file stays as it is).
pub fn open_pin(
    dir: &Path,
    suite_id: &[u8; 16],
    slot: u32,
    tag: &[u8],
    now_ms: i64,
) -> Result<(QuickSecret, PinHeader)> {
    // Read once: header and slots come from the same bytes (whose length is checked).
    let (data, header) = read_pin_file(dir, suite_id)?;
    let tag: [u8; 32] = tag.try_into().map_err(|_| GhulError::InvalidInput { msg: "a tag has 32 bytes".into() })?;
    let slot = u8::try_from(slot)
        .ok()
        .filter(|s| (*s as usize) < PIN_SLOTS)
        .ok_or(GhulError::InvalidInput { msg: "no such slot".into() })?;
    let at = PIN_PREFIX_LEN + slot as usize * PIN_SLOT_LEN;
    let nonce: [u8; 24] = data[at..at + 24].try_into().expect("24 bytes");
    let mut aad = data[..PIN_PREFIX_LEN].to_vec();
    aad.push(slot);
    let key = pin_slot_key(&tag, suite_id, header.generation, slot);
    let plain = open(&key, &nonce, &data[at + 24..at + PIN_SLOT_LEN], &aad).ok_or(GhulError::QuickUnlockInvalid)?;
    let mut pk = Zeroizing::new([0u8; 32]);
    let mut secret = Zeroizing::new([0u8; 32]);
    pk.copy_from_slice(&plain[..32]);
    secret.copy_from_slice(&plain[32..64]);
    let last_full_unlock = read_stamp(dir, PIN_NAME, &pk).map_or(header.created_ms, |t| t.max(header.created_ms));
    if now_ms < last_full_unlock || now_ms - last_full_unlock > header.max_age_ms {
        return Err(GhulError::QuickUnlockExpired);
    }
    Ok((QuickSecret { password_key: pk, factor_secret: secret }, header))
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
