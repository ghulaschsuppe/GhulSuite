// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulAuth extras: export an account as otpauth link (for a QR code) and check the device clock.

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

/// Seconds since 1970 of an HTTP date ("Tue, 12 Nov 2026 07:40:00 GMT").
pub(crate) fn parse_http_date(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let day: i64 = parts[1].parse().ok()?;
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        .iter()
        .position(|m| *m == parts[2])? as i64
        + 1;
    let year: i64 = parts[3].parse().ok()?;
    let hms: Vec<i64> = parts[4].split(':').filter_map(|x| x.parse().ok()).collect();
    if hms.len() != 3 {
        return None;
    }
    // Days since 1970 (civil calendar, Howard Hinnant's algorithm).
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hms[0] * 3600 + hms[1] * 60 + hms[2])
}

#[uniffi::export]
impl GhulSuite {
    /// The account as `otpauth://` link, e.g. for a QR code to move it to another app.
    pub fn totp_uri(&self, vault_id: String, id: Vec<u8>) -> Result<String> {
        self.with_open(|o| {
            let (_, c) = o.vault_mut(&vault_id)?;
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

    /// Difference between this device's clock and one of the user's own web servers (seconds;
    /// positive = device is ahead). `None` without such a server or when it cannot be reached.
    /// One-time codes only work with a correct clock.
    pub fn clock_offset(&self, device_unix: i64) -> Result<Option<i64>> {
        let url = self.with_open(|o| {
            Ok(super::servers::all_servers(o)
                .into_iter()
                .map(|s| if !s.url.is_empty() { s.url.clone() } else { s.share_url.clone() })
                .find(|u| u.starts_with("https://")))
        })?;
        let Some(url) = url else { return Ok(None) };
        let Ok(url) = crate::net::http::Url::parse(&url) else { return Ok(None) };
        let resp = crate::net::block_on(
            async move { crate::net::http::request("HEAD", &url, &[], &[], None).await },
            crate::net::TIMEOUT,
        );
        Ok(resp.ok().and_then(|r| r.header("Date").and_then(parse_http_date)).map(|server| device_unix - server))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_dates_and_base32() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:10 GMT"), Some(10));
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784111777));
        assert_eq!(parse_http_date("Thu, 12 Nov 2026 07:40:00 GMT"), Some(1794469200));
        assert_eq!(parse_http_date("nonsense"), None);
        assert_eq!(base32(b"12345678901234567890"), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(pct("a b@c"), "a%20b%40c");
    }
}
