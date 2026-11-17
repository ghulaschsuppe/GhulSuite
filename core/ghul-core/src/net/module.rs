// SPDX-License-Identifier: AGPL-3.0-or-later
//! The GhulSuite server module (`server/ghulsuite.php`): share links on web hosting.
//!
//! The app installs it over SFTP on the server that holds GhulDrive. It has no write
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

/// What the module finds of the test file `shares/check-<name>` of its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The file's content.
    pub token: String,
    /// PHP may write in `shares/` (the state of opened shares goes there).
    pub writes: bool,
}

/// Asks the module to read the test file `shares/check-<name>`; `None` if it cannot.
pub fn check(url: &str, name: &str) -> NetResult<Option<Checked>> {
    let r = get(url, &format!("check={name}"))?;
    if r.status != 200 {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_slice(&r.body).map_err(|_| NetError::Protocol("not the GhulSuite module".into()))?;
    Ok(v.get("token").and_then(serde_json::Value::as_str).map(|t| Checked {
        token: t.to_owned(),
        writes: v.get("write").and_then(serde_json::Value::as_bool).unwrap_or(false),
    }))
}

/// Whether the web server hands out `token` at `url` (a file of the GhulSuite folder that
/// must not be reachable from the web).
pub fn exposes(url: &str, token: &str) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    let r = block_on(http::request("GET", &u, &[], b"", None), std::time::Duration::from_secs(15));
    matches!(r, Ok(r) if r.status == 200 && String::from_utf8_lossy(&r.body).contains(token))
}

/// The address `rel` (a relative path like `../ghulsuite/shares/x`) resolves to next to the
/// file at `url`; `None` if it climbs above the website's root. Parts of `rel` are
/// percent-encoded (a folder named `a#b` stays one part).
pub fn resolve(url: &str, rel: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(end);
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut parts: Vec<String> = path.split('/').map(str::to_owned).collect();
    parts.pop(); // the file itself
    parts.retain(|p| !p.is_empty());
    for p in rel.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(rawurlencode(p)),
        }
    }
    Some(format!("{scheme}://{host}/{}", parts.join("/")))
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

    #[test]
    fn resolves_folders_next_to_the_module() {
        let u = "https://example.org/tools/ghulsuite.php";
        assert_eq!(
            super::resolve(u, "ghulsuite/shares/a").as_deref(),
            Some("https://example.org/tools/ghulsuite/shares/a")
        );
        assert_eq!(
            super::resolve(u, "../ghulsuite/shares/a").as_deref(),
            Some("https://example.org/ghulsuite/shares/a")
        );
        assert_eq!(super::resolve(u, "../../ghulsuite/shares/a"), None, "above the website's root");
        assert_eq!(super::resolve(u, "a#b/c?d/e%f").as_deref(), Some("https://example.org/tools/a%23b/c%3Fd/e%25f"));
        assert_eq!(super::resolve("https://example.org?x=/y", "g").as_deref(), Some("https://example.org/g"));
        assert_eq!(
            super::resolve("http://127.0.0.1:8080/ghulsuite.php?x=1", "./g/s").as_deref(),
            Some("http://127.0.0.1:8080/g/s")
        );
    }
}
