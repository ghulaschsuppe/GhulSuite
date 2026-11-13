// SPDX-License-Identifier: AGPL-3.0-or-later
//! Everyday test and stress tests: the suite used like in real life.
//!
//! - `a_month_of_everyday_use_on_two_devices` (runs with every `cargo test`): a phone and a
//!   laptop share one suite for 30 simulated days. Every day both add, change and delete
//!   logins, 2FA codes, appointments and Drive files, go offline for days, sync at random
//!   moments, restart the app, change the password and make a backup. At the end both devices
//!   must show exactly the same, nothing may be lost, and the server must not contain plaintext.
//! - The stress tests are `#[ignore]`d and run in release mode in CI
//!   (`cargo test --release -p ghul-core -- --ignored stress`): thousands of entries, codes and
//!   appointments, big and many Drive files, many app starts, and parallel access from several
//!   threads. They print their timings and fail when something becomes unreasonably slow.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::super::cal::{CalCalendar, CalDeleteMode, CalEvent};
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

/// Small deterministic random generator (xorshift): the same "month" on every run.
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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
    /// A fresh 2FA secret (32 Base32 characters).
    fn secret(&mut self) -> String {
        (0..32).map(|_| b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[self.below(32) as usize] as char).collect()
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

/// A word that must never show up in plaintext on the server.
const MARKER: &str = "GhulGeheimnis";

fn pw_of(e: &EntryData) -> String {
    e.fields
        .iter()
        .find_map(|f| match (&f.name[..], &f.value) {
            ("password", FieldValue::Secret { value }) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn set_pw(e: &mut EntryData, pw: &str) {
    for f in e.fields.iter_mut().filter(|f| f.name == "password") {
        f.value = FieldValue::Secret { value: pw.into() };
    }
}

/// Everything one device created and still expects to see (on both devices, after syncing).
#[derive(Default)]
struct Expected {
    /// title -> (id, password)
    logins: BTreeMap<String, (Vec<u8>, String)>,
    /// issuer -> id
    totps: BTreeMap<String, Vec<u8>>,
    /// title -> (id, start)
    events: BTreeMap<String, (String, String)>,
    /// name -> (id, content)
    files: BTreeMap<String, (String, Vec<u8>)>,
}

struct Device {
    name: &'static str,
    dirs: Dirs,
    suite: Arc<GhulSuite>,
    mine: Expected,
    counter: u32,
}

impl Device {
    /// The app is closed and started again: a new instance on the same folders, unlocked.
    fn restart(&mut self, password: &str, keyfile: &[u8], drive: &Arc<dyn RemoteFs>) {
        self.suite.lock();
        self.suite = self.dirs.suite();
        self.suite.unlock(password.into(), None, None, None, Some(keyfile.to_vec())).unwrap();
        self.suite.drive_use_folder(drive.clone()).unwrap();
    }
}

fn all_titles(s: &GhulSuite) -> Vec<String> {
    titles(s)
}

fn totp_issuers(s: &GhulSuite) -> Vec<String> {
    let mut v: Vec<String> = s.totp_codes(1_800_000_000).unwrap().into_iter().map(|c| c.issuer).collect();
    v.sort();
    v
}

fn event_titles(s: &GhulSuite) -> Vec<String> {
    let mut v: Vec<String> = s
        .cal_occurrences("2026-12-01".into(), "2027-02-01".into(), String::new(), "de".into())
        .unwrap()
        .into_iter()
        .filter(|o| !o.event_id.is_empty())
        .map(|o| o.title)
        .collect();
    v.sort();
    v
}

fn drive_names(s: &GhulSuite) -> Vec<String> {
    let mut v: Vec<String> =
        s.drive_list(None, true).unwrap().into_iter().filter(|i| !i.is_dir).map(|i| i.name).collect();
    v.sort();
    v
}

#[test]
fn a_month_of_everyday_use_on_two_devices() {
    let server_dir = std::env::temp_dir().join(format!("ghul-everyday-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let sync = LocalFs::new(server_dir.join("sync"));
    let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server_dir.join("drive")));
    let kf = generate_keyfile();
    let mut password = "Alltag-pw-2026".to_string();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

    // Day 0: the phone sets everything up, the laptop joins.
    let phone_dirs = Dirs::new("everyday-phone");
    let phone_suite = phone_dirs.suite();
    phone_suite
        .create(
            options(
                &password,
                Some(kf.clone()),
                &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA"), (VaultKind::Cal, "Kalender")],
            ),
            None,
            None,
            None,
        )
        .unwrap();
    phone_suite.drive_use_folder(drive.clone()).unwrap();
    let pass = vault_id(&phone_suite, "Privat");
    let totp = vault_id(&phone_suite, "2FA");
    let cal_vault = vault_id(&phone_suite, "Kalender");
    let calendar = phone_suite
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: cal_vault.clone(),
            name: "Familie".into(),
            color: 0xd6336c,
            default_reminder: 15,
            events: 0,
        })
        .unwrap();
    // One login both devices change now and then (the family Wi-Fi).
    let wifi = phone_suite.save_entry(pass.clone(), login("Familie WLAN", "router", "start")).unwrap();
    phone_suite.sync_folder(&sync).unwrap();

    let laptop_dirs = Dirs::new("everyday-laptop");
    copy_suite(&server_dir.join("sync"), &laptop_dirs);
    let laptop_suite = laptop_dirs.suite();
    laptop_suite.unlock(password.clone(), None, None, None, Some(kf.clone())).unwrap();
    laptop_suite.drive_use_folder(drive.clone()).unwrap();
    assert_eq!(all_titles(&laptop_suite), vec!["Familie WLAN"]);

    let mut devices = [
        Device { name: "Handy", dirs: phone_dirs, suite: phone_suite, mine: Expected::default(), counter: 0 },
        Device { name: "Laptop", dirs: laptop_dirs, suite: laptop_suite, mine: Expected::default(), counter: 0 },
    ];
    let mut backup: Option<(Vec<u8>, usize)> = None;
    let started = Instant::now();

    for day in 1..=30u32 {
        for d in devices.iter_mut() {
            let s = d.suite.clone();
            // Logins: new ones, changed passwords, now and then one deleted.
            for _ in 0..rng.below(4) {
                d.counter += 1;
                let title = format!("{}-{:03} {MARKER}", d.name, d.counter);
                let pw = format!("pw-{}-{}", day, rng.below(1_000_000));
                let id = s.save_entry(pass.clone(), login(&title, "ich@example.org", &pw)).unwrap();
                d.mine.logins.insert(title, (id, pw));
            }
            if rng.chance(60) && !d.mine.logins.is_empty() {
                let k = d.mine.logins.keys().nth(rng.below(d.mine.logins.len() as u64) as usize).unwrap().clone();
                let (id, _) = d.mine.logins[&k].clone();
                let mut e = s.get_entry(pass.clone(), id.clone()).unwrap();
                let pw = format!("neu-{}-{}", day, rng.below(1_000_000));
                set_pw(&mut e, &pw);
                s.save_entry(pass.clone(), e).unwrap();
                d.mine.logins.insert(k, (id, pw));
            }
            if rng.chance(15) && !d.mine.logins.is_empty() {
                let k = d.mine.logins.keys().nth(rng.below(d.mine.logins.len() as u64) as usize).unwrap().clone();
                let (id, _) = d.mine.logins.remove(&k).unwrap();
                s.delete_entry(pass.clone(), id).unwrap();
            }
            // Both change the shared Wi-Fi password now and then (conflicts on purpose).
            if rng.chance(20) {
                let mut e = s.get_entry(pass.clone(), wifi.clone()).unwrap();
                set_pw(&mut e, &format!("wlan-{}-{}", d.name, day));
                s.save_entry(pass.clone(), e).unwrap();
            }
            // 2FA codes.
            if rng.chance(30) {
                d.counter += 1;
                let issuer = format!("{}-Dienst-{}", d.name, d.counter);
                let mut input = totp_input();
                input.issuer = issuer.clone();
                input.account = format!("konto{}@example.org", d.counter);
                input.secret_base32 = rng.secret();
                d.mine.totps.insert(issuer, s.add_totp(totp.clone(), input).unwrap());
            }
            if rng.chance(8) && !d.mine.totps.is_empty() {
                let k = d.mine.totps.keys().next().unwrap().clone();
                let id = d.mine.totps.remove(&k).unwrap();
                s.delete_entry(totp.clone(), id).unwrap();
            }
            // Appointments in December and January.
            for _ in 0..rng.below(3) {
                d.counter += 1;
                let title = format!("{} Termin {}", d.name, d.counter);
                let start =
                    format!("2026-12-{:02}T{:02}:{:02}", 1 + rng.below(28), 7 + rng.below(12), 15 * rng.below(4));
                let end = format!("{}T23:00", &start[..10]);
                let id = s
                    .cal_save_event(CalEvent {
                        calendar_id: calendar.clone(),
                        title: title.clone(),
                        start: start.clone(),
                        end,
                        reminders: vec![15],
                        ..Default::default()
                    })
                    .unwrap();
                d.mine.events.insert(title, (id, start));
            }
            if rng.chance(15) && !d.mine.events.is_empty() {
                let k = d.mine.events.keys().next().unwrap().clone();
                let (id, start) = d.mine.events.remove(&k).unwrap();
                s.cal_delete_event(id, start[..10].to_owned(), CalDeleteMode::All).unwrap();
            }
            // Drive: photos and documents, renamed and deleted now and then.
            if rng.chance(35) {
                d.counter += 1;
                let name = format!("{}-{}.bin", d.name, d.counter);
                let max = if rng.chance(10) { 3_000_000 } else { 200_000 };
                let size = 1 + rng.below(max) as usize;
                let mut data = rng.bytes(size);
                data.extend_from_slice(MARKER.as_bytes());
                let id = upload(&s, None, &name, &data);
                d.mine.files.insert(name, (id, data));
            }
            if rng.chance(10) && !d.mine.files.is_empty() {
                let k = d.mine.files.keys().next().unwrap().clone();
                let (id, data) = d.mine.files.remove(&k).unwrap();
                let renamed = format!("umbenannt-{k}");
                s.drive_rename(id.clone(), renamed.clone()).unwrap();
                d.mine.files.insert(renamed, (id, data));
            }
            if rng.chance(8) && !d.mine.files.is_empty() {
                let k = d.mine.files.keys().last().unwrap().clone();
                let (id, _) = d.mine.files.remove(&k).unwrap();
                s.drive_delete(id).unwrap();
            }
            // Online now? Sometimes a device stays offline for days.
            if rng.chance(65) {
                s.sync_folder(&sync).unwrap();
            }
        }

        // Once a week the laptop is restarted; on day 10 the phone too.
        if day % 7 == 0 {
            devices[1].restart(&password, &kf, &drive);
        }
        if day == 10 {
            devices[0].restart(&password, &kf, &drive);
        }
        // Day 15: new master password on the phone; the laptop is asked for it once.
        if day == 15 {
            let new_pw = "Neues-Alltag-pw-15".to_string();
            devices[0].suite.sync_folder(&sync).unwrap();
            devices[1].suite.sync_folder(&sync).unwrap();
            devices[0].suite.change_password(password.clone(), new_pw.clone()).unwrap();
            devices[0].suite.sync_folder(&sync).unwrap();
            assert!(matches!(devices[1].suite.sync_folder(&sync), Err(GhulError::PasswordChangedElsewhere)));
            devices[1].suite.adopt_password_folder(&sync, &new_pw).unwrap();
            password = new_pw;
        }
        // Day 20: backup on the laptop.
        if day == 20 {
            let data = devices[1].suite.suite_backup().unwrap();
            let count = all_titles(&devices[1].suite).len();
            backup = Some((data, count));
        }
    }

    // Evening of day 30: both online, two rounds.
    for _ in 0..2 {
        for d in &devices {
            d.suite.sync_folder(&sync).unwrap();
        }
    }
    let (phone, laptop) = (&devices[0], &devices[1]);

    // Both devices show exactly the same.
    assert_eq!(all_titles(&phone.suite), all_titles(&laptop.suite));
    assert_eq!(totp_issuers(&phone.suite), totp_issuers(&laptop.suite));
    assert_eq!(event_titles(&phone.suite), event_titles(&laptop.suite));
    assert_eq!(drive_names(&phone.suite), drive_names(&laptop.suite));
    let wifi_a = pw_of(&phone.suite.get_entry(pass.clone(), wifi.clone()).unwrap());
    assert_eq!(wifi_a, pw_of(&laptop.suite.get_entry(pass.clone(), wifi.clone()).unwrap()));

    // And exactly what was done: nothing lost, nothing deleted that should stay.
    let mut want_titles: Vec<String> = devices.iter().flat_map(|d| d.mine.logins.keys().cloned()).collect();
    want_titles.push("Familie WLAN".into());
    want_titles.sort();
    let mut want_totps: Vec<String> = devices.iter().flat_map(|d| d.mine.totps.keys().cloned()).collect();
    want_totps.sort();
    let mut want_events: Vec<String> = devices.iter().flat_map(|d| d.mine.events.keys().cloned()).collect();
    want_events.sort();
    let mut want_files: Vec<String> = devices.iter().flat_map(|d| d.mine.files.keys().cloned()).collect();
    want_files.sort();
    for d in &devices {
        let s = &d.suite;
        assert_eq!(all_titles(s), want_titles, "{}", d.name);
        assert_eq!(totp_issuers(s), want_totps, "{}", d.name);
        assert_eq!(event_titles(s), want_events, "{}", d.name);
        assert_eq!(drive_names(s), want_files, "{}", d.name);
        for owner in &devices {
            for (title, (id, pw)) in &owner.mine.logins {
                assert_eq!(&pw_of(&s.get_entry(pass.clone(), id.clone()).unwrap()), pw, "{title} on {}", d.name);
            }
            for (name, (id, data)) in &owner.mine.files {
                assert_eq!(&download(s, id), data, "{name} on {}", d.name);
            }
        }
    }
    assert!(want_titles.len() > 40 && want_files.len() > 5 && want_events.len() > 20, "a real month: {want_titles:?}");

    // Nothing readable on the server.
    for f in files_under(&server_dir) {
        let raw = std::fs::read(&f).unwrap();
        assert!(!raw.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()), "{f:?} contains plaintext");
    }

    // The backup of day 20 restores into an empty folder with the password of that day.
    let (data, count) = backup.unwrap();
    let fresh = Dirs::new("everyday-restore");
    let restored = fresh.suite();
    assert!(restored.restore_backup(data).unwrap() > 0);
    restored.unlock("Neues-Alltag-pw-15".into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(all_titles(&restored).len(), count);

    eprintln!(
        "Alltagstest: 30 Tage, {} Anmeldungen, {} 2FA, {} Termine, {} Dateien in {:.1} s",
        want_titles.len(),
        want_totps.len(),
        want_events.len(),
        want_files.len(),
        started.elapsed().as_secs_f64()
    );
    let _ = std::fs::remove_dir_all(&server_dir);
}

/// Fails with a readable message when a step takes longer than `max_secs` (release build).
fn timed<T>(label: &str, max_secs: f64, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let secs = t.elapsed().as_secs_f64();
    eprintln!("Stresstest: {label:<46} {secs:>7.2} s");
    assert!(secs <= max_secs, "{label} took {secs:.2} s (limit {max_secs} s)");
    r
}

#[test]
#[ignore = "stress test: cargo test --release -p ghul-core -- --ignored stress"]
fn stress_big_suite_on_two_devices() {
    let server_dir = std::env::temp_dir().join(format!("ghul-stress-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let sync = LocalFs::new(server_dir.join("sync"));
    let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server_dir.join("drive")));
    let kf = generate_keyfile();
    let mut rng = Rng(42);
    let da = Dirs::new("stress-a");
    let a = da.suite();
    a.create(
        options(
            "Stress-pw-1",
            Some(kf.clone()),
            &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA"), (VaultKind::Cal, "Kalender")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    a.drive_use_folder(drive.clone()).unwrap();
    let pass = vault_id(&a, "Privat");
    let totp = vault_id(&a, "2FA");
    let cal_vault = vault_id(&a, "Kalender");

    // 5,000 logins from a password manager export, 1,000 of them with a 2FA key.
    let mut csv = String::from("\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\"\n");
    for i in 0..5_000 {
        // Different keys: the same key twice counts as a duplicate.
        let otp = if i % 5 == 0 { rng.secret() } else { String::new() };
        csv.push_str(&format!(
            "\"Root/Ordner{}\",\"Konto {i}\",\"user{i}@example.org\",\"pw-{}\",\"https://site{i}.example\",\"Notiz {i}\",\"{otp}\"\n",
            i % 40,
            rng.next()
        ));
    }
    let r = timed("Import 5.000 Anmeldungen + 1.000 2FA (CSV)", 60.0, || {
        a.import_data(csv.into_bytes(), "keepass.csv".into(), Some(pass.clone()), Some(totp.clone()), true, None)
            .unwrap()
    });
    assert_eq!((r.entries, r.totps), (5_000, 1_000));
    timed("Liste aller Anmeldungen", 2.0, || assert_eq!(titles(&a).len(), 5_000));
    timed("Alle 2FA-Codes berechnen", 2.0, || assert_eq!(a.totp_codes(1_800_000_000).unwrap().len(), 1_000));

    // Everyday edits in a big vault: each save writes the whole (encrypted) vault file.
    let ids: Vec<Vec<u8>> =
        a.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.id).take(100).collect();
    timed("100 Passwörter einzeln ändern", 60.0, || {
        for (i, id) in ids.iter().enumerate() {
            let mut e = a.get_entry(pass.clone(), id.clone()).unwrap();
            set_pw(&mut e, &format!("geändert-{i}"));
            a.save_entry(pass.clone(), e).unwrap();
        }
    });

    // 2,000 appointments from a calendar file (ICS).
    let calendar = a
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: cal_vault.clone(),
            name: "Arbeit".into(),
            color: 0x2f74e8,
            default_reminder: 10,
            events: 0,
        })
        .unwrap();
    let mut ics = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//GhulSuite//Stress//DE\r\n");
    for i in 0..2_000 {
        let day = 1 + i % 28;
        let month = 1 + (i / 28) % 12;
        ics.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:stress-{i}@example.org\r\nSUMMARY:Termin {i}\r\nDTSTART:2027{month:02}{day:02}T{:02}0000\r\nDTEND:2027{month:02}{day:02}T{:02}3000\r\nEND:VEVENT\r\n",
            8 + i % 9,
            8 + i % 9
        ));
    }
    ics.push_str("END:VCALENDAR\r\n");
    let n =
        timed("Import 2.000 Termine (ICS)", 30.0, || a.cal_import_ics(calendar.clone(), ics.into_bytes(), 60).unwrap());
    assert_eq!(n, 2_000);
    timed("Jahresansicht mit allen Terminen", 5.0, || {
        let occ = a.cal_occurrences("2027-01-01".into(), "2028-01-01".into(), "DE-NW".into(), "de".into()).unwrap();
        assert!(occ.len() >= 2_000);
    });

    // Drive: 300 small files and one of 64 MB.
    let small: Vec<(String, Vec<u8>)> =
        (0..300).map(|i| (format!("foto-{i:03}.jpg"), rng.bytes(5_000 + (i * 997) % 60_000))).collect();
    let small_ids = timed("300 kleine Dateien hochladen", 120.0, || {
        small.iter().map(|(n, d)| upload(&a, None, n, d)).collect::<Vec<String>>()
    });
    let big = rng.bytes(64 * 1024 * 1024);
    let big_id = timed("64-MB-Datei hochladen", 120.0, || upload(&a, None, "video.mp4", &big));
    timed("64-MB-Datei herunterladen", 60.0, || assert!(download(&a, &big_id) == big));

    // Everything to a second device.
    timed("Erste Synchronisation (hoch)", 60.0, || a.sync_folder(&sync).unwrap());
    let db = Dirs::new("stress-b");
    copy_suite(&server_dir.join("sync"), &db);
    let b = db.suite();
    timed("Zweites Gerät entsperren", 30.0, || {
        b.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap()
    });
    b.drive_use_folder(drive.clone()).unwrap();
    assert_eq!(titles(&b).len(), 5_000);
    assert_eq!(drive_names(&b).len(), 301);
    for (i, id) in small_ids.iter().enumerate().step_by(37) {
        assert_eq!(download(&b, id), small[i].1);
    }

    // Both change things at the same time, many sync rounds.
    timed("20 Runden ändern + synchronisieren auf 2 Geräten", 240.0, || {
        for round in 0..20 {
            a.save_entry(pass.clone(), login(&format!("A-Runde {round}"), "a", "x")).unwrap();
            b.save_entry(pass.clone(), login(&format!("B-Runde {round}"), "b", "y")).unwrap();
            a.sync_folder(&sync).unwrap();
            b.sync_folder(&sync).unwrap();
        }
        a.sync_folder(&sync).unwrap();
    });
    assert_eq!(titles(&a), titles(&b));
    assert_eq!(titles(&a).len(), 5_040);

    // Many app starts.
    timed("10 x sperren und entsperren", 120.0, || {
        for _ in 0..10 {
            a.lock();
            a.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap();
        }
    });
    timed("Sicherung (Backup) der großen Suite", 60.0, || assert!(!a.suite_backup().unwrap().is_empty()));
    let _ = std::fs::remove_dir_all(&server_dir);
}

#[test]
#[ignore = "stress test: cargo test --release -p ghul-core -- --ignored stress"]
fn stress_parallel_access_from_several_threads() {
    let server_dir = std::env::temp_dir().join(format!("ghul-stress-par-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server_dir);
    let sync = Arc::new(LocalFs::new(server_dir.join("sync")));
    let kf = generate_keyfile();
    let d = Dirs::new("stress-par");
    let s = d.suite();
    s.create(
        options("Par-pw-1", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
        None,
        None,
        None,
    )
    .unwrap();
    let pass = vault_id(&s, "Privat");
    let totp = vault_id(&s, "2FA");
    for i in 0..200 {
        s.save_entry(pass.clone(), login(&format!("Start {i}"), "u", "p")).unwrap();
    }
    s.add_totp(totp.clone(), totp_input()).unwrap();

    // Like the app: lists, codes, autofill and sync from several threads at once while two
    // threads keep writing. Nothing may hang, panic or get lost.
    timed("8 Threads gleichzeitig (Lesen, Schreiben, Sync)", 300.0, || {
        let mut handles = Vec::new();
        for t in 0..8 {
            let s = s.clone();
            let pass = pass.clone();
            let sync = sync.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..60 {
                    match t {
                        0 | 1 => {
                            s.save_entry(pass.clone(), login(&format!("T{t}-{i}"), "u", "p")).unwrap();
                        }
                        2 => {
                            s.sync_folder(sync.as_ref()).unwrap();
                        }
                        3 => {
                            let _ = s.autofill_logins(Some("example.org".into()), None, None).unwrap();
                        }
                        4 => {
                            assert_eq!(s.totp_codes(1_800_000_000).unwrap().len(), 1);
                        }
                        _ => {
                            assert!(titles(&s).len() >= 200);
                        }
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("a thread failed");
        }
    });
    s.sync_folder(sync.as_ref()).unwrap();
    assert_eq!(titles(&s).len(), 200 + 2 * 60);
    // The synced copy holds the same.
    let d2 = Dirs::new("stress-par-b");
    copy_suite(&server_dir.join("sync"), &d2);
    let b = d2.suite();
    b.unlock("Par-pw-1".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&b), titles(&s));
    let _ = std::fs::remove_dir_all(&server_dir);
}
