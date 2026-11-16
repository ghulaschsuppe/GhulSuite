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

/// Adds `public` to the test server's authorized_keys. Tests run in parallel: reading the file,
/// adding a line and writing it back lost keys of other tests (ServerAuthFailed in CI), so this
/// appends under a lock.
pub(crate) fn authorize_test_key(authkeys: &str, public: &str) {
    use std::io::Write;
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(authkeys).unwrap();
    f.write_all(format!("{public}\n").as_bytes()).unwrap();
}

#[test]
fn test_keys_added_at_once_are_all_kept() {
    let file = std::env::temp_dir().join(format!("ghul-authkeys-{}", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let path = file.to_str().unwrap().to_string();
    std::thread::scope(|t| {
        for n in 0..16 {
            let path = &path;
            t.spawn(move || (0..20).for_each(|i| authorize_test_key(path, &format!("ssh-ed25519 K{n}-{i}"))));
        }
    });
    let keys = std::fs::read_to_string(&file).unwrap();
    let _ = std::fs::remove_file(&file);
    assert_eq!(keys.lines().count(), 320);
}

/// A fresh key authorised on the test server and a config for a fresh base folder.
pub(crate) fn sftp_test_config(sub: &str) -> Option<SftpConfig> {
    let (host, port, user, root, authkeys, hostkey) = sftp_env()?;
    let (private, public) = generate_key("ghulsuite-test").unwrap();
    authorize_test_key(&authkeys, &public);
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
fn sftp_folders_created_at_once_from_several_connections() {
    // The mail archive uploads in parallel: several connections create the same new folder at
    // the same moment. Each must succeed (bug: "Failure" for all but the first, which ended the
    // archive run).
    let Some(cfg) = sftp_test_config("sftp-mkdir-race") else { return };
    for round in 0..5 {
        let dir = format!("race-{round}/a/b/c");
        let errors: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let cfg = cfg.clone();
                    let dir = dir.clone();
                    scope.spawn(move || {
                        SftpFs::connect(&cfg).and_then(|fs| fs.mkdir_all(&dir)).err().map(|e| e.to_string())
                    })
                })
                .collect();
            handles.into_iter().filter_map(|h| h.join().unwrap()).collect()
        });
        assert!(errors.is_empty(), "round {round}: {errors:?}");
    }
}

/// H3: replacing a file over SFTP never leaves it missing. OpenSSH replaces in one step
/// (posix-rename); without that, a write that stopped between removing the old file and renaming
/// the new one is still read (and put in place by the next write).
#[test]
fn sftp_replacing_a_file_never_loses_it() {
    let Some(cfg) = sftp_test_config("sftp-replace") else { return };
    let fs = SftpFs::connect(&cfg).unwrap();
    assert!(fs.replaces_atomically(), "OpenSSH offers posix-rename");
    fs.write("drive/index.ghldrv", b"A").unwrap();
    fs.write("drive/index.ghldrv", b"B").unwrap();
    assert_eq!(fs.read("drive/index.ghldrv").unwrap().unwrap(), b"B");
    // A write without posix-rename that stopped after removing the old file: only the
    // finished ".ghltmp" is there.
    fs.delete("drive/index.ghldrv").unwrap();
    fs.write("drive/index.ghldrv.ghltmp", b"C").unwrap();
    // With one-step replacing that is an unfinished first write, never read as the content.
    assert_eq!(fs.read("drive/index.ghldrv").unwrap(), None);
    // A server without posix-rename reads it; the next write puts it in place first, then replaces it.
    super::sftp::NO_POSIX_RENAME.store(true, std::sync::atomic::Ordering::SeqCst);
    let plain = SftpFs::connect(&cfg);
    super::sftp::NO_POSIX_RENAME.store(false, std::sync::atomic::Ordering::SeqCst);
    let plain = plain.unwrap();
    assert!(!plain.replaces_atomically());
    assert_eq!(plain.read("drive/index.ghldrv").unwrap().unwrap(), b"C", "the finished new content is read");
    plain.write("drive/index.ghldrv", b"D").unwrap();
    assert_eq!(plain.read("drive/index.ghldrv").unwrap().unwrap(), b"D");
    let names: Vec<String> = plain.list("drive").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["index.ghldrv"], "nothing left behind");
    // Deleting takes the waiting replacement too: the file does not come back.
    plain.write("drive/gone.ghltmp", b"E").unwrap();
    assert_eq!(plain.read("drive/gone").unwrap().unwrap(), b"E");
    plain.delete("drive/gone").unwrap();
    assert_eq!(plain.read("drive/gone").unwrap(), None, "deleted stays deleted");
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
    // A replacement that stopped half way left only its new copy: it is read, the next write
    // puts it in place before writing, and nothing is left behind.
    fs.write("h/x.ghltmp", b"E").unwrap();
    assert_eq!(fs.read("h/x").unwrap().unwrap(), b"E");
    fs.write("h/x", b"F").unwrap();
    assert_eq!(fs.read("h/x").unwrap().unwrap(), b"F");
    let names: Vec<String> = fs.list("h").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, vec!["x"]);
    // Cut off while the next write uploads: the copy that was there is still read.
    fs.write("h/cut-off.ghltmp", b"old content").unwrap();
    assert!(fs.write("h/cut-off", b"the new content").is_err());
    assert_eq!(fs.read("h/cut-off").unwrap().unwrap(), b"old content");
    // Deleting takes such copies too: the file does not come back.
    fs.write("h/y.ghlold", b"G").unwrap();
    fs.write("h/z.ghltmp", b"H").unwrap();
    assert_eq!(fs.read("h/y").unwrap().unwrap(), b"G");
    fs.delete("h/y").unwrap();
    fs.delete("h/z").unwrap();
    assert_eq!(fs.read("h/y").unwrap(), None, "deleted stays deleted");
    assert_eq!(fs.read("h/z").unwrap(), None, "deleted stays deleted");
}

/// Starts PHP's built-in web server for `dir` on a free port and returns it once it answers.
///
/// Tests run in parallel, and a port found free can be taken by another test's server before
/// ours binds it (seen on Windows). Then ours exits and the other one answers: the test would talk
/// to the wrong server. So the server must hand out a token only this folder has; otherwise it is
/// stopped and another port is tried.
/// Like `spawn_php`, but `None` only without PHP: in the CI (`CI` set) a PHP that does not
/// serve the folder fails the test instead of skipping it silently.
pub(crate) fn spawn_php_ci(dir: &std::path::Path) -> Option<(std::process::Child, u16)> {
    std::process::Command::new("php").arg("-v").output().ok()?;
    let served = spawn_php(dir);
    assert!(served.is_some() || std::env::var_os("CI").is_none(), "php -S could not serve {}", dir.display());
    served
}

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

/// A GhulSuite folder with a share in it and the module next to it, under `php -S`: the module
/// must hand out the package and exactly the blocks of the share, with a ticket, and refuse
/// everything else.
pub(crate) struct ModuleSite {
    pub child: std::process::Child,
    pub url: String,
    pub dir: std::path::PathBuf,
}

impl Drop for ModuleSite {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `dir/www/ghulsuite.php` pointing at `dir/ghulsuite`, served by `php -S` from `dir/www`.
pub(crate) fn module_site() -> Option<ModuleSite> {
    std::process::Command::new("php").arg("-v").output().ok()?;
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("ghul-module-{run}"));
    std::fs::create_dir_all(dir.join("www")).unwrap();
    std::fs::create_dir_all(dir.join("ghulsuite/shares")).unwrap();
    std::fs::create_dir_all(dir.join("ghulsuite/drive/blocks")).unwrap();
    std::fs::write(dir.join("www/ghulsuite.php"), super::module::module_file("../ghulsuite").unwrap()).unwrap();
    let (child, port) = spawn_php_ci(&dir.join("www"))?;
    Some(ModuleSite { child, url: format!("http://127.0.0.1:{port}/ghulsuite.php"), dir })
}

fn get(url: &str) -> super::http::Response {
    let u = super::http::Url::parse(url).unwrap();
    super::block_on(super::http::request("GET", &u, &[], b"", None), super::TIMEOUT).unwrap()
}

#[test]
fn module_hands_out_only_the_blocks_of_a_share() {
    let Some(site) = module_site() else { return };
    let base = site.url.clone();
    let suite = site.dir.join("ghulsuite");
    let block = |name: &str, data: &[u8]| {
        let d = suite.join("drive/blocks").join(&name[..2]);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(name), data).unwrap();
    };
    let (a, b, other) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
    block(&a, b"block A");
    block(&b, b"block B");
    block(&other, b"not shared");
    let id = "0123456789abcdef0123456789abcdef";
    let share = |exp: i64, max: u32| {
        format!(r#"{{"package":"UEFDS0FHRQ==","files":[["{a}"],["{b}"]],"exp":{exp},"max":{max}}}"#)
    };
    std::fs::write(suite.join(format!("shares/{id}.json")), share(0, 2)).unwrap();

    // The version answers, and the module can read its folder.
    let v: serde_json::Value = serde_json::from_slice(&get(&format!("{base}?v")).body).unwrap();
    assert_eq!(v["version"], super::module::MODULE_VERSION);
    assert_eq!(v["folder"], true);
    assert_eq!(super::module::probe(&base).unwrap().version, super::module::MODULE_VERSION);
    // Only check files of the shares folder, by exact name.
    std::fs::write(suite.join("shares/check-00112233445566778899aabbccddeeff"), "token").unwrap();
    let checked = super::module::check(&base, "00112233445566778899aabbccddeeff").unwrap().unwrap();
    assert_eq!((checked.token.as_str(), checked.writes), ("token", true));
    assert!(!suite.join("shares/write-00112233445566778899aabbccddeeff").exists(), "the write test is removed");
    // Where PHP cannot write, the check says so (here: a folder blocks the test file).
    std::fs::create_dir(suite.join("shares/write-00112233445566778899aabbccddeeff")).unwrap();
    assert!(!super::module::check(&base, "00112233445566778899aabbccddeeff").unwrap().unwrap().writes);
    std::fs::remove_dir(suite.join("shares/write-00112233445566778899aabbccddeeff")).unwrap();
    assert_eq!(super::module::check(&base, "ffffffffffffffffffffffffffffffff").unwrap(), None);
    assert_eq!(get(&format!("{base}?check=../../etc/passwd")).status, 404);

    // Opening the share: the package, the block names and a ticket.
    let open = get(&format!("{base}?s={id}"));
    assert_eq!(open.status, 200);
    let o: serde_json::Value = serde_json::from_slice(&open.body).unwrap();
    assert_eq!(o["package"], "UEFDS0FHRQ==");
    assert_eq!(o["blocks"][1][0], b.as_str());
    let ticket = o["ticket"].as_str().unwrap().to_owned();
    assert_eq!(ticket.len(), 32);
    // Blocks only with the ticket, only of this share, only by index.
    assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=0&b=0")).body, b"block A");
    assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=1&b=0")).body, b"block B");
    assert_eq!(get(&format!("{base}?s={id}&t={}&f=0&b=0", "0".repeat(32))).status, 403);
    assert_eq!(get(&format!("{base}?s={id}&f=0&b=0")).status, 404);
    for bad in ["f=2&b=0", "f=0&b=1", "f=-1&b=0", "f=0&b=00", "f=0x0&b=0", "f=0&b=0.5", &format!("f={other}&b=0")] {
        assert_eq!(get(&format!("{base}?s={id}&t={ticket}&{bad}")).status, 404, "{bad}");
    }
    // A share whose description names a path instead of a block cannot reach other files.
    let evil = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    std::fs::write(
        suite.join(format!("shares/{evil}.json")),
        r#"{"package":"x","files":[["../../www/ghulsuite.php"]],"exp":0,"max":0}"#,
    )
    .unwrap();
    let t2: serde_json::Value = serde_json::from_slice(&get(&format!("{base}?s={evil}")).body).unwrap();
    assert_eq!(get(&format!("{base}?s={evil}&t={}&f=0&b=0", t2["ticket"].as_str().unwrap())).status, 404);

    // Two downloads allowed: the second opening works, the third is refused; the open tickets
    // keep working (a download in progress is not cut off).
    assert_eq!(get(&format!("{base}?s={id}")).status, 200);
    assert_eq!(get(&format!("{base}?s={id}")).status, 410);
    assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=0&b=0")).status, 200);
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(suite.join(format!("shares/{id}.state"))).unwrap()).unwrap();
    assert_eq!(state["count"], 2);
    assert!(state["tickets"].is_object());
    assert!(
        !String::from_utf8_lossy(&std::fs::read(suite.join(format!("shares/{id}.state"))).unwrap()).contains(&ticket),
        "tickets are stored hashed"
    );

    // A broken state is refused, never read as "nobody opened it yet".
    let state_file = suite.join(format!("shares/{id}.state"));
    let kept = std::fs::read(&state_file).unwrap();
    for broken in [&b"{broken"[..], b""] {
        std::fs::write(&state_file, broken).unwrap();
        assert_eq!(get(&format!("{base}?s={id}")).status, 503, "{broken:?}");
        assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=0&b=0")).status, 503, "{broken:?}");
        assert_eq!(std::fs::read(&state_file).unwrap(), broken, "left as it was");
    }
    // Too many open tickets: new openings wait, the tickets of those downloading stay valid.
    let mut full: serde_json::Value = serde_json::from_slice(&kept).unwrap();
    full["count"] = 0.into();
    let until = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
    for i in 0..1000 {
        if full["tickets"].as_object().unwrap().len() >= 1000 {
            break;
        }
        full["tickets"][format!("{i:064x}")] = until.into();
    }
    std::fs::write(&state_file, serde_json::to_vec(&full).unwrap()).unwrap();
    std::fs::write(suite.join(format!("shares/{id}.json")), share(0, 0)).unwrap();
    // A refused opening writes nothing (the state would be replaced by a new file).
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&state_file).unwrap());
    assert_eq!(get(&format!("{base}?s={id}")).status, 429);
    #[cfg(unix)]
    assert_eq!(std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&state_file).unwrap()), inode);
    // A write that fails (here a folder blocks the temporary file, as a full disk would) leaves
    // the old state as it was: no used-up link opens again.
    std::fs::write(&state_file, &kept).unwrap();
    let tmp = suite.join(format!("shares/{id}.state.tmp"));
    std::fs::create_dir(&tmp).unwrap();
    assert_eq!(get(&format!("{base}?s={id}")).status, 503);
    assert_eq!(std::fs::read(&state_file).unwrap(), kept);
    std::fs::remove_dir(&tmp).unwrap();
    assert_eq!(get(&format!("{base}?s={id}")).status, 200);
    assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=0&b=0")).status, 200);

    // Expired: refused at once, also for an open ticket.
    std::fs::write(suite.join(format!("shares/{id}.json")), share(1000, 0)).unwrap();
    assert_eq!(get(&format!("{base}?s={id}")).status, 410);
    assert_eq!(get(&format!("{base}?s={id}&t={ticket}&f=0&b=0")).status, 410);
    // Withdrawn (description gone): nothing.
    std::fs::remove_file(suite.join(format!("shares/{id}.json"))).unwrap();
    assert_eq!(get(&format!("{base}?s={id}")).status, 404);
    for bad in ["../../etc/passwd", "0123", &"A".repeat(32)] {
        assert_eq!(get(&format!("{base}?s={bad}")).status, 404, "{bad}");
    }
    // Nothing can be written over the web.
    let post = super::http::Url::parse(&format!("{base}?s={id}")).unwrap();
    let r = super::block_on(super::http::request("POST", &post, &[], b"x", None), super::TIMEOUT).unwrap();
    assert_eq!(r.status, 405);

    // The page that opens shares, with a strict content security policy.
    let page = get(&base);
    assert_eq!(page.status, 200);
    assert!(page.header("Content-Security-Policy").unwrap().contains("default-src 'none'"));
    assert!(String::from_utf8_lossy(&page.body).contains("ghulsuite.php?asset=js"));
    assert!(String::from_utf8_lossy(&get(&format!("{base}?asset=js")).body).contains("GHSHARE1"));
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
