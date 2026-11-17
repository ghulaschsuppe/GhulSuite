// SPDX-License-Identifier: AGPL-3.0-or-later
//! The key change after a change of the protection (concept, section 6.4): an older copy of the
//! suite file reaches neither GhulDrive nor the servers any more.

use super::super::keychange::{KeyChangeState, AUTH_KEYS_FOR_TESTS};
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

fn names(s: &GhulSuite) -> Result<Vec<String>, GhulError> {
    let mut v: Vec<String> = s.drive_list(None, true)?.into_iter().map(|i| i.name).collect();
    v.sort();
    Ok(v)
}

fn local_drive(name: &str) -> std::path::PathBuf {
    let server = std::env::temp_dir().join(format!("ghul-kc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    server
}

fn attach(s: &GhulSuite, server: &std::path::Path) {
    let fs: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server));
    s.drive_use_folder(fs).unwrap();
}

/// After a new password, Drive's index is written with a new key: the suite reads on, an older
/// copy of the suite file (with the earlier key) reads nothing any more, nor the copy before.
#[test]
fn a_new_password_gives_drive_a_new_key() {
    let (da, old) = (Dirs::new("kc-a"), Dirs::new("kc-old"));
    let kf = generate_keyfile();
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("pw");
    attach(&a, &server);
    upload(&a, None, "a.txt", b"A");
    upload(&a, None, "b.txt", b"B");
    assert!(server.join("drive/index.bak").exists());
    super::everyday::copy_folder(&da, &old);
    let (key_before, _) = a.drive_test_keys();

    a.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    let st = a.key_change_status().unwrap();
    assert!(st.running);
    assert_eq!(st.drive_waiting, vec!["test".to_string()]);
    assert!(st.servers.is_empty(), "no server, no SSH key to change");
    // The next Drive call writes the index with the new key.
    assert_eq!(names(&a).unwrap(), vec!["a.txt", "b.txt"]);
    assert_ne!(a.drive_test_keys().0, key_before);
    assert!(!server.join("drive/index.bak").exists(), "the copy before had the earlier key");
    let st = a.key_change_continue().unwrap();
    assert!(!st.running, "{st:?}");
    assert_eq!(download(&a, &a.drive_list(None, true).unwrap()[0].id), b"A");
    upload(&a, None, "c.txt", b"C");

    // The older copy of the suite file opens with the old password, but Drive stays closed.
    let o = old.suite();
    o.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    attach(&o, &server);
    assert!(matches!(names(&o), Err(GhulError::Corrupted { .. })));
    assert!(o.drive_create_folder(None, "x".into()).is_err());
    assert!(names(&a).unwrap().contains(&"c.txt".to_string()), "nothing was written over");

    // After locking: the new password, Drive as before.
    a.lock();
    a.unlock("pw-neu-7890".into(), None, None, None, Some(kf)).unwrap();
    attach(&a, &server);
    assert_eq!(names(&a).unwrap(), vec!["a.txt", "b.txt", "c.txt"]);
    let _ = std::fs::remove_dir_all(&server);
}

/// A key change that cannot finish now (the index is locked by another device) continues later,
/// also after locking, and the earlier key works until then.
#[test]
fn a_key_change_continues_where_it_stopped() {
    let d = Dirs::new("kc-resume");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("resume");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    let code = s.new_recovery_code("neu".into()).unwrap();
    assert!(!code.is_empty());
    // Another device holds the index (a fresh lock): the change waits.
    let fresh = format!("other {}", super::super::suite::now_ms());
    std::fs::write(server.join("drive/index.lock"), fresh).unwrap();
    assert!(s.key_change_continue().is_ok());
    assert!(s.key_change_status().unwrap().running);
    s.lock();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    attach(&s, &server);
    assert!(s.key_change_status().unwrap().running, "kept over locking");
    std::fs::remove_file(server.join("drive/index.lock")).unwrap();
    let st = s.key_change_continue().unwrap();
    assert!(!st.running, "{st:?}");
    assert_eq!(names(&s).unwrap(), vec!["a.txt"]);
    let _ = std::fs::remove_dir_all(&server);
}

/// Adding a key or key file changes nothing for older copies (they are not weaker): no change.
#[test]
fn adding_a_factor_starts_no_key_change() {
    let d = Dirs::new("kc-add");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("add");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    s.add_keyfile(generate_keyfile(), "Datei".into()).unwrap();
    assert!(!s.key_change_status().unwrap().running);
    let _ = std::fs::remove_dir_all(&server);
}

/// Points the key change at another authorized_keys file for the test, and back.
struct AuthKeysAt;

impl AuthKeysAt {
    fn set(dir: &str, file: &str) -> AuthKeysAt {
        *AUTH_KEYS_FOR_TESTS.lock().unwrap_or_else(|p| p.into_inner()) = Some((dir.into(), file.into()));
        AuthKeysAt
    }
}

impl Drop for AuthKeysAt {
    fn drop(&mut self) {
        *AUTH_KEYS_FOR_TESTS.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

fn states(st: &super::super::keychange::KeyChangeStatus) -> Vec<KeyChangeState> {
    st.servers.iter().map(|s| s.state).collect()
}

/// Against the SFTP test server: the new SSH key goes in, the earlier one out; checked both ways.
/// Where the hoster keeps keys elsewhere (here: a file the server does not read), the user adds
/// and removes them by hand and the app checks.
#[test]
fn sftp_key_change_swaps_the_ssh_key() {
    use super::super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let (da, old) = (Dirs::new("kc-sftp"), Dirs::new("kc-sftp-old"));
    let kf = generate_keyfile();
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let first = a.ssh_public_key().unwrap();
    crate::net::tests::authorize_test_key(&authkeys, &first);
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let base = format!("{root}/kc-{run}");
    let input = ServerInput { name: "S".into(), host, port, user, password: None, base: base.clone() };
    let id = saved(a.save_server(input, Some(hostkey)).unwrap());
    a.drive_setup(id).unwrap();
    upload(&a, None, "a.txt", b"A");
    super::everyday::copy_folder(&da, &old);
    let body = |k: &str| k.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    let file_has = |k: &str| std::fs::read_to_string(&authkeys).unwrap().contains(&body(k));
    let (dir, name) = authkeys.rsplit_once('/').unwrap();

    // Automatic: the server reads ~/.ssh/authorized_keys (here: the test server's file).
    {
        let _at = AuthKeysAt::set(dir, name);
        a.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
        let st = a.key_change_status().unwrap();
        assert!(st.running);
        assert!(states(&st).iter().all(|s| *s == KeyChangeState::Waiting));
        let second = st.public_key.clone();
        assert_ne!(body(&second), body(&first));
        // Until the change is through, the earlier key still reaches the server.
        assert_eq!(names(&a).unwrap(), vec!["a.txt"]);
        let st = {
            let _file = crate::net::tests::AUTHKEYS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            a.key_change_continue().unwrap()
        };
        assert!(!st.running, "{st:?}");
        assert!(file_has(&second) && !file_has(&first));
    }
    assert_eq!(names(&a).unwrap(), vec!["a.txt"]);
    // The older copy of the suite file: its key no longer logs in.
    let o = old.suite();
    o.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert!(matches!(names(&o), Err(GhulError::ServerAuthFailed)), "{:?}", names(&o));

    // By hand: the hoster does not read the file the app changes.
    let elsewhere = format!("{root}/kc-keys-{run}");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let second = a.key_change_status().unwrap();
    assert!(!second.running);
    let second = a.ssh_public_key().unwrap();
    {
        let _at = AuthKeysAt::set(&elsewhere, "authorized_keys");
        a.change_password("pw-neu-7890".into(), "pw-dritt-555".into()).unwrap();
        let st = a.key_change_continue().unwrap();
        assert!(st.running);
        assert!(states(&st).iter().all(|s| *s == KeyChangeState::AddByHand), "{st:?}");
        // Meanwhile the earlier key still works.
        assert_eq!(names(&a).unwrap(), vec!["a.txt"]);
        // The user adds the new key in the hoster's panel.
        crate::net::tests::authorize_test_key(&authkeys, &st.public_key);
        let st = a.key_change_continue().unwrap();
        assert!(states(&st).iter().all(|s| *s == KeyChangeState::RemoveByHand), "{st:?}");
        // ... and removes the earlier one there.
        {
            let _file = crate::net::tests::AUTHKEYS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let text = std::fs::read_to_string(&authkeys).unwrap();
            let kept: String = text.lines().filter(|l| !l.contains(&body(&second))).map(|l| format!("{l}\n")).collect();
            std::fs::write(&authkeys, kept).unwrap();
        }
        let st = a.key_change_continue().unwrap();
        assert!(!st.running, "{st:?}");
    }
    a.lock();
    a.unlock("pw-dritt-555".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(names(&a).unwrap(), vec!["a.txt"]);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&elsewhere);
}
