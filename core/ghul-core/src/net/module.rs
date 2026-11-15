// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite server module (`server/ghulsuite.php`): share links on web hosting.
//!
//! The app installs it over SFTP or FTPS on the server that holds GhulDrive. It has no write
//! access over the web and no secret: it reads the share descriptions the app writes into the
//! GhulSuite folder and hands out the encrypted blocks of a share (see `app::shares`).

use super::http::{self, Url};
use super::{block_on, NetError, NetResult, TIMEOUT};

const TEMPLATE: &str = include_str!("../../server/ghulsuite.php");
const VIEWER_HTML: &str = include_str!("../../share-viewer/index.html");
const VIEWER_JS: &str = include_str!("../../share-viewer/viewer.js");
const VIEWER_CSS: &str = include_str!("../../share-viewer/viewer.css");
/// Version of the module this app creates (`GHUL_VERSION` in `server/ghulsuite.php`).
pub const MODULE_VERSION: u32 = 3;

/// The PHP file for a server whose GhulSuite folder is at `base` (relative to the PHP file's
/// folder, or absolute). `None` if `base` contains characters that do not belong in a path.
pub fn module_file(base: &str) -> Option<String> {
    if base.is_empty() || base.chars().any(|c| c.is_control() || c == '\'' || c == '\\') {
        return None;
    }
    Some(
        TEMPLATE
            .replace("__GHUL_BASE__", base)
            .replace("__GHUL_VIEWER_HTML__", &super::tls::base64(VIEWER_HTML.as_bytes()))
            .replace("__GHUL_VIEWER_JS__", &super::tls::base64(VIEWER_JS.as_bytes()))
            .replace("__GHUL_VIEWER_CSS__", &super::tls::base64(VIEWER_CSS.as_bytes())),
    )
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

/// What a module answers to `?v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub version: u32,
    /// The GhulSuite folder it was given exists for it (`drive/` or `suite/` inside).
    pub folder: bool,
}

fn get(url: &str, query: &str) -> NetResult<http::Response> {
    let sep = if url.contains('?') { '&' } else { '?' };
    let u = Url::parse(&format!("{url}{sep}{query}")).map_err(|_| NetError::Protocol("invalid address".into()))?;
    block_on(http::request("GET", &u, &[], b"", None), TIMEOUT)
}

/// Asks the module at `url` for its version.
pub fn probe(url: &str) -> NetResult<ModuleInfo> {
    let r = get(url, "v")?;
    if r.status != 200 {
        return Err(NetError::Protocol(format!("HTTP {}", r.status)));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&r.body).map_err(|_| NetError::Protocol("not the GhulSuite module".into()))?;
    let version = v
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| NetError::Protocol("not the GhulSuite module".into()))?;
    Ok(ModuleInfo {
        version: version as u32,
        folder: v.get("folder").and_then(serde_json::Value::as_bool).unwrap_or(false),
    })
}

/// The content the module reads from the test file `shares/check-<name>` of its folder.
pub fn check(url: &str, name: &str) -> NetResult<Option<Vec<u8>>> {
    let r = get(url, &format!("check={name}"))?;
    Ok((r.status == 200).then_some(r.body))
}

/// Whether `url` delivers the module's PHP source instead of running it (PHP not active there).
pub fn reveals_source(url: &str) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    let r = block_on(http::request("GET", &u, &[], b"", None), std::time::Duration::from_secs(15));
    matches!(r, Ok(r) if {
        let body = String::from_utf8_lossy(&r.body);
        body.contains("<?php") || body.contains("GHUL_BASE")
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_file_refuses_odd_folders() {
        assert!(super::module_file("../ghulsuite").unwrap().contains("const GHUL_BASE = '../ghulsuite';"));
        for bad in ["", "a'b", "a\\b", "a\nb"] {
            assert!(super::module_file(bad).is_none(), "{bad:?}");
        }
        assert!(!super::module_file("x").unwrap().contains("__GHUL_"), "every placeholder is filled in");
    }
}
