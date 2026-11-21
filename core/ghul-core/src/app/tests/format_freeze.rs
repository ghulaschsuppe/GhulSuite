// SPDX-License-Identifier: AGPL-3.0-or-later
//! Format 3 is frozen (docs/FORMAT.md, "Kompatibilitätszusage"): every later version of the app
//! opens these files, written by the app of 21.11.2026, with all their content. The files are
//! checked in and never written again; a change of the format keeps reading them.
//!
//! Made once with `cargo test -p ghul-core --lib write_format3_fixtures -- --ignored` (only for a
//! new set of files of a NEW format version, never to replace these).

use super::super::archive::in_memory::{source, BytesSink};
use super::super::cal::{CalCalendar, CalEvent};
use super::*;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/format3");
const PW: &str = "Format-3-bleibt-lesbar!";
const DOC: &[u8] = b"GhulSuite format 3: this document must open in every later version.\n";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DIR}/{name}")).unwrap_or_else(|e| panic!("{DIR}/{name}: {e}"))
}

/// What every opened copy must hold.
fn check_content(s: &GhulSuite) {
    let vaults = s.list_vaults().unwrap();
    let names: Vec<&str> = vaults.iter().map(|v| v.name.as_str()).collect();
    for n in ["Privat", "2FA", "Kalender", "Dokumente"] {
        assert!(names.contains(&n), "vault {n} in {names:?}");
    }
    // GhulPass: the login with its secret.
    let pass = vault_id(s, "Privat");
    let logins = s.list_entries(VaultKind::Pass, Some(pass.clone())).unwrap();
    let bank = logins.iter().find(|e| e.title == "Bank (Format 3)").expect("login");
    let entry = s.get_entry(pass, bank.id.clone()).unwrap();
    let secret = entry.fields.iter().find(|f| f.name == "password").map(|f| f.value.clone());
    assert_eq!(secret, Some(FieldValue::Secret { value: "geheim-123".into() }));
    // GhulAuth: the code of RFC 6238 (SHA-1, 8 digits) at t = 59.
    let codes = s.totp_codes(59).unwrap();
    let code = codes.iter().find(|c| c.issuer == "Example").expect("2FA entry");
    assert_eq!(code.code, "94287082");
    // GhulCal: the calendar and its event.
    let cal = vault_id(s, "Kalender");
    let calendars = s.cal_calendars().unwrap();
    let familie = calendars.iter().find(|c| c.name == "Familie" && c.vault_id == cal).expect("calendar");
    let occ = s.cal_occurrences("2026-12-01".into(), "2026-12-31".into(), String::new(), "de".into()).unwrap();
    let ev = occ.iter().find(|o| o.title == "Weihnachtsessen").expect("event");
    assert_eq!(ev.calendar_id, familie.id);
    // GhulDocs: the folder and the document, byte for byte.
    let docs = vault_id(s, "Dokumente");
    let root = s.docs_list(docs.clone(), None).unwrap();
    let folder = root.iter().find(|i| i.name == "Ordner").expect("folder");
    let inside = s.docs_list(docs.clone(), Some(folder.id.clone())).unwrap();
    let file = inside.iter().find(|i| i.name == "probe.txt").expect("document");
    let sink = Arc::new(BytesSink::default());
    s.docs_read_file(docs, file.id.clone(), sink.clone(), None).unwrap();
    assert_eq!(&*sink.0.lock().unwrap(), DOC);
}

/// The suite file of format 3 opens with password and key file, on a device that never saw it.
#[test]
fn a_suite_file_of_format_3_opens_with_password_and_key_file() {
    let d = Dirs::new("format3-suite");
    let s = d.suite();
    s.archive_open_bytes(fixture("suite.ghlzip"), PW.into(), None, None, None, Some(fixture("key.ghlkey"))).unwrap();
    s.archive_accept().unwrap();
    check_content(&s);
    // And opens again after locking (the working copy written by this version).
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(fixture("key.ghlkey"))).unwrap();
    check_content(&s);
}

/// The same file with the recovery code of the emergency sheet (the code alone opens it).
#[test]
fn a_suite_file_of_format_3_opens_with_the_recovery_code() {
    let code = String::from_utf8(fixture("recovery-code.txt")).unwrap();
    let d = Dirs::new("format3-code");
    let s = d.suite();
    s.archive_open_with_code_bytes(fixture("suite.ghlzip"), code.trim().into(), "Neues-Passwort-456!".into()).unwrap();
    s.archive_accept().unwrap();
    check_content(&s);
}

/// A vault file of format 3 on its own (e.g. a `.ghlpss` from a USB stick).
#[test]
fn a_vault_file_of_format_3_opens_on_its_own() {
    let d = Dirs::new("format3-single");
    let s = d.suite();
    let kind = s
        .open_single_vault(fixture("privat.ghlpss"), PW.into(), None, None, None, Some(fixture("key.ghlkey")))
        .unwrap();
    assert_eq!(kind, VaultKind::Pass);
    let titles: Vec<String> = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.title).collect();
    assert!(titles.contains(&"Bank (Format 3)".to_string()), "{titles:?}");
}

/// Wrong password or key file: refused as such (not "damaged", not "update required").
#[test]
fn a_suite_file_of_format_3_says_why_it_does_not_open() {
    let d = Dirs::new("format3-wrong");
    let s = d.suite();
    let r = s.archive_open_bytes(
        fixture("suite.ghlzip"),
        "falsch-123456".into(),
        None,
        None,
        None,
        Some(fixture("key.ghlkey")),
    );
    assert!(matches!(r, Err(GhulError::WrongPassword)), "{r:?}");
    let r = s.archive_open_bytes(fixture("suite.ghlzip"), PW.into(), None, None, None, Some(generate_keyfile()));
    assert!(matches!(r, Err(GhulError::WrongKeyfile)), "{r:?}");
}

/// Writes the files above (see the module comment): only for a new format version.
#[test]
#[ignore]
fn write_format3_fixtures() {
    let d = Dirs::new("format3-write");
    let s = d.suite();
    let kf = generate_keyfile();
    let res = s
        .create(
            options(
                PW,
                Some(kf.clone()),
                &[
                    (VaultKind::Pass, "Privat"),
                    (VaultKind::Totp, "2FA"),
                    (VaultKind::Cal, "Kalender"),
                    (VaultKind::Docs, "Dokumente"),
                ],
            ),
            None,
            None,
            None,
        )
        .unwrap();
    let pass = vault_id(&s, "Privat");
    s.save_entry(pass.clone(), login("Bank (Format 3)", "kunde", "geheim-123")).unwrap();
    s.add_totp(vault_id(&s, "2FA"), totp_input()).unwrap();
    let cal = vault_id(&s, "Kalender");
    let calendar = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: cal,
            name: "Familie".into(),
            color: 0xd6336c,
            default_reminder: 15,
            events: 0,
        })
        .unwrap();
    s.cal_save_event(CalEvent {
        calendar_id: calendar,
        title: "Weihnachtsessen".into(),
        start: "2026-12-24T18:00".into(),
        end: "2026-12-24T21:00".into(),
        reminders: vec![60],
        location: "Zuhause".into(),
        ..Default::default()
    })
    .unwrap();
    let docs = vault_id(&s, "Dokumente");
    let folder = s.docs_create_folder(docs.clone(), None, "Ordner".into()).unwrap();
    s.docs_add_file(docs, Some(folder.id), "probe.txt".into(), "text/plain".into(), source(DOC.to_vec()), None)
        .unwrap();
    let built = s.archive_build_bytes("Format-3-Fixture".into()).unwrap();
    std::fs::create_dir_all(DIR).unwrap();
    std::fs::write(format!("{DIR}/suite.ghlzip"), &built.data).unwrap();
    std::fs::write(format!("{DIR}/key.ghlkey"), &kf).unwrap();
    std::fs::write(format!("{DIR}/privat.ghlpss"), s.export_vault(pass).unwrap()).unwrap();
    std::fs::write(format!("{DIR}/recovery-code.txt"), res.recovery_code.unwrap()).unwrap();
}

/// The files themselves stay as they were written on 21.11.2026: a set made anew would prove
/// nothing about the files people have (writing them again is for a new format version only).
#[test]
fn the_files_of_format_3_are_the_ones_of_21_11_2026() {
    use sha2::{Digest, Sha256};
    for (name, sum) in [
        ("key.ghlkey", "30f1e6194d867a9549e3a277dc3bf18ea9ebb43bc9709ebce0410644b3dabd07"),
        ("privat.ghlpss", "1ea1c316d8703830da3e3a3cc0ec62887fbe9028d1be541829df9e886db3ce3e"),
        ("recovery-code.txt", "88f632c83ee5a722b735108340878d517be0ac6ba11fc776e75f09f2c66ac9a5"),
        ("suite.ghlzip", "9fc99298e459b036997b82b929e53ffb093e278764231fa4089d6acb95cea57b"),
    ] {
        assert_eq!(crate::kdf::hex(&Sha256::digest(fixture(name))), sum, "{name} was changed");
    }
}

// --- more kinds of files of format 3 (round 5, M5) -----------------------------------------------

const EXTRA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/format3-extra");
const ATTACHMENT: &[u8] = b"Anhang im Format 3\n";
/// A key of a newer version in an entry and in the payload: kept as it is (promise 2).
const FUTURE_KEY: &str = "x_future_field";

fn extra(name: &str) -> Vec<u8> {
    std::fs::read(format!("{EXTRA_DIR}/{name}")).unwrap_or_else(|e| panic!("{EXTRA_DIR}/{name}: {e}"))
}

/// A GhulDocs package (`.ghldocs`) of format 3 opens on its own, the document byte for byte.
#[test]
fn a_ghuldocs_package_of_format_3_opens() {
    let d = Dirs::new("format3-ghldocs");
    let s = d.suite();
    let vid = s
        .open_single_docs(
            source(extra("dokumente.ghldocs")),
            PW.into(),
            None,
            None,
            None,
            Some(extra("key.ghlkey")),
            None,
        )
        .unwrap();
    let root = s.docs_list(vid.clone(), None).unwrap();
    let file = root.iter().find(|i| i.name == "probe.txt").expect("document");
    let sink = Arc::new(BytesSink::default());
    s.docs_read_file(vid, file.id.clone(), sink.clone(), None).unwrap();
    assert_eq!(&*sink.0.lock().unwrap(), DOC);
}

/// Single `.ghltotp` and `.ghlcal` files of format 3 open on their own.
#[test]
fn single_2fa_and_calendar_files_of_format_3_open() {
    let d = Dirs::new("format3-totp");
    let s = d.suite();
    assert_eq!(
        s.open_single_vault(extra("2fa.ghltotp"), PW.into(), None, None, None, Some(extra("key.ghlkey"))).unwrap(),
        VaultKind::Totp
    );
    assert_eq!(s.totp_codes(59).unwrap()[0].code, "94287082");
    let d = Dirs::new("format3-cal");
    let s = d.suite();
    assert_eq!(
        s.open_single_vault(extra("kalender.ghlcal"), PW.into(), None, None, None, Some(extra("key.ghlkey"))).unwrap(),
        VaultKind::Cal
    );
    let occ = s.cal_occurrences("2026-12-01".into(), "2026-12-31".into(), String::new(), "de".into()).unwrap();
    assert!(occ.iter().any(|o| o.title == "Weihnachtsessen"), "{occ:?}");
}

/// A GhulPass file of format 3 with an attachment and keys of a newer version: the attachment
/// opens, the unknown keys are kept, also after an edit.
#[test]
fn a_pass_file_of_format_3_keeps_attachments_and_unknown_keys() {
    let d = Dirs::new("format3-attach");
    let s = d.suite();
    s.open_single_vault(extra("anhang.ghlpss"), PW.into(), None, None, None, Some(extra("key.ghlkey"))).unwrap();
    let e = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().find(|e| e.title == "Mit Anhang").unwrap();
    assert_eq!(s.entry_attachment(e.vault_id.clone(), e.id.clone(), "probe.txt".into()).unwrap(), ATTACHMENT);
    let keys = |s: &GhulSuite| {
        let vid = e.vault_id.clone();
        s.with_open(|o| {
            let (_, c) = o.vault_mut(&vid)?;
            Ok((
                c.payload.extra.contains_key(FUTURE_KEY),
                c.payload.entries.iter().any(|x| x.extra.contains_key(FUTURE_KEY)),
            ))
        })
        .unwrap()
    };
    assert_eq!(keys(&s), (true, true), "unknown keys read");
    let mut entry = s.get_entry(e.vault_id.clone(), e.id.clone()).unwrap();
    entry.fields.push(FieldData { name: "notes".into(), value: FieldValue::Text { value: "bearbeitet".into() } });
    s.save_entry(e.vault_id.clone(), entry).unwrap();
    assert_eq!(keys(&s), (true, true), "unknown keys kept after an edit");
}

/// Writes the files of `format3-extra` (only once, like `write_format3_fixtures`). No FIDO2 slot:
/// the software authenticator of the tests makes new keys every run, so its file could not be
/// opened again.
#[test]
#[ignore]
fn write_format3_extra_fixtures() {
    let d = Dirs::new("format3-extra-write");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(
        options(
            PW,
            Some(kf.clone()),
            &[
                (VaultKind::Pass, "Anhang"),
                (VaultKind::Totp, "2FA"),
                (VaultKind::Cal, "Kalender"),
                (VaultKind::Docs, "Dokumente"),
            ],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    let pass = vault_id(&s, "Anhang");
    let id = s.save_entry(pass.clone(), login("Mit Anhang", "kunde", "geheim-456")).unwrap();
    s.add_entry_attachment(pass.clone(), id.clone(), "probe.txt".into(), ATTACHMENT.to_vec()).unwrap();
    s.with_open(|o| {
        let (_, c) = o.vault_mut(&pass)?;
        c.payload.extra.insert(FUTURE_KEY.into(), ciborium::value::Value::Text("from a newer version".into()));
        let e = c.payload.entries.iter_mut().find(|e| e.id == id).unwrap();
        e.extra.insert(FUTURE_KEY.into(), ciborium::value::Value::Integer(42.into()));
        Ok(())
    })
    .unwrap();
    s.with_open(|o| s.save_vault(o, &pass)).unwrap();
    s.add_totp(vault_id(&s, "2FA"), totp_input()).unwrap();
    let calendar = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, "Kalender"),
            name: "Familie".into(),
            color: 0xd6336c,
            default_reminder: 15,
            events: 0,
        })
        .unwrap();
    s.cal_save_event(CalEvent {
        calendar_id: calendar,
        title: "Weihnachtsessen".into(),
        start: "2026-12-24T18:00".into(),
        end: "2026-12-24T21:00".into(),
        ..Default::default()
    })
    .unwrap();
    let docs = vault_id(&s, "Dokumente");
    s.docs_add_file(docs.clone(), None, "probe.txt".into(), "text/plain".into(), source(DOC.to_vec()), None).unwrap();
    let sink = Arc::new(BytesSink::default());
    s.docs_export_vault(docs, sink.clone(), None).unwrap();
    std::fs::create_dir_all(EXTRA_DIR).unwrap();
    std::fs::write(format!("{EXTRA_DIR}/key.ghlkey"), &kf).unwrap();
    std::fs::write(format!("{EXTRA_DIR}/dokumente.ghldocs"), &*sink.0.lock().unwrap()).unwrap();
    std::fs::write(format!("{EXTRA_DIR}/anhang.ghlpss"), s.export_vault(pass).unwrap()).unwrap();
    std::fs::write(format!("{EXTRA_DIR}/2fa.ghltotp"), s.export_vault(vault_id(&s, "2FA")).unwrap()).unwrap();
    std::fs::write(format!("{EXTRA_DIR}/kalender.ghlcal"), s.export_vault(vault_id(&s, "Kalender")).unwrap()).unwrap();
}

/// The files of `format3-extra` stay as they were written (see `the_files_of_format_3_are_the_ones_of_21_11_2026`).
#[test]
fn the_extra_files_of_format_3_are_unchanged() {
    use sha2::{Digest, Sha256};
    for (name, sum) in [
        ("2fa.ghltotp", "61d851e3800ebd0d732393a8a6652dcbd9e9b978fbab9bf6d588e8ebacde830b"),
        ("anhang.ghlpss", "36337f08d8e700a906e3fef37d6dd6fa5afbeb64633cf9bf99777598e44c36ea"),
        ("dokumente.ghldocs", "b5bc2118d86d4273610749051124114476e29487e6c46d2b05037d07ad11ac4f"),
        ("kalender.ghlcal", "b0abc72658c48630554309e0266fffe31561e23b919ff8633fd4023b080624c0"),
        ("key.ghlkey", "f130dcf146ce7c9ba606688b16737bc6264fae3813dc85119150131201b609dc"),
    ] {
        assert_eq!(crate::kdf::hex(&Sha256::digest(extra(name))), sum, "{name} was changed");
    }
}
