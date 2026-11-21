// SPDX-License-Identifier: AGPL-3.0-or-later
//! Everyday test and stress tests: the suite used like in real life.
//!
//! - `a_month_of_everyday_use_on_two_devices` (runs with every `cargo test`): a phone and a
//!   laptop use one suite for 30 simulated days, the suite moving between them like on a USB
//!   stick. Every day the device that has it adds, changes and deletes logins, 2FA codes and
//!   appointments, restarts the app, changes the password and makes a backup. At the end both
//!   devices must show exactly the same, nothing may be lost, and no file the app wrote may
//!   contain plaintext.
//! - The stress tests are `#[ignore]`d and run in release mode in CI
//!   (`cargo test --release -p ghul-core -- --ignored stress`): thousands of entries, codes and
//!   appointments, many app starts, and parallel access from several threads. They print their timings and fail when something becomes unreasonably slow.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::super::cal::{CalCalendar, CalDeleteMode, CalEvent};
use super::*;

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
}

/// A word that must never show up in plaintext in a file.
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

/// Everything the suite is expected to hold.
#[derive(Default)]
struct Expected {
    /// title -> (id, password)
    logins: BTreeMap<String, (Vec<u8>, String)>,
    /// issuer -> id
    totps: BTreeMap<String, Vec<u8>>,
    /// title -> (id, start)
    events: BTreeMap<String, (String, String)>,
}

struct Device {
    name: &'static str,
    dirs: Dirs,
    suite: Arc<GhulSuite>,
    counter: u32,
}

impl Device {
    /// The app is closed and started again: a new instance on the same folders, unlocked.
    fn restart(&mut self, password: &str, keyfile: &[u8]) {
        self.suite.lock();
        self.suite = self.dirs.suite();
        self.suite.unlock(password.into(), None, None, None, Some(keyfile.to_vec())).unwrap();
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

/// The suite moves to the other device (like on a USB stick): saved into a file there, opened
/// here; the app here starts again afterwards.
fn hand_over(from: &Device, to: &mut Device, password: &str, keyfile: &[u8]) {
    let built = from.suite.archive_build_bytes(from.name.into()).unwrap();
    from.suite.archive_saved(built.state, built.change).unwrap();
    let check =
        to.suite.archive_open_bytes(built.data, password.into(), None, None, None, Some(keyfile.to_vec())).unwrap();
    assert!(!check.unsaved_changes, "{} worked on an old copy", to.name);
    to.suite.archive_accept().unwrap();
    to.restart(password, keyfile);
}

/// Makes `to`'s suite folder an exact copy of `from`'s.
pub(super) fn copy_folder(from: &Dirs, to: &Dirs) {
    let (a, b) = (from.storage(), to.storage());
    for f in b.list().unwrap() {
        b.delete(f).unwrap();
    }
    for f in a.list().unwrap() {
        b.write(f.clone(), a.read(f).unwrap().unwrap()).unwrap();
    }
}

#[test]
fn a_month_of_everyday_use_on_two_devices() {
    let kf = generate_keyfile();
    let mut password = "Alltag-pw-2026".to_string();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

    // Day 0: the phone sets everything up, the laptop gets a copy.
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

    let laptop_dirs = Dirs::new("everyday-laptop");
    let mut devices = [
        Device { name: "Handy", dirs: phone_dirs, suite: phone_suite, counter: 0 },
        Device { name: "Laptop", suite: laptop_dirs.suite(), dirs: laptop_dirs, counter: 0 },
    ];
    {
        let [phone, laptop] = &mut devices;
        hand_over(phone, laptop, &password, &kf);
    }
    assert_eq!(all_titles(&devices[1].suite), vec!["Familie WLAN"]);
    // What the suite holds now (one state, no merging: whoever has it works on it).
    let mut exp = Expected::default();
    let mut holder = 0usize;
    let mut backup: Option<(Vec<u8>, usize)> = None;
    let started = Instant::now();

    for day in 1..=30u32 {
        // Some days the suite moves to the other device.
        if rng.chance(40) {
            let to = 1 - holder;
            let [a, b] = &mut devices;
            if to == 1 {
                hand_over(a, b, &password, &kf);
            } else {
                hand_over(b, a, &password, &kf);
            }
            holder = to;
        }
        let d = &mut devices[holder];
        let s = d.suite.clone();
        // Logins: new ones, changed passwords, now and then one deleted.
        for _ in 0..rng.below(4) {
            d.counter += 1;
            let title = format!("{}-{:03} {MARKER}", d.name, d.counter);
            let pw = format!("pw-{}-{}", day, rng.below(1_000_000));
            let id = s.save_entry(pass.clone(), login(&title, "ich@example.org", &pw)).unwrap();
            exp.logins.insert(title, (id, pw));
        }
        if rng.chance(60) && !exp.logins.is_empty() {
            let k = exp.logins.keys().nth(rng.below(exp.logins.len() as u64) as usize).unwrap().clone();
            let (id, _) = exp.logins[&k].clone();
            let mut e = s.get_entry(pass.clone(), id.clone()).unwrap();
            let pw = format!("neu-{}-{}", day, rng.below(1_000_000));
            set_pw(&mut e, &pw);
            s.save_entry(pass.clone(), e).unwrap();
            exp.logins.insert(k, (id, pw));
        }
        if rng.chance(15) && !exp.logins.is_empty() {
            let k = exp.logins.keys().nth(rng.below(exp.logins.len() as u64) as usize).unwrap().clone();
            let (id, _) = exp.logins.remove(&k).unwrap();
            s.delete_entry(pass.clone(), id).unwrap();
        }
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
            exp.totps.insert(issuer, s.add_totp(totp.clone(), input).unwrap());
        }
        if rng.chance(8) && !exp.totps.is_empty() {
            let k = exp.totps.keys().next().unwrap().clone();
            let id = exp.totps.remove(&k).unwrap();
            s.delete_entry(totp.clone(), id).unwrap();
        }
        // Appointments in December and January.
        for _ in 0..rng.below(3) {
            d.counter += 1;
            let title = format!("{} Termin {}", d.name, d.counter);
            let start = format!("2026-12-{:02}T{:02}:{:02}", 1 + rng.below(28), 7 + rng.below(12), 15 * rng.below(4));
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
            exp.events.insert(title, (id, start));
        }
        if rng.chance(15) && !exp.events.is_empty() {
            let k = exp.events.keys().next().unwrap().clone();
            let (id, start) = exp.events.remove(&k).unwrap();
            s.cal_delete_event(id, start[..10].to_owned(), CalDeleteMode::All).unwrap();
        }
        // Once a week the laptop is restarted; on day 10 the phone too.
        if day % 7 == 0 && holder == 1 {
            devices[1].restart(&password, &kf);
        }
        if day == 10 && holder == 0 {
            devices[0].restart(&password, &kf);
        }
        // Day 15: new master password; the other device gets it with the next copy.
        if day == 15 {
            let new_pw = "Neues-Alltag-pw-15".to_string();
            devices[holder].suite.change_password(super::kf_proof(&password, &kf), None, None, new_pw.clone()).unwrap();
            password = new_pw;
        }
        // Day 20: backup.
        if day == 20 {
            let built = devices[holder].suite.archive_build_bytes("Laptop".into()).unwrap();
            devices[holder].suite.archive_saved(built.state, built.change).unwrap();
            let data = built.data;
            let count = all_titles(&devices[holder].suite).len();
            backup = Some((data, count));
        }
    }

    // Evening of day 30: the suite goes to the other device once more.
    {
        let to = 1 - holder;
        let [a, b] = &mut devices;
        if to == 1 {
            hand_over(a, b, &password, &kf);
        } else {
            hand_over(b, a, &password, &kf);
        }
    }

    // Both devices show exactly what was done: nothing lost, nothing deleted that should stay.
    let mut want_titles: Vec<String> = exp.logins.keys().cloned().collect();
    want_titles.push("Familie WLAN".into());
    want_titles.sort();
    let want_totps: Vec<String> = exp.totps.keys().cloned().collect();
    let want_events: Vec<String> = exp.events.keys().cloned().collect();
    let wifi_a = pw_of(&devices[0].suite.get_entry(pass.clone(), wifi.clone()).unwrap());
    for d in &devices {
        let s = &d.suite;
        assert_eq!(all_titles(s), want_titles, "{}", d.name);
        assert_eq!(totp_issuers(s), want_totps, "{}", d.name);
        assert_eq!(event_titles(s), want_events, "{}", d.name);
        assert_eq!(pw_of(&s.get_entry(pass.clone(), wifi.clone()).unwrap()), wifi_a);
        for (title, (id, pw)) in &exp.logins {
            assert_eq!(&pw_of(&s.get_entry(pass.clone(), id.clone()).unwrap()), pw, "{title} on {}", d.name);
        }
    }
    assert!(
        want_titles.len() > 20 && want_events.len() > 10,
        "a real month: {} logins, {} events",
        want_titles.len(),
        want_events.len()
    );
    assert!(devices.iter().all(|d| d.counter > 0), "both devices worked on the suite");

    // Nothing readable in the files of either device (working copy and device data).
    for f in devices.iter().flat_map(|d| files_under(&d.dirs.root)) {
        let raw = std::fs::read(&f).unwrap();
        assert!(!raw.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()), "{f:?} contains plaintext");
    }

    // The suite file of day 20 opens on a new device with the password of that day.
    let (data, count) = backup.unwrap();
    let fresh = Dirs::new("everyday-restore");
    let restored = fresh.suite();
    restored.archive_open_bytes(data, "Neues-Alltag-pw-15".into(), None, None, None, Some(kf.clone())).unwrap();
    restored.archive_accept().unwrap();
    assert_eq!(all_titles(&restored).len(), count);

    eprintln!(
        "Alltagstest: 30 Tage, {} Anmeldungen, {} 2FA, {} Termine in {:.1} s",
        want_titles.len(),
        want_totps.len(),
        want_events.len(),
        started.elapsed().as_secs_f64()
    );
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
    let n = timed("Import 2.000 Termine (ICS)", 30.0, || {
        a.cal_import_ics(calendar.clone(), ics.into_bytes(), Arc::new(super::super::caltext::FixedOffset(60))).unwrap()
    });
    assert_eq!(n, 2_000);
    timed("Jahresansicht mit allen Terminen", 5.0, || {
        let occ = a.cal_occurrences("2027-01-01".into(), "2028-01-01".into(), "DE-NW".into(), "de".into()).unwrap();
        assert!(occ.len() >= 2_000);
    });

    // Everything to a second device.
    let db = Dirs::new("stress-b");
    timed("Suite auf das zweite Gerät kopieren", 60.0, || copy_folder(&da, &db));
    let b = db.suite();
    timed("Zweites Gerät entsperren", 30.0, || {
        b.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap()
    });
    assert_eq!(titles(&b).len(), 5_000);

    // The suite goes back and forth, each device changes something in between.
    timed("20 Runden ändern + Suite weitergeben auf 2 Geräten", 240.0, || {
        for round in 0..20 {
            a.save_entry(pass.clone(), login(&format!("A-Runde {round}"), "a", "x")).unwrap();
            a.lock();
            copy_folder(&da, &db);
            b.lock();
            b.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap();
            b.save_entry(pass.clone(), login(&format!("B-Runde {round}"), "b", "y")).unwrap();
            b.lock();
            copy_folder(&db, &da);
            a.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap();
        }
        b.unlock("Stress-pw-1".into(), None, None, None, Some(kf.clone())).unwrap();
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
    timed("Suite-Datei der großen Suite speichern", 60.0, || {
        assert!(!a.archive_build_bytes("A".into()).unwrap().data.is_empty())
    });
}

#[test]
#[ignore = "stress test: cargo test --release -p ghul-core -- --ignored stress"]
fn stress_parallel_access_from_several_threads() {
    let kf = generate_keyfile();
    let d = Dirs::new("stress-par");
    let s = d.suite();
    s.create(
        options("Par-pw-1-Kq7wZ", Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA")]),
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

    // Like the app: lists, codes and locking from several threads at once while two
    // threads keep writing. Nothing may hang, panic or get lost.
    timed("8 Threads gleichzeitig (Lesen, Schreiben, Sichern)", 300.0, || {
        let mut handles = Vec::new();
        for t in 0..8 {
            let s = s.clone();
            let pass = pass.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..60 {
                    match t {
                        0 | 1 => {
                            s.save_entry(pass.clone(), login(&format!("T{t}-{i}"), "u", "p")).unwrap();
                        }
                        2 => {
                            assert!(!s.archive_build_bytes("T".into()).unwrap().data.is_empty());
                        }
                        3 => {
                            let _ = s.list_entries(VaultKind::Pass, None).unwrap();
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
    assert_eq!(titles(&s).len(), 200 + 2 * 60);
    // A copy of the folder holds the same.
    let d2 = Dirs::new("stress-par-b");
    copy_folder(&d, &d2);
    let b = d2.suite();
    b.unlock("Par-pw-1-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(titles(&b), titles(&s));
}
