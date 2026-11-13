// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public attack and edge-case collections run through GhulSuite:
//! - XSS payloads (PayloadsAllTheThings, SecLists, DOMPurify's test fixtures) through the HTML
//!   mail filter;
//! - CRLF injection payloads (PayloadsAllTheThings) through every mail header value;
//! - the URL test data of the web platform tests (WHATWG) through the autofill host parser – a
//!   browser and GhulSuite must agree on which site a URL belongs to;
//! - JSONTestSuite (318 valid, invalid and pathological JSON files) through every JSON import;
//! - the calendar files ical4j and python-icalendar test themselves with (valid, invalid and
//!   what fuzzing found there) through the calendar import.

use super::super::mail::{OutgoingAttachment, OutgoingMail};
use super::*;

fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}/tests/data", env!("CARGO_MANIFEST_DIR")))
}

fn lines(file: &str) -> Vec<String> {
    std::fs::read_to_string(data_dir().join(file))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .filter(|l| !l.trim().is_empty())
        .collect()
}

/// Percent-decoding as a careless app or server would do it.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn safe_url(u: &str) -> bool {
    let l = u.trim().to_ascii_lowercase();
    l.is_empty()
        || l.starts_with("https://")
        || l.starts_with("http://")
        || l.starts_with("mailto:")
        || l.starts_with("cid:")
}

#[test]
fn xss_collections_through_the_html_mail_filter() {
    let mut payloads = lines("payloads_xss_intruders.txt");
    payloads.extend(lines("seclists_xss_jhaddix.txt"));
    let dompurify: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(data_dir().join("dompurify_payloads.json")).unwrap()).unwrap();
    payloads.extend(dompurify);
    assert!(payloads.len() > 450, "{}", payloads.len());
    for p in &payloads {
        for variant in [p.clone(), percent_decode(p), super::super::mailhtml::decode_entities(p)] {
            let mail = super::super::mailhtml::parse(&variant);
            for b in &mail.blocks {
                assert!(safe_url(&b.image_src), "image {:?} from {p}", b.image_src);
                for span in &b.spans {
                    assert!(safe_url(&span.link), "link {:?} from {p}", span.link);
                    let t = span.text.to_ascii_lowercase();
                    // Text is shown as text, never run – but script-like markup must not survive
                    // as such (it would look like a trick to the reader).
                    assert!(
                        !t.contains("<script") && !t.contains("<iframe") && !t.contains("<svg"),
                        "text {:?} from {p}",
                        span.text
                    );
                }
            }
        }
    }
}

#[test]
fn crlf_collection_through_every_mail_header() {
    let d = Dirs::new("crlf");
    let s = d.suite();
    s.create(options("pw-123456", Some(generate_keyfile()), &[(VaultKind::Pass, "P")]), None, None, None).unwrap();
    let acc = s
        .mail_save_account(
            None,
            super::super::mail::MailAccountInput {
                name: "P".into(),
                display_name: "Max".into(),
                email: "max@example.org".into(),
                username: "max@example.org".into(),
                password: "x".into(),
                servers: super::super::mail::mail_suggest_servers("max@example.org".into()),
                imap_cert: None,
                smtp_cert: None,
                signature: String::new(),
            },
        )
        .unwrap();
    let mut payloads = lines("payloads_crlf.txt");
    payloads.extend([
        "\r\nBcc: spy@evil.example".into(),
        "\nX-Evil: 1".into(),
        "\r\n\r\n<html>".into(),
        "\u{2028}X-Evil: 2".into(),
    ]);
    for p in &payloads {
        for v in [p.clone(), percent_decode(p)] {
            let mail = OutgoingMail {
                account_id: acc.clone(),
                to: vec!["bob@example.org".into()],
                cc: vec![],
                bcc: vec![],
                subject: format!("Hallo{v}"),
                text: "Text".into(),
                in_reply_to: format!("x@y{v}"),
                attachments: vec![OutgoingAttachment {
                    name: format!("a{v}.pdf"),
                    mime: format!("application/pdf{v}"),
                    data: vec![1],
                }],
                pgp: false,
            };
            let (raw, rcpt) = super::super::mail::build_for_tests(&s, &mail).unwrap();
            assert_eq!(rcpt, vec!["bob@example.org".to_string()], "{v:?}");
            let text = String::from_utf8_lossy(&raw);
            let header = &text[..text.find("\r\n\r\n").unwrap_or(text.len())];
            for line in header.split("\r\n").filter(|l| !l.starts_with([' ', '\t'])) {
                let name = line.split(':').next().unwrap_or("").to_ascii_lowercase();
                assert!(
                    [
                        "from",
                        "to",
                        "subject",
                        "date",
                        "message-id",
                        "in-reply-to",
                        "references",
                        "content-type",
                        "mime-version",
                        "content-transfer-encoding"
                    ]
                    .contains(&name.as_str()),
                    "injected header {line:?} from {v:?}"
                );
            }
        }
    }
}

#[test]
fn urls_belong_to_the_same_site_as_in_a_browser() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(data_dir().join("wpt_urltestdata.json")).unwrap()).unwrap();
    let mut checked = 0;
    for c in cases.iter().filter(|c| c.is_object()) {
        let input = c["input"].as_str().unwrap_or("");
        // Never a crash, whatever the input.
        let ours = super::super::autofill::host_of(input);
        if c["failure"].as_bool() == Some(true)
            || c["base"].as_str().is_some_and(|b| b != "about:blank" && !b.is_empty())
        {
            continue;
        }
        let proto = c["protocol"].as_str().unwrap_or("");
        if proto != "http:" && proto != "https:" {
            continue;
        }
        let browser = c["hostname"].as_str().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
        let browser = browser.strip_prefix("www.").unwrap_or(&browser).to_owned();
        // Hosts GhulSuite never fills for (no dot, IPv6, percent-encoded, non-ASCII) are skipped.
        if !browser.contains('.')
            || browser.starts_with('[')
            || browser.contains('%')
            || !browser.is_ascii()
            || browser.starts_with("xn--")
        {
            continue;
        }
        // Where GhulSuite finds a host, it must be the one a browser goes to.
        if let Some(h) = ours {
            assert_eq!(
                super::super::autofill::base_domain(&h),
                super::super::autofill::base_domain(&browser),
                "{input:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 60, "{checked}");
}

#[test]
fn json_test_suite_through_every_json_import() {
    let dir = data_dir().join("jsontestsuite");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert!(files.len() > 300);
    for f in files {
        let data = std::fs::read(&f).unwrap();
        for name in ["x.json", "aegis.json", "bitwarden.json", "2fas.2fas", "proton.json"] {
            let _ = super::super::import::parse(&data, name);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Calendar files of other programs
// ---------------------------------------------------------------------------------------------

fn files_under_dir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            out.extend(files_under_dir(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// About 300 calendar files from the test suites of ical4j and python-icalendar: real exports of
/// many programs, broken files and inputs their fuzzing found. Whatever the import accepts must
/// be a sound event, and exporting and reading it again must give the same event.
#[test]
fn public_calendar_files_import_soundly_and_survive_a_round_trip() {
    use super::super::cal::CalEvent;
    use super::super::caldate::{Date, DateTime};
    use super::super::caltext::{ics_export, ics_import};
    type Essence<'a> = (&'a str, &'a str, &'a str, bool, &'a str, &'a str, String, &'a str, &'a [String], &'a [u32]);
    fn essence(e: &CalEvent) -> Essence<'_> {
        (
            &e.title,
            &e.location,
            &e.notes,
            e.all_day,
            &e.start,
            &e.end,
            format!("{:?}", e.repeat),
            &e.repeat_until,
            &e.exceptions,
            &e.reminders,
        )
    }
    let (mut files, mut readable, mut events_seen) = (0, 0, 0);
    for path in files_under_dir(&data_dir().join("ics")) {
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        files += 1;
        for offset in [0, 60, -480, 765] {
            let Ok(events) = ics_import(&text, offset) else { continue };
            assert!(!events.is_empty(), "{name}");
            if offset == 0 {
                readable += 1;
                events_seen += events.len();
            }
            for e in &events {
                if e.all_day {
                    let (s, en) = (Date::parse(&e.start), Date::parse(&e.end));
                    assert!(s.is_some() && en.is_some() && en >= s, "{name}: {e:?}");
                } else {
                    let (s, en) = (DateTime::parse(&e.start), DateTime::parse(&e.end));
                    assert!(s.is_some() && en.is_some() && en >= s, "{name}: {e:?}");
                }
                assert!(e.exceptions.iter().all(|d| Date::parse(d).is_some()), "{name}: {e:?}");
                assert!(e.repeat_until.is_empty() || Date::parse(&e.repeat_until).is_some(), "{name}: {e:?}");
            }
            let back = ics_import(&ics_export(&events, "Test"), offset).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(back.len(), events.len(), "{name}");
            for (a, b) in events.iter().zip(&back) {
                assert_eq!(essence(a), essence(b), "{name}");
            }
        }
    }
    assert!(
        files > 250 && readable > 150 && events_seen > 1000,
        "{files} files, {readable} readable, {events_seen} events"
    );
}

/// The same files through the app's own way: into a calendar, shown, exported, locked and opened.
#[test]
fn public_calendar_files_through_a_calendar() {
    use super::super::cal::CalCalendar;
    use super::super::caltext::FixedOffset;
    let d = Dirs::new("ics-files");
    let s = d.suite();
    let kf = generate_keyfile();
    s.create(options("pw pw pw pw", Some(kf.clone()), &[(VaultKind::Cal, "Kalender")]), None, None, None).unwrap();
    let calendar = |name: &str| {
        s.cal_save_calendar(CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, "Kalender"),
            name: name.into(),
            color: 1,
            default_reminder: -1,
            events: 0,
        })
        .unwrap()
    };
    let all = calendar("Alles");
    let mut added = 0;
    for path in files_under_dir(&data_dir().join("ics")) {
        let bytes = std::fs::read(&path).unwrap();
        match s.cal_import_ics(all.clone(), bytes, Arc::new(FixedOffset(60))) {
            Ok(n) => added += n,
            Err(GhulError::InvalidInput { .. }) => {}
            Err(e) => panic!("{path:?}: {e:?}"),
        }
    }
    assert!(added > 1000, "{added}");
    let shown = s.cal_occurrences("2000-01-01".into(), "2030-01-01".into(), "DE-BY".into(), "de".into()).unwrap();
    assert!(shown.len() > 1000, "{}", shown.len());
    s.cal_reminders("2026-11-13T10:00".into(), 400).unwrap();
    // Out and into a second calendar: nothing is lost, nothing is doubled within one calendar.
    let ics = s.cal_export_ics(vec![all.clone()]).unwrap();
    assert_eq!(s.cal_import_ics(all.clone(), ics.clone().into_bytes(), Arc::new(FixedOffset(60))).unwrap(), 0);
    let copy = calendar("Kopie");
    let copied = s.cal_import_ics(copy.clone(), ics.into_bytes(), Arc::new(FixedOffset(60))).unwrap();
    let count = |id: &str| s.cal_calendars().unwrap().into_iter().find(|c| c.id == id).unwrap().events;
    assert_eq!(copied, count(&all), "exported and read again");
    assert_eq!(count(&copy), count(&all));
    s.lock();
    s.unlock("pw pw pw pw".into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(count(&copy), copied);
}
