// SPDX-License-Identifier: AGPL-3.0-or-later
//! Binary layout of GhulSuite containers (format version 1). See `docs/FORMAT.md`.
//!
//! ```text
//! magic[8] | version u16 | header_len u32 | header[header_len] | payload_nonce[24] | payload_ciphertext
//! ```
//! All integers are little-endian. The header is a sequence of TLV records
//! (`tag u16 | len u32 | value`) in ascending tag order.

use super::error::{Result, VaultError};

pub const FORMAT_VERSION: u16 = 1;
pub const PREFIX_LEN: usize = 14;
pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;
pub const MAX_HEADER_LEN: usize = 64 * 1024;
pub const MAX_FILE_LEN: usize = 256 * 1024 * 1024;
pub const MAX_SLOTS: usize = 32;
pub const MAX_RP_ID_LEN: usize = 255;
pub const MAX_CREDENTIAL_ID_LEN: usize = 1023;

/// Argon2id limits. The minimum protects against downgraded parameters, the maximum against
/// files that would exhaust memory or time when opened.
pub const MIN_MEMORY_KIB: u32 = 64 * 1024;
pub const MAX_MEMORY_KIB: u32 = 2 * 1024 * 1024;
pub const MIN_ITERATIONS: u32 = 3;
pub const MAX_ITERATIONS: u32 = 64;
pub const MIN_PARALLELISM: u32 = 1;
pub const MAX_PARALLELISM: u32 = 16;

pub const KDF_ARGON2ID_V13: u8 = 1;
pub const CIPHER_XCHACHA20_POLY1305: u8 = 1;

pub const SLOT_FIDO2_HMAC_SECRET: u8 = 1;
pub const SLOT_RECOVERY_CODE: u8 = 2;
pub const SLOT_KEYFILE: u8 = 3;
pub const SLOT_OPTION_UV_REQUIRED: u8 = 0x01;

const TAG_CONTAINER_ID: u16 = 0x0001;
const TAG_SUITE_ID: u16 = 0x0002;
const TAG_KDF: u16 = 0x0003;
const TAG_CIPHER: u16 = 0x0004;
const TAG_FLAGS: u16 = 0x0005;
const TAG_SLOT: u16 = 0x0010;
const TAG_VMK: u16 = 0x0020;
/// Tags with this bit set are optional: readers that do not know them skip them.
const TAG_OPTIONAL_BIT: u16 = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerKind {
    Pass,
    Totp,
    Suite,
    /// GhulCal calendar vault.
    Cal,
    /// GhulDrive: Drive key, storage and settings.
    Drive,
    /// GhulMail: accounts, keys, drafts and settings.
    Mail,
}

impl ContainerKind {
    pub fn magic(self) -> &'static [u8; 8] {
        match self {
            ContainerKind::Pass => b"GHULPASS",
            ContainerKind::Totp => b"GHULTOTP",
            ContainerKind::Suite => b"GHULSUIT",
            ContainerKind::Cal => b"GHULCLDR",
            ContainerKind::Drive => b"GHULDRIV",
            ContainerKind::Mail => b"GHULMAIL",
        }
    }

    pub fn from_magic(magic: &[u8]) -> Option<Self> {
        [
            ContainerKind::Pass,
            ContainerKind::Totp,
            ContainerKind::Suite,
            ContainerKind::Cal,
            ContainerKind::Drive,
            ContainerKind::Mail,
        ]
        .into_iter()
        .find(|k| k.magic().as_slice() == magic)
    }

    /// Value of the `kind` field in the payload.
    pub fn name(self) -> &'static str {
        match self {
            ContainerKind::Pass => "pass",
            ContainerKind::Totp => "totp",
            ContainerKind::Suite => "suite",
            ContainerKind::Cal => "cal",
            ContainerKind::Drive => "drive",
            ContainerKind::Mail => "mail",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ContainerKind::Pass => "ghlpss",
            ContainerKind::Totp => "ghltotp",
            ContainerKind::Suite => "ghlsuite",
            ContainerKind::Cal => "ghlcal",
            ContainerKind::Drive => "ghldrv",
            ContainerKind::Mail => "ghlmail",
        }
    }
}

/// Argon2id parameters and salt (shared by all containers of a suite by default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: [u8; 32],
}

impl KdfParams {
    pub fn validate(&self) -> Result<()> {
        if !(MIN_MEMORY_KIB..=MAX_MEMORY_KIB).contains(&self.memory_kib)
            || !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&self.iterations)
            || !(MIN_PARALLELISM..=MAX_PARALLELISM).contains(&self.parallelism)
        {
            return Err(VaultError::format("Argon2id parameters out of range"));
        }
        Ok(())
    }

    /// Same cost and salt, i.e. the same password key.
    pub fn same_derivation(&self, other: &KdfParams) -> bool {
        self == other
    }
}

/// AEAD-wrapped 32-byte secret: 24-byte nonce and 48-byte ciphertext (incl. tag).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrapped {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: [u8; 32 + TAG_LEN],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fido2Slot {
    pub slot_id: [u8; 16],
    pub rp_id: String,
    pub credential_id: Vec<u8>,
    /// PRF input; the hmac-secret salt is `SHA-256("WebAuthn PRF" || 0x00 || prf_input)`.
    pub prf_input: [u8; 32],
    pub uv_required: bool,
    pub wrapped: Wrapped,
}

/// Recovery code slot: wraps the container key (VMK) directly, so the code alone opens the
/// container (and, in the suite, the keyring with all vault keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoverySlot {
    pub slot_id: [u8; 16],
    pub wrapped: Wrapped,
}

/// Key file slot: wraps the factor secret `S` with a key derived from a `.ghlkey` file, the
/// alternative second factor to a security key. `keyfile_id` only tells which file is meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyfileSlot {
    pub slot_id: [u8; 16],
    pub keyfile_id: [u8; 8],
    pub wrapped: Wrapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Fido2(Fido2Slot),
    Recovery(RecoverySlot),
    Keyfile(KeyfileSlot),
}

impl Slot {
    pub fn id(&self) -> &[u8; 16] {
        match self {
            Slot::Fido2(s) => &s.slot_id,
            Slot::Recovery(s) => &s.slot_id,
            Slot::Keyfile(s) => &s.slot_id,
        }
    }

    /// Whether the slot yields the factor secret `S` (all except recovery slots).
    pub fn provides_factor(&self) -> bool {
        !matches!(self, Slot::Recovery(_))
    }

    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Slot::Fido2(s) => {
                out.push(SLOT_FIDO2_HMAC_SECRET);
                out.extend_from_slice(&s.slot_id);
                out.push(s.rp_id.len() as u8);
                out.extend_from_slice(s.rp_id.as_bytes());
                out.extend_from_slice(&(s.credential_id.len() as u16).to_le_bytes());
                out.extend_from_slice(&s.credential_id);
                out.extend_from_slice(&s.prf_input);
                out.push(if s.uv_required { SLOT_OPTION_UV_REQUIRED } else { 0 });
                encode_wrapped(&s.wrapped, out);
            }
            Slot::Recovery(s) => {
                out.push(SLOT_RECOVERY_CODE);
                out.extend_from_slice(&s.slot_id);
                encode_wrapped(&s.wrapped, out);
            }
            Slot::Keyfile(s) => {
                out.push(SLOT_KEYFILE);
                out.extend_from_slice(&s.slot_id);
                out.extend_from_slice(&s.keyfile_id);
                encode_wrapped(&s.wrapped, out);
            }
        }
    }

    fn parse(value: &[u8]) -> Result<Slot> {
        let mut r = Reader::new(value);
        let slot = match r.u8()? {
            SLOT_FIDO2_HMAC_SECRET => {
                let slot_id = r.array::<16>()?;
                let rp_len = r.u8()? as usize;
                if rp_len == 0 {
                    return Err(VaultError::format("empty RP ID"));
                }
                let rp_id = std::str::from_utf8(r.bytes(rp_len)?)
                    .map_err(|_| VaultError::format("RP ID is not UTF-8"))?
                    .to_owned();
                let cred_len = r.u16()? as usize;
                if cred_len == 0 || cred_len > MAX_CREDENTIAL_ID_LEN {
                    return Err(VaultError::format("invalid credential ID length"));
                }
                let credential_id = r.bytes(cred_len)?.to_vec();
                let prf_input = r.array::<32>()?;
                let options = r.u8()?;
                if options & !SLOT_OPTION_UV_REQUIRED != 0 {
                    return Err(VaultError::format("unknown slot options"));
                }
                if options & SLOT_OPTION_UV_REQUIRED == 0 {
                    return Err(VaultError::format("FIDO2 slots without PIN verification are not allowed"));
                }
                let wrapped = parse_wrapped(&mut r)?;
                Slot::Fido2(Fido2Slot { slot_id, rp_id, credential_id, prf_input, uv_required: true, wrapped })
            }
            SLOT_RECOVERY_CODE => {
                let slot_id = r.array::<16>()?;
                let wrapped = parse_wrapped(&mut r)?;
                Slot::Recovery(RecoverySlot { slot_id, wrapped })
            }
            SLOT_KEYFILE => {
                let slot_id = r.array::<16>()?;
                let keyfile_id = r.array::<8>()?;
                let wrapped = parse_wrapped(&mut r)?;
                Slot::Keyfile(KeyfileSlot { slot_id, keyfile_id, wrapped })
            }
            other => return Err(VaultError::format(format!("unknown slot type {other}"))),
        };
        r.finish()?;
        Ok(slot)
    }
}

fn encode_wrapped(w: &Wrapped, out: &mut Vec<u8>) {
    out.extend_from_slice(&w.nonce);
    out.extend_from_slice(&w.ciphertext);
}

fn parse_wrapped(r: &mut Reader<'_>) -> Result<Wrapped> {
    Ok(Wrapped { nonce: r.array()?, ciphertext: r.array()? })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub container_id: [u8; 16],
    pub suite_id: [u8; 16],
    pub kdf: KdfParams,
    pub slots: Vec<Slot>,
    pub vmk: Wrapped,
}

impl Header {
    pub fn validate(&self) -> Result<()> {
        self.kdf.validate()?;
        if self.slots.is_empty() || self.slots.len() > MAX_SLOTS {
            return Err(VaultError::format("invalid number of slots"));
        }
        for (i, s) in self.slots.iter().enumerate() {
            if self.slots[..i].iter().any(|o| o.id() == s.id()) {
                return Err(VaultError::format("duplicate slot ID"));
            }
            if let Slot::Fido2(f) = s {
                if f.rp_id.is_empty() || f.rp_id.len() > MAX_RP_ID_LEN {
                    return Err(VaultError::format("invalid RP ID length"));
                }
                if f.credential_id.is_empty() || f.credential_id.len() > MAX_CREDENTIAL_ID_LEN {
                    return Err(VaultError::format("invalid credential ID length"));
                }
                if !f.uv_required {
                    return Err(VaultError::format("FIDO2 slots without PIN verification are not allowed"));
                }
            }
        }
        // A second factor (security key or key file) is always required.
        if !self.slots.iter().any(Slot::provides_factor) {
            return Err(VaultError::format("no slot provides the factor secret"));
        }
        Ok(())
    }

    /// Encodes the TLV records in canonical order.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::with_capacity(256 + 128 * self.slots.len());
        record(&mut out, TAG_CONTAINER_ID, &self.container_id);
        record(&mut out, TAG_SUITE_ID, &self.suite_id);
        let mut kdf = Vec::with_capacity(45);
        kdf.push(KDF_ARGON2ID_V13);
        kdf.extend_from_slice(&self.kdf.memory_kib.to_le_bytes());
        kdf.extend_from_slice(&self.kdf.iterations.to_le_bytes());
        kdf.extend_from_slice(&self.kdf.parallelism.to_le_bytes());
        kdf.extend_from_slice(&self.kdf.salt);
        record(&mut out, TAG_KDF, &kdf);
        record(&mut out, TAG_CIPHER, &[CIPHER_XCHACHA20_POLY1305]);
        // No flags are defined yet; the record is kept for future use.
        record(&mut out, TAG_FLAGS, &0u32.to_le_bytes());
        for slot in &self.slots {
            let mut v = Vec::with_capacity(160);
            slot.encode(&mut v);
            record(&mut out, TAG_SLOT, &v);
        }
        let mut vmk = Vec::with_capacity(72);
        encode_wrapped(&self.vmk, &mut vmk);
        record(&mut out, TAG_VMK, &vmk);
        if out.len() > MAX_HEADER_LEN {
            return Err(VaultError::format("header too large"));
        }
        Ok(out)
    }

    pub fn parse(data: &[u8]) -> Result<Header> {
        let mut r = Reader::new(data);
        let mut last_tag = 0u16;
        let (mut container_id, mut suite_id, mut kdf, mut cipher, mut flags, mut vmk) =
            (None, None, None, None, None, None);
        let mut slots = Vec::new();

        while !r.is_empty() {
            let tag = r.u16()?;
            let len = r.u32()? as usize;
            let value = r.bytes(len)?;
            if tag < last_tag || (tag == last_tag && tag != TAG_SLOT) {
                return Err(VaultError::format("header records out of order or duplicated"));
            }
            last_tag = tag;
            match tag {
                TAG_CONTAINER_ID => container_id = Some(fixed::<16>(value)?),
                TAG_SUITE_ID => suite_id = Some(fixed::<16>(value)?),
                TAG_KDF => {
                    let mut k = Reader::new(value);
                    if k.u8()? != KDF_ARGON2ID_V13 {
                        return Err(VaultError::format("unknown KDF"));
                    }
                    let params = KdfParams {
                        memory_kib: k.u32()?,
                        iterations: k.u32()?,
                        parallelism: k.u32()?,
                        salt: k.array()?,
                    };
                    k.finish()?;
                    kdf = Some(params);
                }
                TAG_CIPHER => cipher = Some(fixed::<1>(value)?[0]),
                TAG_FLAGS => flags = Some(u32::from_le_bytes(fixed::<4>(value)?)),
                TAG_SLOT => {
                    if slots.len() >= MAX_SLOTS {
                        return Err(VaultError::format("too many slots"));
                    }
                    slots.push(Slot::parse(value)?);
                }
                TAG_VMK => {
                    let mut w = Reader::new(value);
                    let wrapped = parse_wrapped(&mut w)?;
                    w.finish()?;
                    vmk = Some(wrapped);
                }
                t if t & TAG_OPTIONAL_BIT != 0 => {} // unknown optional record: authenticated, ignored
                t => return Err(VaultError::format(format!("unknown header record 0x{t:04x}"))),
            }
        }

        let missing = |name: &str| VaultError::format(format!("missing header record: {name}"));
        if cipher.ok_or_else(|| missing("cipher"))? != CIPHER_XCHACHA20_POLY1305 {
            return Err(VaultError::format("unknown cipher"));
        }
        let flags = flags.ok_or_else(|| missing("flags"))?;
        if flags != 0 {
            return Err(VaultError::format("unknown flags"));
        }
        let header = Header {
            container_id: container_id.ok_or_else(|| missing("container ID"))?,
            suite_id: suite_id.ok_or_else(|| missing("suite ID"))?,
            kdf: kdf.ok_or_else(|| missing("KDF"))?,
            slots,
            vmk: vmk.ok_or_else(|| missing("VMK"))?,
        };
        header.validate()?;
        Ok(header)
    }
}

fn record(out: &mut Vec<u8>, tag: u16, value: &[u8]) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

fn fixed<const N: usize>(value: &[u8]) -> Result<[u8; N]> {
    value.try_into().map_err(|_| VaultError::format("invalid record length"))
}

/// The parts of a container file.
pub struct FileParts<'a> {
    pub kind: ContainerKind,
    pub header: &'a [u8],
    /// Everything before the payload nonce; used as AAD for the payload.
    pub aad: &'a [u8],
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: &'a [u8],
}

pub fn split_file(data: &[u8]) -> Result<FileParts<'_>> {
    if data.len() > MAX_FILE_LEN {
        return Err(VaultError::format("file too large"));
    }
    if data.len() < PREFIX_LEN {
        return Err(VaultError::format("file too short"));
    }
    let kind = ContainerKind::from_magic(&data[..8]).ok_or_else(|| VaultError::format("not a GhulSuite container"))?;
    let version = u16::from_le_bytes([data[8], data[9]]);
    if version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion(version));
    }
    let header_len = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Err(VaultError::format("invalid header length"));
    }
    let header_end = PREFIX_LEN + header_len;
    if data.len() < header_end + NONCE_LEN + TAG_LEN {
        return Err(VaultError::format("file truncated"));
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&data[header_end..header_end + NONCE_LEN]);
    Ok(FileParts {
        kind,
        header: &data[PREFIX_LEN..header_end],
        aad: &data[..header_end],
        nonce,
        ciphertext: &data[header_end + NONCE_LEN..],
    })
}

/// Writes magic, version and header length.
pub fn encode_prefix(kind: ContainerKind, header_len: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(kind.magic());
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(header_len as u32).to_le_bytes());
}

/// Bounds-checked little-endian reader.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.data.len() < n {
            return Err(VaultError::format("unexpected end of data"));
        }
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        Ok(head)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().expect("length checked"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn finish(&self) -> Result<()> {
        if self.data.is_empty() {
            Ok(())
        } else {
            Err(VaultError::format("trailing bytes"))
        }
    }
}
