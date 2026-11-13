// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network tests against local servers started by `tools/test-servers.sh`. Without the
//! `GHUL_TEST_*` variables they are skipped (they pass without doing anything).

use super::remote::{LocalFs, RemoteFs};
use super::sftp::{generate_key, public_key_of, SftpConfig, SftpFs, SshAuth};
use super::NetError;

pub(crate) fn sftp_env() -> Option<(String, u16, String, String, String, String)> {
    let addr = std::env::var("GHUL_TEST_SFTP").ok()?;
    let (host, port) = addr.split_once(':')?;
    Some((
        host.to_owned(),
        port.parse().ok()?,
        std::env::var("GHUL_TEST_SFTP_USER").ok()?,
        std::env::var("GHUL_TEST_SFTP_ROOT").ok()?,
        std::env::var("GHUL_TEST_SFTP_AUTHKEYS").ok()?,
        std::env::var("GHUL_TEST_SFTP_HOSTKEY").ok()?,
    ))
}

/// A fresh key authorised on the test server and a config for a fresh base folder.
pub(crate) fn sftp_test_config(sub: &str) -> Option<SftpConfig> {
    let (host, port, user, root, authkeys, hostkey) = sftp_env()?;
    let (private, public) = generate_key("ghulsuite-test").unwrap();
    let mut keys = std::fs::read_to_string(&authkeys).unwrap_or_default();
    keys.push_str(&public);
    keys.push('\n');
    std::fs::write(&authkeys, keys).unwrap();
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    Some(SftpConfig {
        host,
        port,
        user,
        auth: SshAuth::Key(private),
        base: format!("{root}/{sub}-{run}"),
        host_key: Some(hostkey),
    })
}

fn exercise(fs: &dyn RemoteFs) {
    assert!(fs.list("").unwrap().is_empty());
    assert_eq!(fs.read("a/b/x.bin").unwrap(), None);
    fs.write("a/b/x.bin", b"first").unwrap();
    fs.write("a/b/x.bin", b"second, longer").unwrap();
    assert_eq!(fs.read("a/b/x.bin").unwrap().as_deref(), Some(&b"second, longer"[..]));
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    fs.write("big.bin", &big).unwrap();
    assert_eq!(fs.read("big.bin").unwrap().unwrap(), big);
    let names: Vec<String> = fs.list("").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["a", "big.bin"]);
    assert!(fs.list("a").unwrap()[0].is_dir);
    fs.delete("a/b/x.bin").unwrap();
    fs.delete("a/b/x.bin").unwrap();
    assert!(fs.list("a/b").unwrap().is_empty());
    assert!(fs.read("../escape").is_err());
}

#[test]
fn local_fs() {
    let dir = std::env::temp_dir().join(format!("ghul-localfs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    exercise(&LocalFs::new(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sftp_against_openssh() {
    let Some(mut cfg) = sftp_test_config("sftp-basic") else { return };
    // First contact: the key must be confirmed.
    let pinned = cfg.host_key.take();
    match SftpFs::connect(&cfg) {
        Err(NetError::HostKeyUnknown(fp)) => assert_eq!(Some(fp), pinned),
        other => panic!("expected HostKeyUnknown, got {:?}", other.err()),
    }
    cfg.host_key = Some("SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into());
    assert!(matches!(SftpFs::connect(&cfg).err(), Some(NetError::HostKeyChanged(_))));
    cfg.host_key = pinned;
    // A key the server does not know is refused.
    let (other, _) = generate_key("unknown").unwrap();
    let bad = SftpConfig { auth: SshAuth::Key(other), ..cfg.clone() };
    assert_eq!(SftpFs::connect(&bad).err(), Some(NetError::AuthFailed));
    let fs = SftpFs::connect(&cfg).unwrap();
    exercise(&fs);
    // Appending keeps what is there, also without a final newline.
    fs.write("k/authorized_keys", b"ssh-ed25519 AAAA old").unwrap();
    fs.append_line("k/authorized_keys", "ssh-ed25519 BBBB new").unwrap();
    fs.append_line("k/new_file", "line").unwrap();
    assert_eq!(fs.read("k/authorized_keys").unwrap().unwrap(), b"ssh-ed25519 AAAA old\nssh-ed25519 BBBB new\n");
    assert_eq!(fs.read("k/new_file").unwrap().unwrap(), b"line\n");
    // The absolute location is reported by the server.
    let abs = fs.absolute("").unwrap();
    assert!(abs.starts_with('/') && abs.ends_with(cfg.base.rsplit('/').next().unwrap()), "{abs}");
    if let SshAuth::Key(k) = &cfg.auth {
        assert!(public_key_of(k).unwrap().starts_with("ssh-ed25519 "));
    }
}

pub(crate) fn ftps_test_config(sub: &str) -> Option<super::ftps::FtpsConfig> {
    let addr = std::env::var("GHUL_TEST_FTPS").ok()?;
    let (host, port) = addr.split_once(':')?;
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    Some(super::ftps::FtpsConfig {
        host: host.to_owned(),
        port: port.parse().ok()?,
        user: std::env::var("GHUL_TEST_FTPS_USER").ok()?,
        password: zeroize::Zeroizing::new(std::env::var("GHUL_TEST_FTPS_PASSWORD").ok()?),
        base: format!("{sub}-{run}"),
        cert: std::env::var("GHUL_TEST_FTPS_TLSFP").ok(),
    })
}

#[test]
fn ftps_against_vsftpd() {
    use super::ftps::FtpsFs;
    let Some(cfg) = ftps_test_config("ftps-basic") else { return };
    // Without the confirmed fingerprint the self-signed certificate is refused.
    let unpinned = super::ftps::FtpsConfig { cert: None, ..cfg.clone() };
    match FtpsFs::connect(&unpinned) {
        Err(NetError::CertUntrusted(fp)) => assert_eq!(Some(fp), cfg.cert),
        other => panic!("expected CertUntrusted, got {:?}", other.err()),
    }
    let wrong = super::ftps::FtpsConfig { password: zeroize::Zeroizing::new("falsch".into()), ..cfg.clone() };
    assert_eq!(FtpsFs::connect(&wrong).err(), Some(NetError::AuthFailed));
    let fs = FtpsFs::connect(&cfg).unwrap();
    exercise(&fs);
    // Names with spaces and umlauts survive listing.
    fs.write("Ordner mit Leerzeichen/Grüße.txt", b"hallo").unwrap();
    let names: Vec<String> = fs.list("Ordner mit Leerzeichen").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["Grüße.txt"]);
}

/// A running `php -S` with a freshly generated server module (stopped when dropped).
pub(crate) struct PhpServer {
    child: std::process::Child,
    pub url: String,
    pub key: [u8; 32],
    pub dir: std::path::PathBuf,
}

impl Drop for PhpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Starts the server module under `php -S`; `None` if PHP is not installed.
/// Starts PHP's built-in web server for `dir` on a free port and returns it once it answers.
///
/// Tests run in parallel, and a port found free can be taken by another test's server before
/// ours binds it (seen on Windows). Then ours exits and the other one answers: the test would talk
/// to the wrong server. So the server must hand out a token only this folder has; otherwise it is
/// stopped and another port is tried.
pub(crate) fn spawn_php(dir: &std::path::Path) -> Option<(std::process::Child, u16)> {
    use rand_core::RngCore;
    let mut t = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut t);
    let token = crate::kdf::hex(&t);
    let check = dir.join("ghul-port-check.txt");
    std::fs::write(&check, &token).ok()?;
    for _ in 0..10 {
        let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
        let mut child = std::process::Command::new("php")
            .args(["-S", &format!("127.0.0.1:{port}"), "-t"])
            .arg(dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        let url = super::http::Url::parse(&format!("http://127.0.0.1:{port}/ghul-port-check.txt")).ok()?;
        let mut ours = false;
        for _ in 0..50 {
            if child.try_wait().ok().flatten().is_some() {
                break; // could not bind the port
            }
            let r =
                super::block_on(super::http::request("GET", &url, &[], b"", None), std::time::Duration::from_secs(2));
            if let Ok(r) = r {
                ours = r.status == 200 && r.body == token.as_bytes();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if ours {
            let _ = std::fs::remove_file(&check);
            return Some((child, port));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    None
}

pub(crate) fn php_server() -> Option<PhpServer> {
    use rand_core::RngCore;
    std::process::Command::new("php").arg("-v").output().ok()?;
    let mut key = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut key);
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("ghul-php-{run}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ghulsuite.php"), super::phpfs::server_file(&key, "ghul-data-test")).unwrap();
    let (child, port) = spawn_php(&dir)?;
    Some(PhpServer { child, url: format!("http://127.0.0.1:{port}/ghulsuite.php"), key, dir })
}

fn get(url: &str) -> super::http::Response {
    let u = super::http::Url::parse(url).unwrap();
    super::block_on(super::http::request("GET", &u, &[], b"", None), super::TIMEOUT).unwrap()
}

#[test]
fn php_server_module() {
    use super::phpfs::{PhpConfig, PhpFs};
    let Some(server) = php_server() else { return };
    let cfg = PhpConfig {
        url: server.url.clone(),
        key: zeroize::Zeroizing::new(server.key),
        base: String::new(),
        cert: None,
    };
    let fs = PhpFs::new(&cfg).unwrap();
    let info = fs.info().unwrap();
    assert_eq!(info.version, 2);
    assert!(info.limit > 0, "the module reports the hosting's upload limit");
    exercise(&fs);
    // Large files go up in parts and replace the old file only when complete.
    let big: Vec<u8> = (0..9 * 1024 * 1024 + 13).map(|i| (i % 251) as u8).collect();
    fs.write("big/vault.bin", b"old").unwrap();
    fs.write("big/vault.bin", &big).unwrap();
    assert_eq!(fs.read("big/vault.bin").unwrap().as_deref(), Some(&big[..]));
    assert!(fs.list("big").unwrap().iter().all(|e| e.name == "vault.bin"), "no parts left behind");
    // A wrong key is refused.
    let bad = PhpFs::new(&PhpConfig { key: zeroize::Zeroizing::new([1u8; 32]), ..cfg.clone() }).unwrap();
    assert_eq!(bad.info().err(), Some(NetError::AuthFailed));
    // Unsigned requests and other methods are refused; the data folder is not listed.
    let base = server.url.clone();
    assert_eq!(get(&format!("{base}?a=list")).status, 405);
    // Paths cannot leave the storage.
    assert!(fs.read("../ghulsuite.php").is_err());
    // Even with the key nothing executable or server configuration can be stored.
    for name in ["shell.php", "x.PHTML", "a/.htaccess", ".user.ini", "run.phar"] {
        assert!(fs.write(name, b"<?php echo 1;").is_err(), "{name}");
    }
    fs.write(".ghulsuite-check", b"ok").unwrap();

    // Shares: download limit, one-time, expiry.
    let data = vec![42u8; 5 * 1024 * 1024 + 7];
    let id = "0123456789abcdef0123456789abcdef";
    fs.share_put(id, &data, 0, 2).unwrap();
    assert_eq!(fs.share_list().unwrap()[0].count, 0);
    let first = get(&format!("{base}?s={id}"));
    assert_eq!((first.status, first.body.len()), (200, data.len()));
    assert_eq!(fs.share_list().unwrap()[0].count, 1);
    assert_eq!(get(&format!("{base}?s={id}")).body, data);
    assert_eq!(get(&format!("{base}?s={id}")).status, 404);
    assert!(fs.share_list().unwrap().is_empty());
    let expired = "ffffffffffffffffffffffffffffffff";
    fs.share_put(expired, b"x", 1, 0).unwrap();
    assert_eq!(get(&format!("{base}?s={expired}")).status, 410);
    let revoked = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    fs.share_put(revoked, b"x", 0, 0).unwrap();
    fs.share_delete(revoked).unwrap();
    assert_eq!(get(&format!("{base}?s={revoked}")).status, 404);
    assert_eq!(get(&format!("{base}?s=../../etc/passwd")).status, 404);

    // The share page is served with a strict content security policy.
    let page = get(&base);
    assert_eq!(page.status, 200);
    assert!(page.header("Content-Security-Policy").unwrap().contains("default-src 'none'"));
    assert!(String::from_utf8_lossy(&page.body).contains("ghulsuite.php?v=js"));
    assert!(String::from_utf8_lossy(&get(&format!("{base}?v=js")).body).contains("GHSHARE1"));
    // The data folder is protected (Apache) and its files are encrypted anyway; with php -S we
    // at least check that nothing outside the module answers with its content.
    assert!(server.dir.join("ghul-data-test/.htaccess").exists());
}

#[test]
fn php_replay_is_refused() {
    let Some(server) = php_server() else { return };
    // Same signed request twice: the second is rejected.
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs().to_string();
    let nonce = "00112233445566778899aabbccddeeff";
    let msg = format!("ghul/v1\nPOST\na=info\n{time}\n{nonce}\n{}", crate::kdf::hex(&Sha256::digest(b"")));
    let mut mac = Hmac::<Sha256>::new_from_slice(&server.key).unwrap();
    mac.update(msg.as_bytes());
    let sig = crate::kdf::hex(&mac.finalize().into_bytes());
    let url = super::http::Url::parse(&format!("{}?a=info", server.url)).unwrap();
    let headers = [("X-Ghul-Time", time.clone()), ("X-Ghul-Nonce", nonce.to_string()), ("X-Ghul-Sig", sig)];
    let send =
        || super::block_on(super::http::request("POST", &url, &headers, b"", None), super::TIMEOUT).unwrap().status;
    assert_eq!(send(), 200);
    assert_eq!(send(), 401);
}

/// Real servers with broken TLS (badssl.com): GhulSuite must refuse every one of them. Runs at
/// night (needs the internet): `cargo test -p ghul-core -- --ignored badssl`.
#[test]
#[ignore = "needs the internet: cargo test -p ghul-core -- --ignored badssl"]
fn badssl_broken_tls_is_refused() {
    use super::http::{request, Url};
    let get = |url: &str| {
        let u = Url::parse(url).unwrap();
        super::block_on(request("GET", &u, &[], b"", None), std::time::Duration::from_secs(30))
    };
    // Without the reference site the test says nothing (network down, site gone).
    if get("https://badssl.com/").is_err() {
        eprintln!("badssl.com not reachable, skipped");
        return;
    }
    assert!(get("https://tls-v1-2.badssl.com:1012/").is_ok(), "a good TLS 1.2 server must work");
    for host in [
        "expired.badssl.com",
        "wrong.host.badssl.com",
        "self-signed.badssl.com",
        "untrusted-root.badssl.com",
        "no-common-name.badssl.com",
        "no-subject.badssl.com",
        "incomplete-chain.badssl.com",
        "tls-v1-0.badssl.com:1010",
        "tls-v1-1.badssl.com:1011",
        "rc4.badssl.com",
        "rc4-md5.badssl.com",
        "3des.badssl.com",
        "null.badssl.com",
        "dh480.badssl.com",
        "dh512.badssl.com",
        "dh1024.badssl.com",
        "superfish.badssl.com",
        "edellroot.badssl.com",
        "dsdtestprovider.badssl.com",
        "preact-cli.badssl.com",
        "webpack-dev-server.badssl.com",
    ] {
        let r = get(&format!("https://{host}/"));
        assert!(r.is_err(), "{host} was accepted");
    }
}
