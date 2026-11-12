// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal CBOR support for CTAP2.
//!
//! Encoding is done with our own small encoder because CTAP2 authenticators require the
//! *CTAP2 canonical* form (RFC 7049 §3.9 key ordering), which general-purpose encoders do
//! not guarantee. Decoding uses `ciborium`.

use ciborium::value::Value;

/// A CBOR value for encoding requests.
#[derive(Clone, Debug, PartialEq)]
pub enum Cbor {
    Int(i64),
    Bytes(Vec<u8>),
    Text(String),
    Bool(bool),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
}

impl Cbor {
    pub fn text(s: &str) -> Self {
        Cbor::Text(s.to_owned())
    }

    pub fn bytes(b: &[u8]) -> Self {
        Cbor::Bytes(b.to_vec())
    }

    /// Encodes the value in CTAP2 canonical form.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        match self {
            Cbor::Int(v) if *v >= 0 => write_head(out, 0, *v as u64),
            Cbor::Int(v) => write_head(out, 1, (-1 - *v) as u64),
            Cbor::Bytes(b) => {
                write_head(out, 2, b.len() as u64);
                out.extend_from_slice(b);
            }
            Cbor::Text(s) => {
                write_head(out, 3, s.len() as u64);
                out.extend_from_slice(s.as_bytes());
            }
            Cbor::Array(items) => {
                write_head(out, 4, items.len() as u64);
                for item in items {
                    item.encode_into(out);
                }
            }
            Cbor::Map(entries) => {
                // CTAP2 canonical ordering: major type, then length, then bytewise.
                let mut encoded: Vec<(Vec<u8>, Vec<u8>)> =
                    entries.iter().map(|(k, v)| (k.encode(), v.encode())).collect();
                encoded.sort_by(|(a, _), (b, _)| {
                    (a[0] >> 5, a.len(), a.as_slice()).cmp(&(b[0] >> 5, b.len(), b.as_slice()))
                });
                write_head(out, 5, encoded.len() as u64);
                for (k, v) in encoded {
                    out.extend_from_slice(&k);
                    out.extend_from_slice(&v);
                }
            }
            Cbor::Bool(false) => out.push(0xf4),
            Cbor::Bool(true) => out.push(0xf5),
        }
    }
}

fn write_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let m = major << 5;
    if value < 24 {
        out.push(m | value as u8);
    } else if value <= u8::MAX as u64 {
        out.push(m | 24);
        out.push(value as u8);
    } else if value <= u16::MAX as u64 {
        out.push(m | 25);
        out.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value <= u32::MAX as u64 {
        out.push(m | 26);
        out.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

/// Decodes a single CBOR item from the start of `data` and returns it together with the
/// number of bytes consumed.
pub fn decode_prefix(data: &[u8]) -> Result<(Value, usize), String> {
    let mut reader: &[u8] = data;
    let value: Value = ciborium::de::from_reader(&mut reader).map_err(|e| format!("invalid CBOR: {e}"))?;
    Ok((value, data.len() - reader.len()))
}

/// Decodes exactly one CBOR item spanning all of `data`.
pub fn decode(data: &[u8]) -> Result<Value, String> {
    let (value, used) = decode_prefix(data)?;
    if used != data.len() {
        return Err("trailing bytes after CBOR item".into());
    }
    Ok(value)
}

/// Looks up an integer key in a CBOR map.
pub fn map_get_int(map: &Value, key: i64) -> Option<&Value> {
    map.as_map()?.iter().find_map(|(k, v)| match k {
        Value::Integer(i) if i128::from(*i) == key as i128 => Some(v),
        _ => None,
    })
}

/// Looks up a text key in a CBOR map.
pub fn map_get_text<'a>(map: &'a Value, key: &str) -> Option<&'a Value> {
    map.as_map()?.iter().find_map(|(k, v)| match k {
        Value::Text(t) if t == key => Some(v),
        _ => None,
    })
}

pub fn as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Integer(i) => u64::try_from(i128::from(*i)).ok(),
        _ => None,
    }
}

pub fn as_bytes(v: &Value) -> Option<&[u8]> {
    match v {
        Value::Bytes(b) => Some(b.as_slice()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_and_heads() {
        assert_eq!(Cbor::Int(0).encode(), vec![0x00]);
        assert_eq!(Cbor::Int(23).encode(), vec![0x17]);
        assert_eq!(Cbor::Int(24).encode(), vec![0x18, 0x18]);
        assert_eq!(Cbor::Int(1000).encode(), vec![0x19, 0x03, 0xe8]);
        assert_eq!(Cbor::Int(-1).encode(), vec![0x20]);
        assert_eq!(Cbor::Int(-25).encode(), vec![0x38, 0x18]);
        assert_eq!(Cbor::Bool(true).encode(), vec![0xf5]);
    }

    #[test]
    fn canonical_map_ordering() {
        // COSE key: expected order 1, 3, -1, -2, -3
        let m = Cbor::Map(vec![
            (Cbor::Int(-3), Cbor::Int(0)),
            (Cbor::Int(-1), Cbor::Int(1)),
            (Cbor::Int(3), Cbor::Int(-25)),
            (Cbor::Int(-2), Cbor::Int(0)),
            (Cbor::Int(1), Cbor::Int(2)),
        ]);
        let enc = m.encode();
        assert_eq!(enc, vec![0xa5, 0x01, 0x02, 0x03, 0x38, 0x18, 0x20, 0x01, 0x21, 0x00, 0x22, 0x00]);

        // Text keys: shorter first ("id" < "name" < "displayName")
        let user = Cbor::Map(vec![
            (Cbor::text("displayName"), Cbor::text("x")),
            (Cbor::text("name"), Cbor::text("x")),
            (Cbor::text("id"), Cbor::text("x")),
        ]);
        let enc = user.encode();
        let decoded = decode(&enc).unwrap();
        let keys: Vec<_> = decoded.as_map().unwrap().iter().map(|(k, _)| k.as_text().unwrap().to_owned()).collect();
        assert_eq!(keys, vec!["id", "name", "displayName"]);
    }

    #[test]
    fn decode_prefix_reports_consumed_bytes() {
        let mut data = Cbor::Map(vec![(Cbor::Int(1), Cbor::bytes(&[1, 2, 3]))]).encode();
        let len = data.len();
        data.extend_from_slice(&[0xaa, 0xbb]);
        let (v, used) = decode_prefix(&data).unwrap();
        assert_eq!(used, len);
        assert_eq!(as_bytes(map_get_int(&v, 1).unwrap()).unwrap(), &[1, 2, 3]);
    }
}
