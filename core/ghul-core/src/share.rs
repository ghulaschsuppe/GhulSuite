// SPDX-License-Identifier: AGPL-3.0-or-later
//! Share packages: one file that a browser can decrypt with WebCrypto alone.
//!
//! ```text
//! header (48 bytes)
//!   "GHSHARE1" | version u8 = 1 | flags u8 (bit 0: password) | reserved u16 = 0
//!   chunk_size u32 BE | pbkdf2 iterations u32 BE | salt[16] | nonce_prefix[8] | meta_len u32 BE
//! chunk 0      AES-256-GCM(meta JSON)                     meta_len bytes
//! chunk 1..n   AES-256-GCM(chunk_size bytes of the file)  last one shorter
//!
//! nonce(i) = nonce_prefix ‖ u32 BE i
//! aad(i)   = header ‖ u8 (1 for the last chunk of the package, else 0); detects truncation
//! key      = HKDF-SHA256(ikm = link_key[32] (‖ PBKDF2-SHA256(password, salt, iterations, 32)),
//!                        salt = empty, info = "ghul/v1/share")
//! ```
//!
//! The link key travels only in the URL fragment (`#id.key`), which browsers never send to the
//! server. Every share gets a new random key, so a share never reveals anything about the vault
//! or other shares.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

pub const MAGIC: &[u8; 8] = b"GHSHARE1";
pub const HEADER_LEN: usize = 48;
pub const CHUNK_SIZE: u32 = 1024 * 1024;
pub const PBKDF2_ITERATIONS: u32 = 600_000;
/// Upper bound accepted when opening: a manipulated header must not stall the recipient.
pub const MAX_ITERATIONS: u32 = 10_000_000;
const INFO: &[u8] = b"ghul/v1/share";
const TAG: usize = 16;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShareError {
    #[error("not a share package")]
    Format,
    #[error("wrong key or password, or the package was changed")]
    Decrypt,
}

/// What the recipient sees before downloading.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ShareMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
    pub created: i64,
    /// Unix ms, 0 = no expiry. Enforced by the server module, by deleting the package, and by
    /// the viewer (the value is inside the encrypted package and cannot be changed).
    #[serde(default)]
    pub expires: i64,
}

/// A sealed package and the secret half of its link.
pub struct Sealed {
    pub data: Vec<u8>,
    pub link_key: Zeroizing<[u8; 32]>,
}

fn derive(link_key: &[u8; 32], password: Option<(&str, &[u8; 16], u32)>) -> Zeroizing<[u8; 32]> {
    let mut ikm = Zeroizing::new(link_key.to_vec());
    if let Some((pw, salt, iterations)) = password {
        let mut stretched = Zeroizing::new([0u8; 32]);
        pbkdf2::pbkdf2_hmac::<Sha256>(pw.as_bytes(), salt, iterations, stretched.as_mut());
        ikm.extend_from_slice(stretched.as_ref());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<Sha256>::new(None, &ikm).expand(INFO, key.as_mut()).expect("32 bytes is a valid HKDF length");
    key
}

fn nonce(prefix: &[u8], i: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(prefix);
    n[8..].copy_from_slice(&i.to_be_bytes());
    n
}

fn aad(header: &[u8], last: bool) -> Vec<u8> {
    let mut a = header.to_vec();
    a.push(u8::from(last));
    a
}

/// Encrypts `content` with its metadata. `password` adds a second secret besides the link.
pub fn seal(meta: &ShareMeta, content: &[u8], password: Option<&str>) -> Sealed {
    seal_with(meta, content, password, PBKDF2_ITERATIONS, CHUNK_SIZE)
}

pub(crate) fn seal_with(
    meta: &ShareMeta,
    content: &[u8],
    password: Option<&str>,
    iterations: u32,
    chunk: u32,
) -> Sealed {
    let mut link_key = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(link_key.as_mut());
    let mut salt = [0u8; 16];
    let mut prefix = [0u8; 8];
    OsRng.fill_bytes(&mut prefix);
    let pw = password.filter(|p| !p.is_empty());
    if pw.is_some() {
        OsRng.fill_bytes(&mut salt);
    }
    let meta_json = serde_json::to_vec(meta).expect("metadata serializes");
    let meta_len = (meta_json.len() + TAG) as u32;
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(1);
    header.push(u8::from(pw.is_some()));
    header.extend_from_slice(&[0, 0]);
    header.extend_from_slice(&chunk.to_be_bytes());
    header.extend_from_slice(&(if pw.is_some() { iterations } else { 0 }).to_be_bytes());
    header.extend_from_slice(&salt);
    header.extend_from_slice(&prefix);
    header.extend_from_slice(&meta_len.to_be_bytes());
    debug_assert_eq!(header.len(), HEADER_LEN);

    let key = derive(&link_key, pw.map(|p| (p, &salt, iterations)));
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("32-byte key");
    let chunks: Vec<&[u8]> = content.chunks(chunk as usize).collect();
    let mut out = Vec::with_capacity(HEADER_LEN + meta_json.len() + content.len() + TAG * (chunks.len() + 1));
    out.extend_from_slice(&header);
    let seal_chunk = |i: u32, plain: &[u8], last: bool| {
        cipher
            .encrypt(Nonce::from_slice(&nonce(&prefix, i)), Payload { msg: plain, aad: &aad(&header, last) })
            .expect("AES-GCM encryption cannot fail for these sizes")
    };
    out.extend_from_slice(&seal_chunk(0, &meta_json, chunks.is_empty()));
    for (i, c) in chunks.iter().enumerate() {
        out.extend_from_slice(&seal_chunk(i as u32 + 1, c, i + 1 == chunks.len()));
    }
    Sealed { data: out, link_key }
}

/// Decrypts a package (used by tests and to check uploads; the recipient uses the web page).
pub fn open(data: &[u8], link_key: &[u8; 32], password: Option<&str>) -> Result<(ShareMeta, Vec<u8>), ShareError> {
    if data.len() < HEADER_LEN || &data[..8] != MAGIC || data[8] != 1 {
        return Err(ShareError::Format);
    }
    let header = &data[..HEADER_LEN];
    let has_pw = data[9] & 1 == 1;
    let chunk = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
    let iterations = u32::from_be_bytes(header[16..20].try_into().unwrap());
    if has_pw && !(1..=MAX_ITERATIONS).contains(&iterations) {
        return Err(ShareError::Format);
    }
    let salt: [u8; 16] = header[20..36].try_into().unwrap();
    let prefix = &header[36..44];
    let meta_len = u32::from_be_bytes(header[44..48].try_into().unwrap()) as usize;
    if chunk == 0 || meta_len < TAG || HEADER_LEN + meta_len > data.len() {
        return Err(ShareError::Format);
    }
    let pw = if has_pw { Some((password.ok_or(ShareError::Decrypt)?, &salt, iterations)) } else { None };
    let key = derive(link_key, pw);
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("32-byte key");
    let body = &data[HEADER_LEN + meta_len..];
    let open_chunk = |i: u32, ct: &[u8], last: bool| {
        cipher
            .decrypt(Nonce::from_slice(&nonce(prefix, i)), Payload { msg: ct, aad: &aad(header, last) })
            .map_err(|_| ShareError::Decrypt)
    };
    let meta_plain = open_chunk(0, &data[HEADER_LEN..HEADER_LEN + meta_len], body.is_empty())?;
    let meta: ShareMeta = serde_json::from_slice(&meta_plain).map_err(|_| ShareError::Format)?;
    let mut content = Vec::with_capacity(body.len());
    let mut rest = body;
    let mut i = 1u32;
    while !rest.is_empty() {
        let take = (chunk + TAG).min(rest.len());
        if take <= TAG {
            return Err(ShareError::Format);
        }
        content.extend_from_slice(&open_chunk(i, &rest[..take], take == rest.len())?);
        rest = &rest[take..];
        i += 1;
    }
    Ok((meta, content))
}

/// URL-safe Base64 without padding (for the link key in the fragment).
pub fn b64url(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * k)) as usize & 63] as char);
        }
    }
    out
}

pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for ch in s.bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> ShareMeta {
        ShareMeta { name: "Steuer 2025.pdf".into(), mime: "application/pdf".into(), size: 0, created: 1, expires: 0 }
    }

    #[test]
    fn roundtrip_sizes_and_password() {
        for len in [0usize, 1, 99, 100, 101, 250] {
            let content: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let s = seal_with(&meta(), &content, None, 1000, 100);
            let (m, c) = open(&s.data, &s.link_key, None).unwrap();
            assert_eq!((m.name.as_str(), c), ("Steuer 2025.pdf", content.clone()));
            let p = seal_with(&meta(), &content, Some("geheim"), 1000, 100);
            assert_eq!(open(&p.data, &p.link_key, Some("geheim")).unwrap().1, content);
            assert_eq!(open(&p.data, &p.link_key, Some("falsch")).err(), Some(ShareError::Decrypt));
            assert_eq!(open(&p.data, &p.link_key, None).err(), Some(ShareError::Decrypt));
        }
    }

    #[test]
    fn tampering_and_truncation_are_detected() {
        let content = vec![7u8; 350];
        let s = seal_with(&meta(), &content, None, 1000, 100);
        let wrong = [9u8; 32];
        assert_eq!(open(&s.data, &wrong, None).err(), Some(ShareError::Decrypt));
        for pos in [0, 9, 13, 30, 50, 120, s.data.len() - 1] {
            let mut d = s.data.clone();
            d[pos] ^= 1;
            assert!(open(&d, &s.link_key, None).is_err(), "flip at {pos}");
        }
        // Cut off the last chunk: the previous one is not marked as last.
        let cut = s.data.len() - (50 + TAG);
        assert!(open(&s.data[..cut], &s.link_key, None).is_err());
        // Drop the whole data part.
        let meta_end = HEADER_LEN + u32::from_be_bytes(s.data[44..48].try_into().unwrap()) as usize;
        assert!(open(&s.data[..meta_end], &s.link_key, None).is_err());
    }

    /// Writes packages for the browser test of the share viewer (tools/test-share-viewer.js).
    #[test]
    fn viewer_fixture() {
        let Ok(dir) = std::env::var("GHUL_SHARE_FIXTURE") else { return };
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir.join("s")).unwrap();
        let content: Vec<u8> = (0..2_500_000u32).map(|i| (i % 253) as u8).collect();
        std::fs::write(dir.join("expected.bin"), &content).unwrap();
        let mut keys = String::new();
        for (n, pw, expires) in [("plain", None, 0), ("pw", Some("Grüße 123"), 0), ("expired", None, 1000)] {
            let m = ShareMeta {
                name: format!("{n}.bin"),
                mime: "application/octet-stream".into(),
                size: content.len() as u64,
                created: 0,
                expires,
            };
            let s = seal_with(&m, &content, pw, 10_000, CHUNK_SIZE);
            // The id is independent of the key (never derive it from the link key).
            let id = crate::kdf::hex(&rand_core::RngCore::next_u64(&mut rand_core::OsRng).to_be_bytes()).repeat(2);
            std::fs::write(dir.join("s").join(&id), &s.data).unwrap();
            keys.push_str(&format!("{n} {id}.{}\n", b64url(s.link_key.as_ref())));
        }
        std::fs::write(dir.join("links.txt"), keys).unwrap();
    }

    #[test]
    fn absurd_iterations_are_refused() {
        let m = ShareMeta { name: "a".into(), mime: "b".into(), size: 1, created: 0, expires: 0 };
        let s = seal_with(&m, b"x", Some("pw"), 1000, CHUNK_SIZE);
        let mut data = s.data.clone();
        data[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(open(&data, &s.link_key, Some("pw")).unwrap_err(), ShareError::Format);
        assert_eq!(open(&s.data, &s.link_key, Some("pw")).unwrap().1, b"x");
    }

    #[test]
    fn base64url() {
        for len in 0..40 {
            let d: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
            assert_eq!(b64url_decode(&b64url(&d)).unwrap(), d);
        }
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }
}
