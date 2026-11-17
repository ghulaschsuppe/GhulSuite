// SPDX-License-Identifier: AGPL-3.0-or-later
//! Share links: end-to-end encrypted, handed out by the GhulSuite server module (PHP) on the
//! server that holds GhulDrive. Nothing is copied.
//!
//! Every file content in GhulDrive has its own key (`drive::FileKey`). A share is a small
//! encrypted package (`crate::share`) with the names, sizes and keys of its files, sealed with
//! a new random link key (and, optionally, a password). The app writes it over SFTP/FTPS as
//! `shares/<id>.json` into the GhulSuite folder, together with the list of the files' block
//! names. The module answers the package and then only those blocks, and enforces expiry,
//! download limit and one-time links; the recipient's browser decrypts block by block. The
//! link key is only in the link after `#`.
//!
//! A share shows the content as it was when it was made: its blocks stay on the server until
//! the link ends, even if the file changes or is deleted. Content that is not in GhulDrive (a
//! login shared as text) gets blocks of its own, deleted with the share.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use super::drive::SharedFile;
use super::error::{GhulError, Result};
use super::servers;
use super::suite::{now_ms, put_setting, setting, GhulSuite, Open, SUITE_FILE};
use crate::net::remote::RemoteFs;
use crate::share::{b64url, seal, ShareMeta};

const SHARES_KEY: &str = "shares";
/// The package of a share holds a list of files, not a file (the recipient page checks this).
pub(crate) const ENVELOPE_TYPE: &str = "application/x-ghulsuite-share";

/// How long after its expiry the app removes a share (see `server_state`).
const CLEANUP_GRACE_MS: i64 = 86_400_000;

/// Detail of a check that could not be made because an earlier step failed. The app compares
/// this text (`ServerSetup.kt`); a test pins it.
pub const SHARE_CHECK_SKIPPED: &str = "skipped";

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
    /// The GhulDrive items it was made of (to ask before they are deleted).
    #[serde(default)]
    items: Vec<String>,
    /// Blocks that belong only to this share (content that is not in GhulDrive).
    #[serde(default)]
    own_blocks: Vec<String>,
}

impl Drop for StoredShare {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.url);
    }
}

/// `shares/<id>.json` on the server: what the module needs to hand out a share.
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct ServerShare {
    /// The sealed package (Base64): names, sizes and keys of the files.
    pub package: String,
    /// Block names of each file, in order.
    pub files: Vec<Vec<String>>,
    /// Unix ms, 0 = no expiry.
    pub exp: i64,
    /// 0 = unlimited.
    pub max: u32,
}

/// `shares/<id>.state`, written by the module: downloads so far and open tickets.
#[derive(Deserialize, Default)]
struct ServerState {
    #[serde(default)]
    count: i64,
    /// Ticket hash → valid until (Unix seconds).
    #[serde(default)]
    tickets: BTreeMap<String, i64>,
}

/// One file in the package.
#[derive(Serialize, Deserialize)]
pub(crate) struct EnvelopeFile {
    pub name: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime: String,
    /// The file key, URL-safe Base64.
    pub key: String,
    /// Its blocks in order: sealed with the package, so a server cannot reorder, drop or
    /// repeat blocks without the recipient noticing.
    #[serde(default)]
    pub blocks: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Envelope {
    pub files: Vec<EnvelopeFile>,
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
    /// 0 = unlimited.
    pub max_downloads: u32,
    /// Downloads so far; -1 while the server cannot be reached.
    pub downloads: i64,
    /// The complete link including the key.
    pub url: String,
}

#[derive(uniffi::Record, Clone)]
pub struct ShareOptions {
    /// Optional second secret; recipients must enter it besides opening the link.
    pub password: Option<String>,
    /// 0 = no expiry.
    pub expires_days: u32,
    /// 0 = unlimited; 1 = one-time link.
    pub max_downloads: u32,
}

super::redact::redacted_debug!(ShareOptions { expires_days, max_downloads });

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

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    OsRng.fill_bytes(&mut b);
    crate::kdf::hex(&b)
}

pub(super) fn mime_of(name: &str) -> &'static str {
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

fn json_path(id: &str) -> String {
    format!("shares/{id}.json")
}

fn state_path(id: &str) -> String {
    format!("shares/{id}.state")
}

/// The block names a description lists, read as leniently as the module reads it (missing
/// or odd `exp`/`max` do not matter; entries that are no block name are skipped).
fn described_blocks(data: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) else { return Vec::new() };
    let Some(files) = v.get("files").and_then(serde_json::Value::as_array) else { return Vec::new() };
    files
        .iter()
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|b| b.len() == 64 && b.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
        .map(str::to_owned)
        .collect()
}

/// Every block a share on this server still hands out (from the descriptions on the server,
/// so shares made on other devices count too), except those of the share `skip`. An error
/// reading them is an error: nothing may be deleted then (a transient failure must not break
/// live links). A description the module could not read either pins nothing.
pub(super) fn pinned_blocks_except(fs: &dyn RemoteFs, skip: Option<&str>) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let skip = skip.map(|id| format!("{id}.json"));
    for e in fs.list("shares")?.iter().filter(|e| !e.is_dir && e.name.ends_with(".json")) {
        if skip.as_deref() == Some(e.name.as_str()) {
            continue;
        }
        let Some(data) = fs.read(&format!("shares/{}", e.name))? else { continue };
        out.extend(described_blocks(&data));
    }
    Ok(out)
}

/// Every block a share on this server still hands out (see `pinned_blocks_except`).
pub(super) fn pinned_blocks(fs: &dyn RemoteFs) -> Result<BTreeSet<String>> {
    pinned_blocks_except(fs, None)
}

/// What the server says about a share: `None` = gone (ended there, expired, used up),
/// otherwise the downloads so far.
fn server_state(fs: &dyn RemoteFs, s: &StoredShare, now: i64) -> Result<Option<i64>> {
    // The description on the server counts (the module enforces it), not the app's copy.
    let Some(described) = fs.read(&json_path(&s.id))? else { return Ok(None) };
    let described: ServerShare = serde_json::from_slice(&described).unwrap_or_default();
    let (exp, max) = (described.exp.max(0), described.max);
    // The module ends the link on time by the server's clock; the app only cleans up, a day
    // later, so a device clock running ahead never deletes a link that still works.
    let late = now - CLEANUP_GRACE_MS;
    if (exp > 0 && exp < late) || (s.expires > 0 && s.expires < late) {
        return Ok(None);
    }
    let state: ServerState =
        fs.read(&state_path(&s.id))?.and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
    // Used up: gone once the last download had time to finish.
    let open_ticket = state.tickets.values().any(|until| until * 1000 > now);
    if max > 0 && state.count >= i64::from(max) && !open_ticket {
        return Ok(None);
    }
    Ok(Some(state.count))
}

impl GhulSuite {
    /// Writes a share for `files` to GhulDrive's server and remembers it.
    fn publish(
        &self,
        o: &mut Open,
        title: String,
        files: Vec<SharedFile>,
        items: Vec<String>,
        own_blocks: Vec<String>,
        opts: &ShareOptions,
    ) -> Result<ShareInfo> {
        let server_id = self.drive_server(o).ok_or(GhulError::NotConfigured)?;
        let srv = servers::server(o, &server_id)?;
        let module = srv
            .module
            .clone()
            .ok_or_else(|| GhulError::InvalidInput { msg: "share links need the server module".into() })?;
        let now = now_ms();
        let expires = if opts.expires_days == 0 { 0 } else { now + i64::from(opts.expires_days) * 86_400_000 };
        let envelope = Envelope {
            files: files
                .iter()
                .map(|f| EnvelopeFile {
                    name: f.path.clone(),
                    size: f.size,
                    mime: mime_of(&f.path).into(),
                    key: b64url(f.key.bytes()),
                    blocks: f.blocks.clone(),
                })
                .collect(),
        };
        let content = zeroize::Zeroizing::new(serde_json::to_vec(&envelope).expect("serializes"));
        let size = files.iter().map(|f| f.size).sum();
        let meta = ShareMeta { name: title.clone(), mime: ENVELOPE_TYPE.into(), size, created: now, expires };
        let password = opts.password.as_deref().filter(|p| !p.is_empty());
        let sealed = seal(&meta, &content, password);
        let id = random_hex(16);
        let described = ServerShare {
            package: crate::net::tls::base64(&sealed.data),
            files: files.iter().map(|f| f.blocks.clone()).collect(),
            exp: expires,
            max: opts.max_downloads,
        };
        let fs = self.drive_fs(o)?;
        fs.write(&json_path(&id), &serde_json::to_vec(&described).expect("serializes"))?;
        let stored = StoredShare {
            url: format!("{}#{id}.{}", module.url, b64url(sealed.link_key.as_ref())),
            id,
            server: srv.id.clone(),
            name: title,
            size,
            created: now,
            expires,
            password: password.is_some(),
            max_downloads: opts.max_downloads,
            items,
            own_blocks,
        };
        let mut list = shares(o);
        list.push(stored.clone());
        put_setting(&mut o.suite, SHARES_KEY, &list)?;
        self.save_container(&mut o.suite, SUITE_FILE)?;
        Ok(info(&stored, 0))
    }

    /// Deletes a share from its server: its description, its state and its own blocks.
    /// `ShareNotDeleted` if its server is no longer GhulDrive's (B14: never report a link as
    /// gone while it may still work).
    fn share_remove(&self, o: &mut Open, s: &StoredShare) -> Result<()> {
        // On the storage location it was made on (also while another one is active).
        let loc = self.drive_location_of(o, &s.server);
        if !servers::drive_servers(o).iter().any(|x| x.id == loc) {
            return Err(GhulError::ShareNotDeleted);
        }
        super::drive::at_location(Some(loc), || self.share_remove_here(o, s))
    }

    fn share_remove_here(&self, o: &mut Open, s: &StoredShare) -> Result<()> {
        let fs = self.drive_fs(o)?;
        // Its blocks: those that no file uses any more (deleted or changed since, or content
        // that never was in GhulDrive) go with it, unless another share still hands them out.
        // Everything that can fail is read first: once the description is gone, its block list
        // is gone too, and a retry could not clean up any more.
        let mut blocks = s.own_blocks.clone();
        if let Some(described) = fs.read(&json_path(&s.id))? {
            blocks.extend(described_blocks(&described));
        }
        let pinned = pinned_blocks_except(fs.as_ref(), Some(&s.id))?;
        let unused = self.drive_unused(o, &blocks)?;
        fs.delete(&json_path(&s.id))?;
        let _ = fs.delete(&state_path(&s.id));
        let _ = fs.delete(&format!("shares/{}.lock", s.id));
        for b in unused.iter().filter(|b| !pinned.contains(*b)) {
            fs.delete(&super::drive::block_path(b))?;
        }
        Ok(())
    }

    /// Ends every share made on the storage location `id` (before it is removed). When its
    /// server cannot be reached, the error says so.
    /// With `force` a share that cannot be ended is forgotten here all the same (its server is
    /// gone for good). Shares ended before an error are forgotten in any case.
    pub(super) fn shares_end_on_location(&self, o: &mut Open, id: &str, force: bool) -> Result<()> {
        let mine: Vec<StoredShare> =
            shares(o).into_iter().filter(|s| self.drive_location_of(o, &s.server) == id).collect();
        let mut gone = Vec::new();
        for s in &mine {
            match self.share_remove(o, s) {
                Ok(()) => gone.push(s.id.clone()),
                Err(_) if force => gone.push(s.id.clone()),
                Err(e) => {
                    self.forget_shares(o, &gone)?;
                    return Err(e);
                }
            }
        }
        self.forget_shares(o, &gone)
    }

    /// Removes shares from the list.
    fn forget_shares(&self, o: &mut Open, gone: &[String]) -> Result<()> {
        if gone.is_empty() {
            return Ok(());
        }
        let keep: Vec<StoredShare> = shares(o).into_iter().filter(|s| !gone.contains(&s.id)).collect();
        put_setting(&mut o.suite, SHARES_KEY, &keep)?;
        self.save_container(&mut o.suite, SUITE_FILE)
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Whether share links can be made: GhulDrive is set up and its server has the server
    /// module. `None` = yes; otherwise the server to install the module on (empty: GhulDrive
    /// is not set up).
    pub fn share_missing_module(&self) -> Result<Option<String>> {
        self.with_open(|o| {
            let Some(id) = self.drive_server(o) else { return Ok(Some(String::new())) };
            let srv = servers::server(o, &id)?;
            Ok(srv.module.is_none().then_some(id))
        })
    }

    /// Shares GhulDrive files or folders as one link; the recipient gets every file on its own
    /// (a folder with its sub-folders). Nothing is copied, so any size works.
    pub fn share_drive(&self, ids: Vec<String>, name: Option<String>, options: ShareOptions) -> Result<ShareInfo> {
        if ids.is_empty() {
            return Err(GhulError::InvalidInput { msg: "nothing to share".into() });
        }
        self.with_open(|o| {
            let files = self.drive_snapshot(o, &ids)?;
            if files.is_empty() {
                return Err(GhulError::InvalidInput { msg: "nothing to share".into() });
            }
            let title = match (&files[..], name.filter(|n| !n.trim().is_empty())) {
                ([one], _) if ids.len() == 1 && !one.path.contains('/') => one.path.clone(),
                (_, Some(n)) => n.trim().to_owned(),
                (all, None) => all[0].path.split('/').next().unwrap_or("GhulSuite").to_owned(),
            };
            self.publish(o, title, files, ids.clone(), Vec::new(), &options)
        })
    }

    /// Shares content that is not in GhulDrive (e.g. a login as text, a file from the device).
    /// It is encrypted into blocks of its own on GhulDrive's server, deleted with the share.
    pub fn share_bytes(&self, name: String, data: Vec<u8>, options: ShareOptions) -> Result<ShareInfo> {
        let data = zeroize::Zeroizing::new(data);
        self.with_open(|o| {
            // The module first: nothing is written to the server without it.
            let server_id = self.drive_server(o).ok_or(GhulError::NotConfigured)?;
            if servers::server(o, &server_id)?.module.is_none() {
                return Err(GhulError::InvalidInput { msg: "share links need the server module".into() });
            }
            let file = self.drive_put_loose(o, &name, &data)?;
            let own = file.blocks.clone();
            match self.publish(o, name.clone(), vec![file], Vec::new(), own.clone(), &options) {
                Ok(i) => Ok(i),
                Err(e) => {
                    let fs = self.drive_fs(o)?;
                    let _ = self.drive_delete_loose(fs.as_ref(), &own);
                    Err(e)
                }
            }
        })
    }

    /// Active shares with their downloads. Shares that ended on the server (expired, used up)
    /// are removed here, together with what they left on the server. While the server cannot
    /// be reached, everything stays and downloads are -1.
    pub fn share_list(&self) -> Result<Vec<ShareInfo>> {
        self.with_open(|o| {
            let now = now_ms();
            let list = shares(o);
            let own: Vec<String> = servers::drive_servers(o).into_iter().map(|x| x.id.clone()).collect();
            // One connection per storage location (a location that cannot be reached: -1).
            let mut conns: BTreeMap<String, Option<Arc<dyn RemoteFs>>> = BTreeMap::new();
            let mut out = Vec::new();
            let mut gone = Vec::new();
            for s in &list {
                let loc = self.drive_location_of(o, &s.server);
                // The app shows a share with the location it belongs to.
                let with_loc = |mut i: ShareInfo| {
                    i.server_id = loc.clone();
                    i
                };
                if !own.contains(&loc) {
                    out.push(with_loc(info(s, -1)));
                    continue;
                }
                let fs = match conns.get(&loc) {
                    Some(fs) => fs.clone(),
                    None => {
                        let fs = super::drive::at_location(Some(loc.clone()), || self.drive_fs(o)).ok();
                        conns.insert(loc.clone(), fs.clone());
                        fs
                    }
                };
                let state = fs.and_then(|fs| server_state(fs.as_ref(), s, now).ok());
                match state {
                    Some(None) => {
                        if self.share_remove(o, s).is_ok() {
                            gone.push(s.id.clone());
                        }
                    }
                    Some(Some(count)) => out.push(with_loc(info(s, count))),
                    None => out.push(with_loc(info(s, -1))),
                }
            }
            self.forget_shares(o, &gone)?;
            Ok(out)
        })
    }

    /// Ends a share: its link stops working at once. If the server cannot be reached, nothing
    /// changes (try again). If GhulDrive moved to another server, the entry goes but
    /// `ShareNotDeleted` says that the link may keep working on the old one.
    pub fn share_delete(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let found = shares(o).into_iter().find(|s| s.id == id);
            let removed = match found {
                Some(s) => match self.share_remove(o, &s) {
                    Err(GhulError::ShareNotDeleted) => Err(GhulError::ShareNotDeleted),
                    Err(e) => return Err(e),
                    Ok(()) => Ok(()),
                },
                None => Ok(()),
            };
            self.forget_shares(o, std::slice::from_ref(&id))?;
            removed
        })
    }

    /// The shares that contain one of these GhulDrive items (or something inside them): the app
    /// asks before deleting such items, and ends the links.
    pub fn share_links_of(&self, ids: Vec<String>) -> Result<Vec<ShareInfo>> {
        self.with_open(|o| {
            let mut out = Vec::new();
            for s in shares(o) {
                let mut hit = false;
                for item in &s.items {
                    if self.drive_within(o, item, &ids)? {
                        hit = true;
                        break;
                    }
                }
                if hit {
                    out.push(info(&s, -1));
                }
            }
            Ok(out)
        })
    }
}
