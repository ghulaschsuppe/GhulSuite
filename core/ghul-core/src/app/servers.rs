// SPDX-License-Identifier: AGPL-3.0-or-later
//! Own servers (SFTP) for synchronisation and GhulDrive.
//!
//! The server list and the SSH login key live in the suite's encrypted settings, so nobody can
//! see which servers are set up without unlocking. The SSH key is the suite's own Ed25519 key;
//! its public half is added to `~/.ssh/authorized_keys` on the server.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::suite::{new_id, put_setting, setting, GhulSuite, Open};
use std::sync::Arc;

use crate::net::ftps::{FtpsConfig, FtpsFs};
use crate::net::phpfs::{PhpConfig, PhpFs};
use crate::net::remote::RemoteFs;
use crate::net::sftp::{generate_key, public_key_of, SftpConfig, SftpFs, SshAuth};
use crate::net::NetError;

const SERVERS_KEY: &str = "servers";
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
#[derive(uniffi::Record, Clone, Debug)]
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
}

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

pub(super) fn servers(o: &Open) -> Vec<StoredServer> {
    setting(&o.suite, SERVERS_KEY)
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
    let s = servers(o).into_iter().find(|s| s.id == server_id).ok_or(GhulError::NotConfigured)?;
    open_fs(o, &s, None)
}

pub(super) fn server(o: &Open, server_id: &str) -> Result<StoredServer> {
    servers(o).into_iter().find(|s| s.id == server_id).ok_or(GhulError::NotFound)
}

pub(super) fn put_servers(o: &mut Open, list: &[StoredServer]) -> Result<()> {
    put_setting(&mut o.suite, SERVERS_KEY, &list)
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

fn info(s: &StoredServer) -> ServerInfo {
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
    use rand_core::RngCore;
    let mut key = Zeroizing::new([0u8; 32]);
    rand_core::OsRng.fill_bytes(key.as_mut());
    let mut dir = [0u8; 12];
    rand_core::OsRng.fill_bytes(&mut dir);
    let data_dir = format!("ghul-data-{}", crate::kdf::hex(&dir));
    PhpModule {
        file_name: "ghulsuite.php".into(),
        content: crate::net::phpfs::server_file(&key, &data_dir),
        key: crate::kdf::hex(key.as_ref()),
    }
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
    /// (empty) local folder. Logs in with `input.password` or the given `private_key` (from
    /// `generate_ssh_key`). Without `host_key` it returns the server key fingerprint to confirm
    /// and downloads nothing; afterwards it returns an empty string. Then unlock as usual.
    pub fn fetch_from_server(
        &self,
        input: ServerInput,
        host_key: Option<String>,
        private_key: Option<String>,
    ) -> Result<String> {
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
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
            base: input.base.trim().trim_end_matches('/').to_owned(),
            host_key: host_key.clone(),
        };
        let fs = match SftpFs::connect(&cfg) {
            Err(NetError::HostKeyUnknown(fp)) if host_key.is_none() => return Ok(fp),
            other => other?,
        };
        use crate::net::remote::RemoteFs;
        let files: Vec<String> = fs
            .list("suite")?
            .into_iter()
            .filter(|e| !e.is_dir && (super::suite::is_vault_file(&e.name) || e.name == super::suite::SUITE_FILE))
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
        match input.kind {
            ServerKind::Php if input.url.trim().is_empty() => {
                return Err(GhulError::InvalidInput { msg: "address required".into() })
            }
            ServerKind::Sftp | ServerKind::Ftps if input.host.trim().is_empty() || input.user.trim().is_empty() => {
                return Err(GhulError::InvalidInput { msg: "host and user required".into() })
            }
            _ => {}
        }
        self.with_open(|o| {
            let mut stored = stored_from(&input, confirmed.as_deref());
            if stored.kind == "php" {
                php_fs(&stored, "")?.info()?;
            }
            let fs = match open_input(o, &stored, confirmed.is_some())? {
                Ok(fs) => fs,
                Err(fingerprint) => return Ok(ServerSaved::Confirm { fingerprint }),
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
            list.push(stored);
            put_setting(&mut o.suite, SERVERS_KEY, &list)?;
            self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            self.save_container(&mut o.suite, super::suite::SUITE_FILE)?;
            Ok(ServerSaved::Saved { id })
        })
    }

    /// Folders on a stored server below its login folder (for choosing the share folder).
    pub fn server_browse(&self, server_id: String, path: String) -> Result<Vec<String>> {
        self.with_open(|o| {
            let s = server(o, &server_id)?;
            if ServerKind::of(&s) == ServerKind::Php {
                return Err(GhulError::InvalidInput { msg: "the server module needs no folder".into() });
            }
            let fs = open_fs(o, &s, Some(""))?;
            let mut dirs: Vec<String> = fs
                .list(path.trim_matches('/'))?
                .into_iter()
                .filter(|e| e.is_dir && !e.name.starts_with('.'))
                .map(|e| e.name)
                .collect();
            dirs.sort_by_key(|d| d.to_lowercase());
            Ok(dirs)
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
            let mut list = servers(o);
            list.retain(|s| s.id != id);
            put_setting(&mut o.suite, SERVERS_KEY, &list)?;
            super::sync::mark_deleted(&mut o.suite.payload.settings, SERVERS_KEY, &id);
            super::sync::forget_server(o, &id)?;
            self.save_container(&mut o.suite, super::suite::SUITE_FILE)
        })
    }
}
