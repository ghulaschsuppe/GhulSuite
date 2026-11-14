// SPDX-License-Identifier: AGPL-3.0-or-later
//! The crazy user: thousands of random, often pointless or wrong, actions on three devices
//! that share one suite. Delete twice, edit what is gone, rename to nothing, move a folder into
//! itself, lock in between, type wrong passwords, restart the app, import rubbish, synchronise
//! with a wrong folder, ... Afterwards:
//! - nothing crashed, every error is one a user can understand (no "corrupted", no storage error);
//! - everything the user created and nobody deleted is still there, on every device;
//! - all devices show the same.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::cal::{CalCalendar, CalDeleteMode, CalEvent, CalRepeat};
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

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
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        (!v.is_empty()).then(|| &v[self.below(v.len())])
    }
}

/// Errors a normal app shows to the user. Anything else (damaged data, storage trouble,
/// internal states) means the app got confused by the user.
fn acceptable(e: &GhulError) -> bool {
    matches!(
        e,
        GhulError::NotFound
            | GhulError::InvalidInput { .. }
            | GhulError::AlreadyExists
            | GhulError::WrongPassword
            | GhulError::Locked
            | GhulError::NotConfigured
            | GhulError::PasswordChangedElsewhere
            | GhulError::ImportUnknownFormat
            | GhulError::NoVault
            | GhulError::LastFactor
            | GhulError::WrongKeyfile
            | GhulError::KeyfileRequired
            | GhulError::InvalidRecoveryCode
            | GhulError::ImportEncrypted
    )
}

const SILLY: &[&str] = &[
    "",
    " ",
    "  ",
    "\u{200b}",
    "a",
    "Test",
    "test",
    "TEST",
    "😀",
    "../..",
    "/",
    "x/y",
    "null",
    "undefined",
    "NaN",
    "-1",
    "0",
    "\u{202e}gpj.exe",
    "Ä Ö Ü ß",
    "a\nb",
    "\t",
    "'; DROP TABLE x; --",
    "<b>fett</b>",
    "%s%s%s",
    "{{7*7}}",
];

struct Dev {
    dirs: Dirs,
    s: Arc<GhulSuite>,
}

#[derive(Default)]
struct Stats {
    actions: usize,
    errors: BTreeMap<String, usize>,
}

impl Stats {
    fn note<T>(&mut self, what: &str, r: std::result::Result<T, GhulError>) -> Option<T> {
        self.actions += 1;
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                assert!(acceptable(&e), "{what}: unexpected error {e:?}");
                let kind = format!("{e:?}").split([' ', '{', '(']).next().unwrap_or("").to_owned();
                *self.errors.entry(kind).or_default() += 1;
                None
            }
        }
    }
}

fn run(seed: u64, steps: usize) -> Stats {
    let server = std::env::temp_dir().join(format!("ghul-crazy-{seed}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let sync = LocalFs::new(server.join("sync"));
    let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server.join("drive")));
    let kf = generate_keyfile();
    let pw = "Bekloppt-pw-1";
    let mut rng = Rng(seed);
    let mut st = Stats::default();

    let d0 = Dirs::new(&format!("crazy-{seed}-0"));
    let s0 = d0.suite();
    s0.create(
        options(
            pw,
            Some(kf.clone()),
            &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "2FA"), (VaultKind::Cal, "Kalender")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    s0.drive_use_folder(drive.clone()).unwrap();
    let pass = vault_id(&s0, "Privat");
    let totp = vault_id(&s0, "2FA");
    let cal = s0
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s0, "Kalender"),
            name: "Alles".into(),
            color: 1,
            default_reminder: 5,
            events: 0,
        })
        .unwrap();
    s0.sync_folder(&sync).unwrap();
    let mut devs = vec![Dev { dirs: d0, s: s0 }];
    for i in 1..3 {
        let d = Dirs::new(&format!("crazy-{seed}-{i}"));
        copy_suite(&server.join("sync"), &d);
        let s = d.suite();
        s.unlock(pw.into(), None, None, None, Some(kf.clone())).unwrap();
        s.drive_use_folder(drive.clone()).unwrap();
        devs.push(Dev { dirs: d, s });
    }

    // What the user created and never deleted (by anyone): must survive.
    let mut logins: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut deleted_logins: BTreeSet<String> = BTreeSet::new();
    // B18: a login deleted on one device but changed on another device that had not seen the
    // deletion comes back (the change is kept). Which devices edited / deleted each login:
    let mut edited_on: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut deleted_on: BTreeMap<String, usize> = BTreeMap::new();
    let mut counter = 0u32;
    let mut extra_vaults: Vec<String> = Vec::new();
    let mut events: Vec<(String, String)> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut folders: Vec<String> = Vec::new();

    for _ in 0..steps {
        let di = rng.below(devs.len());
        let s = devs[di].s.clone();
        match rng.below(30) {
            0..=4 => {
                counter += 1;
                let title = format!("Login {counter} {}", rng.pick(SILLY).unwrap().trim());
                if let Some(id) = st.note(
                    "add login",
                    s.save_entry(pass.clone(), login(&title, rng.pick(SILLY).unwrap(), rng.pick(SILLY).unwrap())),
                ) {
                    logins.insert(title, id);
                }
            }
            5 => {
                // Edit something, maybe deleted meanwhile, maybe on another device.
                if let Some((t, id)) = logins.iter().nth(rng.below(logins.len())).map(|(t, i)| (t.clone(), i.clone())) {
                    if let Some(mut e) = st.note("get", s.get_entry(pass.clone(), id.clone())) {
                        for f in e.fields.iter_mut().filter(|f| f.name == "password") {
                            f.value = FieldValue::Secret { value: format!("neu {}", rng.next()) };
                        }
                        if st.note("edit", s.save_entry(pass.clone(), e)).is_some() {
                            edited_on.entry(t.clone()).or_default().insert(di);
                        }
                    }
                }
            }
            6 => {
                // Delete, twice in a row sometimes.
                if let Some((t, id)) = logins.iter().nth(rng.below(logins.len())).map(|(t, i)| (t.clone(), i.clone())) {
                    // Not yet on this device (created elsewhere, not synchronised): "not found", nothing deleted.
                    if st.note("delete", s.delete_entry(pass.clone(), id.clone())).is_some() {
                        logins.remove(&t);
                        deleted_on.insert(t.clone(), di);
                        deleted_logins.insert(t);
                    }
                    if rng.chance(50) {
                        st.note("delete again", s.delete_entry(pass.clone(), id.clone()));
                    }
                }
            }
            7 => {
                // Ids that do not exist, of the wrong vault, empty.
                let junk = vec![rng.next() as u8; rng.below(20)];
                st.note("get junk", s.get_entry(pass.clone(), junk.clone()));
                st.note("delete junk", s.delete_entry(totp.clone(), junk));
                st.note("get from wrong vault", s.get_entry("kein-tresor".into(), vec![]));
            }
            8 => {
                let mut t = totp_input();
                t.issuer = rng.pick(SILLY).unwrap().to_string();
                if rng.chance(50) {
                    t.secret_base32 = rng.pick(SILLY).unwrap().to_string();
                }
                st.note("add 2fa", s.add_totp(totp.clone(), t));
            }
            9 => {
                if rng.chance(50) {
                    let name = format!("Spiel {}", rng.below(5));
                    if let Some(id) = st.note("new vault", s.create_vault(VaultKind::Pass, name)) {
                        extra_vaults.push(id);
                    }
                } else if let Some(id) = rng.pick(&extra_vaults).cloned() {
                    st.note("remove vault", s.remove_vault(id.clone()));
                    extra_vaults.retain(|v| v != &id);
                }
            }
            10 => {
                let target =
                    if rng.chance(70) { pass.clone() } else { rng.pick(&extra_vaults).cloned().unwrap_or_default() };
                st.note("rename vault", s.rename_vault(target, rng.pick(SILLY).unwrap().to_string()));
            }
            11 | 12 => {
                let starts = ["2026-12-24T18:00", "2026-02-30T10:00", "", "2026-12-24", "x", "2027-01-01T00:00"];
                let start = rng.pick(&starts).unwrap().to_string();
                counter += 1;
                let ev = CalEvent {
                    calendar_id: if rng.chance(90) { cal.clone() } else { "fremd".into() },
                    title: format!("Termin {counter} {}", rng.pick(SILLY).unwrap()),
                    all_day: start.len() == 10,
                    end: start.clone(),
                    start: start.clone(),
                    repeat: [CalRepeat::None, CalRepeat::Weekly, CalRepeat::Yearly][rng.below(3)],
                    ..Default::default()
                };
                if let Some(id) = st.note("add event", s.cal_save_event(ev)) {
                    events.push((id, start));
                }
            }
            13 => {
                if let Some((id, start)) = rng.pick(&events).cloned() {
                    let day = start.get(..10).unwrap_or("2026-01-01").to_owned();
                    let mode = [CalDeleteMode::One, CalDeleteMode::Following, CalDeleteMode::All][rng.below(3)];
                    st.note("delete event", s.cal_delete_event(id.clone(), day.clone(), mode));
                    st.note("delete event again", s.cal_delete_event(id, day, CalDeleteMode::All));
                }
            }
            14 | 15 => {
                let name = format!("{}{}.txt", rng.pick(SILLY).unwrap(), rng.below(3));
                let folder = if rng.chance(30) { rng.pick(&folders).cloned() } else { None };
                if let Some(up) = st.note("upload begin", s.drive_upload_begin(folder, name)) {
                    st.note("upload chunk", s.drive_upload_chunk(up.clone(), vec![rng.next() as u8; rng.below(5000)]));
                    if rng.chance(90) {
                        if let Some(id) = st.note("upload finish", s.drive_upload_finish(up)) {
                            files.push(id);
                        }
                    }
                }
            }
            16 => {
                if let Some(id) = st.note(
                    "folder",
                    s.drive_create_folder(rng.pick(&folders).cloned(), rng.pick(SILLY).unwrap().to_string()),
                ) {
                    folders.push(id);
                }
            }
            17 => {
                let all: Vec<String> = files.iter().chain(&folders).cloned().collect();
                if let (Some(a), Some(b)) = (rng.pick(&all).cloned(), rng.pick(&folders).cloned()) {
                    st.note("move", s.drive_move(a.clone(), Some(b)));
                    st.note("move to root", s.drive_move(a.clone(), None));
                    st.note("rename", s.drive_rename(a, rng.pick(SILLY).unwrap().to_string()));
                }
            }
            18 => {
                if let Some(id) = rng.pick(&files).cloned() {
                    st.note("trash", s.drive_delete(id.clone()));
                    st.note("trash again", s.drive_delete(id.clone()));
                    if rng.chance(50) {
                        st.note("restore", s.drive_restore(id));
                    }
                }
                if rng.chance(10) {
                    st.note("empty trash", s.drive_empty_trash(0));
                }
            }
            19 => {
                // Lock, then call things while locked, then unlock (wrong password first).
                s.lock();
                assert!(matches!(s.save_entry(pass.clone(), login("x", "y", "z")), Err(GhulError::Locked)));
                let _ = st.note("list while locked", s.list_entries(VaultKind::Pass, None));
                let _ = s.unlock("falsch".into(), None, None, None, Some(kf.clone()));
                s.unlock(pw.into(), None, None, None, Some(kf.clone())).unwrap();
                // (Tests reach Drive through a folder set up by hand; the app reconnects by itself.)
                s.drive_use_folder(drive.clone()).unwrap();
            }
            20 => {
                // The app is killed and started again.
                s.lock();
                let fresh = devs[di].dirs.suite();
                fresh.unlock(pw.into(), None, None, None, Some(kf.clone())).unwrap();
                fresh.drive_use_folder(drive.clone()).unwrap();
                devs[di].s = fresh;
            }
            21..=24 => {
                st.note("sync", s.sync_folder(&sync));
            }
            25 => {
                // Typo in the server folder: an empty one. Must not delete anything.
                // (Deleting there what was deleted on purpose is fine; losing data is not.)
                // A fresh, empty folder each time (a folder used before is a real second server of
                // this suite and may rightly bring other devices' changes).
                let before = titles(&s);
                counter += 1;
                let wrong = LocalFs::new(server.join(format!("vertippt-{counter}")));
                if st.note("sync wrong folder", s.sync_folder(&wrong)).is_some() {
                    assert_eq!(titles(&s), before, "a wrong folder changed the logins");
                }
            }
            26 => {
                let junk: Vec<u8> = (0..rng.below(3000)).map(|_| rng.next() as u8).collect();
                let name = ["x.csv", "x.json", "x.zip", "x.txt", "", "export.1pux"][rng.below(6)];
                st.note(
                    "import rubbish",
                    s.import_data(junk, name.into(), Some(pass.clone()), Some(totp.clone()), true, None),
                );
            }
            27 => {
                st.note("export wrong password", s.export_plain(pass.clone(), "falsch".into()));
                st.note(
                    "confirm code garbage",
                    s.confirm_recovery_code(rng.next() as u32, rng.pick(SILLY).unwrap().to_string()),
                );
                st.note("setup state", s.setup_state());
            }
            28 => {
                st.note("search", s.drive_search(rng.pick(SILLY).unwrap().to_string()));
                st.note("autofill", s.autofill_logins(Some(rng.pick(SILLY).unwrap().to_string()), None, None));
                st.note(
                    "occurrences",
                    s.cal_occurrences("2026-01-01".into(), "2028-01-01".into(), "DE-BY".into(), "de".into()),
                );
            }
            _ => {
                st.note("backup", s.suite_backup());
            }
        }
    }

    // Evening: everyone online and synchronised twice.
    for _ in 0..2 {
        for d in &devs {
            d.s.sync_folder(&sync).unwrap();
        }
    }
    let first = titles(&devs[0].s);
    for (i, d) in devs.iter().enumerate() {
        assert_eq!(titles(&d.s), first, "device {i} differs");
        let codes: usize = d.s.totp_codes(1_800_000_000).unwrap().len();
        assert_eq!(codes, devs[0].s.totp_codes(1_800_000_000).unwrap().len(), "2FA on device {i}");
        for t in logins.keys() {
            assert!(first.contains(t), "{t} was lost (seed {seed})");
        }
        for t in &deleted_logins {
            // Only a change made on another device may bring it back (B18), never nothing.
            let edited_elsewhere = edited_on.get(t).is_some_and(|devs| devs.iter().any(|d| Some(d) != deleted_on.get(t)));
            assert!(!first.contains(t) || edited_elsewhere, "{t} came back after being deleted (seed {seed})");
        }
    }
    let _ = std::fs::remove_dir_all(&server);
    st
}

#[test]
fn a_crazy_user_on_three_devices() {
    let mut total = Stats::default();
    for seed in [1u64, 0xDEAD_BEEF, 0x1234_5678_9ABC] {
        let st = run(seed, 1000);
        total.actions += st.actions;
        for (k, v) in st.errors {
            *total.errors.entry(k).or_default() += v;
        }
    }
    eprintln!("Bekloppter Nutzer: {} Aktionen, abgelehnt (verständlich): {:?}", total.actions, total.errors);
    assert!(total.actions > 3000);
}

#[test]
#[ignore = "long: cargo test --release -p ghul-core -- --ignored crazy"]
fn crazy_user_long_run() {
    let mut actions = 0;
    for seed in 1..=20u64 {
        actions += run(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 5000).actions;
    }
    eprintln!("Bekloppter Nutzer (lang): {actions} Aktionen");
}
