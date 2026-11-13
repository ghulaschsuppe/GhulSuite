// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public attack and edge-case collections run through GhulSuite:
//! - XSS payloads (PayloadsAllTheThings, SecLists, DOMPurify's test fixtures) through the HTML
//!   mail filter;
//! - CRLF injection payloads (PayloadsAllTheThings) through every mail header value;
//! - the URL test data of the web platform tests (WHATWG) through the autofill host parser – a
//!   browser and GhulSuite must agree on which site a URL belongs to;
//! - JSONTestSuite (318 valid, invalid and pathological JSON files) through every JSON import.

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
