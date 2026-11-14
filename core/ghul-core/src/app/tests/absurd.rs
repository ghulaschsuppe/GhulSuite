// SPDX-License-Identifier: AGPL-3.0-or-later
//! Absurd input everywhere: the Big List of Naughty Strings in every text field, huge and empty
//! values, impossible dates, odd file names, header tricks, odd passwords. Allowed results: it
//! works and comes back exactly as entered, or a clear error. Never a crash, never a mangled value.

use std::sync::Arc;

use super::super::cal::{CalCalendar, CalDeleteMode, CalEvent, CalRepeat};
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

fn naughty() -> Vec<String> {
    let path = format!("{}/tests/data/blns.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn field<'a>(e: &'a EntryData, name: &str) -> Option<&'a FieldValue> {
    e.fields.iter().find(|f| f.name == name).map(|f| &f.value)
}

fn note(title: &str, text: &str) -> EntryData {
    EntryData {
        id: vec![],
        entry_type: "note".into(),
        fields: vec![
            FieldData { name: "title".into(), value: FieldValue::Text { value: title.into() } },
            FieldData { name: "notes".into(), value: FieldValue::Text { value: text.into() } },
        ],
        tags: vec![],
        folder: None,
        modified: 0,
        revision: 0,
    }
}

#[test]
fn naughty_strings_come_back_exactly_everywhere() {
    let d = Dirs::new("naughty");
    let s = d.suite();
    s.create(
        options(
            "pw-123456",
            Some(generate_keyfile()),
            &[(VaultKind::Pass, "P"), (VaultKind::Totp, "T"), (VaultKind::Cal, "K")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    let server = std::env::temp_dir().join(format!("ghul-naughty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server.join("drive")));
    s.drive_use_folder(drive.clone()).unwrap();
    let (pass, totp, calv) = (vault_id(&s, "P"), vault_id(&s, "T"), vault_id(&s, "K"));
    let cal = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: calv,
            name: "C".into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap();
    let mut saved = Vec::new();
    for (i, n) in naughty().iter().enumerate() {
        // Logins: every field, exactly back.
        let mut e = login(n, n, n);
        e.fields.push(FieldData { name: "notes".into(), value: FieldValue::Text { value: n.clone() } });
        match s.save_entry(pass.clone(), e) {
            Ok(id) => saved.push((id, n.clone())),
            Err(err) => assert!(n.trim().is_empty(), "#{i} {n:?} refused: {err:?}"),
        }
        // 2FA account names.
        let mut t = totp_input();
        t.issuer = n.clone();
        t.account = format!("{i}{n}");
        t.secret_base32 = format!("GEZDGNBVGY3TQOJQGEZDGNBV{:08}", i)
            .replace('0', "A")
            .replace('1', "B")
            .replace('8', "C")
            .replace('9', "D");
        let _ = s.add_totp(totp.clone(), t);
        // Appointments.
        let ev = s.cal_save_event(CalEvent {
            calendar_id: cal.clone(),
            title: n.clone(),
            location: n.clone(),
            notes: n.clone(),
            start: "2026-12-01T10:00".into(),
            end: "2026-12-01T11:00".into(),
            ..Default::default()
        });
        if let Ok(id) = ev {
            assert_eq!(s.cal_get_event(id).unwrap().title, super::super::drive::trim_invisible(n), "#{i}");
        }
        // Drive: folder and file names (refusing odd names is fine, mangling them is not).
        // Drive trims spaces around names and refuses invisible or disguising ones.
        if let Ok(id) = s.drive_create_folder(None, n.clone()) {
            let item = s.drive_list(None, false).unwrap().into_iter().find(|x| x.id == id).unwrap();
            // Same name twice in one folder: the second gets " (2)", like in every file manager.
            let want = super::super::drive::trim_invisible(n);
            assert!(
                item.name == want || (item.name.starts_with(want) && item.name.ends_with(')')),
                "#{i}: {:?}",
                item.name
            );
            assert!(
                item.name.chars().any(|c| !c.is_whitespace() && !matches!(c, '\u{200b}'..='\u{200d}' | '\u{feff}')),
                "#{i} invisible"
            );
        }
        if i % 7 == 0 {
            if let Ok(up) = s.drive_upload_begin(None, format!("{n}.txt")) {
                s.drive_upload_chunk(up.clone(), n.as_bytes().to_vec()).unwrap();
                let id = s.drive_upload_finish(up).unwrap();
                assert_eq!(download(&s, &id), n.as_bytes(), "#{i}");
            }
        }
        // Vault names and search.
        let _ = s.rename_vault(pass.clone(), n.clone());
        let _ = s.drive_search(n.clone());
    }
    assert!(saved.len() > 400, "{}", saved.len());
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn values_round_trip_after_reopen() {
    let d = Dirs::new("roundtrip");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let pass = vault_id(&s, "P");
    let mut saved = Vec::new();
    for n in naughty() {
        if let Ok(id) = s.save_entry(pass.clone(), login(&n, &n, &n)) {
            saved.push((id, n));
        }
    }
    // Huge values.
    let big = "Ä".repeat(500_000);
    let big_id = s.save_entry(pass.clone(), note("riesig", &big)).unwrap();
    let mut many = note("viele Felder", "x");
    for k in 0..2000 {
        many.fields.push(FieldData { name: format!("feld{k}"), value: FieldValue::Text { value: format!("{k}") } });
    }
    let many_id = s.save_entry(pass.clone(), many).unwrap();
    s.lock();
    let s = d.suite();
    s.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    for (id, n) in &saved {
        let e = s.get_entry(pass.clone(), id.clone()).unwrap();
        // An empty field is simply not stored; everything else comes back unchanged.
        let text = (!n.is_empty()).then(|| FieldValue::Text { value: n.clone() });
        let secret = (!n.is_empty()).then(|| FieldValue::Secret { value: n.clone() });
        assert_eq!(field(&e, "title"), text.as_ref(), "{n:?}");
        assert_eq!(field(&e, "username"), text.as_ref(), "{n:?}");
        assert_eq!(field(&e, "password"), secret.as_ref(), "{n:?}");
    }
    assert_eq!(field(&s.get_entry(pass.clone(), big_id).unwrap(), "notes"), Some(&FieldValue::Text { value: big }));
    assert_eq!(s.get_entry(pass.clone(), many_id).unwrap().fields.len(), 2002);
}

#[test]
fn odd_master_passwords() {
    for pw in [
        "Müller-Pässwort",
        "🔐🔑 sicher 🔐🔑",
        "  Leerzeichen außen  ",
        "\u{202e}rechts-nach-links",
        &"x".repeat(10_000),
    ] {
        let d = Dirs::new("pw");
        let kf = generate_keyfile();
        let s = d.suite();
        s.create(options(pw, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
        s.lock();
        // Wrong passwords many times do not break anything.
        for wrong in ["", " ", pw.trim(), &format!("{pw} "), "x"] {
            if wrong != pw {
                assert!(s.unlock(wrong.into(), None, None, None, Some(kf.clone())).is_err(), "{wrong:?} for {pw:?}");
            }
        }
        s.unlock(pw.into(), None, None, None, Some(kf.clone())).unwrap();
    }
    // The same password typed in another Unicode form (é as one or two characters) works.
    let d = Dirs::new("nfd");
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(options("Caf\u{e9}-Passwort", Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    s.lock();
    s.unlock("Cafe\u{301}-Passwort".into(), None, None, None, Some(kf)).unwrap();
    // Too short or empty is refused clearly.
    let d = Dirs::new("short");
    assert!(d
        .suite()
        .create(options("", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None)
        .is_err());
}

#[test]
fn impossible_dates_and_series() {
    let d = Dirs::new("dates");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Cal, "K")]), None, None, None).unwrap();
    let calv = vault_id(&s, "K");
    let cal = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: calv,
            name: "C".into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap();
    let odd = [
        ("2026-02-30T10:00", "2026-02-30T11:00", false),
        ("2026-11-13T24:00", "2026-11-13T25:00", false),
        ("2026-11-13T12:00", "2026-11-13T08:00", false),
        ("2026-11-20", "2026-11-13", true),
        ("0001-01-01T00:00", "0001-01-01T01:00", false),
        ("9999-12-31T23:00", "9999-12-31T23:59", false),
        ("2028-02-29", "2028-02-29", true),
        ("2026-03-29T02:30", "2026-03-29T03:30", false),
        ("", "", false),
        ("morgen", "übermorgen", false),
        ("2026-1-5T9:5", "2026-1-5T10:5", false),
    ];
    for (start, end, all_day) in odd {
        for repeat in [CalRepeat::None, CalRepeat::Daily, CalRepeat::Weekly, CalRepeat::Monthly, CalRepeat::Yearly] {
            let r = s.cal_save_event(CalEvent {
                calendar_id: cal.clone(),
                title: format!("{start} {repeat:?}"),
                start: start.into(),
                end: end.into(),
                all_day,
                repeat,
                repeat_until: "2020-01-01".into(),
                exceptions: vec!["2026-02-30".into(), "x".into()],
                reminders: vec![0, 1, 525_600, u32::MAX],
                ..Default::default()
            });
            if let Ok(id) = r {
                let _ = s.cal_get_event(id.clone()).unwrap();
                let _ = s.cal_delete_event(id, start.get(..10).unwrap_or("").into(), CalDeleteMode::One);
            }
        }
    }
    // Any range, also backwards and over centuries, answers quickly.
    let t = std::time::Instant::now();
    for (from, to) in
        [("1900-01-01", "2100-01-01"), ("2030-01-01", "2020-01-01"), ("x", "y"), ("", ""), ("2026-02-30", "2026-03-31")]
    {
        let _ = s.cal_occurrences(from.into(), to.into(), "DE-NW".into(), "de".into());
        let _ = s.cal_reminders(format!("{from}T00:00"), 365);
    }
    assert!(t.elapsed().as_secs() < 20, "calendar too slow: {:?}", t.elapsed());
    // Yearly on 29 February shows up in leap years only (or on the 28th), never crashes.
    let _ = s
        .cal_save_event(CalEvent {
            calendar_id: cal.clone(),
            title: "Schalttag".into(),
            start: "2024-02-29".into(),
            end: "2024-02-29".into(),
            all_day: true,
            repeat: CalRepeat::Yearly,
            ..Default::default()
        })
        .unwrap();
    let occ = s.cal_occurrences("2024-01-01".into(), "2033-01-01".into(), String::new(), "de".into()).unwrap();
    assert!(occ.iter().filter(|o| o.title == "Schalttag").count() >= 3);
}

#[test]
fn odd_drive_names_and_moves() {
    let d = Dirs::new("drivenames");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let server = std::env::temp_dir().join(format!("ghul-drivenames-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let fs: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(&server));
    s.drive_use_folder(fs).unwrap();
    for name in ["..", ".", "/", "a/b", "\\", "CON", "", " ", "\u{0}", &"n".repeat(300), "Ordner", "Ordner", "ordner"] {
        let _ = s.drive_create_folder(None, name.into());
    }
    let a = s.drive_create_folder(None, "A".into()).unwrap();
    let b = s.drive_create_folder(Some(a.clone()), "B".into()).unwrap();
    // A folder cannot go into itself or into its own child.
    assert!(s.drive_move(a.clone(), Some(a.clone())).is_err());
    assert!(s.drive_move(a.clone(), Some(b.clone())).is_err());
    // Empty file, same name twice, rename onto an existing name.
    let empty = upload(&s, None, "leer.txt", b"");
    assert_eq!(download(&s, &empty), b"");
    let x = upload(&s, None, "x.txt", b"1");
    let y = upload(&s, None, "y.txt", b"2");
    let _ = s.drive_rename(y.clone(), "x.txt".into());
    assert_eq!(download(&s, &x), b"1", "renaming onto a name must not overwrite another file");
    assert_eq!(download(&s, &y), b"2");
    // Ids that do not exist.
    assert!(s.drive_rename("nope".into(), "z".into()).is_err());
    assert!(s.drive_move("nope".into(), None).is_err());
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn locked_suite_answers_every_call_with_an_error() {
    let d = Dirs::new("lockedcalls");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let pass = vault_id(&s, "P");
    s.lock();
    assert!(s.list_vaults().is_err());
    assert!(s.save_entry(pass.clone(), login("x", "y", "z")).is_err());
    assert!(s.list_entries(VaultKind::Pass, None).is_err());
    assert!(s.totp_codes(0).is_err());
    assert!(s.drive_list(None, false).is_err());
    assert!(s.cal_calendars().is_err());
    assert!(s.suite_backup().is_err());
    assert!(s.autofill_logins(Some("example.org".into()), None, None).is_err());
    // Locking twice, unlocking with nothing.
    s.lock();
    assert!(s.unlock(String::new(), None, None, None, None).is_err());
}

#[test]
fn password_generator_with_absurd_options() {
    use super::super::tools::{generate_password, password_strength, PasswordOptions};
    for length in [0, 1, 2, 3, 4, 5, 64, 1024, 100_000, u32::MAX] {
        for bits in 0..32u32 {
            let o = PasswordOptions {
                length,
                lowercase: bits & 1 != 0,
                uppercase: bits & 2 != 0,
                digits: bits & 4 != 0,
                symbols: bits & 8 != 0,
                avoid_ambiguous: bits & 16 != 0,
            };
            if let Ok(p) = generate_password(o) {
                assert_eq!(p.chars().count() as u32, length, "length {length}");
            }
        }
    }
    for n in naughty() {
        let _ = password_strength(n);
    }
}
