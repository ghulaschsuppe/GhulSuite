// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sharing with people outside the suite: end-to-end encrypted share links.
//!
//! A share is a separate package (`crate::share`) with its own random key, uploaded to a place
//! the web can reach: the share folder of an SFTP or FTPS server (set up and checked once with
//! `share_setup`) or the GhulSuite server module (PHP). The key is only in the link after `#`.
//! The suite remembers its shares (with their links) in its encrypted settings, so they can be
//! copied again, ended, and cleaned up after expiry.

use std::io::Write;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::servers::{self, ServerKind};
use super::suite::{now_ms, put_setting, setting, GhulSuite, Open, SUITE_FILE};
use crate::net::http::{self, Url};
use crate::net::remote::RemoteFs;
use crate::net::{block_on, TIMEOUT};
use crate::share::{b64url, seal, ShareMeta};

const SHARES_KEY: &str = "shares";
/// Largest share (the recipient's browser decrypts it in memory).
const MAX_SHARE: u64 = 1024 * 1024 * 1024;
const VIEWER: [(&str, &str); 3] = [
    ("index.html", include_str!("../../share-viewer/index.html")),
    ("viewer.js", include_str!("../../share-viewer/viewer.js")),
    ("viewer.css", include_str!("../../share-viewer/viewer.css")),
];
const HTACCESS: &str = include_str!("../../share-viewer/htaccess.txt");
const S_HTACCESS: &str = include_str!("../../share-viewer/s-htaccess.txt");

#[derive(Serialize, Deserialize, Clone)]
struct StoredShare {
    id: String,
    server: String,
    name: String,
    size: u64,
    created: i64,
    /// Unix ms, 0 = no expiry.
    expires: i64,
    password: bool,
    max_downloads: u32,
    url: String,
}

impl Drop for StoredShare {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.url);
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ShareInfo {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub size: u64,
    pub created: i64,
    pub expires: i64,
    pub password: bool,
    /// 0 = unlimited (only the server module can limit).
    pub max_downloads: u32,
    /// Downloads so far; -1 if the server cannot count (plain web folder).
    pub downloads: i64,
    /// The complete link including the key.
    pub url: String,
}

/// One step of the share folder check.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ShareCheck {
    /// `write`, `https`, `listing`, `page`
    pub step: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(uniffi::Record, Clone)]
pub struct ShareOptions {
    pub server_id: String,
    /// Optional second secret; recipients must enter it besides opening the link.
    pub password: Option<String>,
    /// 0 = no expiry.
    pub expires_days: u32,
    /// 0 = unlimited; 1 = one-time link. Only with the server module.
    pub max_downloads: u32,
}

super::redact::redacted_debug!(ShareOptions { server_id, expires_days, max_downloads });

fn shares(o: &Open) -> Vec<StoredShare> {
    setting(&o.suite, SHARES_KEY)
}

fn info(s: &StoredShare, downloads: i64) -> ShareInfo {
    ShareInfo {
        id: s.id.clone(),
        server_id: s.server.clone(),
        name: s.name.clone(),
        size: s.size,
        created: s.created,
        expires: s.expires,
        password: s.password,
        max_downloads: s.max_downloads,
        downloads,
        url: s.url.clone(),
    }
}

fn get(url: &Url) -> std::result::Result<http::Response, String> {
    block_on(http::request("GET", url, &[], b"", None), TIMEOUT).map_err(|e| e.to_string())
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    OsRng.fill_bytes(&mut b);
    crate::kdf::hex(&b)
}

/// Writes the recipient page and the folder protection into a share folder.
fn install_viewer(fs: &dyn RemoteFs) -> Result<()> {
    for (name, content) in VIEWER {
        fs.write(name, content.as_bytes())?;
    }
    fs.write(".htaccess", HTACCESS.as_bytes())?;
    fs.write("s/.htaccess", S_HTACCESS.as_bytes())?;
    fs.write("s/index.html", b"")?;
    Ok(())
}

/// Packs several files into one ZIP (stored, the content is encrypted anyway).
fn zip(files: &[(String, Zeroizing<Vec<u8>>)]) -> Result<Zeroizing<Vec<u8>>> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            z.start_file(name.as_str(), opts).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            z.write_all(data).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        }
        z.finish().map_err(|e| GhulError::Storage { msg: e.to_string() })?;
    }
    Ok(Zeroizing::new(out.into_inner()))
}

fn mime_of(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "txt" => "text/plain",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

impl GhulSuite {
    fn share_upload(&self, o: &mut Open, opts: &ShareOptions, name: String, content: &[u8]) -> Result<ShareInfo> {
        let srv = servers::server(o, &opts.server_id)?;
        let kind = ServerKind::of(&srv);
        if opts.max_downloads > 0 && kind != ServerKind::Php {
            return Err(GhulError::InvalidInput { msg: "download limits need the server module".into() });
        }
        if kind != ServerKind::Php && srv.share_url.is_empty() {
            return Err(GhulError::NotConfigured);
        }
        let now = now_ms();
        let expires = if opts.expires_days == 0 { 0 } else { now + i64::from(opts.expires_days) * 86_400_000 };
        let meta = ShareMeta {
            name: name.clone(),
            mime: mime_of(&name).into(),
            size: content.len() as u64,
            created: now,
            expires,
        };
        let password = opts.password.as_deref().filter(|p| !p.is_empty());
        let sealed = seal(&meta, content, password);
        let id = random_hex(16);
        let fragment = format!("{id}.{}", b64url(sealed.link_key.as_ref()));
        let url = if kind == ServerKind::Php {
            let fs = servers::php_fs(&srv, "")?;
            fs.share_put(&id, &sealed.data, expires, opts.max_downloads)?;
            format!("{}#{fragment}", fs.public_url())
        } else {
            let fs = servers::open_fs(o, &srv, Some(&srv.share_dir))?;
            fs.write(&format!("s/{id}"), &sealed.data)?;
            format!("{}/#{fragment}", srv.share_url.trim_end_matches('/'))
        };
        let stored = StoredShare {
            id,
            server: srv.id.clone(),
            name,
            size: content.len() as u64,
            created: now,
            expires,
            password: password.is_some(),
            max_downloads: opts.max_downloads,
            url,
        };
        let mut list = shares(o);
        list.push(stored.clone());
        put_setting(&mut o.suite, SHARES_KEY, &list)?;
        self.save_container(&mut o.suite, SUITE_FILE)?;
        Ok(info(&stored, if kind == ServerKind::Php { 0 } else { -1 }))
    }

    fn share_remove(o: &mut Open, s: &StoredShare) -> Result<()> {
        if let Ok(srv) = servers::server(o, &s.server) {
            if ServerKind::of(&srv) == ServerKind::Php {
                servers::php_fs(&srv, "")?.share_delete(&s.id)?;
            } else if !srv.share_dir.is_empty() {
                servers::open_fs(o, &srv, Some(&srv.share_dir))?.delete(&format!("s/{}", s.id))?;
            }
        }
        Ok(())
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Sets up sharing on an SFTP or FTPS server: `dir` (relative to the login folder) must be
    /// delivered by a web server under `public_url`. Writes a test file, fetches it over https,
    /// checks that the folder cannot be listed and installs the recipient page. Stores the
    /// settings only if every step passed.
    pub fn share_setup(&self, server_id: String, dir: String, public_url: String) -> Result<Vec<ShareCheck>> {
        let dir = dir.trim().trim_end_matches('/').to_owned();
        let public = Url::parse(public_url.trim().trim_end_matches('/'))?;
        // Tests use a plain local web server; the app always needs https.
        if !public.tls && !cfg!(test) {
            return Err(GhulError::InvalidInput { msg: "the address must start with https://".into() });
        }
        self.with_open(|o| {
            let mut srv = servers::server(o, &server_id)?;
            if ServerKind::of(&srv) == ServerKind::Php {
                return Err(GhulError::InvalidInput { msg: "the server module shares without setup".into() });
            }
            if dir.is_empty() || dir == srv.base || dir.starts_with(&format!("{}/", srv.base)) && !srv.base.is_empty() {
                return Err(GhulError::InvalidInput { msg: "choose a folder outside the suite folder".into() });
            }
            let mut checks = Vec::new();
            let fs = servers::open_fs(o, &srv, Some(&dir))?;
            let token = random_hex(16);
            let probe = format!("ghul-check-{}.txt", random_hex(8));
            let wrote = fs.write(&probe, token.as_bytes());
            checks.push(ShareCheck {
                step: "write".into(),
                ok: wrote.is_ok(),
                detail: wrote.err().map(|e| e.to_string()).unwrap_or_default(),
            });
            let fetched = get(&public.join(&probe));
            let https_ok = matches!(&fetched, Ok(r) if r.status == 200 && r.body == token.as_bytes());
            checks.push(ShareCheck {
                step: "https".into(),
                ok: https_ok,
                detail: match &fetched {
                    Ok(r) if !https_ok => format!("HTTP {}", r.status),
                    Err(e) => e.clone(),
                    _ => String::new(),
                },
            });
            let _ = fs.delete(&probe);
            let installed = install_viewer(fs.as_ref());
            let listing_probe = format!("s/ghul-probe-{}", random_hex(8));
            let _ = fs.write(&listing_probe, b"x");
            let listed = get(&public.join("s/"));
            let name = listing_probe.trim_start_matches("s/");
            let listing_ok =
                installed.is_ok() && matches!(&listed, Ok(r) if !String::from_utf8_lossy(&r.body).contains(name));
            checks.push(ShareCheck {
                step: "listing".into(),
                ok: listing_ok,
                detail: if listing_ok { String::new() } else { "the folder content is visible".into() },
            });
            let _ = fs.delete(&listing_probe);
            let page = get(&public.join(""));
            let page_ok =
                matches!(&page, Ok(r) if r.status == 200 && String::from_utf8_lossy(&r.body).contains("viewer.js"));
            checks.push(ShareCheck {
                step: "page".into(),
                ok: page_ok,
                detail: installed.err().map(|e| e.to_string()).unwrap_or_default(),
            });
            if checks.iter().all(|c| c.ok) {
                srv.share_dir = dir.clone();
                srv.share_url = public.to_string();
                match servers::update_server(o, &srv)? {
                    servers::ServerHome::Suite => self.save_container(&mut o.suite, SUITE_FILE)?,
                    servers::ServerHome::Drive => self.save_area(o, super::suite::VaultKind::Drive)?,
                }
            }
            Ok(checks)
        })
    }

    /// Shares GhulDrive files or folders. Several items (or a folder) become one ZIP file.
    pub fn share_drive(&self, ids: Vec<String>, name: Option<String>, options: ShareOptions) -> Result<ShareInfo> {
        if ids.is_empty() {
            return Err(GhulError::InvalidInput { msg: "nothing to share".into() });
        }
        self.with_open(|o| {
            let mut files = Vec::new();
            let mut folder = false;
            for id in &ids {
                let got = self.drive_collect(o, id, MAX_SHARE)?;
                folder |= got.len() != 1 || got.first().is_some_and(|(p, _)| p.contains('/'));
                files.extend(got);
            }
            if files.iter().map(|(_, d)| d.len() as u64).sum::<u64>() > MAX_SHARE {
                return Err(GhulError::InvalidInput { msg: "too large to share".into() });
            }
            let (name, content) = if files.len() == 1 && !folder {
                let (n, d) = files.pop().expect("one file");
                (n, d)
            } else {
                let n = name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "GhulSuite-Freigabe".into());
                (format!("{}.zip", n.trim().trim_end_matches(".zip")), zip(&files)?)
            };
            self.share_upload(o, &options, name, &content)
        })
    }

    /// Shares any content, e.g. a file from the device or a login as text.
    pub fn share_bytes(&self, name: String, data: Vec<u8>, options: ShareOptions) -> Result<ShareInfo> {
        let data = Zeroizing::new(data);
        if data.len() as u64 > MAX_SHARE {
            return Err(GhulError::InvalidInput { msg: "too large to share".into() });
        }
        self.with_open(|o| self.share_upload(o, &options, name.clone(), &data))
    }

    /// Active shares; expired ones are removed first. Download counts come from the server
    /// module where available.
    pub fn share_list(&self) -> Result<Vec<ShareInfo>> {
        self.share_cleanup()?;
        self.with_open(|o| {
            let list = shares(o);
            let mut counts = std::collections::BTreeMap::new();
            let mut gone = Vec::new();
            for srv in servers::servers(o).iter().filter(|s| ServerKind::of(s) == ServerKind::Php) {
                if !list.iter().any(|s| s.server == srv.id) {
                    continue;
                }
                if let Ok(remote) = servers::php_fs(srv, "").and_then(|f| Ok(f.share_list()?)) {
                    let ids: std::collections::BTreeSet<&str> = remote.iter().map(|r| r.id.as_str()).collect();
                    for s in list.iter().filter(|s| s.server == srv.id && !ids.contains(s.id.as_str())) {
                        gone.push(s.id.clone()); // used up or expired on the server
                    }
                    for r in remote {
                        counts.insert(r.id, i64::from(r.count));
                    }
                }
            }
            if !gone.is_empty() {
                let keep: Vec<StoredShare> = list.iter().filter(|s| !gone.contains(&s.id)).cloned().collect();
                put_setting(&mut o.suite, SHARES_KEY, &keep)?;
                for id in &gone {
                    super::sync::mark_deleted(&mut o.suite.payload.settings, SHARES_KEY, id);
                }
                self.save_container(&mut o.suite, SUITE_FILE)?;
            }
            Ok(list
                .iter()
                .filter(|s| !gone.contains(&s.id))
                .map(|s| info(s, counts.get(&s.id).copied().unwrap_or(-1)))
                .collect())
        })
    }

    /// Ends a share: the package is deleted from the server at once.
    pub fn share_delete(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let mut list = shares(o);
            if let Some(s) = list.iter().find(|s| s.id == id).cloned() {
                Self::share_remove(o, &s)?;
            }
            list.retain(|s| s.id != id);
            put_setting(&mut o.suite, SHARES_KEY, &list)?;
            super::sync::mark_deleted(&mut o.suite.payload.settings, SHARES_KEY, &id);
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    /// Deletes expired shares from their servers; returns how many. Unreachable servers are
    /// retried next time.
    pub fn share_cleanup(&self) -> Result<u32> {
        self.with_open(|o| {
            let now = now_ms();
            let list = shares(o);
            let mut keep = Vec::new();
            let mut removed = 0;
            for s in list {
                if s.expires > 0 && s.expires < now && Self::share_remove(o, &s).is_ok() {
                    removed += 1;
                } else {
                    keep.push(s);
                }
            }
            if removed > 0 {
                let gone: Vec<String> =
                    shares(o).iter().filter(|s| !keep.iter().any(|k| k.id == s.id)).map(|s| s.id.clone()).collect();
                put_setting(&mut o.suite, SHARES_KEY, &keep)?;
                for id in gone {
                    super::sync::mark_deleted(&mut o.suite.payload.settings, SHARES_KEY, &id);
                }
                self.save_container(&mut o.suite, SUITE_FILE)?;
            }
            Ok(removed)
        })
    }
}
