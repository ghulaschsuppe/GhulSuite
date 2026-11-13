// SPDX-License-Identifier: AGPL-3.0-or-later
//! Chaos tests: the server and the network misbehave, files break, the user does odd things.
//! Whatever happens, nothing the user saved may be lost, and both devices end up equal.

use std::sync::{Arc, Mutex};

use super::*;
use crate::net::remote::{LocalFs, RemoteEntry, RemoteFs};
use crate::net::{NetError, NetResult};

/// Deterministic random numbers (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// What goes wrong on the "server".
#[derive(Default, Clone, Copy)]
struct Faults {
    /// Percent of calls that fail with a network error.
    fail: u64,
    /// Percent of writes that leave a half-written file behind and then fail (dropped connection).
    torn: u64,
}

/// A folder as server that fails and tears files on request.
struct FaultyFs {
    inner: LocalFs,
    faults: Mutex<(Faults, Rng)>,
}

impl FaultyFs {
    fn new(dir: &std::path::Path, seed: u64) -> Self {
        FaultyFs { inner: LocalFs::new(dir), faults: Mutex::new((Faults::default(), Rng(seed))) }
    }
    fn set(&self, f: Faults) {
        self.faults.lock().unwrap().0 = f;
    }
    fn roll(&self) -> (bool, bool) {
        let mut g = self.faults.lock().unwrap();
        let f = g.0;
        (g.1.chance(f.fail), g.1.chance(f.torn))
    }
}

fn down() -> NetError {
    NetError::Protocol("simulated network failure".into())
}

impl RemoteFs for FaultyFs {
    fn list(&self, dir: &str) -> NetResult<Vec<RemoteEntry>> {
        if self.roll().0 {
            return Err(down());
        }
        self.inner.list(dir)
    }
    fn read(&self, path: &str) -> NetResult<Option<Vec<u8>>> {
        if self.roll().0 {
            return Err(down());
        }
        self.inner.read(path)
    }
    fn write(&self, path: &str, data: &[u8]) -> NetResult<()> {
        let (fail, torn) = self.roll();
        if torn {
            // A server without atomic replace: the old file is gone, half of the new one is there.
            let _ = self.inner.write(path, &data[..data.len() / 2]);
            return Err(down());
        }
        if fail {
            return Err(down());
        }
        self.inner.write(path, data)
    }
    fn delete(&self, path: &str) -> NetResult<()> {
        if self.roll().0 {
            return Err(down());
        }
        self.inner.delete(path)
    }
    fn mkdir_all(&self, dir: &str) -> NetResult<()> {
        self.inner.mkdir_all(dir)
    }
}

struct Pair {
    server: std::path::PathBuf,
    kf: Vec<u8>,
    _da: Dirs,
    db: Dirs,
    a: Arc<GhulSuite>,
    b: Arc<GhulSuite>,
    vault: String,
}

impl Drop for Pair {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.server);
    }
}

/// Two devices that share one synchronised suite with one GhulPass vault ("Privat", 3 entries).
fn pair(name: &str) -> Pair {
    let server = std::env::temp_dir().join(format!("ghul-chaos-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let fs = LocalFs::new(&server);
    let kf = generate_keyfile();
    let (da, db) = (Dirs::new(&format!("chaos-{name}-a")), Dirs::new(&format!("chaos-{name}-b")));
    let a = da.suite();
    a.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let vault = vault_id(&a, "Privat");
    for t in ["Bank", "Mail", "Shop"] {
        a.save_entry(vault.clone(), login(t, "u", "p")).unwrap();
    }
    a.sync_folder(&fs).unwrap();
    copy_suite(&server, &db);
    let b = db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&b), vec!["Bank", "Mail", "Shop"]);
    Pair { server, kf, _da: da, db, a, b, vault }
}

fn vault_file_on_server(p: &Pair) -> std::path::PathBuf {
    let file = vault_file(&p.a, &p.vault);
    p.server.join("suite").join(file)
}

#[test]
fn a_damaged_server_file_never_deletes_a_vault() {
    let p = pair("torn");
    let fs = LocalFs::new(&p.server);
    // The server copy is cut in half (crash while writing, full disk, broken upload).
    let path = vault_file_on_server(&p);
    let data = std::fs::read(&path).unwrap();
    std::fs::write(&path, &data[..data.len() / 2]).unwrap();
    p.b.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.b), vec!["Bank", "Mail", "Shop"], "B must keep its vault");
    // The server got the good copy back, A sees everything too.
    p.a.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.a), vec!["Bank", "Mail", "Shop"]);
    assert_eq!(std::fs::read(&path).unwrap().len() > data.len() / 2, true);
}

#[test]
fn a_missing_server_file_is_put_back_not_deleted_everywhere() {
    let p = pair("missing");
    let fs = LocalFs::new(&p.server);
    std::fs::remove_file(vault_file_on_server(&p)).unwrap();
    p.b.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.b), vec!["Bank", "Mail", "Shop"]);
    p.a.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.a), vec!["Bank", "Mail", "Shop"]);
    assert!(vault_file_on_server(&p).exists());
}

#[test]
fn an_empty_or_wrong_server_folder_deletes_nothing() {
    let p = pair("empty");
    // Typo in the folder, or a fresh server: an empty folder.
    let other = p.server.with_extension("other");
    let _ = std::fs::remove_dir_all(&other);
    let fs = LocalFs::new(&other);
    let r = p.b.sync_folder(&fs).unwrap();
    assert_eq!(r.deleted, 0);
    assert_eq!(titles(&p.b), vec!["Bank", "Mail", "Shop"]);
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn a_damaged_local_file_does_not_delete_the_server_copy() {
    let p = pair("local");
    let fs = LocalFs::new(&p.server);
    // B's own copy breaks (storage error, cut-off copy from a USB stick).
    let file = vault_file(&p.b, &p.vault);
    let st = p.db.storage();
    let data = st.read(file.clone()).unwrap().unwrap();
    st.write(file.clone(), data[..data.len() / 3].to_vec()).unwrap();
    p.b.lock();
    let b = p.db.suite();
    b.unlock("pw-123456".into(), None, None, None, Some(p.kf.clone())).unwrap();
    b.sync_folder(&fs).unwrap();
    // The server copy survives and repairs B.
    assert!(vault_file_on_server(&p).exists());
    assert_eq!(titles(&b), vec!["Bank", "Mail", "Shop"]);
    p.a.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.a), vec!["Bank", "Mail", "Shop"]);
}

#[test]
fn a_file_from_a_newer_app_is_left_alone() {
    let p = pair("newer");
    let fs = LocalFs::new(&p.server);
    // Another device with a newer GhulSuite wrote a kind this version does not know.
    let alien = p.server.join("suite").join("Zukunft.ghlnext");
    let mut bytes = b"GHULNEXT".to_vec();
    bytes.extend_from_slice(&[0u8; 200]);
    std::fs::write(&alien, &bytes).unwrap();
    // And the vault file itself comes in a newer format version.
    let path = vault_file_on_server(&p);
    let mut newer = std::fs::read(&path).unwrap();
    newer[8] = 0xff;
    newer[9] = 0x7f;
    std::fs::write(&path, &newer).unwrap();
    p.b.sync_folder(&fs).unwrap();
    assert_eq!(titles(&p.b), vec!["Bank", "Mail", "Shop"], "nothing deleted");
    assert_eq!(std::fs::read(&path).unwrap(), newer, "the newer file is not overwritten");
    assert_eq!(std::fs::read(&alien).unwrap(), bytes);
}

#[test]
fn deleting_a_vault_on_purpose_still_reaches_every_device() {
    let p = pair("delete");
    let fs = LocalFs::new(&p.server);
    let work = p.a.create_vault(VaultKind::Totp, "Arbeit".into()).unwrap();
    p.a.sync_folder(&fs).unwrap();
    p.b.sync_folder(&fs).unwrap();
    assert!(p.b.list_vaults().unwrap().iter().any(|v| v.id == work));
    p.b.remove_vault(work.clone()).unwrap();
    p.b.sync_folder(&fs).unwrap();
    p.a.sync_folder(&fs).unwrap();
    assert!(p.a.list_vaults().unwrap().iter().all(|v| v.id != work), "gone on A");
    assert!(p.b.list_vaults().unwrap().iter().all(|v| v.id != work), "stays gone on B");
    p.b.sync_folder(&fs).unwrap();
    assert!(p.b.list_vaults().unwrap().iter().all(|v| v.id != work));
}

#[test]
fn flaky_network_and_torn_uploads_lose_nothing() {
    let p = pair("flaky");
    let fs = FaultyFs::new(&p.server, 0xC0FFEE);
    fs.set(Faults { fail: 25, torn: 10 });
    let mut rng = Rng(7);
    let mut expected: Vec<String> = vec!["Bank".into(), "Mail".into(), "Shop".into()];
    let mut failures = 0;
    for round in 0..120 {
        for (name, s) in [("A", &p.a), ("B", &p.b)] {
            if rng.chance(70) {
                let t = format!("{name}-{round}");
                s.save_entry(p.vault.clone(), login(&t, "u", "p")).unwrap();
                expected.push(t);
            }
            if rng.chance(60) && s.sync_folder(&fs).is_err() {
                failures += 1;
            }
        }
    }
    assert!(failures > 20, "the network really failed: {failures}");
    // The network is good again.
    fs.set(Faults::default());
    for _ in 0..2 {
        p.a.sync_folder(&fs).unwrap();
        p.b.sync_folder(&fs).unwrap();
    }
    expected.sort();
    assert_eq!(titles(&p.a), expected);
    assert_eq!(titles(&p.b), expected);
}
