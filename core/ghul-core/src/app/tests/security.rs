// SPDX-License-Identifier: AGPL-3.0-or-later
//! Points of the security report (docs/AUDIT.md, section 4) that were open until now.

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

/// B3: two devices change Drive at the same moment; neither change is lost.
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
    // While A creates its folder, B saves the index (after A loaded it, before A saves).
    let b2 = b.clone();
    let fs = Interfering {
        inner: LocalFs::new(&server),
        reads: Default::default(),
        armed: Default::default(),
        // The change reads the index twice (is it there, load) and once more just before
        // saving: B saves right then.
        at: 3,
        hook: std::sync::Mutex::new(Some(Box::new(move || {
            b2.drive_create_folder(None, "Von B".into()).unwrap();
        }))),
    };
    let fs = Arc::new(fs);
    a.drive_use_folder(fs.clone()).unwrap();
    fs.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    a.drive_create_folder(None, "Von A".into()).unwrap();
    assert!(fs.hook.lock().unwrap().is_none(), "B did not interfere");
    for s in [&a, &b] {
        assert_eq!(drive_names(s).unwrap(), vec!["Von A", "Von B"]);
    }
    let _ = std::fs::remove_dir_all(&server);
}

/// B11: a server cannot make the app derive the password with absurdly expensive parameters.
#[test]
fn a_server_cannot_force_expensive_password_parameters() {
    let d = Dirs::new("b11");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server_dir = std::env::temp_dir().join(format!("ghul-b11-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    s.sync_folder(&server).unwrap();
    let file = server_dir.join("suite/suite.ghlsuite");
    let good = std::fs::read(&file).unwrap();
    let salt = crate::vault::LockedContainer::parse(&good).unwrap().header().kdf.salt;
    let at = good.windows(32).position(|w| w == salt).unwrap();
    let mut forged = good.clone();
    forged[at - 12..at - 8].copy_from_slice(&(2u32 * 1024 * 1024).to_le_bytes());
    forged[at - 8..at - 4].copy_from_slice(&64u32.to_le_bytes());
    std::fs::write(&file, &forged).unwrap();
    assert!(matches!(s.sync_folder(&server), Err(GhulError::PasswordChangedElsewhere)));
    let t = std::time::Instant::now();
    assert!(matches!(s.adopt_password_folder(&server, "pw-123456"), Err(GhulError::InvalidInput { .. })));
    assert!(t.elapsed().as_secs() < 5, "derived with the forged parameters ({:?})", t.elapsed());
    std::fs::write(&file, &good).unwrap();
    s.sync_folder(&server).unwrap();
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// B13: an account (or server) edited on one device reaches the others.
#[test]
fn edited_accounts_reach_the_other_devices() {
    use super::super::mail::MailAccountInput;
    let server_dir = std::env::temp_dir().join(format!("ghul-b13-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let server = LocalFs::new(&server_dir);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new("b13-a"), Dirs::new("b13-b"));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let input = |name: &str| MailAccountInput {
        name: name.into(),
        display_name: "Max".into(),
        email: "max@example.org".into(),
        username: "max@example.org".into(),
        password: "pw".into(),
        servers: super::super::mail::mail_suggest_servers("max@example.org".into()),
        imap_cert: None,
        smtp_cert: None,
        signature: String::new(),
    };
    let id = a.mail_save_account(None, input("Privat")).unwrap();
    a.sync_folder(&server).unwrap();
    copy_suite(&server_dir, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    a.mail_save_account(Some(id.clone()), input("Arbeit")).unwrap();
    a.sync_folder(&server).unwrap();
    b.sync_folder(&server).unwrap();
    let names: Vec<String> = b.mail_accounts().unwrap().into_iter().map(|x| x.name).collect();
    assert_eq!(names, vec!["Arbeit"]);
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// L5: a web server that delivers the module as text (PHP not running) is detected.
#[test]
fn a_web_server_without_php_is_detected() {
    let module = super::super::servers::php_module_create();
    let dir = std::env::temp_dir().join(format!("ghul-l5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ghulsuite.php"), &module.content).unwrap();
    let web = web_server(&dir);
    let url = format!("http://127.0.0.1:{}/ghulsuite.php", web.port);
    assert!(matches!(super::super::servers::php_module_probe(url, module.key.clone()), Err(GhulError::PhpNotExecuted)));
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
