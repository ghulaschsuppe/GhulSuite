// SPDX-License-Identifier: AGPL-3.0-or-later
//! Attacks from outside. The attacker may steal the device's files and suite files (without the
//! master password and the second factor) and hand over files and backups. Promises Z2 (no
//! plaintext outside) and Z3 (no crash).

use std::io::Write as _;
use std::sync::Arc;

use super::super::cal::{CalCalendar, CalEvent};
use super::*;

/// Words that must never show up readable in any file outside the running app.
const MARKS: &[&str] =
    &["KlauPasswortX9", "KlauNutzerX9", "KlauNotizX9", "KlauTerminX9", "KlauOrtX9", "KlauKontoX9", "KlauTresorX9"];
/// 2FA secret "Hello!\xde\xad\xbe\xef" in Base32, its raw bytes must not show up either.
const TOTP_SECRET: &str = "JBSWY3DPEHPK3PXP";

fn all_files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    files_under(dir).into_iter().filter_map(|p| std::fs::read(&p).ok().map(|d| (p, d))).collect()
}

/// Looks for every mark as UTF-8, UTF-16 and in hex or Base64.
fn assert_no_marks(what: &str, data: &[u8]) {
    use base64::Engine;
    for m in MARKS {
        let utf16: Vec<u8> = m.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let forms: Vec<Vec<u8>> = vec![
            m.as_bytes().to_vec(),
            m.to_lowercase().into_bytes(),
            utf16,
            crate::kdf::hex(m.as_bytes()).into_bytes(),
            base64::engine::general_purpose::STANDARD.encode(m.as_bytes()).trim_end_matches('=').as_bytes()[..12]
                .to_vec(),
        ];
        for f in forms {
            assert!(!data.windows(f.len()).any(|w| w == f.as_slice()), "{what} contains {m} ({} bytes)", f.len());
        }
    }
    let raw = b"Hello!\xde\xad\xbe\xef";
    assert!(!data.windows(raw.len()).any(|w| w == raw), "{what} contains the raw 2FA secret");
    assert!(!data.windows(TOTP_SECRET.len()).any(|w| w == TOTP_SECRET.as_bytes()), "{what} contains the 2FA secret");
}

/// A suite with a mark in every area.
fn marked_suite(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>) {
    let d = Dirs::new(name);
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(
        options(
            "pw-123456-Kq7wZ",
            Some(kf.clone()),
            &[(VaultKind::Pass, "KlauTresorX9"), (VaultKind::Totp, "T"), (VaultKind::Cal, "K")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    let pass = vault_id(&s, "KlauTresorX9");
    let mut e = login("Bank", "KlauNutzerX9", "KlauPasswortX9");
    e.fields.push(FieldData { name: "notes".into(), value: FieldValue::Text { value: "KlauNotizX9".into() } });
    s.save_entry(pass.clone(), e).unwrap();
    let mut t = totp_input();
    t.issuer = "Bank".into();
    t.account = "KlauKontoX9".into();
    t.secret_base32 = TOTP_SECRET.into();
    t.digits = 6;
    s.add_totp(vault_id(&s, "T"), t).unwrap();
    let cal = s
        .cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, "K"),
            name: "C".into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap();
    s.cal_save_event(CalEvent {
        calendar_id: cal,
        title: "KlauTerminX9".into(),
        location: "KlauOrtX9".into(),
        start: "2026-12-01T10:00".into(),
        end: "2026-12-01T11:00".into(),
        ..Default::default()
    })
    .unwrap();
    (d, s, kf)
}

#[test]
fn stolen_device_files_and_backup_reveal_nothing() {
    let (d, s, _kf) = marked_suite("leak");
    let backup = s.archive_build_bytes("Dieb".into()).unwrap().data;
    s.lock();
    // Everything the app wrote: its folder (vault files, local state) and the backup.
    for (p, data) in all_files(&d.root) {
        assert_no_marks(&p.display().to_string(), &data);
    }
    // File names say nothing either (F23): not here, not in the backup.
    for p in files_under(&d.root) {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert_no_marks(&format!("file name {name}"), name.as_bytes());
    }
    // Inside the backup ZIP as well (entries stored, but check unpacked too).
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(backup)).unwrap();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut buf).unwrap();
        assert_no_marks(&format!("backup entry {}", f.name().unwrap()), &buf);
        assert_no_marks("backup entry name", f.name().unwrap().as_bytes());
    }
}

#[test]
fn stolen_files_do_not_open_without_both_factors() {
    let (d, s, kf) = marked_suite("steal");
    s.lock();
    // A thief copies the folder to another device.
    let thief = Dirs::new("thief");
    for f in d.storage().list().unwrap() {
        thief.storage().write(f.clone(), d.storage().read(f).unwrap().unwrap()).unwrap();
    }
    let t = thief.suite();
    assert!(t.unlock("pw-123456-Kq7wZ".into(), None, None, None, None).is_err(), "password alone");
    assert!(t.unlock("falsch".into(), None, None, None, Some(kf.clone())).is_err(), "key file alone");
    assert!(
        t.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(generate_keyfile())).is_err(),
        "another key file"
    );
    let mut bent = kf.clone();
    let last = bent.len() - 1;
    bent[last] ^= 1;
    assert!(t.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(bent)).is_err(), "changed key file");
    // The owner still can.
    t.unlock("pw-123456-Kq7wZ".into(), None, None, None, Some(kf)).unwrap();
}

#[test]
fn backups_with_path_tricks_and_bombs_write_nothing_outside() {
    let target = Dirs::new("restore-attack");
    let outside = target.root.join("outside.ghlpss");
    // Stored, as the app writes them: the names themselves are what is checked.
    let make = |entries: &[(&str, &[u8])]| {
        let mut z = super::super::zipstream::ZipWriter::new(Vec::new(), |_| {});
        for (n, d) in entries {
            z.add_bytes(n, d).unwrap();
        }
        z.finish().unwrap()
    };
    let tricks = [
        "../outside.ghlpss",
        "..\\outside.ghlpss",
        "/tmp/abs.ghlpss",
        "C:\\x.ghlpss",
        "suite.ghlsuite:stream",
        ".hidden.ghlpss",
        "a/../../outside.ghlpss",
        "con.ghlpss",
        "x\0.ghlpss",
        "x\u{202e}sspslhg.exe",
    ];
    for t in tricks {
        let zip = make(&[("export.ghlexp", b"GHULEXPT"), ("suite.ghlsuite", b"GHULSUIT"), (t, b"evil")]);
        let s = target.suite();
        let _ = s.archive_open_bytes(zip, "pw".into(), None, None, None, None);
        let _ = s.archive_accept();
        assert!(!outside.exists(), "{t} wrote outside");
        for f in target.storage().list().unwrap() {
            assert!(!f.contains(['/', '\\', ':', '\0']) && !f.starts_with('.'), "{t} created {f:?}");
        }
        let _ = std::fs::remove_dir_all(target.root.join("vault"));
    }
    // A bomb: a few kilobytes that unpack to far more than allowed (limit lowered for the test).
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        z.start_file("suite.ghlsuite", opts).unwrap();
        z.write_all(b"GHULSUIT").unwrap();
        z.start_file("bomb.ghlpss", opts).unwrap();
        let zeros = vec![0u8; 1 << 20];
        for _ in 0..40 {
            z.write_all(&zeros).unwrap();
        }
        z.finish().unwrap();
    }
    let bomb = out.into_inner();
    assert!(bomb.len() < 200_000);
    // Compressed entries are refused before anything is unpacked.
    let s = target.suite();
    assert!(s.archive_protection_bytes(bomb.clone()).is_err(), "a bomb must be refused");
    assert!(s.archive_open_bytes(bomb, "pw".into(), None, None, None, None).is_err(), "a bomb must be refused");
}

#[test]
fn exports_are_safe_to_open_in_a_spreadsheet() {
    let d = Dirs::new("csv");
    let s = d.suite();
    s.create(options("pw-123456-Kq7wZ", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None)
        .unwrap();
    let pass = vault_id(&s, "P");
    // A website stored a "user name" that is a spreadsheet formula (via import ...).
    let formulas = ["=HYPERLINK(\"https://evil\",\"x\")", "+cmd|' /C calc'!A0", "-2+3", "@SUM(1)", "\t=1", "'=quoted"];
    for f in formulas {
        s.save_entry(pass.clone(), login(f, f, f)).unwrap();
    }
    let csv = String::from_utf8(s.export_plain(pass.clone(), "pw-123456-Kq7wZ".into(), true).unwrap()).unwrap();
    for line in csv.lines().skip(1) {
        for cell in line.split(',') {
            let c = cell.trim_start_matches('"');
            assert!(!c.starts_with(['=', '+', '-', '@', '\t']), "formula cell {cell:?} in {line:?}");
        }
    }
    // Importing the own export gives the exact passwords back (titles are trimmed as always).
    let other = Dirs::new("csv-back");
    let o = other.suite();
    o.create(options("pw-123456-Kq7wZ", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None)
        .unwrap();
    let ov = vault_id(&o, "P");
    o.import_data(csv.into_bytes(), "ghulsuite.csv".into(), Some(ov.clone()), None, false, None).unwrap();
    let mut got: Vec<String> = o
        .list_entries(VaultKind::Pass, None)
        .unwrap()
        .into_iter()
        .map(|e| {
            match o
                .get_entry(ov.clone(), e.id)
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "password")
                .map(|f| f.value.clone())
            {
                Some(FieldValue::Secret { value }) => value,
                other => panic!("{other:?}"),
            }
        })
        .collect();
    let mut want: Vec<String> = formulas.iter().map(|f| f.to_string()).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

/// Passwords, keys and codes never appear in debug output (logs, crash reports, error texts).
#[test]
fn secrets_never_show_up_in_debug_output_or_errors() {
    const M: &str = "DebugGeheimX7";
    let shown = [
        format!("{:?}", options(M, Some(M.as_bytes().to_vec()), &[(VaultKind::Pass, "P")])),
        format!("{:?}", login("t", "u", M)),
        format!("{:?}", TotpInput { secret_base32: M.into(), ..totp_input() }),
    ];
    for s in &shown {
        assert!(!s.contains(M) && !s.contains(&format!("{:?}", M.as_bytes())), "{s}");
    }
    // Errors from wrong input name no secret either.
    let d = Dirs::new("debug");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options(M, Some(kf.clone()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let pass = vault_id(&s, "P");
    s.save_entry(pass.clone(), login("Bank", "u", "PasswortGeheimX7")).unwrap();
    s.lock();
    let errors = [
        format!("{:?}", s.unlock(format!("{M}x"), None, None, None, Some(kf.clone())).unwrap_err()),
        format!("{:?}", s.unlock(M.into(), None, None, None, Some(M.as_bytes().to_vec())).unwrap_err()),
        format!("{:?}", s.export_plain(pass.clone(), M.into(), false).unwrap_err()),
    ];
    for e in &errors {
        assert!(!e.contains(M) && !e.contains("PasswortGeheimX7"), "{e}");
    }
}

/// An import archive that claims to be small but unpacks to gigabytes is cut off.
#[test]
fn import_archives_cannot_exhaust_memory() {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        z.start_file("data.json", opts).unwrap();
        let block = vec![b' '; 1 << 20];
        for _ in 0..200 {
            z.write_all(&block).unwrap();
        }
        z.finish().unwrap();
    }
    let bomb = out.into_inner();
    assert!(bomb.len() < 1_000_000, "{}", bomb.len());
    let t = std::time::Instant::now();
    assert!(super::super::import::parse(&bomb, "export.zip").is_err());
    assert!(t.elapsed().as_secs() < 20);
}
