// SPDX-License-Identifier: AGPL-3.0-or-later
//! Encrypted payload: a CBOR document, length-prefixed and padded to hide its size.
//!
//! Unknown keys are preserved (`extra`) so that older clients do not destroy data written by
//! newer ones.

use std::collections::BTreeMap;

use ciborium::value::Value;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::error::{Result, VaultError};
use super::format::ContainerKind;

pub const PAYLOAD_SCHEMA: u32 = 1;
pub const MIN_PADDED_LEN: usize = 4096;
pub const MAX_PAYLOAD_LEN: usize = 128 * 1024 * 1024;

/// Metadata of a slot; kept encrypted so that labels like "Key at home" do not leak.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotMeta {
    pub label: String,
    /// Unix time in milliseconds.
    pub created: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "lowercase")]
pub enum Field {
    Text(String),
    /// Hidden by default in the UI (passwords, PINs, card numbers).
    Secret(String),
    Url(String),
    Bytes(#[serde(with = "serde_bytes")] Vec<u8>),
    Int(i64),
    Bool(bool),
    List(Vec<String>),
}

impl Zeroize for Field {
    fn zeroize(&mut self) {
        match self {
            Field::Text(s) | Field::Secret(s) | Field::Url(s) => s.zeroize(),
            Field::Bytes(b) => b.zeroize(),
            Field::Int(i) => i.zeroize(),
            Field::Bool(b) => *b = false,
            Field::List(l) => l.iter_mut().for_each(Zeroize::zeroize),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Random 16-byte ID.
    #[serde(with = "serde_bytes")]
    pub id: Vec<u8>,
    /// e.g. `login`, `note`, `card`, `identity`, `ssh`, `wifi`, `totp`
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Incremented on every change of this entry (used for merging).
    pub revision: u64,
    /// Unix time in milliseconds of the last change.
    pub modified: i64,
    /// Tombstone: deleted entries are kept (without fields) so that deletions sync.
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Entry {
    pub fn new(id: [u8; 16], entry_type: &str, modified: i64) -> Self {
        Entry {
            id: id.to_vec(),
            entry_type: entry_type.to_owned(),
            revision: 1,
            modified,
            deleted: false,
            fields: BTreeMap::new(),
            tags: Vec::new(),
            folder: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Zeroize for Entry {
    fn zeroize(&mut self) {
        self.fields.values_mut().for_each(Zeroize::zeroize);
        self.tags.iter_mut().for_each(Zeroize::zeroize);
        if let Some(f) = &mut self.folder {
            f.zeroize();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    pub schema: u32,
    pub kind: String,
    /// Monotonic revision of the whole container (rollback detection).
    pub revision: u64,
    /// Copy of the factor secret `S`, so that a recovery via the container key restores it.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes")]
    pub factor_secret: Option<Vec<u8>>,
    /// The recovery code of the suite, so that every container of the suite can get the same
    /// recovery slot (new vaults, new code). Readable only by whoever can open the container.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes")]
    pub recovery_secret: Option<Vec<u8>>,
    /// Slot metadata, keyed by the lower-case hex slot ID.
    #[serde(default)]
    pub slots: BTreeMap<String, SlotMeta>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// Free-form settings (used by the suite container: storage connections, mail accounts, …).
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.entries.iter_mut().for_each(Zeroize::zeroize);
        if let Some(s) = &mut self.factor_secret {
            s.zeroize();
        }
        if let Some(s) = &mut self.recovery_secret {
            s.zeroize();
        }
    }
}

impl Payload {
    pub fn new(kind: ContainerKind) -> Self {
        Payload {
            schema: PAYLOAD_SCHEMA,
            kind: kind.name().to_owned(),
            revision: 0,
            factor_secret: None,
            recovery_secret: None,
            slots: BTreeMap::new(),
            entries: Vec::new(),
            settings: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    pub fn to_cbor(&self) -> Result<Zeroizing<Vec<u8>>> {
        let mut out = Zeroizing::new(Vec::new());
        ciborium::ser::into_writer(self, &mut *out).map_err(|e| VaultError::Payload(e.to_string()))?;
        Ok(out)
    }

    pub fn from_cbor(data: &[u8], kind: ContainerKind) -> Result<Self> {
        let p: Payload = ciborium::de::from_reader(data).map_err(|e| VaultError::Payload(e.to_string()))?;
        if p.schema != PAYLOAD_SCHEMA {
            return Err(VaultError::Payload(format!("unsupported payload schema {}", p.schema)));
        }
        if p.kind != kind.name() {
            return Err(VaultError::Payload("payload kind does not match container".into()));
        }
        Ok(p)
    }
}

/// Padded size: at least 4 KiB, otherwise the next power of two.
pub fn padded_len(cbor_len: usize) -> usize {
    (cbor_len + 4).next_power_of_two().max(MIN_PADDED_LEN)
}

pub fn pad(cbor: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if cbor.len() > MAX_PAYLOAD_LEN {
        return Err(VaultError::Payload("payload too large".into()));
    }
    let total = padded_len(cbor.len());
    let mut out = Zeroizing::new(Vec::with_capacity(total));
    out.extend_from_slice(&(cbor.len() as u32).to_le_bytes());
    out.extend_from_slice(cbor);
    out.resize(total, 0);
    Ok(out)
}

pub fn unpad(data: &[u8]) -> Result<&[u8]> {
    if data.len() < 4 {
        return Err(VaultError::Payload("payload too short".into()));
    }
    let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if len > MAX_PAYLOAD_LEN || len + 4 > data.len() || data.len() != padded_len(len) {
        return Err(VaultError::Payload("invalid payload length".into()));
    }
    if data[4 + len..].iter().any(|&b| b != 0) {
        return Err(VaultError::Payload("invalid padding".into()));
    }
    Ok(&data[4..4 + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_buckets() {
        assert_eq!(padded_len(0), 4096);
        assert_eq!(padded_len(4092), 4096);
        assert_eq!(padded_len(4093), 8192);
        assert_eq!(padded_len(100_000), 131_072);
        let p = pad(b"hello").unwrap();
        assert_eq!(p.len(), 4096);
        assert_eq!(unpad(&p).unwrap(), b"hello");
        let mut bad = p.to_vec();
        bad[100] = 1;
        assert!(unpad(&bad).is_err());
        assert!(unpad(&p[..4095]).is_err());
    }

    #[test]
    fn unknown_keys_are_preserved() {
        let mut p = Payload::new(ContainerKind::Pass);
        let mut e = Entry::new([1; 16], "login", 1_700_000_000_000);
        e.fields.insert("title".into(), Field::Text("Example".into()));
        e.fields.insert("password".into(), Field::Secret("hunter2".into()));
        e.fields.insert("urls".into(), Field::List(vec!["https://example.org".into()]));
        e.extra.insert("future_field".into(), Value::Integer(42.into()));
        p.entries.push(e);
        p.extra.insert("future_top".into(), Value::Text("x".into()));
        let cbor = p.to_cbor().unwrap();
        let back = Payload::from_cbor(&cbor, ContainerKind::Pass).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.entries[0].extra.get("future_field"), Some(&Value::Integer(42.into())));
        assert!(Payload::from_cbor(&cbor, ContainerKind::Totp).is_err());
    }
}
