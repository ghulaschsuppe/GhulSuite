// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content files of GhulDocs (`.ghlblob`, FORMAT.md section 14): the encrypted content of one
//! document, written and read as a stream.
//!
//! ```text
//! magic "GHULBLOB" | version u16 = 1 | id[16]                        prefix, 26 bytes
//! nonce[24] | XChaCha20-Poly1305(BK, section i) incl. tag[16]         section i = 0 .. n-1
//! zero bytes                                                          up to the trailer
//! nonce[24] | XChaCha20-Poly1305(BK, length u64) incl. tag[16]        trailer, 48 bytes
//! ```
//!
//! `BK = HKDF(key, "ghul/v1/docs/blob" | id)` with the document's own random key (it is only in
//! the index of its vault). Sections hold 4 MiB of content, the last one the rest (an empty
//! document has one empty section). Each section is bound to the prefix (magic, version, id),
//! its number and whether it is the last one; the trailer holds the length of the content and
//! is bound to the prefix. The whole file is a multiple of 64 KiB: the zero bytes before the
//! trailer fill it up. Readers take the length from the trailer, compute where everything must
//! be and check that the file is exactly that long and the filling is zero, so it carries
//! nothing; the SHA-256 of the whole file in the index covers it as well.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Tag, XChaCha20Poly1305, XNonce};
use rand_core::{CryptoRngCore, OsRng};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Magic of a content file.
pub(crate) const MAGIC: &[u8; 8] = b"GHULBLOB";
/// Format version of content files.
pub(crate) const VERSION: u16 = 1;
/// Content per section.
pub(crate) const SECTION: usize = 4 * 1024 * 1024;
/// Content files are a multiple of this size.
pub(crate) const PAD: u64 = 64 * 1024;
/// Magic, version and id.
pub(crate) const PREFIX_LEN: usize = 26;
/// Nonce and tag of a section.
const SECTION_OVERHEAD: u64 = 24 + 16;
/// Nonce, length and tag.
const TRAILER_LEN: u64 = 24 + 8 + 16;
const INFO_KEY: &[u8] = b"ghul/v1/docs/blob";
const AAD_END: &[u8] = b"end";
/// File name extension of content files.
pub(crate) const EXTENSION: &str = "ghlblob";

/// Why a content file could not be written or read.
#[derive(Debug)]
pub(crate) enum BlobError {
    /// Reading the content or writing or reading the content file failed.
    Io(io::Error),
    /// Writing the decrypted content to its destination failed.
    Out(io::Error),
    /// The content file is damaged or was changed (or is another one); what is wrong, for tests
    /// and debugging (the app only says that it is damaged).
    Damaged(#[allow(dead_code)] &'static str),
    /// The content is larger than allowed.
    TooLarge,
}

impl From<io::Error> for BlobError {
    fn from(e: io::Error) -> Self {
        BlobError::Io(e)
    }
}

/// What the index keeps of a content file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlobInfo {
    /// Bytes of content.
    pub size: u64,
    /// Bytes of the content file.
    pub len: u64,
    pub sha256: [u8; 32],
    pub crc32: u32,
}

/// Number of sections of a content of `size` bytes.
pub(crate) fn sections(size: u64) -> u64 {
    if size == 0 {
        1
    } else {
        size.div_ceil(SECTION as u64)
    }
}

/// Length of the content file of `size` bytes of content.
pub(crate) fn file_len(size: u64) -> u64 {
    let raw = PREFIX_LEN as u64 + sections(size) * SECTION_OVERHEAD + size + TRAILER_LEN;
    raw.div_ceil(PAD) * PAD
}

/// The name of a content file (32 hex digits of its id and the extension).
pub(crate) fn file_name(id: &[u8; 16]) -> String {
    format!("{}.{EXTENSION}", crate::kdf::hex(id))
}

fn prefix(id: &[u8; 16]) -> [u8; PREFIX_LEN] {
    let mut p = [0u8; PREFIX_LEN];
    p[..8].copy_from_slice(MAGIC);
    p[8..10].copy_from_slice(&VERSION.to_le_bytes());
    p[10..].copy_from_slice(id);
    p
}

fn section_aad(prefix: &[u8; PREFIX_LEN], index: u64, last: bool) -> [u8; PREFIX_LEN + 9] {
    let mut a = [0u8; PREFIX_LEN + 9];
    a[..PREFIX_LEN].copy_from_slice(prefix);
    a[PREFIX_LEN..PREFIX_LEN + 8].copy_from_slice(&index.to_le_bytes());
    a[PREFIX_LEN + 8] = u8::from(last);
    a
}

fn end_aad(prefix: &[u8; PREFIX_LEN]) -> Vec<u8> {
    [prefix.as_slice(), AAD_END].concat()
}

fn cipher(key: &[u8; 32], id: &[u8; 16]) -> XChaCha20Poly1305 {
    let bk = crate::vault::keys::hkdf(key, &[INFO_KEY, id]);
    XChaCha20Poly1305::new(bk.as_ref().into())
}

/// Hashes and counts what goes through.
struct Hashing<'a, W: Write> {
    out: &'a mut W,
    sha: Sha256,
    crc: crc32fast::Hasher,
    len: u64,
}

impl<W: Write> Hashing<'_, W> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)?;
        self.sha.update(data);
        self.crc.update(data);
        self.len += data.len() as u64;
        Ok(())
    }
}

/// Reads until `buf` is full or the content ends; returns the bytes read.
fn fill(src: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match src.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Encrypts the content read from `src` (at most `max` bytes, otherwise [`BlobError::TooLarge`])
/// into `out` as content file `id`. `progress` gets the bytes of content read so far.
pub(crate) fn encrypt(
    id: &[u8; 16],
    key: &[u8; 32],
    src: &mut impl Read,
    max: u64,
    out: &mut impl Write,
    rng: &mut impl CryptoRngCore,
    progress: &mut dyn FnMut(u64),
) -> Result<BlobInfo, BlobError> {
    let cipher = cipher(key, id);
    let prefix = prefix(id);
    let mut w = Hashing { out, sha: Sha256::new(), crc: crc32fast::Hasher::new(), len: 0 };
    w.put(&prefix)?;
    let mut buf = Zeroizing::new(vec![0u8; SECTION]);
    // One byte read ahead tells whether a full section is the last one.
    let mut ahead: Option<u8> = None;
    let mut size = 0u64;
    let mut index = 0u64;
    loop {
        let mut n = 0;
        if let Some(b) = ahead.take() {
            buf[0] = b;
            n = 1;
        }
        n += fill(src, &mut buf[n..])?;
        let last = if n < SECTION {
            true
        } else {
            let mut one = Zeroizing::new([0u8; 1]);
            if fill(src, one.as_mut())? == 0 {
                true
            } else {
                ahead = Some(one[0]);
                false
            }
        };
        size += n as u64;
        if size > max || (ahead.is_some() && size >= max) {
            return Err(BlobError::TooLarge);
        }
        let mut nonce = [0u8; 24];
        rng.fill_bytes(&mut nonce);
        let tag = cipher
            .encrypt_in_place_detached(XNonce::from_slice(&nonce), &section_aad(&prefix, index, last), &mut buf[..n])
            .map_err(|_| BlobError::TooLarge)?;
        w.put(&nonce)?;
        w.put(&buf[..n])?;
        w.put(&tag)?;
        progress(size);
        index += 1;
        if last {
            break;
        }
    }
    // Zero bytes up to the trailer, so the file is a multiple of 64 KiB.
    let mut fill_len = file_len(size) - w.len - TRAILER_LEN;
    let zeros = [0u8; 4096];
    while fill_len > 0 {
        let k = fill_len.min(zeros.len() as u64) as usize;
        w.put(&zeros[..k])?;
        fill_len -= k as u64;
    }
    let mut nonce = [0u8; 24];
    rng.fill_bytes(&mut nonce);
    let mut length = size.to_le_bytes();
    let tag = cipher
        .encrypt_in_place_detached(XNonce::from_slice(&nonce), &end_aad(&prefix), &mut length)
        .map_err(|_| BlobError::TooLarge)?;
    w.put(&nonce)?;
    w.put(&length)?;
    w.put(&tag)?;
    w.out.flush()?;
    debug_assert_eq!(w.len, file_len(size));
    Ok(BlobInfo { size, len: w.len, sha256: w.sha.finalize().into(), crc32: w.crc.finalize() })
}

/// Writes content file `id` into `dir` (a temporary file first, renamed when complete; removed
/// if anything fails). Only encrypted bytes reach the disk.
pub(crate) fn write_file(
    dir: &Path,
    id: &[u8; 16],
    key: &[u8; 32],
    src: &mut impl Read,
    max: u64,
    progress: &mut dyn FnMut(u64),
) -> Result<BlobInfo, BlobError> {
    fs::create_dir_all(dir)?;
    let name = file_name(id);
    let tmp = dir.join(format!("{name}.tmp"));
    let result = (|| {
        let mut w = BufWriter::with_capacity(64 * 1024, File::create(&tmp)?);
        let info = encrypt(id, key, src, max, &mut w, &mut OsRng, progress)?;
        let f = w.into_inner().map_err(|e| e.into_error())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, dir.join(&name))?;
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(info)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn read_exact_at(f: &mut (impl Read + Seek), at: u64, buf: &mut [u8]) -> Result<(), BlobError> {
    f.seek(SeekFrom::Start(at))?;
    f.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => BlobError::Damaged("cut off"),
        _ => BlobError::Io(e),
    })
}

/// Decrypts content file `id` with `size` bytes of content (as the index says) into `out`,
/// section by section: every section is checked before it is written. On an error, what was
/// written so far is incomplete and must be thrown away.
pub(crate) fn decrypt(
    f: &mut (impl Read + Seek),
    id: &[u8; 16],
    key: &[u8; 32],
    size: u64,
    out: &mut impl Write,
    progress: &mut dyn FnMut(u64),
) -> Result<(), BlobError> {
    let len = f.seek(SeekFrom::End(0))?;
    if len != file_len(size) {
        return Err(BlobError::Damaged("length"));
    }
    let mut p = [0u8; PREFIX_LEN];
    read_exact_at(f, 0, &mut p)?;
    let prefix = prefix(id);
    if p != prefix {
        return Err(BlobError::Damaged("not this content file"));
    }
    let cipher = cipher(key, id);
    // The trailer: the length of the content.
    let mut t = [0u8; TRAILER_LEN as usize];
    read_exact_at(f, len - TRAILER_LEN, &mut t)?;
    let (nonce, rest) = t.split_at_mut(24);
    let (length, tag) = rest.split_at_mut(8);
    cipher
        .decrypt_in_place_detached(XNonce::from_slice(nonce), &end_aad(&prefix), length, Tag::from_slice(tag))
        .map_err(|_| BlobError::Damaged("trailer"))?;
    if u64::from_le_bytes(length.try_into().unwrap_or_default()) != size {
        return Err(BlobError::Damaged("length"));
    }
    // The filling between the last section and the trailer: zero, nothing else.
    let n = sections(size);
    let end = PREFIX_LEN as u64 + n * SECTION_OVERHEAD + size;
    let mut zeros = vec![0u8; (len - TRAILER_LEN - end) as usize];
    read_exact_at(f, end, &mut zeros)?;
    if zeros.iter().any(|&b| b != 0) {
        return Err(BlobError::Damaged("filling"));
    }
    f.seek(SeekFrom::Start(PREFIX_LEN as u64))?;
    let mut buf = Zeroizing::new(vec![0u8; SECTION.min(size as usize)]);
    let mut done = 0u64;
    for index in 0..n {
        let k = (size - done).min(SECTION as u64) as usize;
        let mut nonce = [0u8; 24];
        let mut tag = [0u8; 16];
        let cut = |e: io::Error| match e.kind() {
            io::ErrorKind::UnexpectedEof => BlobError::Damaged("cut off"),
            _ => BlobError::Io(e),
        };
        f.read_exact(&mut nonce).map_err(cut)?;
        f.read_exact(&mut buf[..k]).map_err(cut)?;
        f.read_exact(&mut tag).map_err(cut)?;
        cipher
            .decrypt_in_place_detached(
                XNonce::from_slice(&nonce),
                &section_aad(&prefix, index, index + 1 == n),
                &mut buf[..k],
                Tag::from_slice(&tag),
            )
            .map_err(|_| BlobError::Damaged("section"))?;
        out.write_all(&buf[..k]).map_err(BlobError::Out)?;
        done += k as u64;
        progress(done);
    }
    out.flush().map_err(BlobError::Out)?;
    Ok(())
}

/// Opens content file `id` in `dir` and decrypts it into `out` (see [`decrypt`]); a missing
/// file is damage as well.
pub(crate) fn read_file(
    dir: &Path,
    id: &[u8; 16],
    key: &[u8; 32],
    size: u64,
    out: &mut impl Write,
    progress: &mut dyn FnMut(u64),
) -> Result<(), BlobError> {
    let mut f = match File::open(dir.join(file_name(id))) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(BlobError::Damaged("missing")),
        Err(e) => return Err(BlobError::Io(e)),
    };
    decrypt(&mut f, id, key, size, out, progress)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand_chacha::rand_core::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use serde_json::{json, Value as Json};
    use std::io::Cursor;

    /// Deterministic content: byte `i` is `i % 251`.
    pub(crate) fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn enc(id: &[u8; 16], key: &[u8; 32], content: &[u8], seed: u64) -> (Vec<u8>, BlobInfo) {
        let mut out = Vec::new();
        let info = encrypt(
            id,
            key,
            &mut Cursor::new(content),
            u64::MAX,
            &mut out,
            &mut ChaCha20Rng::seed_from_u64(seed),
            &mut |_| {},
        )
        .unwrap();
        (out, info)
    }

    fn dec(file: &[u8], id: &[u8; 16], key: &[u8; 32], size: u64) -> Result<Vec<u8>, BlobError> {
        let mut out = Vec::new();
        decrypt(&mut Cursor::new(file), id, key, size, &mut out, &mut |_| {})?;
        Ok(out)
    }

    const ID: [u8; 16] = [0xa5; 16];
    const KEY: [u8; 32] = [7; 32];

    #[test]
    fn round_trips_of_every_size_that_matters() {
        for size in [0, 1, 5, 65_000, SECTION - 1, SECTION, SECTION + 1, 3 * SECTION, 2 * SECTION + 12_345] {
            let content = pattern(size);
            let (file, info) = enc(&ID, &KEY, &content, size as u64);
            assert_eq!(file.len() as u64, file_len(size as u64), "{size}");
            assert_eq!(file.len() as u64 % PAD, 0);
            assert_eq!(info.len, file.len() as u64);
            assert_eq!(info.size, size as u64);
            assert_eq!(info.sha256, <[u8; 32]>::from(Sha256::digest(&file)));
            assert_eq!(info.crc32, crc32fast::hash(&file));
            assert_eq!(dec(&file, &ID, &KEY, size as u64).unwrap(), content, "{size}");
        }
    }

    #[test]
    fn the_layout_is_exact() {
        assert_eq!(sections(0), 1);
        assert_eq!(sections(SECTION as u64), 1);
        assert_eq!(sections(SECTION as u64 + 1), 2);
        assert_eq!(file_len(0), PAD);
        // 26 + 40 + 48 = 114 bytes of overhead: the content that still fits into 64 KiB.
        assert_eq!(file_len(PAD - 114), PAD);
        assert_eq!(file_len(PAD - 113), 2 * PAD);
        // The largest document: 1 GB.
        let gb = 1_000_000_000u64;
        assert_eq!(sections(gb), 239);
        assert!(file_len(gb) - gb < PAD + 239 * 40 + 114);
    }

    #[test]
    fn too_much_content_is_refused() {
        let content = pattern(SECTION + 10);
        let mut out = Vec::new();
        let r = encrypt(&ID, &KEY, &mut Cursor::new(&content), SECTION as u64 + 9, &mut out, &mut OsRng, &mut |_| {});
        assert!(matches!(r, Err(BlobError::TooLarge)));
        let r = encrypt(&ID, &KEY, &mut Cursor::new(&content[..10]), 9, &mut Vec::new(), &mut OsRng, &mut |_| {});
        assert!(matches!(r, Err(BlobError::TooLarge)));
        // Exactly as much as allowed is fine, also at a section boundary.
        let r = encrypt(
            &ID,
            &KEY,
            &mut Cursor::new(&content[..SECTION]),
            SECTION as u64,
            &mut Vec::new(),
            &mut OsRng,
            &mut |_| {},
        );
        assert!(r.is_ok());
    }

    fn damaged(file: &[u8], size: u64) -> bool {
        matches!(dec(file, &ID, &KEY, size), Err(BlobError::Damaged(_)))
    }

    /// Every change to a content file is noticed: a flipped byte anywhere (each section, the
    /// filling, the trailer, the prefix), sections swapped, cut off, bytes appended, the file
    /// of another id or another document, another key, a wrong length.
    #[test]
    fn every_change_is_noticed() {
        let size = 2 * SECTION as u64 + 1000;
        let content = pattern(size as usize);
        let (file, _) = enc(&ID, &KEY, &content, 1);
        let s = SECTION as u64 + SECTION_OVERHEAD;
        let at = |section: u64, offset: u64| (PREFIX_LEN as u64 + section * s + offset) as usize;
        for pos in [0, 9, 12, at(0, 0), at(0, 30), at(1, 24), at(1, s - 1), at(2, 100), at(2, 24 + 1000 + 16 + 5)] {
            let mut bad = file.clone();
            bad[pos] ^= 1;
            assert!(damaged(&bad, size), "flipped byte at {pos}");
        }
        let mut bad = file.clone();
        let n = bad.len();
        bad[n - 1] ^= 0x80;
        assert!(damaged(&bad, size), "trailer");
        // Sections 0 and 1 swapped.
        let mut swapped = file[..PREFIX_LEN].to_vec();
        swapped.extend_from_slice(&file[at(1, 0)..at(2, 0)]);
        swapped.extend_from_slice(&file[at(0, 0)..at(1, 0)]);
        swapped.extend_from_slice(&file[at(2, 0)..]);
        assert_eq!(swapped.len(), file.len());
        assert!(damaged(&swapped, size));
        // Cut off (also at a 64 KiB boundary, also the last section with the trailer moved).
        assert!(damaged(&file[..file.len() - 1], size));
        assert!(damaged(&file[..file.len() - PAD as usize], size));
        let mut cut = file[..at(2, 0)].to_vec();
        cut.resize(file_len(2 * SECTION as u64) as usize - TRAILER_LEN as usize, 0);
        cut.extend_from_slice(&file[file.len() - TRAILER_LEN as usize..]);
        assert!(damaged(&cut, 2 * SECTION as u64), "a shorter document from the same file");
        // Appended.
        let mut long = file.clone();
        long.extend_from_slice(&[0u8; PAD as usize]);
        assert!(damaged(&long, size));
        // Another id: the same bytes under another name, or another document's file.
        assert!(!damaged(&file, size));
        assert!(matches!(dec(&file, &[1; 16], &KEY, size), Err(BlobError::Damaged(_))));
        let mut renamed = file.clone();
        renamed[10..26].copy_from_slice(&[1; 16]);
        assert!(matches!(dec(&renamed, &[1; 16], &KEY, size), Err(BlobError::Damaged(_))));
        let (other, _) = enc(&[1; 16], &KEY, &content, 2);
        assert!(damaged(&other, size));
        let (other_same_id, _) = enc(&ID, &[8; 32], &content, 3);
        assert!(damaged(&other_same_id, size), "another key");
        assert!(damaged(&file, size - 1), "the index says another length");
        // A changed filling byte (the file is otherwise intact).
        let small = pattern(10);
        let (sfile, _) = enc(&ID, &KEY, &small, 4);
        let mut bad = sfile.clone();
        bad[PREFIX_LEN + 40 + 20] = 1;
        assert!(damaged(&bad, 10), "filling");
    }

    /// A section that fails stops before it is written: what came before is all there is.
    #[test]
    fn nothing_after_a_damaged_section_is_written() {
        let size = SECTION as u64 + 10;
        let (mut file, _) = enc(&ID, &KEY, &pattern(size as usize), 5);
        file[PREFIX_LEN + SECTION + 40 + 30] ^= 1;
        let mut out = Vec::new();
        let r = decrypt(&mut Cursor::new(&file), &ID, &KEY, size, &mut out, &mut |_| {});
        assert!(matches!(r, Err(BlobError::Damaged("section"))));
        assert_eq!(out.len(), SECTION);
    }

    // --- test vectors (docs/testvectors/docs-blob-v1.json) ----------------------------------

    const VECTOR_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/testvectors/docs-blob-v1.json");

    /// The document of the vector shared with the GhulDocs vault in `container-v3.json`.
    pub(crate) fn hello_vector() -> ([u8; 16], [u8; 32], Vec<u8>, Vec<u8>, BlobInfo) {
        let id: [u8; 16] = std::array::from_fn(|i| 0xd0 + i as u8);
        let key: [u8; 32] = std::array::from_fn(|i| i as u8);
        let content = "Grüße aus GhulDocs: Steuerbescheid 2025.pdf".as_bytes().to_vec();
        let (file, info) = enc(&id, &key, &content, 2027);
        (id, key, content, file, info)
    }

    fn vectors() -> Json {
        let mut out = Vec::new();
        let (id, key, content, file, info) = hello_vector();
        let one = |description: &str,
                   id: &[u8; 16],
                   key: &[u8; 32],
                   content: &[u8],
                   file: &[u8],
                   info: &BlobInfo,
                   full: bool| {
            let small = content.len() <= 1024;
            let bk = crate::vault::keys::hkdf(key, &[INFO_KEY, id]);
            let n = sections(info.size);
            let mut nonces = Vec::new();
            let mut at = PREFIX_LEN;
            let mut left = info.size;
            for _ in 0..n {
                nonces.push(crate::kdf::hex(&file[at..at + 24]));
                let k = left.min(SECTION as u64);
                at += 40 + k as usize;
                left -= k;
            }
            let t = file.len() - TRAILER_LEN as usize;
            json!({
                "description": description,
                "id": crate::kdf::hex(id),
                "key": crate::kdf::hex(key),
                "blob_key": crate::kdf::hex(bk.as_ref()),
                "size": info.size,
                "content": if small { Json::String(crate::kdf::hex(content)) } else { Json::Null },
                "content_pattern": if small { Json::Null } else { json!("byte i = i mod 251") },
                "section_nonces": nonces,
                "trailer_nonce": crate::kdf::hex(&file[t..t + 24]),
                "file_len": file.len(),
                "file_sha256": crate::kdf::hex(&info.sha256),
                "file_crc32": info.crc32,
                "file": if full { Json::String(crate::kdf::hex(file)) } else { Json::Null },
            })
        };
        out.push(one(
            "a short text, the document of the GhulDocs vault in container-v3.json",
            &id,
            &key,
            &content,
            &file,
            &info,
            true,
        ));
        let (eid, ekey) = ([0x11; 16], [0x22; 32]);
        let (efile, einfo) = enc(&eid, &ekey, &[], 2028);
        out.push(one(
            "an empty document: one empty section (file given by its nonces and SHA-256)",
            &eid,
            &ekey,
            &[],
            &efile,
            &einfo,
            false,
        ));
        let big = pattern(SECTION + 5);
        let (bid, bkey) = ([0x33; 16], [0x44; 32]);
        let (bfile, binfo) = enc(&bid, &bkey, &big, 2029);
        out.push(one(
            "4 MiB + 5 bytes: two sections (file given by its nonces and SHA-256)",
            &bid,
            &bkey,
            &big,
            &bfile,
            &binfo,
            false,
        ));
        json!({
            "format": "GhulDocs content file (.ghlblob) v1",
            "note": "Generated by ghul-core (app::docs::blob::tests::test_vectors).",
            "vectors": out,
        })
    }

    #[test]
    fn test_vectors() {
        let generated = vectors();
        if std::env::var("GHUL_WRITE_VECTORS").is_ok() {
            std::fs::write(VECTOR_FILE, serde_json::to_string_pretty(&generated).unwrap() + "\n").unwrap();
        }
        let published: Json = serde_json::from_str(
            &std::fs::read_to_string(VECTOR_FILE)
                .unwrap_or_else(|_| panic!("{VECTOR_FILE} missing; run with GHUL_WRITE_VECTORS=1")),
        )
        .unwrap();
        assert_eq!(generated, published, "implementation no longer matches the published content file vectors");
        // The published files open from their inputs alone.
        let unhex = |s: &str| {
            (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<u8>>()
        };
        for v in published["vectors"].as_array().unwrap() {
            let Some(file) = v["file"].as_str() else { continue };
            let id: [u8; 16] = unhex(v["id"].as_str().unwrap()).try_into().unwrap();
            let key: [u8; 32] = unhex(v["key"].as_str().unwrap()).try_into().unwrap();
            let content = dec(&unhex(file), &id, &key, v["size"].as_u64().unwrap()).unwrap();
            assert_eq!(crate::kdf::hex(&content), v["content"].as_str().unwrap());
        }
    }
}
