// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests of the mail archive against a simulated provider (every failure case, fast) and, where
//! the test servers run, against Dovecot and SFTP through the app's API.
//!
//! The rule behind all of them: at no time may a mail be neither at the provider nor checked in
//! the archive. `assert_nothing_lost` checks it after every step.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use super::*;
use crate::net::remote::LocalFs;

const DAY: i64 = 86_400;
/// "Now" of the tests: 1 June 2026, 12:00 UTC.
const NOW_MS: i64 = 1_780_315_200_000;
const NOW: i64 = NOW_MS / 1000;

// --- a simulated provider -------------------------------------------------------------------

#[derive(Clone)]
struct SimMail {
    uid: u32,
    raw: Vec<u8>,
    received: i64,
    seen: bool,
}

struct SimFolder {
    name: String,
    role: String,
    validity: u32,
    next_uid: u32,
    mails: Vec<SimMail>,
}

#[derive(Default)]
struct Sim {
    folders: RefCell<Vec<SimFolder>>,
    /// Renumbers this folder right before the next delete (UIDVALIDITY changes under our feet).
    renumber_before_delete: RefCell<Option<String>>,
    /// Fails every fetch after this many (connection lost).
    fail_fetch_after: Cell<Option<usize>>,
    fetches: Cell<usize>,
    /// Lists every old UID twice (a confused server).
    list_twice: Cell<bool>,
    /// Sizes reported instead of the real ones (huge mails without the memory).
    claimed_sizes: RefCell<BTreeMap<Vec<u8>, u64>>,
    /// Mails fetched (the test of huge mails checks they never are).
    fetched: RefCell<Vec<u32>>,
}

impl Sim {
    fn new() -> Sim {
        let s = Sim::default();
        for (name, role, validity) in [
            ("INBOX", "inbox", 100),
            ("Sent", "sent", 200),
            ("Drafts", "drafts", 300),
            ("Junk", "junk", 400),
            ("Trash", "trash", 500),
            ("Projekte/Haus", "", 600),
        ] {
            s.folders.borrow_mut().push(SimFolder {
                name: name.into(),
                role: role.into(),
                validity,
                next_uid: 1,
                mails: Vec::new(),
            });
        }
        s
    }

    fn add(&self, folder: &str, raw: &[u8], age_days: i64) -> u32 {
        let mut fs = self.folders.borrow_mut();
        let f = fs.iter_mut().find(|f| f.name == folder).unwrap();
        let uid = f.next_uid;
        f.next_uid += 1;
        f.mails.push(SimMail { uid, raw: raw.to_vec(), received: NOW - age_days * DAY, seen: false });
        uid
    }

    fn renumber(&self, folder: &str) {
        let mut fs = self.folders.borrow_mut();
        let f = fs.iter_mut().find(|f| f.name == folder).unwrap();
        f.validity += 1;
        for m in f.mails.iter_mut() {
            m.uid += 1000;
        }
        f.next_uid += 1000;
    }

    /// Every mail at the provider (content only).
    fn all(&self) -> Vec<Vec<u8>> {
        self.folders.borrow().iter().flat_map(|f| f.mails.iter().map(|m| m.raw.clone())).collect()
    }

    /// The size the provider reports (a test can pretend a mail is huge without holding it).
    fn size_of(&self, m: &SimMail) -> u64 {
        self.claimed_sizes.borrow().get(&m.raw).copied().unwrap_or(m.raw.len() as u64)
    }

    fn count(&self, folder: &str) -> usize {
        self.folders.borrow().iter().find(|f| f.name == folder).unwrap().mails.len()
    }
}

impl Provider for Sim {
    fn folders(&self) -> Result<Vec<Folder>> {
        Ok(self.folders.borrow().iter().map(|f| Folder { name: f.name.clone(), role: f.role.clone() }).collect())
    }

    fn old_uids(&self, folder: &str, before: i64) -> Result<(u32, Vec<u32>)> {
        let fs = self.folders.borrow();
        let f = fs.iter().find(|f| f.name == folder).ok_or(GhulError::NotFound)?;
        let mut uids: Vec<u32> = f.mails.iter().filter(|m| m.received < before * DAY).map(|m| m.uid).collect();
        if self.list_twice.get() {
            uids.extend(uids.clone());
        }
        Ok((f.validity, uids))
    }

    fn sizes(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<BTreeMap<u32, u64>> {
        let fs = self.folders.borrow();
        let f = fs.iter().find(|f| f.name == folder).ok_or(GhulError::NotFound)?;
        if f.validity != validity {
            return Ok(BTreeMap::new());
        }
        Ok(f.mails.iter().filter(|m| uids.contains(&m.uid)).map(|m| (m.uid, self.size_of(m))).collect())
    }

    fn fetch(&self, folder: &str, validity: u32, uid: u32) -> Result<Option<Archivable>> {
        let n = self.fetches.get();
        self.fetches.set(n + 1);
        self.fetched.borrow_mut().push(uid);
        if self.fail_fetch_after.get().is_some_and(|max| n >= max) {
            return Err(GhulError::Network { msg: "connection reset".into() });
        }
        let fs = self.folders.borrow();
        let f = fs.iter().find(|f| f.name == folder).ok_or(GhulError::NotFound)?;
        if f.validity != validity {
            return Ok(None);
        }
        Ok(f.mails.iter().find(|m| m.uid == uid).map(|m| Archivable {
            uid,
            raw: m.raw.clone(),
            seen: m.seen,
            flagged: false,
            labels: vec![],
            received: m.received,
        }))
    }

    fn delete(&self, folder: &str, validity: u32, uids: &[u32]) -> Result<bool> {
        if self.renumber_before_delete.borrow().as_deref() == Some(folder) {
            self.renumber_before_delete.borrow_mut().take();
            self.renumber(folder);
        }
        let mut fs = self.folders.borrow_mut();
        let f = fs.iter_mut().find(|f| f.name == folder).ok_or(GhulError::NotFound)?;
        if f.validity != validity || validity == 0 {
            return Ok(false);
        }
        f.mails.retain(|m| !uids.contains(&m.uid));
        Ok(true)
    }
}

// --- the archive in a folder ----------------------------------------------------------------

struct Place {
    root: std::path::PathBuf,
}

impl Place {
    fn new(name: &str) -> Place {
        let mut id = [0u8; 8];
        OsRng.fill_bytes(&mut id);
        let root = std::env::temp_dir().join(format!("ghul-archive-{name}-{}", crate::kdf::hex(&id)));
        std::fs::create_dir_all(&root).unwrap();
        Place { root }
    }

    fn store(&self, s: &Settings) -> Store {
        let fs: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(&self.root));
        let second: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(&self.root));
        Store { fs, fresh: Box::new(move || Ok(second.clone())), keys: Keys::new(s).unwrap(), copy: None }
    }
}

impl Place {
    fn all_files(&self) -> Vec<(String, Vec<u8>)> {
        let mut paths = Vec::new();
        walk(&self.root, &mut paths);
        paths
            .into_iter()
            .map(|p| {
                let rel = p.strip_prefix(&self.root).unwrap().to_string_lossy().replace('\\', "/");
                let data = std::fs::read(&p).unwrap();
                (rel, data)
            })
            .collect()
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn settings_with(epochs: u32) -> Settings {
    let mut s = Settings { id: "a1".into(), ..Settings::default() };
    for n in 0..epochs {
        let mut key = vec![0u8; 32];
        OsRng.fill_bytes(&mut key);
        s.epochs.push(Epoch { n, created: NOW_MS - 1000 + i64::from(n), key });
    }
    s.accounts.insert("acc".into());
    s
}

const ACCOUNT_DIR_EMAIL: &str = "Max.Mustermann@Example.org";

fn dir(store: &Store) -> String {
    store.keys.account_dir(ACCOUNT_DIR_EMAIL)
}

fn mail(n: u32, extra: &str) -> Vec<u8> {
    format!(
        "From: Erika Musterfrau <erika@example.net>\r\nTo: max@example.org\r\nSubject: Geheimer Betreff {n}\r\n\
         Date: Mon, 2 Mar 2026 10:00:00 +0100\r\nMessage-ID: <m{n}@example.net>\r\n\r\nGeheimer Text {n} {extra}\r\n"
    )
    .into_bytes()
}

fn run_at(store: &Store, sim: &Sim, s: &Settings, now_ms: i64, budget: u32) -> Result<MailArchiveRun> {
    run(store, sim, s, "acc", &dir(store), "device-a", now_ms, budget)
}

/// Every mail of the archive (checked: complete and matching its key).
fn archived(store: &Store) -> Vec<(Item, Vec<u8>)> {
    let mut out = Vec::new();
    for m in store.months(&dir(store)).unwrap().values() {
        for i in &m.items {
            assert!(store.verify_item(store.fs.as_ref(), i).unwrap(), "archived mail is complete");
            out.push((i.clone(), store.get_mail(i, false).unwrap().to_vec()));
        }
    }
    out
}

/// The rule: every mail that existed is at the provider or checked in the archive.
fn assert_nothing_lost(store: &Store, sim: &Sim, before: &[Vec<u8>]) {
    let at_provider = sim.all();
    let in_archive: Vec<Vec<u8>> = archived(store).into_iter().map(|(_, raw)| raw).collect();
    for m in before {
        assert!(
            at_provider.contains(m) || in_archive.contains(m),
            "a mail is neither at the provider nor in the archive: {}",
            String::from_utf8_lossy(&m[..m.len().min(80)])
        );
    }
    // The counting rule.
    let distinct_before: std::collections::BTreeSet<&Vec<u8>> = before.iter().collect();
    let mut now: std::collections::BTreeSet<&Vec<u8>> = at_provider.iter().collect();
    now.extend(in_archive.iter());
    assert!(now.len() >= distinct_before.len());
}

/// Each mail of a folder is in the archive at most once.
fn assert_no_duplicates(store: &Store) {
    let mut seen = BTreeMap::new();
    for (i, _) in archived(store) {
        assert!(seen.insert((i.key.clone(), i.folder.clone()), ()).is_none(), "archived twice: {}", i.folder);
    }
}

fn mailbox(sim: &Sim) -> Vec<Vec<u8>> {
    for n in 0..6 {
        sim.add("INBOX", &mail(n, "inbox"), 40 + i64::from(n));
    }
    for n in 6..8 {
        sim.add("INBOX", &mail(n, "new"), 3);
    }
    for n in 10..12 {
        sim.add("Sent", &mail(n, "sent"), 100);
    }
    sim.add("Drafts", &mail(20, "draft"), 100);
    sim.add("Junk", &mail(21, "spam"), 100);
    sim.add("Junk", &mail(22, "spam new"), 1);
    sim.add("Trash", &mail(23, "trash"), 100);
    sim.add("Projekte/Haus", &mail(24, "haus"), 400);
    sim.all()
}

// --- the tests ------------------------------------------------------------------------------

#[test]
fn archives_old_mails_and_leaves_the_provider_alone_by_default() {
    let place = Place::new("default");
    let s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    let before = mailbox(&sim);

    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    // 6 old in the inbox, 2 sent, 1 in an own folder; not drafts, spam, trash (left alone) or new mail.
    assert_eq!(r.archived, 9);
    assert_eq!((r.removed, r.deleted_by_rule, r.remaining), (0, 0, 0));
    assert_eq!(r.waiting_for_confirmation, 9, "deleting at the provider is off");
    assert_eq!(sim.all(), before, "the provider keeps everything");
    let roles: std::collections::BTreeSet<String> = archived(&store).iter().map(|(i, _)| i.folder.clone()).collect();
    assert_eq!(roles.into_iter().collect::<Vec<_>>(), vec!["INBOX", "Projekte/Haus", "Sent"]);
    // The archive returns each mail byte for byte.
    for (i, raw) in archived(&store) {
        assert!(before.contains(&raw));
        assert_eq!(i.size, raw.len() as u64);
    }

    // A second run finds nothing new.
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.archived, 0);
    assert_no_duplicates(&store);
    assert_nothing_lost(&store, &sim, &before);
}

#[test]
fn the_server_sees_no_content_and_no_names() {
    let place = Place::new("blind");
    let s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    mailbox(&sim);
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    let files = place.all_files();
    assert!(files.len() > 5);
    for (path, data) in files {
        for secret in ["Geheimer", "erika", "Erika", "example", "Mustermann", "INBOX", "Projekte", "Haus", "2026-03"] {
            assert!(!path.contains(secret), "{secret} in the file name {path}");
            assert!(!data.windows(secret.len()).any(|w| w == secret.as_bytes()), "{secret} readable in {path}");
        }
    }
}

#[test]
fn the_deadline_is_respected_for_every_choice() {
    for days in [1u32, 7, 30, 90, 365] {
        let place = Place::new("days");
        let mut s = settings_with(1);
        s.days = days;
        let store = place.store(&s);
        let sim = Sim::new();
        for age in [0i64, 2, 8, 31, 91, 366] {
            sim.add("INBOX", &mail(age as u32, "age"), age);
        }
        let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
        let expected = [0i64, 2, 8, 31, 91, 366].iter().filter(|a| **a > i64::from(days)).count() as u32;
        assert_eq!(r.archived, expected, "{days} days");
    }
}

#[test]
fn deleting_needs_the_switch_and_the_first_confirmation() {
    let place = Place::new("delete");
    let mut s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    let before = mailbox(&sim);

    // Switched on but not yet confirmed: the first run deletes nothing.
    s.delete_at_provider = true;
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!((r.archived, r.removed, r.waiting_for_confirmation), (9, 0, 9));
    assert_eq!(sim.all(), before);

    // Confirmed: exactly the archived mails go, nothing else.
    s.confirmed.insert("acc".into());
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!((r.archived, r.removed), (0, 9));
    assert_eq!(sim.count("INBOX"), 2, "the two new ones stay");
    assert_eq!(sim.count("Sent"), 0);
    assert_eq!((sim.count("Drafts"), sim.count("Junk"), sim.count("Trash")), (1, 2, 1), "left alone");
    assert_nothing_lost(&store, &sim, &before);
    assert!(archived(&store).iter().all(|(i, _)| i.removed));

    // New old mail later: archived and deleted in one run.
    let late = mail(50, "later");
    sim.add("INBOX", &late, 45);
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!((r.archived, r.removed), (1, 1));
    let mut all = before.clone();
    all.push(late);
    assert_nothing_lost(&store, &sim, &all);
}

#[test]
fn a_break_after_every_single_step_loses_nothing() {
    for step in ["uploaded", "verified", "indexed", "checked", "deleted"] {
        let place = Place::new("crash");
        let mut s = settings_with(1);
        s.delete_at_provider = true;
        s.confirmed.insert("acc".into());
        let store = place.store(&s);
        let sim = Sim::new();
        let before = mailbox(&sim);

        FAIL_AT.with(|f| *f.borrow_mut() = Some(step));
        let r = run_at(&store, &sim, &s, NOW_MS, 1000);
        FAIL_AT.with(|f| *f.borrow_mut() = None);
        assert!(r.is_err(), "the run stops at {step}");
        assert_nothing_lost(&store, &sim, &before);

        // The next run (the lock is this device's own) finishes the job, without duplicates.
        let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
        assert_eq!(r.remaining, 0, "{step}");
        assert_no_duplicates(&store);
        assert_nothing_lost(&store, &sim, &before);
        assert_eq!(sim.count("INBOX"), 2, "{step}: only the new mails stay at the provider");
        assert_eq!(archived(&store).len(), 9, "{step}");
    }
}

#[test]
fn a_lost_connection_while_fetching_and_small_budgets_continue_where_they_stopped() {
    let place = Place::new("budget");
    let s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    let mut before = Vec::new();
    for n in 0..260 {
        let m = mail(n, "many");
        sim.add("INBOX", &m, 40 + i64::from(n % 50));
        before.push(m);
    }
    sim.fail_fetch_after.set(Some(STEP + 7));
    assert!(run_at(&store, &sim, &s, NOW_MS, 1000).is_err());
    assert_nothing_lost(&store, &sim, &before);
    let partly = archived(&store).len();
    assert!((STEP..260).contains(&partly), "the first step was finished: {partly}");
    sim.fail_fetch_after.set(None);

    let mut rounds = 0;
    loop {
        let r = run_at(&store, &sim, &s, NOW_MS, 40).unwrap();
        rounds += 1;
        if r.remaining == 0 {
            break;
        }
        assert!(rounds < 20);
    }
    assert_eq!(archived(&store).len(), 260);
    assert_no_duplicates(&store);
}

#[test]
fn a_renumbered_folder_is_matched_by_content_and_never_deleted_by_old_numbers() {
    let place = Place::new("renumber");
    let mut s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    let before = mailbox(&sim);
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();

    // The provider renumbers the inbox: nothing is archived twice, the UIDs follow.
    sim.renumber("INBOX");
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.renumbered, 1);
    assert_eq!(r.archived, 6, "matched again (same content, new numbers)");
    assert_no_duplicates(&store);
    assert_eq!(archived(&store).len(), 9);
    assert!(archived(&store).iter().filter(|(i, _)| i.folder == "INBOX").all(|(i, _)| i.validity == 101));

    // Renumbered between the check and the delete: the provider refuses, nothing is marked.
    s.delete_at_provider = true;
    s.confirmed.insert("acc".into());
    *sim.renumber_before_delete.borrow_mut() = Some("INBOX".into());
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(sim.count("INBOX"), 8, "nothing deleted in the renumbered inbox");
    assert!(archived(&store).iter().filter(|(i, _)| i.folder == "INBOX").all(|(i, _)| !i.removed));
    assert_nothing_lost(&store, &sim, &before);
    let _ = r;

    // The next run matches again and only then deletes.
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.renumbered, 1);
    assert_eq!(sim.count("INBOX"), 2);
    assert_nothing_lost(&store, &sim, &before);
    assert_no_duplicates(&store);
}

#[test]
fn duplicate_listings_twins_and_broken_mails() {
    let place = Place::new("odd");
    let mut s = settings_with(1);
    s.delete_at_provider = true;
    s.confirmed.insert("acc".into());
    let store = place.store(&s);
    let sim = Sim::new();
    // The same mail twice in a folder, odd and broken mails, an empty one, a large one (3 blocks).
    let twin = mail(1, "twin");
    sim.add("INBOX", &twin, 50);
    sim.add("INBOX", &twin, 50);
    let mut big = mail(2, "big");
    big.extend((0..9 * 1024 * 1024).map(|i| (i % 251) as u8));
    for odd in [
        b"".to_vec(),
        b"no header at all".to_vec(),
        b"\r\n\r\n".to_vec(),
        vec![0xff, 0xfe, 0x00, 0x13, 0x37],
        b"Subject: =?utf-8?B?/w==?=\n\nkaputt".to_vec(),
        big.clone(),
    ] {
        sim.add("INBOX", &odd, 60);
    }
    sim.list_twice.set(true);
    let before = sim.all();

    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(sim.count("INBOX"), 0, "all archived, checked and deleted");
    assert_eq!(r.removed, 8);
    assert_nothing_lost(&store, &sim, &before);
    assert_no_duplicates(&store);
    let items = archived(&store);
    assert_eq!(items.len(), 7, "the twins are one mail with two UIDs");
    let t = items.iter().find(|(_, raw)| *raw == twin).unwrap();
    assert_eq!(t.0.uids.len(), 2);
    let b = items.iter().find(|(_, raw)| *raw == big).unwrap();
    assert_eq!(b.0.blocks.len(), 3);
}

#[test]
fn a_damaged_or_missing_archive_file_stops_deleting() {
    for damage in ["flip", "missing", "swap"] {
        let place = Place::new("damage");
        let mut s = settings_with(1);
        let store = place.store(&s);
        let sim = Sim::new();
        let before = mailbox(&sim);
        run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();

        // Damage one block of an inbox mail on the server.
        let (victim, _) = archived(&store).into_iter().find(|(i, _)| i.folder == "INBOX").unwrap();
        let path = place.root.join(block_path(&victim.blocks[0]));
        match damage {
            "flip" => {
                let mut d = std::fs::read(&path).unwrap();
                let n = d.len() - 3;
                d[n] ^= 1;
                std::fs::write(&path, d).unwrap();
            }
            "missing" => std::fs::remove_file(&path).unwrap(),
            _ => {
                // Another block in its place.
                let (other, _) = archived(&store).into_iter().find(|(i, _)| i.key != victim.key).unwrap();
                std::fs::copy(place.root.join(block_path(&other.blocks[0])), &path).unwrap();
            }
        }
        s.delete_at_provider = true;
        s.confirmed.insert("acc".into());
        let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
        assert_eq!(r.removed, 8, "{damage}: every other mail goes");
        let left: Vec<Vec<u8>> = sim.all();
        let victim_raw = before.iter().find(|m| store.keys.mail_key(m) == victim.key).unwrap();
        assert!(left.contains(victim_raw), "{damage}: the damaged mail stays at the provider");
    }
}

#[test]
fn spam_and_trash_follow_their_rule() {
    let place = Place::new("rules");
    let mut s = settings_with(1);
    s.spam = ArchiveFolderRule::Delete;
    s.trash = ArchiveFolderRule::Archive;
    s.sent = false;
    let store = place.store(&s);
    let sim = Sim::new();
    mailbox(&sim);
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.deleted_by_rule, 1, "old spam deleted");
    assert_eq!(sim.count("Junk"), 1, "new spam stays");
    assert_eq!(sim.count("Sent"), 2, "sent mail left alone when switched off");
    let folders: Vec<String> = archived(&store).iter().map(|(i, _)| i.folder.clone()).collect();
    assert!(folders.contains(&"Trash".to_string()));
    assert!(!folders.contains(&"Sent".to_string()));
    assert!(!folders.contains(&"Drafts".to_string()), "drafts are never archived");
}

#[test]
fn one_device_at_a_time() {
    let place = Place::new("lock");
    let mut s = settings_with(1);
    s.copies.insert("device-b".into(), DeviceCopy { name: "Windows-PC".into(), ..DeviceCopy::default() });
    let store = place.store(&s);
    let sim = Sim::new();
    mailbox(&sim);
    assert_eq!(store.lock(&dir(&store), "device-b", NOW_MS).unwrap(), Ok(()));
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.busy, "Windows-PC");
    assert_eq!(r.archived, 0);
    // The other device's lock runs out after a while (it crashed): then this one archives.
    let r = run_at(&store, &sim, &s, NOW_MS + LOCK_MS + 1, 1000).unwrap();
    assert_eq!((r.busy.as_str(), r.archived), ("", 9));
    // And gives the lock back at the end.
    assert_eq!(store.lock(&dir(&store), "device-b", NOW_MS + LOCK_MS + 2).unwrap(), Ok(()));
}

#[test]
fn key_epochs_keep_old_backups_from_reading_new_mail() {
    let place = Place::new("epochs");
    let s1 = settings_with(1);
    let sim = Sim::new();
    sim.add("INBOX", &mail(1, "before"), 50);
    run_at(&place.store(&s1), &sim, &s1, NOW_MS, 1000).unwrap();

    // The protection changed: a second epoch; new mail is written with it.
    let mut s2 = s1.clone();
    let mut key = vec![0u8; 32];
    OsRng.fill_bytes(&mut key);
    s2.epochs.push(Epoch { n: 1, created: NOW_MS, key });
    let store2 = place.store(&s2);
    sim.add("INBOX", &mail(2, "after"), 50);
    run_at(&store2, &sim, &s2, NOW_MS, 1000).unwrap();
    assert_eq!(archived(&store2).len(), 2);

    // The old keys open the block of the old mail, never the new one; the index (rewritten) not either.
    let old = place.store(&s1);
    let items = archived(&store2);
    let first = items.iter().find(|(_, raw)| *raw == mail(1, "before")).unwrap();
    let second = items.iter().find(|(_, raw)| *raw == mail(2, "after")).unwrap();
    assert!(old.get_mail(&first.0, false).is_ok());
    assert!(matches!(old.get_mail(&second.0, false), Err(GhulError::Corrupted { .. })));
    assert!(old.months(&dir(&old)).is_err());
}

#[test]
fn an_archive_file_cannot_be_passed_off_as_another() {
    let place = Place::new("swap");
    let s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    sim.add("INBOX", &mail(1, "a"), 50);
    let older = String::from_utf8(mail(2, "b")).unwrap().replace("2 Mar 2026", "2 Mar 2025");
    sim.add("INBOX", older.as_bytes(), 400);
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    let months: Vec<String> = store.months(&dir(&store)).unwrap().keys().cloned().collect();
    assert_eq!(months.len(), 2);
    // One month file copied over the other: refused (the path is part of the encryption).
    std::fs::copy(place.root.join(&months[0]), place.root.join(&months[1])).unwrap();
    assert!(matches!(store.months(&dir(&store)), Err(GhulError::Corrupted { .. })));
}

#[test]
fn moving_the_archive_copies_and_checks_everything() {
    let a = Place::new("from");
    let b = Place::new("to");
    let s = settings_with(1);
    let store = a.store(&s);
    let sim = Sim::new();
    mailbox(&sim);
    Store::write_head(store.fs.as_ref(), &store.keys, &Head { id: s.id.clone(), created: NOW_MS }).unwrap();
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    let target = LocalFs::new(&b.root);
    copy_archive(&store, &target).unwrap();
    let moved = b.store(&s);
    assert_eq!(Store::head(moved.fs.as_ref(), &moved.keys).unwrap().unwrap().id, "a1");
    let mut x: Vec<Vec<u8>> = archived(&store).into_iter().map(|(_, r)| r).collect();
    let mut y: Vec<Vec<u8>> = archived(&moved).into_iter().map(|(_, r)| r).collect();
    x.sort();
    y.sort();
    assert_eq!(x, y);
    // Another archive's head is not ours.
    let other = settings_with(1);
    assert!(Store::head(moved.fs.as_ref(), &Keys::new(&other).unwrap()).is_err());
}

#[test]
fn the_index_entry_of_a_mail_is_listed_and_found() {
    let place = Place::new("summary");
    let s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    sim.add("INBOX", &mail(7, "Rechnung Dachdecker"), 50);
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    let (item, _) = archived(&store).remove(0);
    let m = archived_mail(&item);
    assert_eq!(m.summary.subject, "Geheimer Betreff 7");
    assert_eq!(m.summary.from_address, "erika@example.net");
    assert!(m.summary.preview.contains("Dachdecker"));
    assert!(m.summary.date > 0);
    assert!(m.at_provider);
}

#[test]
fn mails_larger_than_the_limit_stay_at_the_provider() {
    let place = Place::new("huge");
    let mut s = settings_with(1);
    s.delete_at_provider = true;
    s.confirmed.insert("acc".into());
    let store = place.store(&s);
    let sim = Sim::new();
    let before = mailbox(&sim);
    // A mail of 1 GB (as the provider reports it) between ordinary old mails.
    let huge = mail(77, "1 GB attachment");
    let huge_uid = sim.add("INBOX", &huge, 50);
    sim.claimed_sizes.borrow_mut().insert(huge.clone(), 1024 * 1024 * 1024);
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!((r.archived, r.removed, r.too_large), (9, 9, 1));
    assert!(!sim.fetched.borrow().contains(&huge_uid), "a too large mail is never fetched");
    assert!(sim.all().contains(&huge), "it stays at the provider");
    assert_nothing_lost(&store, &sim, &before);
    // The next run does not try it again, and still says so.
    sim.fetched.borrow_mut().clear();
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!((r.archived, r.removed, r.too_large, r.remaining), (0, 0, 1, 0));
    assert!(sim.fetched.borrow().is_empty());
    assert!(sim.all().contains(&huge));
}

/// Counts what is read from the server (for the tests that a run reads only what it needs).
struct CountingFs {
    inner: LocalFs,
    read_bytes: std::sync::atomic::AtomicU64,
}

impl RemoteFs for CountingFs {
    fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
        self.inner.list(dir)
    }
    fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
        let r = self.inner.read(path)?;
        if let Some(d) = &r {
            self.read_bytes.fetch_add(d.len() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(r)
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

#[test]
fn a_run_reads_only_what_it_needs() {
    let place = Place::new("lean");
    let s = settings_with(1);
    let counting = Arc::new(CountingFs { inner: LocalFs::new(&place.root), read_bytes: Default::default() });
    let fs: Arc<dyn RemoteFs> = counting.clone();
    let second = fs.clone();
    let store = Store { fs, fresh: Box::new(move || Ok(second.clone())), keys: Keys::new(&s).unwrap(), copy: None };
    let sim = Sim::new();
    // 600 mails over 50 months, each with a 20 KB attachment.
    for n in 0..600u32 {
        let mut m =
            String::from_utf8(mail(n, "lean")).unwrap().replace("2 Mar 2026", &format!("2 Mar {}", 2020 + n % 5));
        m = m.replace("Mar", ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct"][(n % 10) as usize]);
        let mut raw = m.into_bytes();
        raw.extend(std::iter::repeat_n(b'x', 20_000));
        sim.add("INBOX", &raw, 40 + i64::from(n % 900));
    }
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(archived(&store).len(), 600);
    // Nothing new: the run reads the state file, not the month files of the whole archive.
    counting.read_bytes.store(0, std::sync::atomic::Ordering::Relaxed);
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.archived, 0);
    let read = counting.read_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let months: u64 = place.all_files().iter().filter(|(p, _)| p.contains("/m/")).map(|(_, d)| d.len() as u64).sum();
    assert!(read * 4 < months, "read {read} bytes, the month files are {months}");
    // One new old mail: only its month is read and written.
    sim.add("INBOX", &mail(9999, "one more"), 45);
    counting.read_bytes.store(0, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(run_at(&store, &sim, &s, NOW_MS, 1000).unwrap().archived, 1);
    let read = counting.read_bytes.load(std::sync::atomic::Ordering::Relaxed);
    assert!(read * 4 < months, "read {read} bytes for one mail, the month files are {months}");
}

#[test]
fn a_lost_state_file_is_rebuilt_from_the_month_files() {
    let place = Place::new("state");
    let mut s = settings_with(1);
    let store = place.store(&s);
    let sim = Sim::new();
    let before = mailbox(&sim);
    run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    // The state file (with the index) is gone or damaged: rebuilt, nothing archived twice.
    for damage in ["gone", "garbage"] {
        let path = place.root.join(dir(&store)).join("state");
        match damage {
            "gone" => std::fs::remove_file(&path).unwrap(),
            _ => std::fs::write(&path, b"GHULARS1 not a state file at all, but long enough to try").unwrap(),
        }
        let r = run_at(&store, &sim, &s, NOW_MS, 1000);
        if damage == "garbage" {
            // A damaged state file is an error, never a reason to archive or delete anything.
            assert!(r.is_err());
            std::fs::remove_file(&path).unwrap();
            run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
        } else {
            assert_eq!(r.unwrap().archived, 0, "{damage}");
        }
        assert_no_duplicates(&store);
        assert_eq!(archived(&store).len(), 9);
    }
    // Deleting still works from the rebuilt index, and still reads every mail back first.
    s.delete_at_provider = true;
    s.confirmed.insert("acc".into());
    let r = run_at(&store, &sim, &s, NOW_MS, 1000).unwrap();
    assert_eq!(r.removed, 9);
    assert_nothing_lost(&store, &sim, &before);
}

/// 30 000 mails, some with large attachments, through the whole cycle with random failures:
/// archived, checked, deleted at the provider, read back and compared. Runs in the nightly run.
#[test]
#[ignore]
fn chaos_large_mailbox_with_random_failures() {
    let place = Place::new("large");
    let mut s = settings_with(1);
    s.delete_at_provider = true;
    s.confirmed.insert("acc".into());
    let store = place.store(&s);
    let sim = Sim::new();
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for n in 0..30_000u32 {
        let folder = ["INBOX", "Sent", "Projekte/Haus"][(n % 3) as usize];
        let mut m = mail(n, "bulk");
        if n % 100 == 0 {
            m.extend((0..(next() % (3 * 1024 * 1024))).map(|i| (i % 253) as u8));
        }
        sim.add(folder, &m, 31 + (next() % 3000) as i64);
    }
    let before = sim.all();
    let steps = ["uploaded", "verified", "indexed", "checked", "deleted"];
    let mut rounds = 0;
    loop {
        rounds += 1;
        assert!(rounds < 400);
        let fail = (next() % 4 == 0).then(|| steps[(next() % 5) as usize]);
        FAIL_AT.with(|f| *f.borrow_mut() = fail);
        let r = run_at(&store, &sim, &s, NOW_MS, 2_000);
        FAIL_AT.with(|f| *f.borrow_mut() = None);
        if let Ok(r) = r {
            if r.remaining == 0 && sim.all().is_empty() {
                break;
            }
        }
        if rounds % 25 == 0 {
            assert_nothing_lost(&store, &sim, &before);
        }
    }
    assert_nothing_lost(&store, &sim, &before);
    assert_no_duplicates(&store);
    assert_eq!(archived(&store).len(), 30_000);
}
