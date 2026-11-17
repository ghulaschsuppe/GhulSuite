// SPDX-License-Identifier: AGPL-3.0-or-later
//! Points of the security report (docs/AUDIT.md, section 4) that were open until now.

use super::super::suite::now_ms;
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

fn drive_names(s: &GhulSuite) -> Result<Vec<String>, GhulError> {
    let mut v: Vec<String> = s.drive_list(None, true)?.into_iter().map(|i| i.name).collect();
    v.sort();
    Ok(v)
}

/// B3: a server that was reset (old index put back, index deleted) does not make files vanish.
#[test]
fn an_old_or_missing_drive_index_is_refused() {
    let d = Dirs::new("b3");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-b3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let connect = |s: &GhulSuite| {
        let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(&server));
        s.drive_use_folder(drive)
    };
    connect(&s).unwrap();
    upload(&s, None, "a.txt", b"A");
    let index = server.join("drive/index.ghldrv");
    let old = std::fs::read(&index).unwrap();
    upload(&s, None, "b.txt", b"B");
    assert_eq!(drive_names(&s).unwrap(), vec!["a.txt", "b.txt"]);

    // The server gets its old index back: refused, and nothing is written over it.
    std::fs::write(&index, &old).unwrap();
    assert!(matches!(drive_names(&s), Err(GhulError::DriveRolledBack)));
    assert!(matches!(s.drive_create_folder(None, "Neu".into()), Err(GhulError::DriveRolledBack)));
    assert_eq!(std::fs::read(&index).unwrap(), old);
    // Also after locking: the revision seen is kept in the Drive file.
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(matches!(connect(&s).and_then(|_| drive_names(&s)), Err(GhulError::DriveRolledBack)));
    // No index at all: refused too, instead of an empty Drive that the next upload would save.
    std::fs::remove_file(&index).unwrap();
    assert!(matches!(drive_names(&s), Err(GhulError::DriveRolledBack)));
    assert!(std::fs::read(&index).is_err());
    // Put back on purpose: the owner takes that state.
    std::fs::write(&index, &old).unwrap();
    s.drive_accept_server_state().unwrap();
    assert_eq!(drive_names(&s).unwrap(), vec!["a.txt"]);
    s.drive_create_folder(None, "Neu".into()).unwrap();
    let _ = std::fs::remove_dir_all(&server);
}

/// Another device that saves the Drive index between our load and our save.
struct Interfering {
    inner: LocalFs,
    reads: std::sync::atomic::AtomicUsize,
    /// Reads are counted from here on.
    armed: std::sync::atomic::AtomicBool,
    at: usize,
    hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl RemoteFs for Interfering {
    fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
        self.inner.list(dir)
    }
    fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
        if path.ends_with("index.ghldrv") && self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            let n = self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == self.at {
                if let Some(hook) = self.hook.lock().unwrap().take() {
                    hook();
                }
            }
        }
        self.inner.read(path)
    }
    fn write(&self, path: &str, data: &[u8]) -> crate::net::NetResult<()> {
        self.inner.write(path, data)
    }
    fn delete(&self, path: &str) -> crate::net::NetResult<()> {
        self.inner.delete(path)
    }
    fn mkdir_all(&self, dir: &str) -> crate::net::NetResult<()> {
        self.inner.mkdir_all(dir)
    }
}

/// B3: two devices change Drive at the same moment; neither change is lost. While A changes the
/// index (holding its lock), B's change waits and then applies to A's result.
#[test]
fn two_devices_changing_drive_at_once_lose_nothing() {
    let (da, db) = (Dirs::new("b3-a"), Dirs::new("b3-b"));
    let kf = generate_keyfile();
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-b3-race-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    a.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    // Device B: the same suite (copied folder), the same Drive.
    for e in std::fs::read_dir(da.root.join("vault")).unwrap().flatten() {
        db.storage().write(e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()).unwrap();
    }
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    b.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    // While A changes the index (after loading it), B starts its own change.
    let b2 = b.clone();
    let started: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>> = Arc::new(std::sync::Mutex::new(None));
    let started2 = started.clone();
    let fs = Interfering {
        inner: LocalFs::new(&server),
        reads: Default::default(),
        armed: Default::default(),
        at: 1,
        hook: std::sync::Mutex::new(Some(Box::new(move || {
            *started2.lock().unwrap() = Some(std::thread::spawn(move || {
                b2.drive_create_folder(None, "Von B".into()).unwrap();
            }));
            // B is waiting for A's lock now.
            std::thread::sleep(std::time::Duration::from_millis(300));
        }))),
    };
    let fs = Arc::new(fs);
    a.drive_use_folder(fs.clone()).unwrap();
    fs.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    a.drive_create_folder(None, "Von A".into()).unwrap();
    assert!(fs.hook.lock().unwrap().is_none(), "B did not interfere");
    started.lock().unwrap().take().unwrap().join().unwrap();
    for s in [&a, &b] {
        assert_eq!(drive_names(s).unwrap(), vec!["Von A", "Von B"]);
    }
    assert!(!server.join("drive/index.lock").exists(), "no lock left");
    let _ = std::fs::remove_dir_all(&server);
}

/// A device that stopped while it held the lock does not block Drive for ever; a fresh lock of
/// another device is waited for, then the change is refused.
#[test]
fn a_left_lock_is_taken_over_a_fresh_one_waited_for() {
    let d = Dirs::new("lock");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    s.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    let lock = server.join("drive/index.lock");
    std::fs::create_dir_all(server.join("drive")).unwrap();
    std::fs::write(&lock, format!("someone {}", now_ms() - 10 * 60_000)).unwrap();
    s.drive_create_folder(None, "Trotzdem".into()).unwrap();
    assert!(!lock.exists());
    std::fs::write(&lock, format!("someone {}", now_ms())).unwrap();
    let r = s.drive_create_folder(None, "Wartet".into());
    assert!(matches!(r, Err(GhulError::Network { .. })), "{r:?}");
    assert!(lock.exists(), "another device's lock stays");
    std::fs::remove_file(&lock).unwrap();
    assert_eq!(drive_names(&s).unwrap(), vec!["Trotzdem"]);
    let _ = std::fs::remove_dir_all(&server);
}

/// A damaged index: the state before the last change (`index.bak`) is offered and, taken,
/// written as the index again. Nothing is written over the damaged one meanwhile.
#[test]
fn a_damaged_index_falls_back_to_the_one_before() {
    let d = Dirs::new("bak");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-bak-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    s.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    upload(&s, None, "a.txt", b"A");
    upload(&s, None, "b.txt", b"B");
    let index = server.join("drive/index.ghldrv");
    let mut data = std::fs::read(&index).unwrap();
    let n = data.len();
    data[n - 5] ^= 1;
    std::fs::write(&index, &data).unwrap();
    assert!(matches!(drive_names(&s), Err(GhulError::DriveRolledBack)));
    assert!(matches!(s.drive_create_folder(None, "X".into()), Err(GhulError::DriveRolledBack)));
    assert_eq!(std::fs::read(&index).unwrap(), data, "not written over");
    s.drive_accept_server_state().unwrap();
    assert_eq!(drive_names(&s).unwrap(), vec!["a.txt"], "the state before the last change");
    s.drive_create_folder(None, "Weiter".into()).unwrap();
    assert_eq!(drive_names(&s).unwrap(), vec!["Weiter", "a.txt"]);
    let _ = std::fs::remove_dir_all(&server);
}

/// The index of before 0.9 (not bound to the suite) still opens, and is bound at the next
/// change; an index carried over from another suite with the same key does not open.
#[test]
fn the_old_index_format_opens_and_is_bound_to_the_suite() {
    let d = Dirs::new("v1");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-v1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    s.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    upload(&s, None, "a.txt", b"A");
    let index = server.join("drive/index.ghldrv");
    let v2 = std::fs::read(&index).unwrap();
    assert!(v2.starts_with(b"GHULDRV2"));
    // The same content as version 1 (what 0.8 wrote).
    let (key, suite) = s.drive_test_keys();
    let plain = super::super::drive::decrypt(b"GHULDRV2", &key, &v2, &suite).unwrap();
    std::fs::write(&index, super::super::drive::encrypt(b"GHULDRV1", &key, &plain, b"")).unwrap();
    assert_eq!(drive_names(&s).unwrap(), vec!["a.txt"]);
    s.drive_create_folder(None, "Neu".into()).unwrap();
    assert!(std::fs::read(&index).unwrap().starts_with(b"GHULDRV2"));
    // Bound to another suite: does not open (also not as the copy before).
    let other = super::super::drive::encrypt(b"GHULDRV2", &key, &plain, &[9u8; 16]);
    std::fs::write(&index, &other).unwrap();
    std::fs::remove_file(server.join("drive/index.bak")).unwrap();
    assert!(drive_names(&s).is_err());
    let _ = std::fs::remove_dir_all(&server);
}

/// L5: a web server that delivers the module as text (PHP not running) is detected: it does not
/// answer as the module, and the app sees its source.
#[test]
fn a_web_server_without_php_is_detected() {
    let file = crate::net::module::module_file("../ghulsuite").unwrap();
    let dir = std::env::temp_dir().join(format!("ghul-l5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ghulsuite.php"), &file).unwrap();
    let web = web_server(&dir);
    let url = format!("http://127.0.0.1:{}/ghulsuite.php", web.port);
    assert!(crate::net::module::probe(&url).is_err());
    assert!(crate::net::module::reveals_source(&url));
    let _ = std::fs::remove_dir_all(&dir);
}

/// L7: a confirmed contact key is never replaced by a key from a mail; a changed key is shown.
#[test]
fn confirmed_pgp_keys_stay_and_key_changes_are_reported() {
    use super::super::mailcrypt;
    let d = Dirs::new("l7");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Mail, "Mail")]), None, None, None).unwrap();
    let key = |email: &str| {
        let (armored, _) = mailcrypt::generate("X", email).unwrap();
        mailcrypt::public_of(&mailcrypt::secret_key(&armored).unwrap())
    };
    let learn = |email: &str, k: &pgp::composed::SignedPublicKey, seen: i64, mail: &str| {
        s.with_open(|o| super::super::mail::remember_key_seen(o, email, k, "autocrypt", seen, mail)).unwrap()
    };
    let info = |email: &str| s.mail_known_keys().unwrap().into_iter().find(|k| k.email == email).unwrap();
    // Unconfirmed: a newer mail with another key replaces it, but the change is reported.
    let (c1, c2) = (key("carol@example.org"), key("carol@example.org"));
    learn("carol@example.org", &c1, 100, "m1");
    learn("carol@example.org", &c2, 200, "m2");
    let carol = info("carol@example.org");
    assert_eq!(carol.fingerprint, mailcrypt::pretty_fingerprint(&mailcrypt::fingerprint(&c2)));
    assert_eq!(carol.previous, mailcrypt::pretty_fingerprint(&mailcrypt::fingerprint(&c1)));
    s.mail_verify_key("carol@example.org".into()).unwrap();
    assert!(info("carol@example.org").verified && info("carol@example.org").previous.is_empty());
    // Confirmed: a mail with another key changes nothing, it is only reported.
    let c3 = key("carol@example.org");
    learn("carol@example.org", &c3, 300, "m3");
    let carol = info("carol@example.org");
    assert_eq!(carol.fingerprint, mailcrypt::pretty_fingerprint(&mailcrypt::fingerprint(&c2)));
    assert!(carol.verified);
    assert_eq!(carol.offered, mailcrypt::pretty_fingerprint(&mailcrypt::fingerprint(&c3)));
}

/// Blocks nothing uses any more are removed once a day (older than a day only: an upload
/// writes its blocks before its entry); used ones stay, and nothing goes while the server's
/// index is older than the one seen.
#[test]
fn unused_blocks_are_tidied_up() {
    let d = Dirs::new("tidy");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-tidy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    s.drive_use_folder(Arc::new(LocalFs::new(&server))).unwrap();
    upload(&s, None, "a.txt", b"A");
    let blocks = server.join("drive/blocks");
    let used: Vec<std::path::PathBuf> = std::fs::read_dir(&blocks)
        .unwrap()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten().map(|f| f.path()))
        .collect();
    assert_eq!(used.len(), 1);
    let day_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 86_400);
    let orphan = |name: &str, old: bool| {
        let dir = blocks.join(&name[..2]);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, b"x").unwrap();
        if old {
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(day_ago).unwrap();
        }
        path
    };
    let old = orphan(&"ab".repeat(32), true);
    let young = orphan(&"cd".repeat(32), false);
    // The used block is old as well: it stays all the same.
    std::fs::File::options().write(true).open(&used[0]).unwrap().set_modified(day_ago).unwrap();
    let foreign = blocks.join("ab").join("notiz.txt");
    std::fs::write(&foreign, b"not ours").unwrap();
    std::fs::File::options().write(true).open(&foreign).unwrap().set_modified(day_ago).unwrap();

    assert_eq!(s.drive_tidy(false).unwrap(), 1);
    assert!(!old.exists() && young.exists() && used[0].exists() && foreign.exists());
    // Not again the same day (unless asked).
    let old2 = orphan(&"ef".repeat(32), true);
    assert_eq!(s.drive_tidy(false).unwrap(), 0);
    assert!(old2.exists());
    // An older index on the server: nothing is removed.
    let index = server.join("drive/index.ghldrv");
    let before = std::fs::read(&index).unwrap();
    upload(&s, None, "b.txt", b"B");
    std::fs::write(&index, &before).unwrap();
    assert!(matches!(s.drive_tidy(true), Err(GhulError::DriveRolledBack)));
    assert!(old2.exists());
    let _ = std::fs::remove_dir_all(&server);
}
