// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite server module (`server/ghulsuite.php`) as storage and share target.
//!
//! Every request is signed: HMAC-SHA256 over method, the signed query parameters, a timestamp,
//! a one-time nonce and the SHA-256 of the body. The server accepts a request only within five
//! minutes and only once. The access key is created by the app and written into the PHP file it
//! generates; it never travels over the network.

use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::http::{self, Url};
use super::remote::{check_path, RemoteEntry, RemoteFs};
use super::{block_on, NetError, NetResult, TIMEOUT};

const TEMPLATE: &str = include_str!("../../server/ghulsuite.php");
const VIEWER_HTML: &str = include_str!("../../share-viewer/index.html");
const VIEWER_JS: &str = include_str!("../../share-viewer/viewer.js");
const VIEWER_CSS: &str = include_str!("../../share-viewer/viewer.css");
/// Parts of a share upload (web hosting limits the size of one request).
const PART: usize = 4 * 1024 * 1024;
/// Signed query parameters in the order the server checks them.
const SIGNED: [&str; 6] = ["a", "p", "id", "exp", "max", "n"];

/// The PHP file for a new server module, with its own access key and data folder.
pub fn server_file(key: &[u8; 32], data_dir: &str) -> String {
    TEMPLATE
        .replace("__GHUL_KEY__", &crate::kdf::hex(key))
        .replace("__GHUL_DATA__", data_dir)
        .replace("__GHUL_VIEWER_HTML__", &super::tls::base64(VIEWER_HTML.as_bytes()))
        .replace("__GHUL_VIEWER_JS__", &super::tls::base64(VIEWER_JS.as_bytes()))
        .replace("__GHUL_VIEWER_CSS__", &super::tls::base64(VIEWER_CSS.as_bytes()))
}

/// Percent-encoding like PHP's `rawurlencode` (RFC 3986 unreserved characters stay).
pub(crate) fn rawurlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[derive(Clone)]
pub struct PhpConfig {
    /// Address of the PHP file, e.g. `https://example.org/ghulsuite.php`.
    pub url: String,
    pub key: Zeroizing<[u8; 32]>,
    /// Folder inside the module's storage ("" = its root).
    pub base: String,
    pub cert: Option<String>,
}

pub struct PhpFs {
    url: Url,
    key: Zeroizing<[u8; 32]>,
    base: String,
    cert: Option<String>,
}

/// A share as the server module reports it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct ServerShare {
    pub id: String,
    pub exp: i64,
    pub max: u32,
    pub count: u32,
}

#[derive(serde::Deserialize)]
struct Entry {
    n: String,
    d: bool,
    s: u64,
}

impl PhpFs {
    pub fn new(cfg: &PhpConfig) -> NetResult<PhpFs> {
        let url = Url::parse(&cfg.url)?;
        if !url.path.split('?').next().unwrap_or("").ends_with(".php") {
            return Err(NetError::Protocol("the address must end with .php".into()));
        }
        Ok(PhpFs { url, key: cfg.key.clone(), base: cfg.base.trim_matches('/').to_owned(), cert: cfg.cert.clone() })
    }

    /// The public address of the module (also the address of the share page).
    pub fn public_url(&self) -> String {
        let path = self.url.path.split('?').next().unwrap_or("/");
        format!("{}{path}", self.url.to_string_without_path())
    }

    fn full(&self, path: &str) -> NetResult<String> {
        check_path(path)?;
        Ok(match (self.base.is_empty(), path.is_empty()) {
            (true, _) => path.to_owned(),
            (false, true) => self.base.clone(),
            (false, false) => format!("{}/{path}", self.base),
        })
    }

    fn call(&self, params: &[(&str, String)], body: &[u8]) -> NetResult<http::Response> {
        let mut query = Vec::new();
        for k in SIGNED {
            if let Some((_, v)) = params.iter().find(|(p, _)| *p == k) {
                query.push(format!("{k}={}", rawurlencode(v)));
            }
        }
        let query = query.join("&");
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NetError::Protocol("clock".into()))?
            .as_secs()
            .to_string();
        let mut n = [0u8; 16];
        OsRng.fill_bytes(&mut n);
        let nonce = crate::kdf::hex(&n);
        let msg = format!("ghul/v1\nPOST\n{query}\n{time}\n{nonce}\n{}", crate::kdf::hex(&Sha256::digest(body)));
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_ref()).expect("any key length");
        mac.update(msg.as_bytes());
        let sig = crate::kdf::hex(&mac.finalize().into_bytes());
        let path = self.url.path.split('?').next().unwrap_or("/");
        let url = Url { path: format!("{path}?{query}"), ..self.url.clone() };
        let headers = [
            ("X-Ghul-Time", time),
            ("X-Ghul-Nonce", nonce),
            ("X-Ghul-Sig", sig),
            ("Content-Type", "application/octet-stream".to_owned()),
        ];
        let limit = TIMEOUT + std::time::Duration::from_secs((body.len() / (256 * 1024)) as u64);
        let r = block_on(http::request("POST", &url, &headers, body, self.cert.as_deref()), limit)?;
        match r.status {
            200 => Ok(r),
            401 => Err(NetError::AuthFailed),
            404 => Err(NetError::NotFound(String::new())),
            _ => Err(NetError::Protocol(format!(
                "server module answered {}: {}",
                r.status,
                String::from_utf8_lossy(&r.body).chars().take(200).collect::<String>()
            ))),
        }
    }

    /// Checks the address and the key; returns the module's version.
    pub fn info(&self) -> NetResult<u32> {
        let r = self.call(&[("a", "info".into())], b"")?;
        let v: serde_json::Value =
            serde_json::from_slice(&r.body).map_err(|_| NetError::Protocol("not a GhulSuite server module".into()))?;
        v["version"]
            .as_u64()
            .map(|v| v as u32)
            .ok_or_else(|| NetError::Protocol("not a GhulSuite server module".into()))
    }

    /// Uploads a share package; `expires` in Unix ms (0 = none), `max_downloads` 0 = unlimited.
    pub fn share_put(&self, id: &str, data: &[u8], expires: i64, max_downloads: u32) -> NetResult<()> {
        let parts: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(PART).collect() };
        for (n, part) in parts.iter().enumerate() {
            self.call(&[("a", "share_part".into()), ("id", id.into()), ("n", n.to_string())], part)?;
        }
        self.call(
            &[
                ("a", "share_done".into()),
                ("id", id.into()),
                ("exp", expires.to_string()),
                ("max", max_downloads.to_string()),
            ],
            b"",
        )?;
        Ok(())
    }

    pub fn share_delete(&self, id: &str) -> NetResult<()> {
        self.call(&[("a", "share_delete".into()), ("id", id.into())], b"").map(|_| ())
    }

    pub fn share_list(&self) -> NetResult<Vec<ServerShare>> {
        let r = self.call(&[("a", "share_list".into())], b"")?;
        serde_json::from_slice(&r.body).map_err(|e| NetError::Protocol(e.to_string()))
    }
}

impl RemoteFs for PhpFs {
    fn list(&self, dir: &str) -> NetResult<Vec<RemoteEntry>> {
        let r = self.call(&[("a", "list".into()), ("p", self.full(dir)?)], b"")?;
        let list: Vec<Entry> = serde_json::from_slice(&r.body).map_err(|e| NetError::Protocol(e.to_string()))?;
        let mut out: Vec<RemoteEntry> =
            list.into_iter().map(|e| RemoteEntry { name: e.n, is_dir: e.d, size: e.s }).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn read(&self, path: &str) -> NetResult<Option<Vec<u8>>> {
        match self.call(&[("a", "read".into()), ("p", self.full(path)?)], b"") {
            Ok(r) => Ok(Some(r.body)),
            Err(NetError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, path: &str, data: &[u8]) -> NetResult<()> {
        self.call(&[("a", "write".into()), ("p", self.full(path)?)], data).map(|_| ())
    }

    fn delete(&self, path: &str) -> NetResult<()> {
        self.call(&[("a", "delete".into()), ("p", self.full(path)?)], b"").map(|_| ())
    }

    fn mkdir_all(&self, dir: &str) -> NetResult<()> {
        let full = self.full(dir)?;
        if full.is_empty() {
            return Ok(());
        }
        self.call(&[("a", "mkdir".into()), ("p", full)], b"").map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_like_php() {
        assert_eq!(rawurlencode("drive/blocks/ab/x y.bin"), "drive%2Fblocks%2Fab%2Fx%20y.bin");
        assert_eq!(rawurlencode("Grüße~-_."), "Gr%C3%BC%C3%9Fe~-_.");
    }

    #[test]
    fn generated_file_has_no_placeholders() {
        let f = server_file(&[7u8; 32], "ghul-data-abc");
        assert!(!f.contains("__GHUL_"));
        assert!(f.contains(&"07".repeat(32)));
    }
}
