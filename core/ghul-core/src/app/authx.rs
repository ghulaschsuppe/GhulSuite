// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulAuth extras: export an account as otpauth link (for a QR code).

use super::error::{GhulError, Result};
use super::suite::{text_field, GhulSuite};
use crate::vault::Field;

/// Base32 (RFC 4648) without padding, as authenticator apps expect it.
fn base32(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(A[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[uniffi::export]
impl GhulSuite {
    /// The account as `otpauth://` link, e.g. for a QR code to move it to another app. It holds
    /// the 2FA secret in the clear, so only with the master password (M13: who finds the app
    /// unlocked must not photograph it); a wrong one is `WrongPassword`.
    pub fn totp_uri(&self, vault_id: String, id: Vec<u8>, password: String) -> Result<String> {
        if password.is_empty() || !self.verify_password(password)? {
            return Err(GhulError::WrongPassword);
        }
        self.with_open(|o| {
            let (_, c) = o.entry_vault_mut(&vault_id)?;
            let e = c
                .payload
                .entries
                .iter()
                .find(|e| e.id == id && !e.deleted && e.entry_type == "totp")
                .ok_or(GhulError::NotFound)?;
            let Some(Field::Bytes(secret)) = e.fields.get("secret") else { return Err(GhulError::NotFound) };
            let int = |k: &str, d: i64| match e.fields.get(k) {
                Some(Field::Int(x)) => *x,
                _ => d,
            };
            let issuer = text_field(e, "issuer");
            let account = text_field(e, "account");
            let algo = text_field(e, "algorithm");
            let label = if issuer.is_empty() { pct(&account) } else { format!("{}:{}", pct(&issuer), pct(&account)) };
            let mut uri = format!("otpauth://totp/{label}?secret={}", base32(secret));
            if !issuer.is_empty() {
                uri.push_str(&format!("&issuer={}", pct(&issuer)));
            }
            if !algo.is_empty() && !algo.eq_ignore_ascii_case("SHA1") {
                uri.push_str(&format!("&algorithm={}", algo.to_uppercase()));
            }
            uri.push_str(&format!("&digits={}&period={}", int("digits", 6), int("period", 30)));
            Ok(uri)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_and_percent_encoding() {
        assert_eq!(base32(b"12345678901234567890"), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(pct("a b@c"), "a%20b%40c");
    }
}
