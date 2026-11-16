// SPDX-License-Identifier: AGPL-3.0-or-later
//! Own servers (SFTP or FTPS) for synchronisation and GhulDrive, each optionally with the
//! GhulSuite server module (PHP) for share links, installed over the same login.
//!
//! The servers for synchronisation and the SSH login key live in the suite's encrypted settings,
//! so nobody can see which servers are set up without unlocking. A server that only GhulDrive
//! uses lives in the Drive file instead. The SSH key is the suite's own Ed25519 key; its public
//! half is added to `~/.ssh/authorized_keys` on the server.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::areas::{area_setting, put_area_setting};
use super::error::{GhulError, Result};
use super::suite::{new_id, put_setting, setting, GhulSuite, Open, VaultKind};
use std::sync::Arc;

use crate::net::ftps::{FtpsConfig, FtpsFs};
use crate::net::remote::RemoteFs;
use crate::net::sftp::{generate_key, public_key_of, SftpConfig, SftpFs, SshAuth};
use crate::net::NetError;

/// The server list, in the suite file (synchronisation) and in the Drive file (Drive's own server).
pub(super) const SERVERS_KEY: &str = "servers";
const SSH_KEY: &str = "ssh_key";

#[derive(Serialize, Deserialize, Clone, Default)]
pub(super) struct StoredServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Empty: log in with the suite's SSH key.
    #[serde(default)]
    pub password: String,
    /// Folder on the server for GhulSuite (suite files under `suite/`, Drive under `drive/`).
    pub base: String,
    pub host_key: String,
    /// Absolute path of `base` on the server, as reported by the server (for display).
    #[serde(default)]
    pub location: String,
    /// "sftp" (also when empty) or "ftps".
    #[serde(default)]
    pub kind: String,
    /// The server module for share links on this server, if installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<StoredModule>,
    /// Last edit (Unix ms): the later edit wins when devices synchronise (B13).
    #[serde(default)]
    pub changed: i64,
}

/// The server module of a server: where its PHP file is and the address it answers at.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub(super) struct StoredModule {
    /// Address of `ghulsuite.php`, e.g. `https://example.org/ghulsuite.php`.
    pub url: String,
    /// Folder of the PHP file, relative to the login folder.
    pub dir: String,
    pub version: u32,
}

/// How GhulSuite reaches a server.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerKind {
    /// SSH/SFTP: own server, NAS, root server. Password or the suite's SSH key.
    Sftp,
    /// FTP with TLS (explicit, port 21): typical web hosting.
    Ftps,
}

impl ServerKind {
    fn as_str(self) -> &'static str {
        match self {
            ServerKind::Sftp => "sftp",
            ServerKind::Ftps => "ftps",
        }
    }

    pub(super) fn of(s: &StoredServer) -> ServerKind {
        match s.kind.as_str() {
            "ftps" => ServerKind::Ftps,
            _ => ServerKind::Sftp,
        }
    }
}

/// Result of "check connection" for a server: nothing is stored.
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum ServerTest {
    /// First contact: the user compares this fingerprint and checks again with it confirmed.
    Confirm { fingerprint: String },
    /// Login works and the folder can be written: the server may be saved.
    Ok,
}

/// Result of checking a server.
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum ServerSaved {
    /// First contact: the user compares this fingerprint (SSH host key or TLS certificate) and
    /// calls again with it confirmed. Nothing is stored yet.
    Confirm {
        fingerprint: String,
    },
    Saved {
        id: String,
    },
}

/// A server as entered by the user.
#[derive(uniffi::Record, Clone)]
pub struct ServerInput {
    pub kind: ServerKind,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// SFTP: `None` = log in with the suite's SSH key (recommended). FTPS: the password.
    pub password: Option<String>,
    pub base: String,
}

super::redact::redacted_debug!(ServerInput { kind, name, host, port, user, base });

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ServerInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub base: String,
    pub uses_key: bool,
    pub host_key: String,
    /// Where GhulSuite's folder is on the server, e.g. `/home/max/ghulsuite`.
    pub location: String,
    pub kind: ServerKind,
    /// Address of the server module (share links); empty = not installed.
    pub module_url: String,
    /// Version of the installed module (0 = none).
    pub module_version: u32,
    /// The version this app brings: an older module can be updated.
    pub module_current: u32,
}

impl Drop for StoredServer {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.password);
    }
}

/// Servers in the suite file (offered for synchronisation).
pub(super) fn servers(o: &Open) -> Vec<StoredServer> {
    let list: Vec<StoredServer> = setting(&o.suite, SERVERS_KEY);
    // Older apps also stored the server module alone as a server: no longer a server of its own.
    list.into_iter().filter(|s| s.kind != "php").collect()
}

/// GhulDrive's own server (in the Drive file); at most one.
pub(super) fn drive_servers(o: &Open) -> Vec<StoredServer> {
    let list: Vec<StoredServer> = area_setting(o, VaultKind::Drive, SERVERS_KEY);
    list.into_iter().filter(|s| s.kind != "php").collect()
}

/// Every server this suite knows: the suite's and the Drive file's.
pub(super) fn all_servers(o: &Open) -> Vec<StoredServer> {
    let mut list = servers(o);
    list.extend(drive_servers(o));
    list
}

fn ssh_private_key(o: &mut Open) -> Result<Zeroizing<String>> {
    let stored: String = setting(&o.suite, SSH_KEY);
    if !stored.is_empty() {
        return Ok(Zeroizing::new(stored));
    }
    let (private, _) = generate_key("GhulSuite").map_err(GhulError::from)?;
    put_setting(&mut o.suite, SSH_KEY, &private.as_str())?;
    Ok(private)
}

pub(super) fn sftp_config(o: &mut Open, s: &StoredServer) -> Result<SftpConfig> {
    let auth = if s.password.is_empty() {
        SshAuth::Key(ssh_private_key(o)?)
    } else {
        SshAuth::Password(Zeroizing::new(s.password.clone()))
    };
    Ok(SftpConfig {
        host: s.host.clone(),
        port: s.port,
        user: s.user.clone(),
        auth,
        base: s.base.clone(),
        host_key: (!s.host_key.is_empty()).then(|| s.host_key.clone()),
    })
}

/// Opens a stored server with its own folder, or with `base` instead (e.g. the login folder).
pub(super) fn open_fs(o: &mut Open, s: &StoredServer, base: Option<&str>) -> Result<Arc<dyn RemoteFs>> {
    let base = base.map(str::to_owned).unwrap_or_else(|| s.base.clone());
    Ok(match ServerKind::of(s) {
        ServerKind::Sftp => {
            let mut cfg = sftp_config(o, s)?;
            cfg.base = base;
            Arc::new(SftpFs::connect(&cfg)?)
        }
        ServerKind::Ftps => Arc::new(FtpsFs::connect(&FtpsConfig {
            host: s.host.clone(),
            port: s.port,
            user: s.user.clone(),
            password: Zeroizing::new(s.password.clone()),
            base,
            cert: (!s.host_key.is_empty()).then(|| s.host_key.clone()),
        })?),
    })
}

/// Connects to a stored server.
pub(super) fn connect(o: &mut Open, server_id: &str) -> Result<Arc<dyn RemoteFs>> {
    let s = all_servers(o).into_iter().find(|s| s.id == server_id).ok_or(GhulError::NotConfigured)?;
    open_fs(o, &s, None)
}

pub(super) fn server(o: &Open, server_id: &str) -> Result<StoredServer> {
    all_servers(o).into_iter().find(|s| s.id == server_id).ok_or(GhulError::NotFound)
}

/// Where a changed server is stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ServerHome {
    Suite,
    Drive,
}

/// Replaces a stored server with `changed` (same id) in the file it lives in.
pub(super) fn update_server(o: &mut Open, changed: &StoredServer) -> Result<ServerHome> {
    let mut list = servers(o);
    let mut changed = changed.clone();
    changed.changed = super::suite::now_ms();
    let changed = &changed;
    if let Some(s) = list.iter_mut().find(|s| s.id == changed.id) {
        *s = changed.clone();
        put_setting(&mut o.suite, SERVERS_KEY, &list)?;
        return Ok(ServerHome::Suite);
    }
    let mut list = drive_servers(o);
    if let Some(s) = list.iter_mut().find(|s| s.id == changed.id) {
        *s = changed.clone();
        put_area_setting(o, VaultKind::Drive, SERVERS_KEY, &list)?;
        return Ok(ServerHome::Drive);
    }
    Err(GhulError::NotFound)
}

/// A server as entered, before it is stored.
fn stored_from(input: &ServerInput, confirmed: Option<&str>) -> StoredServer {
    let kind = input.kind;
    let default_port = match kind {
        ServerKind::Sftp => 22,
        ServerKind::Ftps => 21,
    };
    let host = input.host.trim().to_owned();
    StoredServer {
        id: crate::kdf::hex(&new_id()[..8]),
        name: if input.name.trim().is_empty() { host.clone() } else { input.name.trim().to_owned() },
        host,
        port: if input.port == 0 { default_port } else { input.port },
        user: input.user.trim().to_owned(),
        password: input.password.clone().unwrap_or_default(),
        base: input.base.trim().trim_end_matches('/').to_owned(),
        host_key: confirmed.unwrap_or_default().to_owned(),
        location: String::new(),
        kind: kind.as_str().to_owned(),
        module: None,
        changed: 0,
    }
}

/// Opens a server as entered. `Err(fingerprint)` inside `Ok` means: confirm this first.
fn open_input(
    o: &mut Open,
    s: &StoredServer,
    confirmed: bool,
) -> Result<std::result::Result<Arc<dyn RemoteFs>, String>> {
    match open_fs(o, s, None) {
        Ok(fs) => Ok(Ok(fs)),
        Err(GhulError::HostKeyUnknown { fingerprint }) | Err(GhulError::CertUntrusted { fingerprint })
            if !confirmed =>
        {
            Ok(Err(fingerprint))
        }
        Err(e) => Err(e),
    }
}

impl GhulSuite {
    /// Stores a changed server in the file it lives in and saves that file.
    pub(super) fn store_server(&self, o: &mut Open, srv: &StoredServer) -> Result<()> {
        match update_server(o, srv)? {
            ServerHome::Suite => self.save_container(&mut o.suite, super::suite::SUITE_FILE),
            ServerHome::Drive => self.save_area(o, VaultKind::Drive),
        }
    }

    /// A connection to a stored server, reused while it works.
    pub(super) fn remote(
        &self,
        o: &mut Open,
        server_id: &str,
    ) -> Result<std::sync::Arc<dyn crate::net::remote::RemoteFs>> {
        let mut cache = self.net.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((id, fs)) = &cache.conn {
            if id == server_id && fs.list("").is_ok() {
                return Ok(fs.clone());
            }
        }
        let fs = connect(o, server_id)?;
        cache.conn = Some((server_id.to_owned(), fs.clone()));
        Ok(fs)
    }
}

pub(super) fn info(s: &StoredServer) -> ServerInfo {
    ServerInfo {
        id: s.id.clone(),
        name: s.name.clone(),
        host: s.host.clone(),
        port: s.port,
        user: s.user.clone(),
        base: s.base.clone(),
        uses_key: ServerKind::of(s) == ServerKind::Sftp && s.password.is_empty(),
        host_key: s.host_key.clone(),
        location: if s.location.is_empty() { s.base.clone() } else { s.location.clone() },
        kind: ServerKind::of(s),
        module_url: s.module.as_ref().map(|m| m.url.clone()).unwrap_or_default(),
        module_version: s.module.as_ref().map_or(0, |m| m.version),
        module_current: crate::net::module::MODULE_VERSION,
    }
}

/// Folders of a server as entered.
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum ServerFolders {
    /// First contact: compare this fingerprint and call again with it confirmed.
    Confirm {
        fingerprint: String,
    },
    Folders {
        folders: Vec<String>,
    },
}

/// Sub-folders of `path` (no hidden ones), sorted by name.
fn folders(fs: &dyn RemoteFs, path: &str) -> Result<Vec<String>> {
    let mut dirs: Vec<String> = fs
        .list(path.trim_matches('/'))?
        .into_iter()
        .filter(|e| e.is_dir && !e.name.starts_with('.'))
        .map(|e| e.name)
        .collect();
    dirs.sort_by_key(|d| d.to_lowercase());
    Ok(dirs)
}

/// Folders web hostings deliver as the website's root.
const WEB_ROOTS: &[&str] = &["public_html", "htdocs", "httpdocs", "html", "www", "web", "public"];

/// Likely web addresses of the module uploaded into `dir`, the most likely first. A folder
/// named like a domain is the website's root on many hostings (`example.org/tools` →
/// `https://example.org/tools/`); otherwise the server's host without `ftp.`/`sftp.`/`ssh.`
/// with the usual root folders left out.
pub(super) fn module_urls(host: &str, dir: &str) -> Vec<String> {
    let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let file = "ghulsuite.php";
    let join = |host: &str, rest: &[&str]| {
        let mut path = rest.join("/");
        if !path.is_empty() {
            path.push('/');
        }
        format!("https://{host}/{path}{file}")
    };
    let is_domain = |p: &str| {
        p.contains('.')
            && !p.starts_with('.')
            && !p.ends_with('.')
            && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            && p.rsplit('.').next().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
    };
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate().rev() {
        if is_domain(p) {
            let rest: Vec<&str> = parts[i + 1..].iter().copied().skip_while(|r| WEB_ROOTS.contains(r)).collect();
            out.push(join(p, &rest));
        }
    }
    let host = host.trim();
    let host = ["ftp.", "sftp.", "ssh."].iter().find_map(|p| host.strip_prefix(p)).unwrap_or(host);
    if !host.is_empty() {
        let rest: Vec<&str> = parts.iter().copied().skip_while(|r| WEB_ROOTS.contains(r)).collect();
        out.push(join(host, &rest));
        if !host.starts_with("www.") {
            out.push(join(&format!("www.{host}"), &rest));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|u| seen.insert(u.clone()));
    out
}

/// Where the web could hand out `file` of the GhulSuite folder `base`: next to the module
/// (`rel` = the folder as seen from the module), and at the addresses the folder itself would
/// have as a website folder (on the module's host and on the server's), e.g. when the module
/// answers on a subdomain whose root is another folder. At most 8, without duplicates.
pub(super) fn folder_addresses(module: &str, host: &str, base: &str, rel: &str, file: &str) -> Vec<String> {
    let mut out: Vec<String> = crate::net::module::resolve(module, &format!("{rel}/{file}")).into_iter().collect();
    let origin = module.split_once("://").map(|(scheme, rest)| {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        format!("{scheme}://{}", &rest[..end])
    });
    let parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if let Some(origin) = origin {
        for n in 1..=parts.len().min(3) {
            let tail: Vec<String> =
                parts[parts.len() - n..].iter().map(|p| crate::net::module::rawurlencode(p)).collect();
            out.push(format!("{origin}/{}/{file}", tail.join("/")));
        }
    }
    for guess in module_urls(host, base) {
        if let Some(dir) = guess.strip_suffix(MODULE_FILE) {
            out.push(format!("{dir}{file}"));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|u| !u.contains("/../") && seen.insert(u.clone()));
    out.truncate(8);
    out
}

/// One step of installing the server module.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ModuleCheck {
    /// `upload` (file written), `https` (the module answers at the address), `php` (PHP runs
    /// there), `folder` (it reads the GhulSuite folder of this server and may write there;
    /// detail `no-write` if it may not), `private` (the web server does not hand out the
    /// GhulSuite folder itself; detail: the address where it does).
    pub step: String,
    pub ok: bool,
    pub detail: String,
}

/// Result of installing the server module.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ModuleInstalled {
    /// The address it answers at; `None` when it does not work yet (see `checks`; with another
    /// address, call again).
    pub url: Option<String>,
    /// The addresses that were tried, the most likely first.
    pub tried: Vec<String>,
    pub checks: Vec<ModuleCheck>,
}

const MODULE_FILE: &str = "ghulsuite.php";
/// Detail of the `folder` step when PHP can read the GhulSuite folder but not write there.
pub const MODULE_NO_WRITE: &str = "no-write";

/// Folder path without empty parts and without "." (".." stays as it is).
fn parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|p| !p.is_empty() && *p != ".").collect()
}

/// The path of `to` as seen from the folder `from` (both absolute): `../ghulsuite`.
pub(super) fn relative_path(from: &str, to: &str) -> Option<String> {
    let (f, t) = (parts(from), parts(to));
    if f.contains(&"..") || t.contains(&"..") {
        return None;
    }
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = std::iter::repeat("..").take(f.len() - common).collect();
    out.extend(&t[common..]);
    Some(if out.is_empty() { ".".into() } else { out.join("/") })
}

/// An address the module may be installed at: https, or a local test server.
fn module_address_ok(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
}

/// Checks the required fields of a server as entered.
fn check_input(input: &ServerInput) -> Result<()> {
    if input.host.trim().is_empty() || input.user.trim().is_empty() {
        return Err(GhulError::InvalidInput { msg: "host and user required".into() });
    }
    Ok(())
}

/// Checks a server as entered: logs in, checks that its folder is writable and fills in where
/// the folder is. `Err(fingerprint)` inside `Ok`: confirm this first.
pub(super) fn check_server(
    o: &mut Open,
    input: &ServerInput,
    confirmed: Option<&str>,
) -> Result<std::result::Result<StoredServer, String>> {
    check_input(input)?;
    let mut stored = stored_from(input, confirmed);
    let fs = match open_input(o, &stored, confirmed.is_some())? {
        Ok(fs) => fs,
        Err(fingerprint) => return Ok(Err(fingerprint)),
    };
    fs.write("suite/.ghulsuite-check", b"ok")?;
    fs.delete("suite/.ghulsuite-check")?;
    stored.location = match ServerKind::of(&stored) {
        ServerKind::Sftp => {
            let cfg = sftp_config(o, &stored)?;
            SftpFs::connect(&cfg).ok().and_then(|f| f.absolute("").ok()).unwrap_or_default()
        }
        ServerKind::Ftps => format!("/{}", stored.base),
    };
    Ok(Ok(stored))
}

/// A new SSH key pair for a device that has no suite yet (to fetch the suite from the server).
#[derive(uniffi::Record, Clone)]
pub struct SshKeyPair {
    pub private_key: String,
    pub public_key: String,
}

#[uniffi::export]
pub fn generate_ssh_key() -> Result<SshKeyPair> {
    let (private, public) = generate_key("GhulSuite device")?;
    Ok(SshKeyPair { private_key: private.to_string(), public_key: public })
}

#[uniffi::export]
impl GhulSuite {
    /// New device: downloads the suite files from `<base>/suite/` on the server into the
    /// (empty) local folder. SFTP logs in with `input.password` or the given `private_key` (from
    /// `generate_ssh_key`), FTPS with `input.password` (U7). Without `host_key` an unknown SFTP host key or an
    /// untrusted FTPS certificate is returned as fingerprint to confirm and nothing is downloaded;
    /// otherwise it returns an empty string. Then unlock as usual.
    pub fn fetch_from_server(
        &self,
        input: ServerInput,
        host_key: Option<String>,
        private_key: Option<String>,
    ) -> Result<String> {
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        use crate::net::remote::RemoteFs;
        let base = input.base.trim().trim_end_matches('/').to_owned();
        let fs: Arc<dyn RemoteFs> = match input.kind {
            ServerKind::Sftp => {
                let auth = match (&input.password, private_key) {
                    (Some(p), _) if !p.is_empty() => SshAuth::Password(Zeroizing::new(p.clone())),
                    (_, Some(k)) => SshAuth::Key(Zeroizing::new(k)),
                    _ => return Err(GhulError::InvalidInput { msg: "password or key required".into() }),
                };
                let cfg = SftpConfig {
                    host: input.host.trim().to_owned(),
                    port: if input.port == 0 { 22 } else { input.port },
                    user: input.user.trim().to_owned(),
                    auth,
                    base,
                    host_key: host_key.clone(),
                };
                match SftpFs::connect(&cfg) {
                    Err(NetError::HostKeyUnknown(fp)) if host_key.is_none() => return Ok(fp),
                    other => Arc::new(other?),
                }
            }
            ServerKind::Ftps => {
                let password = input.password.clone().filter(|p| !p.is_empty());
                let Some(password) = password else {
                    return Err(GhulError::InvalidInput { msg: "password required".into() });
                };
                let cfg = FtpsConfig {
                    host: input.host.trim().to_owned(),
                    port: if input.port == 0 { 21 } else { input.port },
                    user: input.user.trim().to_owned(),
                    password: Zeroizing::new(password),
                    base,
                    cert: host_key.clone(),
                };
                match FtpsFs::connect(&cfg) {
                    Err(NetError::CertUntrusted(fp)) if host_key.is_none() => return Ok(fp),
                    other => Arc::new(other?),
                }
            }
        };
        let files: Vec<String> = fs
            .list("suite")?
            .into_iter()
            .filter(|e| !e.is_dir && (super::suite::is_vault_file(&e.name) || e.name == super::suite::SUITE_FILE))
            .filter(|e| super::storage::plain_file_name(&e.name))
            .map(|e| e.name)
            .collect();
        if files.is_empty() {
            return Err(GhulError::NoVault);
        }
        for f in files {
            if let Some(data) = fs.read(&format!("suite/{f}"))? {
                self.storage.write(f, data)?;
            }
        }
        Ok(String::new())
    }

    /// The suite's SSH public key (created on first use), to add to `authorized_keys` on the server.
    pub fn ssh_public_key(&self) -> Result<String> {
        self.with_open(|o| {
            let had: String = setting(&o.suite, SSH_KEY);
            let private = ssh_private_key(o)?;
            if had.is_empty() {
                self.save_container(&mut o.suite, super::suite::SUITE_FILE)?;
            }
            Ok(public_key_of(&private)?)
        })
    }

    /// "Check connection": logs in and checks that the folder can be written, without storing
    /// anything. On first contact it returns the fingerprint to confirm.
    pub fn server_test(&self, input: ServerInput, confirmed: Option<String>) -> Result<ServerTest> {
        self.with_open(|o| {
            Ok(match check_server(o, &input, confirmed.as_deref())? {
                Ok(_) => ServerTest::Ok,
                Err(fingerprint) => ServerTest::Confirm { fingerprint },
            })
        })
    }

    /// Checks a server and stores it. On first contact it returns the fingerprint of the SSH host
    /// key (SFTP) or of a not publicly trusted TLS certificate (FTPS) for the user to compare;
    /// call again with it in `confirmed`. Then it logs in, checks that the folder is writable and
    /// stores the server.
    pub fn save_server(&self, input: ServerInput, confirmed: Option<String>) -> Result<ServerSaved> {
        self.with_open(|o| {
            let stored = match check_server(o, &input, confirmed.as_deref())? {
                Ok(s) => s,
                Err(fingerprint) => return Ok(ServerSaved::Confirm { fingerprint }),
            };
            let mut list = servers(o);
            list.retain(|s| {
                !(s.kind == stored.kind
                    && s.host == stored.host
                    && s.port == stored.port
                    && s.user == stored.user
                    && s.base == stored.base)
            });
            let id = stored.id.clone();
            let mut stored = stored;
            stored.changed = super::suite::now_ms();
            list.push(stored);
            put_setting(&mut o.suite, SERVERS_KEY, &list)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            self.save_container(&mut o.suite, super::suite::SUITE_FILE)?;
            Ok(ServerSaved::Saved { id })
        })
    }

    /// Folders on a server as entered, before it is stored (for the folder picker of the server
    /// form). `path` is relative to the login folder. SFTP and FTPS only.
    pub fn server_browse_input(
        &self,
        input: ServerInput,
        confirmed: Option<String>,
        path: String,
    ) -> Result<ServerFolders> {
        check_input(&input)?;
        self.with_open(|o| {
            let mut stored = stored_from(&input, confirmed.as_deref());
            stored.base = String::new();
            let fs = match open_input(o, &stored, confirmed.is_some())? {
                Ok(fs) => fs,
                Err(fingerprint) => return Ok(ServerFolders::Confirm { fingerprint }),
            };
            Ok(ServerFolders::Folders { folders: folders(fs.as_ref(), &path)? })
        })
    }

    /// Folders on a stored server below its login folder (for choosing the module's folder).
    pub fn server_browse(&self, server_id: String, path: String) -> Result<Vec<String>> {
        self.with_open(|o| {
            let s = server(o, &server_id)?;
            let fs = open_fs(o, &s, Some(""))?;
            folders(fs.as_ref(), &path)
        })
    }

    /// Installs the server module (share links) on a stored server over its own login: writes
    /// `ghulsuite.php` into `dir` (relative to the login folder, inside the website) with the
    /// path to this server's GhulSuite folder, and checks that it answers at `url` (empty: the
    /// usual addresses are tried), that PHP runs there and that it can read the GhulSuite
    /// folder. Only when everything works is it remembered for the server.
    pub fn server_module_install(&self, server_id: String, dir: String, url: String) -> Result<ModuleInstalled> {
        // Relative to the login folder, or absolute (SFTP).
        let dir = dir.trim().trim_end_matches('/').to_owned();
        let url = url.trim().to_owned();
        if !url.is_empty() && !module_address_ok(&url) {
            return Err(GhulError::InvalidInput { msg: "the address must start with https://".into() });
        }
        let (srv, web, base_fs, base_path) = self.with_open(|o| {
            let srv = server(o, &server_id)?;
            let web = open_fs(o, &srv, Some(&dir))?;
            let base_fs = open_fs(o, &srv, None)?;
            // Where the login folder is: SFTP knows it, FTPS starts at "/".
            let home = match ServerKind::of(&srv) {
                ServerKind::Sftp => {
                    let cfg = SftpConfig { base: String::new(), ..sftp_config(o, &srv)? };
                    SftpFs::connect(&cfg)?.absolute("")?
                }
                ServerKind::Ftps => "/".to_owned(),
            };
            let abs =
                |p: &str| if p.starts_with('/') { p.to_owned() } else { format!("{}/{p}", home.trim_end_matches('/')) };
            let base_path = relative_path(&abs(&dir), &abs(&srv.base))
                .ok_or_else(|| GhulError::InvalidInput { msg: "invalid folder".into() })?;
            Ok((srv, web, base_fs, base_path))
        })?;
        let file = crate::net::module::module_file(&base_path)
            .ok_or_else(|| GhulError::InvalidInput { msg: "invalid folder".into() })?;
        let path = if dir.is_empty() { MODULE_FILE.to_owned() } else { format!("{dir}/{MODULE_FILE}") };
        web.write(MODULE_FILE, file.as_bytes())?;
        // Should the GhulSuite folder be inside the website, the web server must not deliver it.
        let _ = base_fs.write(".htaccess", b"Require all denied\nDeny from all\n");
        let mut checks = vec![ModuleCheck { step: "upload".into(), ok: true, detail: path.clone() }];
        let mut name = [0u8; 16];
        let mut token = [0u8; 16];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut name);
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut token);
        let (name, token) = (crate::kdf::hex(&name), crate::kdf::hex(&token));
        let check_file = format!("shares/check-{name}");
        base_fs.write(&check_file, token.as_bytes())?;
        let tried = if url.is_empty() { module_urls(&srv.host, &dir) } else { vec![url] };
        let answering = tried.iter().find_map(|u| {
            crate::net::module::probe(u)
                .ok()
                .filter(|i| i.version == crate::net::module::MODULE_VERSION)
                .map(|_| u.clone())
        });
        let checked = answering
            .as_ref()
            .and_then(|u| crate::net::module::check(u, &name).ok().flatten())
            .filter(|c| c.token == token);
        // The GhulSuite folder must not be reachable from the web: try where it would be.
        let exposed = answering.as_ref().and_then(|u| {
            folder_addresses(u, &srv.host, &srv.base, &base_path, &check_file)
                .into_iter()
                .find(|at| crate::net::module::exposes(at, &token))
        });
        let _ = base_fs.delete(&check_file);
        let shown = answering.clone().unwrap_or_else(|| tried.first().cloned().unwrap_or_default());
        if answering.is_none() && tried.iter().any(|u| crate::net::module::reveals_source(u)) {
            // L5: delivered as text instead of run: take the file down again.
            let _ = web.delete(MODULE_FILE);
            checks.push(ModuleCheck { step: "php".into(), ok: false, detail: shown });
            return Ok(ModuleInstalled { url: None, tried, checks });
        }
        checks.push(ModuleCheck { step: "https".into(), ok: answering.is_some(), detail: shown.clone() });
        let folder_detail = match &checked {
            _ if answering.is_none() => super::shares::SHARE_CHECK_SKIPPED.to_owned(),
            Some(c) if !c.writes => MODULE_NO_WRITE.to_owned(),
            _ => String::new(),
        };
        let reads = checked.as_ref().is_some_and(|c| c.writes);
        checks.push(ModuleCheck { step: "folder".into(), ok: reads, detail: folder_detail });
        if reads {
            checks.push(ModuleCheck {
                step: "private".into(),
                ok: exposed.is_none(),
                detail: exposed.clone().unwrap_or_default(),
            });
        }
        if exposed.is_some() {
            // Links would work, but anyone could fetch the folder: no module until it is moved
            // (also not one installed earlier there: the app must not offer links any more).
            let _ = web.delete(MODULE_FILE);
            if srv.module.as_ref().is_some_and(|m| m.dir == dir) {
                self.with_open(|o| {
                    let mut srv = srv.clone();
                    srv.module = None;
                    self.store_server(o, &srv)
                })?;
            }
        }
        if !reads || exposed.is_some() {
            return Ok(ModuleInstalled { url: None, tried, checks });
        }
        let url = answering.expect("checked");
        self.with_open(|o| {
            let mut srv = srv.clone();
            srv.module =
                Some(StoredModule { url: url.clone(), dir: dir.clone(), version: crate::net::module::MODULE_VERSION });
            self.store_server(o, &srv)
        })?;
        Ok(ModuleInstalled { url: Some(url), tried, checks })
    }

    /// Writes the module of this app's version over an older one (same place, same folder).
    /// Returns the version it answers with afterwards.
    pub fn server_module_update(&self, server_id: String) -> Result<u32> {
        let m = self.with_open(|o| server(o, &server_id)?.module.clone().ok_or(GhulError::NotConfigured))?;
        let done = self.server_module_install(server_id, m.dir.clone(), m.url.clone())?;
        if done.url.is_none() {
            return Err(GhulError::Network { msg: "the server module does not answer".into() });
        }
        Ok(crate::net::module::MODULE_VERSION)
    }

    /// Removes the server module from the server (links of this server stop working).
    pub fn server_module_remove(&self, server_id: String) -> Result<()> {
        self.with_open(|o| {
            let mut srv = server(o, &server_id)?;
            if let Some(m) = srv.module.take() {
                open_fs(o, &srv, Some(&m.dir))?.delete(MODULE_FILE)?;
            }
            self.store_server(o, &srv)
        })
    }

    /// Logs in once with the password in `input` and adds the suite's SSH key to
    /// `~/.ssh/authorized_keys` (appended; existing keys stay). The password is not stored;
    /// afterwards save the server without password. Without `host_key` it returns the server's
    /// key fingerprint to confirm; afterwards an empty string.
    pub fn install_ssh_key(&self, input: ServerInput, host_key: Option<String>) -> Result<String> {
        let password = input
            .password
            .clone()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| GhulError::InvalidInput { msg: "password required".into() })?;
        self.with_open(|o| {
            let private = ssh_private_key(o)?;
            let public = public_key_of(&private)?;
            let cfg = SftpConfig {
                host: input.host.trim().to_owned(),
                port: if input.port == 0 { 22 } else { input.port },
                user: input.user.trim().to_owned(),
                auth: SshAuth::Password(Zeroizing::new(password.clone())),
                base: String::new(),
                host_key: host_key.clone(),
            };
            let fs = match SftpFs::connect(&cfg) {
                Err(NetError::HostKeyUnknown(fp)) if host_key.is_none() => return Ok(fp),
                other => other?,
            };
            use crate::net::remote::RemoteFs;
            let body = public.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
            let existing = fs.read(".ssh/authorized_keys")?.unwrap_or_default();
            if !String::from_utf8_lossy(&existing).contains(&body) {
                fs.append_line(".ssh/authorized_keys", &public)?;
            }
            // The key is new in the settings if it was just created.
            self.save_container(&mut o.suite, super::suite::SUITE_FILE)?;
            Ok(String::new())
        })
    }

    pub fn list_servers(&self) -> Result<Vec<ServerInfo>> {
        self.with_open(|o| Ok(servers(o).iter().map(info).collect()))
    }

    pub fn remove_server(&self, id: String) -> Result<()> {
        self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
        self.with_open(|o| {
            if drive_servers(o).iter().any(|s| s.id == id) {
                return Err(GhulError::InvalidInput { msg: "GhulDrive's server is changed in GhulDrive".into() });
            }
            let mut list = servers(o);
            list.retain(|s| s.id != id);
            put_setting(&mut o.suite, SERVERS_KEY, &list)?;
            super::sync::mark_deleted(&mut o.suite.payload.settings, SERVERS_KEY, &id);
            super::sync::forget_server(o, &id)?;
            self.save_container(&mut o.suite, super::suite::SUITE_FILE)
        })
    }
}

#[cfg(test)]
mod path_tests {
    use super::{folder_addresses, relative_path};

    #[test]
    fn the_suite_folder_is_looked_for_where_the_web_could_serve_it() {
        // Module on a subdomain whose root is public_html/sub; the folder is public_html/gs,
        // above that root: not next to the module, but on the main site and the module's host.
        let at = folder_addresses(
            "https://sub.example.org/ghulsuite.php",
            "ftp.example.org",
            "public_html/gs",
            "../gs",
            "shares/check-x",
        );
        assert!(at.contains(&"https://example.org/gs/shares/check-x".to_owned()), "{at:?}");
        assert!(at.contains(&"https://sub.example.org/gs/shares/check-x".to_owned()), "{at:?}");
        assert!(at.len() <= 8 && !at.iter().any(|u| u.contains("..")), "{at:?}");
        // Next to the module, with odd folder names encoded.
        let at = folder_addresses("https://example.org/t/ghulsuite.php", "", "/srv/a#b", "a#b", "shares/c");
        assert_eq!(at[0], "https://example.org/t/a%23b/shares/c");
    }

    #[test]
    fn relative_paths_from_the_module_to_the_suite_folder() {
        assert_eq!(relative_path("/home/u/public_html", "/home/u/ghulsuite").as_deref(), Some("../ghulsuite"));
        assert_eq!(relative_path("/home/u/public_html/tools", "/home/u/ghulsuite").as_deref(), Some("../../ghulsuite"));
        assert_eq!(relative_path("/", "/ghulsuite").as_deref(), Some("ghulsuite"));
        assert_eq!(relative_path("/a/b/", "/a/b").as_deref(), Some("."));
        assert_eq!(relative_path("/www", "/www/data/gs").as_deref(), Some("data/gs"));
        assert_eq!(relative_path("/a/../b", "/c"), None);
    }
}
