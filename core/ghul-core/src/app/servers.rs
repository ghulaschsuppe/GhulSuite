// SPDX-License-Identifier: AGPL-3.0-or-later
//! Own servers (SFTP, FTPS, PHP module) for synchronisation and GhulDrive.
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
use crate::net::phpfs::{PhpConfig, PhpFs};
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
    /// "sftp" (also when empty), "ftps" or "php".
    #[serde(default)]
    pub kind: String,
    /// PHP server module: address of `ghulsuite.php`.
    #[serde(default)]
    pub url: String,
    /// Sharing: folder (relative to the login folder) that the web server delivers, and its
    /// public address. Empty = sharing not set up. The PHP module needs neither.
    #[serde(default)]
    pub share_dir: String,
    #[serde(default)]
    pub share_url: String,
    /// PHP module: the FTPS login it was uploaded with, kept to update it later (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<StoredUpload>,
    /// Last edit (Unix ms): the later edit wins when devices synchronise (B13).
    #[serde(default)]
    pub changed: i64,
}

/// FTPS login that a PHP module was uploaded with.
#[derive(Serialize, Deserialize, Clone, Default)]
pub(super) struct StoredUpload {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// TLS certificate fingerprint confirmed by the user (not publicly trusted certificates).
    #[serde(default)]
    pub cert: String,
    /// Folder of the PHP file, relative to the FTPS login folder.
    pub dir: String,
    /// Name of the module's data folder (kept when the module is updated).
    pub data_dir: String,
}

impl Drop for StoredUpload {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.password);
    }
}

/// How GhulSuite reaches a server.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerKind {
    /// SSH/SFTP: own server, NAS, root server. Password or the suite's SSH key.
    Sftp,
    /// FTP with TLS (explicit, port 21): typical web hosting.
    Ftps,
    /// The GhulSuite server module (one PHP file) on web hosting without SSH.
    Php,
}

impl ServerKind {
    fn as_str(self) -> &'static str {
        match self {
            ServerKind::Sftp => "sftp",
            ServerKind::Ftps => "ftps",
            ServerKind::Php => "php",
        }
    }

    pub(super) fn of(s: &StoredServer) -> ServerKind {
        match s.kind.as_str() {
            "ftps" => ServerKind::Ftps,
            "php" => ServerKind::Php,
            _ => ServerKind::Sftp,
        }
    }
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
    /// PHP module: the access key (hex) from `php_module_create`.
    pub password: Option<String>,
    pub base: String,
    /// PHP module: address of `ghulsuite.php` (host, port and user are not used).
    pub url: String,
    /// PHP module: the FTPS login it was uploaded with, to keep for later updates
    /// (`PhpInstalled.upload`). `None`: the login is not kept.
    #[uniffi(default = None)]
    pub upload: Option<UploadAccess>,
}

super::redact::redacted_debug!(ServerInput { kind, name, host, port, user, base, url, upload });

/// FTPS login that a PHP module was uploaded with (from `php_module_install`).
#[derive(uniffi::Record, Clone)]
pub struct UploadAccess {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub cert: String,
    pub dir: String,
    pub data_dir: String,
}

super::redact::redacted_debug!(UploadAccess { host, port, user, cert, dir, data_dir });

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
    /// Public address for share links; empty = sharing not set up.
    pub share_url: String,
    pub share_dir: String,
}

impl Drop for StoredServer {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.password);
    }
}

/// Servers in the suite file (offered for synchronisation).
pub(super) fn servers(o: &Open) -> Vec<StoredServer> {
    setting(&o.suite, SERVERS_KEY)
}

/// GhulDrive's own server (in the Drive file); at most one.
pub(super) fn drive_servers(o: &Open) -> Vec<StoredServer> {
    area_setting(o, VaultKind::Drive, SERVERS_KEY)
}

/// The mail archive's own server (in the Mail file); at most one.
pub(super) fn archive_servers(o: &Open) -> Vec<StoredServer> {
    area_setting(o, VaultKind::Mail, ARCHIVE_SERVERS_KEY)
}

pub(super) const ARCHIVE_SERVERS_KEY: &str = "archive_servers";

/// Every server this suite knows: the suite's, the Drive file's and the mail archive's.
pub(super) fn all_servers(o: &Open) -> Vec<StoredServer> {
    let mut list = servers(o);
    list.extend(drive_servers(o));
    list.extend(archive_servers(o));
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

fn php_key(hex: &str) -> Result<Zeroizing<[u8; 32]>> {
    let bytes = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2).unwrap_or("x"), 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| GhulError::InvalidInput { msg: "invalid access key".into() })?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| GhulError::InvalidInput { msg: "invalid access key".into() })?;
    Ok(Zeroizing::new(key))
}

/// Opens a stored server with its own folder, or with `base` instead (e.g. the share folder).
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
        ServerKind::Php => Arc::new(php_fs(s, &base)?),
    })
}

pub(super) fn php_fs(s: &StoredServer, base: &str) -> Result<PhpFs> {
    Ok(PhpFs::new(&PhpConfig { url: s.url.clone(), key: php_key(&s.password)?, base: base.to_owned(), cert: None })?)
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
        ServerKind::Php => 443,
    };
    let host = if kind == ServerKind::Php {
        crate::net::http::Url::parse(&input.url).map(|u| u.host).unwrap_or_default()
    } else {
        input.host.trim().to_owned()
    };
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
        url: input.url.trim().to_owned(),
        share_dir: String::new(),
        share_url: String::new(),
        upload: input.upload.as_ref().filter(|_| kind == ServerKind::Php).map(|u| StoredUpload {
            host: u.host.clone(),
            port: u.port,
            user: u.user.clone(),
            password: u.password.clone(),
            cert: u.cert.clone(),
            dir: u.dir.clone(),
            data_dir: u.data_dir.clone(),
        }),
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
        share_url: if ServerKind::of(s) == ServerKind::Php { s.url.clone() } else { s.share_url.clone() },
        share_dir: s.share_dir.clone(),
    }
}

/// A new server module: the PHP file to upload and the access key it contains.
#[derive(uniffi::Record, Clone)]
pub struct PhpModule {
    pub file_name: String,
    pub content: String,
    /// Access key (hex); goes into `ServerInput.password` when saving the server.
    pub key: String,
}

/// Creates a server module with a new random access key and data folder name.
#[uniffi::export]
pub fn php_module_create() -> PhpModule {
    let (key, data_dir) = php_module_key();
    php_module_file(&key, &data_dir)
}

/// A new access key and data folder name for a server module.
fn php_module_key() -> (Zeroizing<[u8; 32]>, String) {
    use rand_core::RngCore;
    let mut key = Zeroizing::new([0u8; 32]);
    rand_core::OsRng.fill_bytes(key.as_mut());
    let mut dir = [0u8; 12];
    rand_core::OsRng.fill_bytes(&mut dir);
    (key, format!("ghul-data-{}", crate::kdf::hex(&dir)))
}

fn php_module_file(key: &[u8; 32], data_dir: &str) -> PhpModule {
    PhpModule {
        file_name: "ghulsuite.php".into(),
        content: crate::net::phpfs::server_file(key, data_dir),
        key: crate::kdf::hex(key),
    }
}

/// What a server module reports.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PhpModuleInfo {
    pub version: u32,
    /// The newest version this app brings along (older modules can be updated).
    pub current: u32,
    /// Largest request the hosting accepts in bytes (0 = not reported); larger files go in parts.
    pub limit: u64,
    pub free: u64,
}

/// Checks that a server module answers at `url` with access key `key` (hex). If the web server
/// delivers the file as text instead of running it (its access key would be readable), the
/// answer is `PhpNotExecuted` (L5).
#[uniffi::export]
pub fn php_module_probe(url: String, key: String) -> Result<PhpModuleInfo> {
    let fs =
        PhpFs::new(&PhpConfig { url: url.trim().to_owned(), key: php_key(&key)?, base: String::new(), cert: None })?;
    match fs.info() {
        Ok(i) => Ok(PhpModuleInfo {
            version: i.version,
            current: crate::net::phpfs::MODULE_VERSION,
            limit: i.limit,
            free: i.free,
        }),
        Err(_) if reveals_source(url.trim()) => Err(GhulError::PhpNotExecuted),
        Err(e) => Err(e.into()),
    }
}

/// Whether `url` delivers the module's PHP source (with its key) instead of running it.
fn reveals_source(url: &str) -> bool {
    let Ok(u) = crate::net::http::Url::parse(url) else { return false };
    let r =
        crate::net::block_on(crate::net::http::request("GET", &u, &[], b"", None), std::time::Duration::from_secs(15));
    matches!(r, Ok(r) if {
        let body = String::from_utf8_lossy(&r.body);
        body.contains("<?php") || body.contains("GHUL_KEY")
    })
}

/// Result of installing a server module.
#[derive(uniffi::Record, Clone)]
pub struct PhpInstalled {
    /// Set: confirm this certificate fingerprint first and call again; nothing was uploaded.
    pub confirm: Option<String>,
    /// The address the module answered at; `None` when none of `tried` worked (ask the user
    /// for the address and check it with `php_module_probe`).
    pub url: Option<String>,
    /// The addresses that were tried, the most likely first.
    pub tried: Vec<String>,
    /// Access key (hex); goes into `ServerInput.password`.
    pub key: String,
    pub limit: u64,
    /// The FTPS login, for `ServerInput.upload` if the user keeps it for later updates.
    pub upload: Option<UploadAccess>,
}

super::redact::redacted_debug!(PhpInstalled { confirm, url, tried, limit, upload });

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

/// Likely web addresses of a PHP file uploaded by FTPS into `dir`, the most likely first.
/// A folder named like a domain is the website's root on many hostings
/// (`example.org/tools` → `https://example.org/tools/`); otherwise the FTPS host without `ftp.`
/// with the usual root folders left out.
pub(super) fn module_urls(ftps_host: &str, dir: &str) -> Vec<String> {
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
    let host = ftps_host.trim();
    let host = host.strip_prefix("ftp.").unwrap_or(host);
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

/// Checks the required fields of a server as entered.
fn check_input(input: &ServerInput) -> Result<()> {
    match input.kind {
        ServerKind::Php if input.url.trim().is_empty() => {
            Err(GhulError::InvalidInput { msg: "address required".into() })
        }
        ServerKind::Sftp | ServerKind::Ftps if input.host.trim().is_empty() || input.user.trim().is_empty() => {
            Err(GhulError::InvalidInput { msg: "host and user required".into() })
        }
        _ => Ok(()),
    }
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
    if stored.kind == "php" {
        php_fs(&stored, "")?.info()?;
    }
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
        ServerKind::Php => stored.url.clone(),
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
    /// `generate_ssh_key`), FTPS with `input.password`, the PHP module with its access key in
    /// `input.password` and `input.url` (U7). Without `host_key` an unknown SFTP host key or an
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
            ServerKind::Php => {
                let s = stored_from(&input, None);
                Arc::new(php_fs(&s, &s.base)?)
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
                    && s.base == stored.base
                    && s.url == stored.url)
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
        if input.kind == ServerKind::Php {
            return Err(GhulError::InvalidInput { msg: "the server module needs no folder".into() });
        }
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

    /// Installs the server module through FTPS on the same web hosting: creates the PHP file with
    /// a new access key, uploads it into `dir` (relative to the FTPS login folder) and looks for
    /// the address it answers at. `url`: the address the user entered (empty: try the usual
    /// ones). On first contact with a certificate that is not publicly trusted it returns the
    /// fingerprint to confirm, without uploading anything.
    pub fn php_module_install(
        &self,
        ftps: ServerInput,
        confirmed: Option<String>,
        dir: String,
        url: String,
    ) -> Result<PhpInstalled> {
        if ftps.kind != ServerKind::Ftps {
            return Err(GhulError::InvalidInput { msg: "the module is uploaded with FTPS".into() });
        }
        check_input(&ftps)?;
        let dir = dir.trim().trim_matches('/').to_owned();
        let (key, data_dir) = php_module_key();
        let module = php_module_file(&key, &data_dir);
        let upload = self.with_open(|o| {
            let mut stored = stored_from(&ftps, confirmed.as_deref());
            stored.base = String::new();
            let fs = match open_input(o, &stored, confirmed.is_some())? {
                Ok(fs) => fs,
                Err(fingerprint) => return Ok(Err(fingerprint)),
            };
            let path = if dir.is_empty() { module.file_name.clone() } else { format!("{dir}/{}", module.file_name) };
            fs.write(&path, module.content.as_bytes())?;
            Ok(Ok(UploadAccess {
                host: stored.host.clone(),
                port: stored.port,
                user: stored.user.clone(),
                password: stored.password.clone(),
                cert: stored.host_key.clone(),
                dir: dir.clone(),
                data_dir: data_dir.clone(),
            }))
        })?;
        let upload = match upload {
            Ok(u) => u,
            Err(fingerprint) => {
                return Ok(PhpInstalled {
                    confirm: Some(fingerprint),
                    url: None,
                    tried: vec![],
                    key: String::new(),
                    limit: 0,
                    upload: None,
                })
            }
        };
        let tried = if url.trim().is_empty() { module_urls(&upload.host, &dir) } else { vec![url.trim().to_owned()] };
        let found = tried.iter().find_map(|u| php_module_probe(u.clone(), module.key.clone()).ok().map(|i| (u, i)));
        // L5: delivered as text, the key is readable: take the file down again at once.
        if found.is_none() && tried.iter().any(|u| reveals_source(u)) {
            self.with_open(|o| {
                let mut stored = stored_from(&ftps, confirmed.as_deref());
                stored.base = String::new();
                if let Ok(fs) = open_input(o, &stored, true)? {
                    let path =
                        if dir.is_empty() { module.file_name.clone() } else { format!("{dir}/{}", module.file_name) };
                    fs.delete(&path)?;
                }
                Ok(())
            })?;
            return Err(GhulError::PhpNotExecuted);
        }
        Ok(PhpInstalled {
            confirm: None,
            url: found.as_ref().map(|(u, _)| (*u).clone()),
            limit: found.map(|(_, i)| i.limit).unwrap_or(0),
            tried,
            key: module.key,
            upload: Some(upload),
        })
    }

    /// Uploads a new version of a server module through the FTPS login it was installed with;
    /// the access key and the stored files stay. Returns the module's version afterwards.
    pub fn php_module_update(&self, server_id: String) -> Result<u32> {
        let (srv, fs) = self.with_open(|o| {
            let srv = server(o, &server_id)?;
            let u = srv.upload.clone().ok_or(GhulError::NotConfigured)?;
            let mut ftps = StoredServer::default();
            ftps.host = u.host.clone();
            ftps.port = u.port;
            ftps.user = u.user.clone();
            ftps.password = u.password.clone();
            ftps.host_key = u.cert.clone();
            ftps.kind = "ftps".into();
            Ok((srv, open_fs(o, &ftps, Some(""))?))
        })?;
        let u = srv.upload.as_ref().ok_or(GhulError::NotConfigured)?;
        let module = php_module_file(&*php_key(&srv.password)?, &u.data_dir);
        let path = if u.dir.is_empty() { module.file_name.clone() } else { format!("{}/{}", u.dir, module.file_name) };
        fs.write(&path, module.content.as_bytes())?;
        self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
        Ok(php_fs(&srv, "")?.info()?.version)
    }

    /// Folders on a stored server below its login folder (for choosing the share folder).
    pub fn server_browse(&self, server_id: String, path: String) -> Result<Vec<String>> {
        self.with_open(|o| {
            let s = server(o, &server_id)?;
            if ServerKind::of(&s) == ServerKind::Php {
                return Err(GhulError::InvalidInput { msg: "the server module needs no folder".into() });
            }
            let fs = open_fs(o, &s, Some(""))?;
            folders(fs.as_ref(), &path)
        })
    }

    /// Uploads a new server module (from `php_module_create`) through another stored server,
    /// e.g. FTPS on the same web hosting, into `dir` (relative to its login folder).
    pub fn php_module_upload(&self, via_server_id: String, dir: String, module: PhpModule) -> Result<()> {
        self.with_open(|o| {
            let s = server(o, &via_server_id)?;
            let fs = open_fs(o, &s, Some(""))?;
            let dir = dir.trim_matches('/');
            let path = if dir.is_empty() { module.file_name.clone() } else { format!("{dir}/{}", module.file_name) };
            fs.write(&path, module.content.as_bytes())?;
            Ok(())
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
