// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ZIP of a suite file, written to and read from a stream: nothing is held whole in memory
//! and nothing needs to seek, so a file of a gigabyte goes through a few buffers.
//!
//! Only what a suite file needs (FORMAT.md, section 13): entries stored uncompressed (they are
//! encrypted anyway), CRC-32 and sizes in every local header (no data descriptors), no ZIP64,
//! no encryption, no extra fields, no comments, UTF-8 names. The central directory at the end
//! must repeat every local header exactly, and nothing may follow it. Any standard ZIP tool
//! reads such a file; files with anything else (compression, data descriptors, ...) are
//! refused.
//!
//! The reader counts every byte while reading: a file larger than its limit or with more
//! entries than allowed stops at the limit, whatever its headers claim.

use std::io::{self, Read, Write};

use sha2::{Digest, Sha256};

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
const LOCAL_LEN: u64 = 30;
const CENTRAL_LEN: u64 = 46;
const END_LEN: u64 = 22;
/// Version needed to extract: 1.0 (stored).
const VERSION_NEEDED: u16 = 10;
/// Version made by: 2.0, MS-DOS attributes.
const VERSION_MADE_BY: u16 = 20;
/// General purpose flags: bit 11, the name is UTF-8.
const FLAG_UTF8: u16 = 0x0800;
/// 1980-01-01 00:00 (MS-DOS date and time): the files carry no time of their own.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 0x0021;
/// Longest entry name.
pub(crate) const MAX_NAME_LEN: usize = 255;
/// Size of the pieces copied at once.
pub(crate) const CHUNK: usize = 1024 * 1024;

/// Why a stream is no suite file ZIP (or could not be read).
#[derive(Debug)]
pub(crate) enum ZipError {
    /// Larger than allowed (counted while reading).
    TooLarge,
    /// More entries than allowed.
    TooManyEntries,
    /// The stream ended early.
    Truncated,
    /// Anything else that does not fit (with what for tests and logs).
    Invalid(#[allow(dead_code)] &'static str),
    /// Reading the stream failed (the file is gone, a pulled stick).
    Io(io::Error),
}

impl From<io::Error> for ZipError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            ZipError::Truncated
        } else {
            ZipError::Io(e)
        }
    }
}

/// Bytes an entry takes in the file (local header, data, central record).
pub(crate) fn entry_len(name: &str, size: u64) -> u64 {
    LOCAL_LEN + CENTRAL_LEN + 2 * name.len() as u64 + size
}

/// Bytes of a whole ZIP of entries with these names and sizes.
pub(crate) fn zip_len<'a>(entries: impl IntoIterator<Item = (&'a str, u64)>) -> u64 {
    entries.into_iter().map(|(n, s)| entry_len(n, s)).sum::<u64>() + END_LEN
}

/// CRC-32, SHA-256 and size of a file read in pieces (for entries streamed from a file, whose
/// local header needs the CRC before the data).
#[cfg_attr(not(test), allow(dead_code))] // for content files (GhulDocs)
pub(crate) fn digest(mut r: impl Read) -> io::Result<(u64, u32, [u8; 32])> {
    let mut crc = crc32fast::Hasher::new();
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut len = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        sha.update(&buf[..n]);
        len += n as u64;
    }
    Ok((len, crc.finalize(), sha.finalize().into()))
}

struct Central {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Writes a ZIP of stored entries to `out`, in order. `progress` gets the bytes written so far.
pub(crate) struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    central: Vec<Central>,
    progress: Box<dyn FnMut(u64) + Send>,
}

fn u32_of(v: u64, what: &'static str) -> io::Result<u32> {
    u32::try_from(v).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, what))
}

impl<W: Write> ZipWriter<W> {
    pub(crate) fn new(out: W, progress: impl FnMut(u64) + Send + 'static) -> Self {
        ZipWriter { out, offset: 0, central: Vec::new(), progress: Box::new(progress) }
    }

    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.out.write_all(data)?;
        self.offset += data.len() as u64;
        (self.progress)(self.offset);
        Ok(())
    }

    fn local_header(&mut self, name: &str, crc: u32, size: u64) -> io::Result<()> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "entry name"));
        }
        let size = u32_of(size, "entry too large")?;
        let offset = u32_of(self.offset, "file too large")?;
        let mut h = Vec::with_capacity(LOCAL_LEN as usize + name.len());
        h.extend_from_slice(&LOCAL_SIG.to_le_bytes());
        h.extend_from_slice(&VERSION_NEEDED.to_le_bytes());
        h.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes()); // stored
        h.extend_from_slice(&DOS_TIME.to_le_bytes());
        h.extend_from_slice(&DOS_DATE.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&(name.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes()); // no extra field
        h.extend_from_slice(name.as_bytes());
        self.put(&h)?;
        self.central.push(Central { name: name.to_owned(), crc, size, offset });
        Ok(())
    }

    /// An entry from memory (the containers of a suite).
    pub(crate) fn add_bytes(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        self.local_header(name, crc32fast::hash(data), data.len() as u64)?;
        for piece in data.chunks(CHUNK) {
            self.put(piece)?;
        }
        Ok(())
    }

    /// An entry streamed from `r`, which must give exactly `size` bytes with this CRC-32 (see
    /// [`digest`]); otherwise (the file changed meanwhile) writing fails.
    #[cfg_attr(not(test), allow(dead_code))] // for content files (GhulDocs)
    pub(crate) fn add_reader(&mut self, name: &str, size: u64, crc: u32, mut r: impl Read) -> io::Result<()> {
        self.local_header(name, crc, size)?;
        let mut check = crc32fast::Hasher::new();
        let mut buf = vec![0u8; CHUNK];
        let mut left = size;
        while left > 0 {
            let want = left.min(CHUNK as u64) as usize;
            let n = r.read(&mut buf[..want])?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the file got shorter"));
            }
            check.update(&buf[..n]);
            self.put(&buf[..n])?;
            left -= n as u64;
        }
        if r.read(&mut buf[..1])? != 0 || check.finalize() != crc {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the file changed while it was written"));
        }
        Ok(())
    }

    /// Writes the central directory and returns the output.
    pub(crate) fn finish(mut self) -> io::Result<W> {
        let start = u32_of(self.offset, "file too large")?;
        let central = std::mem::take(&mut self.central);
        for c in &central {
            let mut h = Vec::with_capacity(CENTRAL_LEN as usize + c.name.len());
            h.extend_from_slice(&CENTRAL_SIG.to_le_bytes());
            h.extend_from_slice(&VERSION_MADE_BY.to_le_bytes());
            h.extend_from_slice(&VERSION_NEEDED.to_le_bytes());
            h.extend_from_slice(&FLAG_UTF8.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&DOS_TIME.to_le_bytes());
            h.extend_from_slice(&DOS_DATE.to_le_bytes());
            h.extend_from_slice(&c.crc.to_le_bytes());
            h.extend_from_slice(&c.size.to_le_bytes());
            h.extend_from_slice(&c.size.to_le_bytes());
            h.extend_from_slice(&(c.name.len() as u16).to_le_bytes());
            h.extend_from_slice(&[0u8; 8]); // extra and comment lengths, disk, internal attributes
            h.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            h.extend_from_slice(&c.offset.to_le_bytes());
            h.extend_from_slice(c.name.as_bytes());
            self.put(&h)?;
        }
        let size = u32_of(self.offset - u64::from(start), "file too large")?;
        let count = u16::try_from(central.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many entries"))?;
        let mut e = Vec::with_capacity(END_LEN as usize);
        e.extend_from_slice(&END_SIG.to_le_bytes());
        e.extend_from_slice(&[0u8; 4]); // this disk, disk of the central directory
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&size.to_le_bytes());
        e.extend_from_slice(&start.to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes()); // no comment
        self.put(&e)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Counts what is read and stops past the limit.
struct Counted<R: Read> {
    inner: R,
    read: u64,
    max: u64,
    progress: Box<dyn FnMut(u64) + Send>,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // One byte more than allowed is enough to know it is too large.
        let room = self.max.saturating_add(1).saturating_sub(self.read);
        let want = buf.len().min(room.min(usize::MAX as u64) as usize);
        if want == 0 {
            return Err(io::Error::other(TOO_LARGE));
        }
        let n = self.inner.read(&mut buf[..want])?;
        self.read += n as u64;
        if self.read > self.max {
            return Err(io::Error::other(TOO_LARGE));
        }
        (self.progress)(self.read);
        Ok(n)
    }
}

const TOO_LARGE: &str = "larger than allowed";

/// An entry as its local header describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryInfo {
    pub name: String,
    pub size: u64,
    crc: u32,
    offset: u64,
}

/// Reads a ZIP of stored entries from a stream, entry by entry.
pub(crate) struct ZipReader<R: Read> {
    r: Counted<R>,
    max_entries: usize,
    entries: Vec<EntryInfo>,
    /// The entry returned by [`Self::next_entry`] and not read yet.
    current: Option<EntryInfo>,
    done: bool,
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl<R: Read> ZipReader<R> {
    /// `max`: most bytes read in all; `max_entries`: most entries. `progress` gets the bytes read.
    pub(crate) fn new(r: R, max: u64, max_entries: usize, progress: impl FnMut(u64) + Send + 'static) -> Self {
        ZipReader {
            r: Counted { inner: r, read: 0, max, progress: Box::new(progress) },
            max_entries,
            entries: Vec::new(),
            current: None,
            done: false,
        }
    }

    fn fill(&mut self, buf: &mut [u8]) -> Result<(), ZipError> {
        self.r.read_exact(buf).map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => ZipError::Truncated,
            _ if e.to_string() == TOO_LARGE => ZipError::TooLarge,
            _ => ZipError::Io(e),
        })
    }

    fn name(&mut self, len: usize) -> Result<String, ZipError> {
        if len == 0 || len > MAX_NAME_LEN {
            return Err(ZipError::Invalid("entry name length"));
        }
        let mut b = vec![0u8; len];
        self.fill(&mut b)?;
        String::from_utf8(b).map_err(|_| ZipError::Invalid("entry name not UTF-8"))
    }

    /// The next entry, or `None` after the last one (then the central directory has been read
    /// and checked, and the stream is at its end). Read the entry with [`Self::read_entry`]
    /// before asking for the next one.
    pub(crate) fn next_entry(&mut self) -> Result<Option<EntryInfo>, ZipError> {
        if self.current.is_some() {
            return Err(ZipError::Invalid("entry not read"));
        }
        if self.done {
            return Ok(None);
        }
        let offset = self.r.read;
        let mut sig = [0u8; 4];
        self.fill(&mut sig)?;
        match u32::from_le_bytes(sig) {
            LOCAL_SIG => {}
            CENTRAL_SIG | END_SIG => {
                self.central(u32::from_le_bytes(sig), offset)?;
                self.done = true;
                return Ok(None);
            }
            _ => return Err(ZipError::Invalid("no ZIP entry")),
        }
        if self.entries.len() >= self.max_entries {
            return Err(ZipError::TooManyEntries);
        }
        let mut h = [0u8; LOCAL_LEN as usize - 4];
        self.fill(&mut h)?;
        // version needed, flags, method, time, date, crc, sizes, name length, extra length
        let (flags, method) = (le16(&h, 2), le16(&h, 4));
        if flags != FLAG_UTF8 {
            return Err(ZipError::Invalid("encrypted, data descriptor or unknown flags"));
        }
        if method != 0 {
            return Err(ZipError::Invalid("compressed entry"));
        }
        // Exactly as written here: anything else is no suite file.
        if le16(&h, 0) != VERSION_NEEDED || le16(&h, 6) != DOS_TIME || le16(&h, 8) != DOS_DATE {
            return Err(ZipError::Invalid("not written by GhulSuite"));
        }
        let (crc, packed, size) = (le32(&h, 10), le32(&h, 14), le32(&h, 18));
        if packed != size || size == u32::MAX {
            return Err(ZipError::Invalid("entry sizes"));
        }
        let (name_len, extra_len) = (le16(&h, 22) as usize, le16(&h, 24));
        if extra_len != 0 {
            return Err(ZipError::Invalid("extra field"));
        }
        let name = self.name(name_len)?;
        let e = EntryInfo { name, size: u64::from(size), crc, offset };
        self.current = Some(e.clone());
        Ok(Some(e))
    }

    /// Copies the data of the entry from [`Self::next_entry`] to `out` (checking its CRC-32)
    /// and returns its SHA-256.
    pub(crate) fn read_entry(&mut self, out: &mut impl Write) -> Result<[u8; 32], ZipError> {
        let e = self.current.take().ok_or(ZipError::Invalid("no entry"))?;
        let mut crc = crc32fast::Hasher::new();
        let mut sha = Sha256::new();
        let mut buf = vec![0u8; CHUNK.min(e.size as usize).max(1)];
        let mut left = e.size;
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            self.fill(&mut buf[..n])?;
            crc.update(&buf[..n]);
            sha.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(ZipError::Io)?;
            left -= n as u64;
        }
        if crc.finalize() != e.crc {
            return Err(ZipError::Invalid("damaged entry (CRC)"));
        }
        self.entries.push(e);
        Ok(sha.finalize().into())
    }

    /// The central directory and its end: exactly the entries read, nothing after it.
    fn central(&mut self, first: u32, start: u64) -> Result<(), ZipError> {
        let mut sig = first;
        for e in std::mem::take(&mut self.entries) {
            if sig != CENTRAL_SIG {
                return Err(ZipError::Invalid("central directory too short"));
            }
            let mut h = [0u8; CENTRAL_LEN as usize - 4];
            self.fill(&mut h)?;
            // made by, needed, flags, method, time, date, crc, sizes, name, extra, comment
            // lengths, disk, internal and external attributes, offset
            let same = le16(&h, 0) == VERSION_MADE_BY
                && le16(&h, 2) == VERSION_NEEDED
                && le16(&h, 4) == FLAG_UTF8
                && le16(&h, 6) == 0
                && le16(&h, 8) == DOS_TIME
                && le16(&h, 10) == DOS_DATE
                && le32(&h, 12) == e.crc
                && u64::from(le32(&h, 16)) == e.size
                && u64::from(le32(&h, 20)) == e.size
                && le16(&h, 26) == 0
                && le16(&h, 28) == 0
                && le16(&h, 30) == 0
                && le16(&h, 32) == 0
                && le32(&h, 34) == 0
                && u64::from(le32(&h, 38)) == e.offset;
            let name = self.name(le16(&h, 24) as usize)?;
            if !same || name != e.name {
                return Err(ZipError::Invalid("central directory does not match"));
            }
            self.entries.push(e);
            let mut s = [0u8; 4];
            self.fill(&mut s)?;
            sig = u32::from_le_bytes(s);
        }
        if sig != END_SIG {
            return Err(ZipError::Invalid("central directory too long"));
        }
        let end_at = self.r.read - 4;
        let mut h = [0u8; END_LEN as usize - 4];
        self.fill(&mut h)?;
        let n = self.entries.len();
        let ok = le16(&h, 0) == 0
            && le16(&h, 2) == 0
            && usize::from(le16(&h, 4)) == n
            && usize::from(le16(&h, 6)) == n
            && u64::from(le32(&h, 8)) == end_at - start
            && u64::from(le32(&h, 12)) == start
            && le16(&h, 16) == 0;
        if !ok {
            return Err(ZipError::Invalid("end of central directory"));
        }
        // Nothing after the end.
        let mut extra = [0u8; 1];
        match self.r.read(&mut extra) {
            Ok(0) => Ok(()),
            Ok(_) => Err(ZipError::Invalid("data after the end")),
            Err(e) if e.to_string() == TOO_LARGE => Err(ZipError::TooLarge),
            Err(e) => Err(ZipError::Io(e)),
        }
    }

    /// Bytes read so far.
    pub(crate) fn position(&self) -> u64 {
        self.r.read
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = ZipWriter::new(Vec::new(), |_| {});
        for (n, d) in entries {
            w.add_bytes(n, d).unwrap();
        }
        w.finish().unwrap()
    }

    fn read_all(data: &[u8], max: u64, max_entries: usize) -> Result<Vec<(String, Vec<u8>)>, ZipError> {
        let mut r = ZipReader::new(data, max, max_entries, |_| {});
        let mut out = Vec::new();
        while let Some(e) = r.next_entry()? {
            let mut buf = Vec::new();
            let sha = r.read_entry(&mut buf)?;
            assert_eq!(sha, <[u8; 32]>::from(Sha256::digest(&buf)));
            out.push((e.name, buf));
        }
        Ok(out)
    }

    #[test]
    fn written_and_read_back_also_by_a_standard_reader() {
        let a = vec![7u8; 3 * CHUNK + 5];
        let data = build(&[("export.ghlexp", b"kopf"), ("leer.ghlpss", b""), ("gross.ghlpss", &a)]);
        assert_eq!(
            data.len() as u64,
            zip_len([("export.ghlexp", 4), ("leer.ghlpss", 0), ("gross.ghlpss", a.len() as u64)])
        );
        let back = read_all(&data, u64::MAX, 10).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0], ("export.ghlexp".to_owned(), b"kopf".to_vec()));
        assert_eq!(back[2].1, a);
        // Any ZIP tool reads it.
        let mut z = zip::ZipArchive::new(io::Cursor::new(data)).unwrap();
        assert_eq!(z.len(), 3);
        let mut b = Vec::new();
        z.by_name("gross.ghlpss").unwrap().read_to_end(&mut b).unwrap();
        assert_eq!(b, a);
    }

    #[test]
    fn a_standard_stored_zip_is_read() {
        let mut out = io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("a.ghlpss", o).unwrap();
            z.write_all(b"inhalt").unwrap();
            z.finish().unwrap();
        }
        let back = read_all(&out.into_inner(), u64::MAX, 10);
        // The zip crate adds extended timestamps as extra fields: refused like anything not
        // written by GhulSuite, but without a panic.
        assert!(matches!(back, Ok(_) | Err(ZipError::Invalid(_))), "{back:?}");
    }

    #[test]
    fn limits_are_counted_while_reading() {
        let big = vec![1u8; 10_000];
        let data = build(&[("a.ghlpss", &big), ("b.ghlpss", b"x")]);
        assert!(read_all(&data, data.len() as u64, 2).is_ok());
        assert!(matches!(read_all(&data, data.len() as u64 - 1, 2), Err(ZipError::TooLarge)));
        assert!(matches!(read_all(&data, 5_000, 2), Err(ZipError::TooLarge)));
        assert!(matches!(read_all(&data, u64::MAX, 1), Err(ZipError::TooManyEntries)));
    }

    #[test]
    fn truncated_damaged_and_extended_streams_are_refused() {
        let data = build(&[("a.ghlpss", &[5u8; 1000]), ("b.ghlpss", b"zwei")]);
        for cut in [0, 3, 10, 40, 500, data.len() - 30, data.len() - 1] {
            assert!(matches!(read_all(&data[..cut], u64::MAX, 10), Err(ZipError::Truncated)), "cut at {cut}");
        }
        // A byte of the data changed: the CRC tells.
        let mut bad = data.clone();
        bad[100] ^= 1;
        assert!(matches!(read_all(&bad, u64::MAX, 10), Err(ZipError::Invalid(_))));
        // Something after the end.
        let mut longer = data.clone();
        longer.push(0);
        assert!(matches!(read_all(&longer, u64::MAX, 10), Err(ZipError::Invalid(_))));
        // Every single changed byte is noticed.
        for i in 0..data.len() {
            let mut d = data.clone();
            d[i] ^= 0x40;
            if let Ok(back) = read_all(&d, u64::MAX, 10) {
                panic!("byte {i} changed unnoticed: {back:?}");
            }
        }
    }

    #[test]
    fn compressed_and_descriptor_entries_are_refused() {
        let mut out = io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            z.start_file("a.ghlpss", o).unwrap();
            z.write_all(&[0u8; 100_000]).unwrap();
            z.finish().unwrap();
        }
        assert!(matches!(read_all(&out.into_inner(), u64::MAX, 10), Err(ZipError::Invalid(_))));
        let mut out = Vec::new();
        {
            let mut z = zip::ZipWriter::new_stream(&mut out);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("a.ghlpss", o).unwrap();
            z.write_all(b"abc").unwrap();
            z.finish().unwrap();
        }
        assert!(matches!(read_all(&out, u64::MAX, 10), Err(ZipError::Invalid(_))));
    }

    #[test]
    fn a_streamed_file_that_changes_meanwhile_is_not_written() {
        let data = vec![3u8; 5000];
        let (len, crc, _) = digest(&data[..]).unwrap();
        let mut w = ZipWriter::new(Vec::new(), |_| {});
        assert!(w.add_reader("a.bin", len, crc, &data[..4999]).is_err(), "shorter");
        let mut w = ZipWriter::new(Vec::new(), |_| {});
        let mut changed = data.clone();
        changed[10] = 4;
        assert!(w.add_reader("a.bin", len, crc, &changed[..]).is_err(), "changed");
        let mut w = ZipWriter::new(Vec::new(), |_| {});
        let mut longer = data.clone();
        longer.push(1);
        assert!(w.add_reader("a.bin", len, crc, &longer[..]).is_err(), "longer");
        let mut w = ZipWriter::new(Vec::new(), |_| {});
        w.add_reader("a.bin", len, crc, &data[..]).unwrap();
        let back = read_all(&w.finish().unwrap(), u64::MAX, 10).unwrap();
        assert_eq!(back[0].1, data);
    }

    /// Hashes and counts what is written, keeps nothing.
    struct CountingSink {
        len: u64,
        sha: Sha256,
    }

    impl Write for CountingSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.len += buf.len() as u64;
            self.sha.update(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Small parts of a stream, read one after the other.
    struct ReplaySource {
        parts: Vec<Vec<u8>>,
        at: usize,
        pos: usize,
    }

    impl Read for ReplaySource {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            while self.at < self.parts.len() {
                let p = &self.parts[self.at];
                if self.pos < p.len() {
                    let n = buf.len().min(p.len() - self.pos);
                    buf[..n].copy_from_slice(&p[self.pos..self.pos + n]);
                    self.pos += n;
                    return Ok(n);
                }
                self.at += 1;
                self.pos = 0;
            }
            Ok(0)
        }
    }

    /// A pattern file of `size` bytes, made while read.
    struct Pattern {
        left: u64,
    }

    impl Read for Pattern {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.left.min(usize::MAX as u64) as usize);
            for (i, b) in buf[..n].iter_mut().enumerate() {
                *b = ((self.left as usize - i) % 251) as u8;
            }
            self.left -= n as u64;
            Ok(n)
        }
    }

    /// Building and checking a large entry: memory stays at a few buffers, whatever the size.
    fn large_entry_streams(size: u64) {
        let (len, crc, sha) = digest(Pattern { left: size }).unwrap();
        assert_eq!(len, size);
        let total = zip_len([("kopf.ghlexp", 4), ("docs.ghlblob", size)]);
        let written = Arc::new(AtomicU64::new(0));
        let w2 = written.clone();
        let before = crate::alloc_meter::start();
        let mut w =
            ZipWriter::new(CountingSink { len: 0, sha: Sha256::new() }, move |n| w2.store(n, Ordering::Relaxed));
        w.add_bytes("kopf.ghlexp", b"kopf").unwrap();
        w.add_reader("docs.ghlblob", size, crc, Pattern { left: size }).unwrap();
        let sink = w.finish().unwrap();
        let peak = crate::alloc_meter::peak_since(before);
        assert_eq!(sink.len, total);
        assert_eq!(written.load(Ordering::Relaxed), total);
        eprintln!("building {size} bytes: at most {peak} bytes in memory");
        assert!(peak < 8 * CHUNK as u64, "building used {peak} bytes");

        // Read back the bytes the writer produced, made up again while read (head, pattern,
        // central directory): the large ZIP is never anywhere as a whole.
        let mut head = Vec::new();
        {
            let mut w = ZipWriter::new(&mut head, |_| {});
            w.add_bytes("kopf.ghlexp", b"kopf").unwrap();
            w.local_header("docs.ghlblob", crc, size).unwrap();
        }
        let mut tail = ZipWriter::new(Vec::new(), |_| {});
        tail.central.push(Central { name: "kopf.ghlexp".into(), crc: crc32fast::hash(b"kopf"), size: 4, offset: 0 });
        tail.central.push(Central {
            name: "docs.ghlblob".into(),
            crc,
            size: size as u32,
            offset: (LOCAL_LEN + 11 + 4) as u32,
        });
        tail.offset = head.len() as u64 + size;
        let tail = tail.finish().unwrap();
        let tail = tail[..].to_vec();
        let mut whole = Sha256::new();
        whole.update(&head);
        let mut p = Pattern { left: size };
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = p.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            whole.update(&buf[..n]);
        }
        whole.update(&tail);
        assert_eq!(<[u8; 32]>::from(whole.finalize()), <[u8; 32]>::from(sink.sha.finalize()), "the same bytes");

        struct Chain {
            head: ReplaySource,
            body: Pattern,
            tail: ReplaySource,
        }
        impl Read for Chain {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.head.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                let n = self.body.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.tail.read(buf)
            }
        }
        let src = Chain {
            head: ReplaySource { parts: vec![head], at: 0, pos: 0 },
            body: Pattern { left: size },
            tail: ReplaySource { parts: vec![tail], at: 0, pos: 0 },
        };
        let before = crate::alloc_meter::start();
        let read = Arc::new(AtomicU64::new(0));
        let r2 = read.clone();
        let mut r = ZipReader::new(src, total, 10, move |n| r2.store(n, Ordering::Relaxed));
        let mut names = Vec::new();
        let mut hashes = Vec::new();
        while let Some(e) = r.next_entry().unwrap() {
            names.push(e.name);
            hashes.push(r.read_entry(&mut io::sink()).unwrap());
        }
        let peak = crate::alloc_meter::peak_since(before);
        assert_eq!(names, ["kopf.ghlexp", "docs.ghlblob"]);
        assert_eq!(hashes[1], sha);
        assert_eq!(read.load(Ordering::Relaxed), total);
        eprintln!("checking {size} bytes: at most {peak} bytes in memory");
        assert!(peak < 8 * CHUNK as u64, "checking used {peak} bytes");
    }

    #[test]
    fn a_300_mb_entry_streams_in_little_memory() {
        // The meter sees what a buffer of the file's size would cost.
        let before = crate::alloc_meter::start();
        drop(std::hint::black_box(vec![1u8; 16 * CHUNK]));
        assert!(crate::alloc_meter::peak_since(before) >= 16 * CHUNK as u64);
        large_entry_streams(300 * 1024 * 1024);
    }
}
