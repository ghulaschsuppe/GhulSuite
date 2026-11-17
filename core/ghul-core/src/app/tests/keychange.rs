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

/// The suite file with the current protection is saved: only then the key change goes on.
fn save_file(s: &GhulSuite) {
    let built = s.archive_build("Test".into()).unwrap();
    s.archive_saved(built.state, built.change).unwrap();
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
    // Until the suite file with the new password is saved, the earlier one is the only copy:
    // Drive stays with the earlier key, the change waits.
    let st = a.key_change_status().unwrap();
    assert!(st.running && st.waiting_for_save, "{st:?}");
    upload(&a, None, "b.txt", b"B");
    assert_eq!(names(&a).unwrap(), vec!["a.txt", "b.txt"]);
    assert!(a.key_change_continue().unwrap().waiting_for_save);
    {
        let o = old.suite();
        o.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
        attach(&o, &server);
        assert_eq!(names(&o).unwrap(), vec!["a.txt", "b.txt"], "the earlier file still opens Drive");
    }
    save_file(&a);
    let st = a.key_change_status().unwrap();
    assert!(st.running && !st.waiting_for_save);
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
    save_file(&s);
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

/// A damaged index during the change: the copy before it is not written as the new index
/// without the owner (the last change would be lost silently). After "take the server's state"
/// it has the new key and the change goes on.
#[test]
fn a_damaged_index_is_not_rekeyed_silently() {
    let d = Dirs::new("kc-damaged");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("damaged");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    upload(&s, None, "b.txt", b"B");
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    save_file(&s);
    let index = server.join("drive/index.ghldrv");
    std::fs::write(&index, b"kaputt").unwrap();
    assert!(matches!(names(&s), Err(GhulError::DriveRolledBack)));
    assert_eq!(std::fs::read(&index).unwrap(), b"kaputt", "nothing written over it");
    assert!(s.key_change_status().unwrap().running);
    s.drive_accept_server_state().unwrap();
    assert_eq!(names(&s).unwrap(), vec!["a.txt"]);
    assert!(!s.key_change_continue().unwrap().running);
    let _ = std::fs::remove_dir_all(&server);
}

/// A server put back to an older state during the change: refused as without a change.
#[test]
fn an_older_index_is_not_rekeyed_silently() {
    let d = Dirs::new("kc-older");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("older");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    let index = server.join("drive/index.ghldrv");
    let old = std::fs::read(&index).unwrap();
    upload(&s, None, "b.txt", b"B");
    upload(&s, None, "c.txt", b"C");
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    save_file(&s);
    std::fs::write(&index, &old).unwrap();
    assert!(matches!(names(&s), Err(GhulError::DriveRolledBack)));
    assert_eq!(std::fs::read(&index).unwrap(), old);
    assert!(s.key_change_status().unwrap().running);
    let _ = std::fs::remove_dir_all(&server);
}

/// Once the change is through, Drive's earlier keys are gone: a backup of the server from before
/// the change (or an index someone with an older copy put there) is not read and not taken over,
/// and nothing is written over it.
#[test]
fn after_the_change_an_index_with_the_earlier_key_is_refused() {
    let d = Dirs::new("kc-backup");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("backup");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    let index = server.join("drive/index.ghldrv");
    let backup = std::fs::read(&index).unwrap();
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    save_file(&s);
    assert!(!s.key_change_continue().unwrap().running);
    upload(&s, None, "b.txt", b"B");
    // The earlier key is no longer in the suite. (The hoster puts back its folder: no copy before.)
    std::fs::write(&index, &backup).unwrap();
    let _ = std::fs::remove_file(server.join("drive/index.bak"));
    let r = names(&s);
    assert!(matches!(r, Err(GhulError::Corrupted { .. })), "{r:?}");
    assert!(s.drive_accept_server_state().is_err());
    assert_eq!(std::fs::read(&index).unwrap(), backup, "nothing written over it");
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn many_changes_keep_the_key_a_waiting_location_needs() {
    let d = Dirs::new("kc-many");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = local_drive("many");
    attach(&s, &server);
    upload(&s, None, "a.txt", b"A");
    for i in 0..20 {
        s.new_recovery_code(format!("Code {i}")).unwrap();
        // Before saving, Drive keeps the key its index has (here: the one before all changes).
        upload(&s, None, &format!("{i}.txt"), b"x");
    }
    assert_eq!(names(&s).unwrap().len(), 21);
    assert!(s.key_change_continue().unwrap().waiting_for_save);
    save_file(&s);
    assert_eq!(names(&s).unwrap().len(), 21);
    assert!(!s.key_change_continue().unwrap().running);
    let _ = std::fs::remove_dir_all(&server);
}

/// Removing the earlier keys from authorized_keys keeps every other line byte for byte.
#[test]
fn only_the_earlier_keys_leave_authorized_keys() {
    let text = b"ssh-ed25519 AAAAold one\nssh-rsa BBBB other \xff\xfe bytes\n# note\nssh-ed25519 AAAAold two";
    let out = super::super::keychange::without_lines_for_tests(text, &["ssh-ed25519 AAAAold".to_string()]);
    assert_eq!(out, b"ssh-rsa BBBB other \xff\xfe bytes\n# note\n");
    let same = super::super::keychange::without_lines_for_tests(text, &[String::new()]);
    assert_eq!(same, text);
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

/// The tests that point the key change elsewhere run one after the other (one setting for all).
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// A server still waiting is removed: the change ends anyway (the earlier keys go).
#[test]
fn sftp_key_change_ends_when_the_waiting_server_is_removed() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    use super::super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("kc-removed");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    crate::net::tests::authorize_test_key(&authkeys, &s.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let input = ServerInput { name: "S".into(), host, port, user, password: None, base: format!("{root}/kcr-{run}") };
    let id = saved(s.save_server(input, Some(hostkey)).unwrap());
    let elsewhere = format!("{root}/kcr-keys-{run}");
    std::fs::create_dir_all(&elsewhere).unwrap();
    {
        let _at = AuthKeysAt::set(&elsewhere, "authorized_keys");
        s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
        save_file(&s);
        let st = s.key_change_continue().unwrap();
        assert!(states(&st).iter().all(|x| *x == KeyChangeState::AddByHand), "{st:?}");
    }
    s.remove_server(id).unwrap();
    let st = s.key_change_continue().unwrap();
    assert!(!st.running, "{st:?}");
    assert!(!s.key_change_status().unwrap().running);
    let _ = std::fs::remove_dir_all(&elsewhere);
    let _ = std::fs::remove_dir_all(format!("{root}/kcr-{run}"));
}

/// Against the SFTP test server: the new SSH key goes in, the earlier one out; checked both ways.
/// Where the hoster keeps keys elsewhere (here: a file the server does not read), the user adds
/// and removes them by hand and the app checks.
#[test]
fn sftp_key_change_swaps_the_ssh_key() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
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
        // Not before the suite file is saved: the earlier key stays the only one.
        let st = {
            let _file = crate::net::tests::AUTHKEYS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            a.key_change_continue().unwrap()
        };
        assert!(st.waiting_for_save && states(&st).iter().all(|s| *s == KeyChangeState::Waiting));
        assert!(file_has(&first) && !file_has(&st.public_key));
        save_file(&a);
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
        save_file(&a);
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

    // No key logs in at all (the old one was removed in the panel before the new one was added):
    // the user is asked to add the new one, not told to wait for the network.
    let third = a.ssh_public_key().unwrap();
    {
        let _at = AuthKeysAt::set(&elsewhere, "authorized_keys");
        a.change_password("pw-dritt-555".into(), "pw-viert-666".into()).unwrap();
        save_file(&a);
        {
            let _file = crate::net::tests::AUTHKEYS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let text = std::fs::read_to_string(&authkeys).unwrap();
            let kept: String = text.lines().filter(|l| !l.contains(&body(&third))).map(|l| format!("{l}\n")).collect();
            std::fs::write(&authkeys, kept).unwrap();
        }
        let st = a.key_change_continue().unwrap();
        let ids: std::collections::HashSet<_> = st.servers.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), st.servers.len(), "every server once: {st:?}");
        assert!(states(&st).iter().all(|s| *s == KeyChangeState::AddByHand), "{st:?}");
        crate::net::tests::authorize_test_key(&authkeys, &st.public_key);
        let st = a.key_change_continue().unwrap();
        assert!(!st.running, "{st:?}");
    }
    a.lock();
    a.unlock("pw-viert-666".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(names(&a).unwrap(), vec!["a.txt"]);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

/// A storage location removed during a key change, its files kept on the server: it gets the
/// new key first (otherwise its files could not be opened once the earlier key is gone). If that
/// does not work, removing it is refused.
#[test]
fn sftp_a_location_removed_during_a_key_change_stays_readable() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    use super::super::servers::ServerInput;
    let Some((host, port, user, root, authkeys, hostkey)) = crate::net::tests::sftp_env() else { return };
    let d = Dirs::new("kc-loc");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    crate::net::tests::authorize_test_key(&authkeys, &s.ssh_public_key().unwrap());
    let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let (first, second) = (format!("{root}/kcl-{run}-a"), format!("{root}/kcl-{run}-b"));
    let input = |base: &str| ServerInput {
        name: "S".into(),
        host: host.clone(),
        port,
        user: user.clone(),
        password: None,
        base: base.into(),
    };
    let a = saved(s.drive_save_server(input(&first), Some(hostkey.clone())).unwrap());
    upload(&s, None, "a.txt", b"A");
    upload(&s, None, "b.txt", b"B");
    let b = saved(s.drive_save_server(input(&second), Some(hostkey.clone())).unwrap());
    s.drive_switch_location(b.clone()).unwrap();
    s.change_password("pw-123456".into(), "pw-neu-7890".into()).unwrap();
    assert!(s.key_change_status().unwrap().drive_waiting.len() == 2);
    // Removed before the suite file is saved: refused (the change cannot go on yet).
    assert!(matches!(s.drive_remove_location(a.clone(), false, false), Err(GhulError::KeyChangeNotSaved)));
    save_file(&s);

    // Its index cannot be read (damaged, no copy before): not removed.
    let index = std::path::Path::new(&first).join("drive/index.ghldrv");
    let bak = std::path::Path::new(&first).join("drive/index.bak");
    let (good, good_bak) = (std::fs::read(&index).unwrap(), std::fs::read(&bak).unwrap());
    let mut bad = good.clone();
    let n = bad.len();
    bad[n - 5] ^= 1;
    std::fs::write(&index, &bad).unwrap();
    std::fs::remove_file(&bak).unwrap();
    assert!(s.drive_remove_location(a.clone(), false, false).is_err());
    assert_eq!(s.drive_locations().unwrap().len(), 2, "nothing removed");

    // Damaged with the copy before still there: switching to it works, the owner decides there.
    std::fs::write(&bak, &good_bak).unwrap();
    s.drive_switch_location(a.clone()).unwrap();
    assert!(matches!(names(&s), Err(GhulError::DriveRolledBack)));
    s.drive_accept_server_state().unwrap();
    assert_eq!(names(&s).unwrap(), vec!["a.txt"]);
    s.drive_switch_location(b).unwrap();
    s.drive_remove_location(a, false, false).unwrap();
    assert_eq!(s.drive_locations().unwrap().len(), 1);

    let (dir, name) = authkeys.rsplit_once('/').unwrap();
    {
        let _at = AuthKeysAt::set(dir, name);
        let _file = crate::net::tests::AUTHKEYS_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let st = s.key_change_continue().unwrap();
        assert!(!st.running, "{st:?}");
    }
    // Added again after the change: its files open with the new key.
    let again = saved(s.drive_save_server(input(&first), Some(hostkey)).unwrap());
    s.drive_switch_location(again).unwrap();
    assert_eq!(names(&s).unwrap(), vec!["a.txt"]);
    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}
