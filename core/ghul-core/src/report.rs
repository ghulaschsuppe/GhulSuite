// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sending an error report (with the user's note) to the developer, through the project's
//! website (report.php on www.ghulsuite.de, which forwards it by e-mail and stores nothing).
//!
//! Only when the user taps "Send"; the report is shown to them before. Against spam the server
//! asks for a proof of work: SHA-256("ghulsuite-report|<time>|<content hash>|<nonce>") must start
//! with [`BITS`] zero bits, over exactly this content and at this time (about a million hashes,
//! well under a second on a phone, but expensive for anyone sending thousands).

use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::app::error::{GhulError, Result};
use crate::net::block_on;
use crate::net::http::{self, Url};

const REPORT_URL: &str = "https://www.ghulsuite.de/report.php";
/// Zero bits the proof must start with (the server asks for this many).
const BITS: u32 = 20;
const MAX_REPORT: usize = 32 * 1024;
const MAX_NOTE: usize = 4000;

/// Sends the report with an optional note and an optional e-mail address for questions.
#[uniffi::export]
pub fn report_send(report: String, note: String, contact: String) -> Result<()> {
    send_to(REPORT_URL, &report, &note, &contact, BITS)
}

fn send_to(url: &str, report: &str, note: &str, contact: &str, bits: u32) -> Result<()> {
    let note = note.trim();
    let contact = contact.trim();
    if note.chars().count() > MAX_NOTE {
        return Err(GhulError::InvalidInput { msg: "the note is too long".into() });
    }
    if !contact.is_empty() && !plausible_mail(contact) {
        return Err(GhulError::InvalidInput { msg: "invalid e-mail address for questions".into() });
    }
    // A long report (a crash log) is cut at the end; its beginning says the most.
    let report = cut(report.trim_start(), MAX_REPORT);
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let nonce = solve(report, note, contact, time, bits);
    let body = serde_json::json!({
        "report": report, "note": note, "contact": contact, "time": time, "nonce": nonce,
    })
    .to_string();
    let url = Url::parse(url)?;
    let headers = [("Content-Type", "application/json".to_owned()), ("Accept", "application/json".to_owned())];
    let resp = block_on(http::request("POST", &url, &headers, body.as_bytes(), None), Duration::from_secs(60))?;
    let status: String = serde_json::from_slice::<serde_json::Value>(&resp.body)
        .ok()
        .and_then(|v| v["status"].as_str().map(str::to_owned))
        .unwrap_or_default();
    match (resp.status, status.as_str()) {
        (200, _) | (409, _) => Ok(()),
        (413, _) => Err(GhulError::InvalidInput { msg: "the report is too long".into() }),
        (429, _) => {
            Err(GhulError::Network { msg: "too many reports from here right now, please try again in an hour".into() })
        }
        (400, "clock") => Err(GhulError::InvalidInput { msg: "the clock of this device is wrong".into() }),
        (400, _) | (415, _) => Err(GhulError::InvalidInput { msg: "the report was refused".into() }),
        _ => Err(GhulError::Network { msg: "the report service is not available right now".into() }),
    }
}

fn plausible_mail(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else { return false };
    s.len() <= 200
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !s.chars().any(|c| c.is_whitespace() || c.is_control() || "<>\"',;".contains(c))
}

/// At most `max` bytes, cut at a character boundary.
fn cut(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn content_hash(report: &str, note: &str, contact: &str) -> String {
    let mut h = Sha256::new();
    h.update(report.as_bytes());
    h.update([0]);
    h.update(note.as_bytes());
    h.update([0]);
    h.update(contact.as_bytes());
    crate::kdf::hex(&h.finalize())
}

fn zero_bits(hash: &[u8]) -> u32 {
    let mut n = 0;
    for b in hash {
        if *b == 0 {
            n += 8;
        } else {
            return n + b.leading_zeros();
        }
    }
    n
}

fn proof(time: u64, content: &str, nonce: &str) -> [u8; 32] {
    Sha256::digest(format!("ghulsuite-report|{time}|{content}|{nonce}").as_bytes()).into()
}

/// The nonce that makes the proof start with `bits` zero bits.
fn solve(report: &str, note: &str, contact: &str, time: u64, bits: u32) -> String {
    let content = content_hash(report, note, contact);
    let mut n: u64 = 0;
    loop {
        let nonce = n.to_string();
        if zero_bits(&proof(time, &content, &nonce)) >= bits {
            return nonce;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const REPORT: &str = "GhulSuite 0.5.0-dev.1\nghul-core 0.5.0\nAndroid 15\n\n-";

    #[test]
    fn proofs_count_leading_zero_bits() {
        assert_eq!(zero_bits(&[0, 0, 0x0f, 0xff]), 20);
        assert_eq!(zero_bits(&[0x80]), 0);
        assert_eq!(zero_bits(&[0x01]), 7);
        let nonce = solve(REPORT, "note", "", 1_700_000_000, 12);
        assert!(zero_bits(&proof(1_700_000_000, &content_hash(REPORT, "note", ""), &nonce)) >= 12);
        // The proof belongs to this content and time: changed, it no longer fits (almost surely).
        let changed = (0..20).filter(|i| {
            let note = format!("note {i}");
            zero_bits(&proof(1_700_000_000, &content_hash(REPORT, &note, ""), &nonce)) >= 12
        });
        assert!(changed.count() <= 1);
        assert_eq!(cut("äää", 3), "ä");
        assert!(plausible_mail("a.b@example.org"));
        for bad in ["a", "@x.de", "a@x", "a b@x.de", "a@x.de\r\nBcc: c@d.de", "a@.de", "<a@x.de>"] {
            assert!(!plausible_mail(bad), "{bad}");
        }
    }

    /// A server on this computer: answers `status` and passes on what it got.
    fn server(code: u16, status: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = c.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(p) = text.find("\r\n\r\n") {
                    let len: usize = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if got.len() >= p + 4 + len {
                        tx.send(text[p + 4..].to_string()).unwrap();
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let body = format!("{{\"status\":\"{status}\"}}");
            let _ = write!(
                c,
                "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
        });
        (format!("http://127.0.0.1:{port}/report.php"), rx)
    }

    #[test]
    fn a_report_carries_note_contact_and_a_valid_proof() {
        let (url, rx) = server(200, "ok");
        send_to(&url, REPORT, "  Beim Öffnen von Mails stürzt es ab.  ", "ich@example.org", 12).unwrap();
        let v: serde_json::Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!(v["report"], REPORT);
        assert_eq!(v["note"], "Beim Öffnen von Mails stürzt es ab.");
        assert_eq!(v["contact"], "ich@example.org");
        let content = content_hash(REPORT, "Beim Öffnen von Mails stürzt es ab.", "ich@example.org");
        let p = proof(v["time"].as_u64().unwrap(), &content, v["nonce"].as_str().unwrap());
        assert!(zero_bits(&p) >= 12);
    }

    #[test]
    fn answers_of_the_server_become_clear_messages() {
        for (code, status, want) in [
            (409, "already_sent", None),
            (429, "busy", Some("too many reports from here right now, please try again in an hour")),
            (400, "clock", Some("the clock of this device is wrong")),
            (400, "proof", Some("the report was refused")),
            (413, "too_large", Some("the report is too long")),
            (503, "mail", Some("the report service is not available right now")),
        ] {
            let (url, _rx) = server(code, status);
            let r = send_to(&url, REPORT, "", "", 8);
            match want {
                None => assert!(r.is_ok(), "{code}"),
                Some(msg) => assert!(format!("{:?}", r.unwrap_err()).contains(msg), "{code} {status}"),
            }
        }
        // Checked before anything is sent.
        assert!(send_to("http://127.0.0.1:9/x", REPORT, &"x".repeat(4001), "", 8).is_err());
        assert!(send_to("http://127.0.0.1:9/x", REPORT, "", "no address", 8).is_err());
    }
}
