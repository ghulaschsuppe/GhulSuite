// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quick fuzzing of every parser that reads data from outside (servers, mails, files, links,
//! other apps). Thousands of random inputs and mutations of real examples per parser, on every
//! test run. Promise Z3 of docs/TESTKONZEPT.md: whatever comes in, no crash – an error at most.
//! (Long runs with libFuzzer live in `core/fuzz`.)

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Deterministic random numbers (xorshift); the same inputs on every run.
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
}

/// Snippets that often break parsers.
const DICT: &[&str] = &[
    "\r\n",
    "\n",
    "\r",
    "\0",
    "\u{20ac}",
    "\u{202e}",
    "\u{feff}",
    "\u{1f468}\u{200d}\u{1f469}",
    "%",
    "%00",
    "\"",
    "'",
    "\\",
    ";",
    ":",
    ",",
    "=",
    "==",
    "?",
    "&",
    "#",
    "/",
    "../",
    "<",
    ">",
    "</",
    "<script>",
    "-->",
    "]]>",
    "0",
    "-1",
    "99999999999999999999",
    "18446744073709551616",
    "4294967296",
    "ffffffffffffffff",
    "0x",
    "NaN",
    "1e999",
    "=?utf-8?b?",
    "=?iso-8859-1?q?=FC?=",
    "BEGIN:VEVENT",
    "END:VCALENDAR",
    "RRULE:",
    "DTSTART:",
    "Content-Type: multipart/mixed; boundary=",
    "--",
    "base64",
    "chunked",
    "Transfer-Encoding: chunked",
    "Content-Length: ",
    "{",
    "}",
    "[",
    "]",
    "null",
    "true",
    "\t",
    " ",
    "  ",
    "\u{00a0}",
    "\u{2028}",
    "\u{d7ff}",
];

fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut d = seed.to_vec();
    for _ in 0..1 + rng.below(4) {
        match rng.below(9) {
            0 if !d.is_empty() => {
                let i = rng.below(d.len());
                d[i] ^= 1 << rng.below(8);
            }
            1 if !d.is_empty() => {
                let i = rng.below(d.len());
                d[i] = rng.next() as u8;
            }
            2 if !d.is_empty() => {
                let a = rng.below(d.len());
                let b = (a + rng.below(64)).min(d.len());
                d.drain(a..b);
            }
            3 if !d.is_empty() => {
                let a = rng.below(d.len());
                let b = (a + rng.below(256)).min(d.len());
                let part = d[a..b].to_vec();
                let at = rng.below(d.len());
                for _ in 0..1 + rng.below(3) {
                    d.splice(at..at, part.iter().copied());
                }
            }
            4 => {
                let w = DICT[rng.below(DICT.len())].as_bytes();
                let at = rng.below(d.len() + 1);
                d.splice(at..at, w.iter().copied());
            }
            5 if !d.is_empty() => d.truncate(rng.below(d.len())),
            6 => {
                let n = rng.below(32);
                let at = rng.below(d.len() + 1);
                let junk: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                d.splice(at..at, junk);
            }
            7 if !d.is_empty() => {
                // Swap two regions (out-of-order data).
                let a = rng.below(d.len());
                let b = rng.below(d.len());
                d.swap(a, b);
            }
            _ => {
                // A long run of one byte (huge numbers, long lines).
                let at = rng.below(d.len() + 1);
                let b = if rng.below(2) == 0 { b'9' } else { b'A' };
                d.splice(at..at, std::iter::repeat(b).take(rng.below(5000)));
            }
        }
    }
    d
}

fn naughty() -> Vec<String> {
    let path = format!("{}/tests/data/blns.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Feeds `f` with the seeds, every naughty string, random bytes and `rounds` mutations.
pub(crate) fn fuzz(name: &str, seeds: &[Vec<u8>], rounds: usize, f: impl Fn(&[u8])) {
    let mut rng = Rng(0x5EED ^ name.len() as u64 ^ (name.as_bytes()[0] as u64) << 32);
    let mut inputs: Vec<Vec<u8>> = seeds.to_vec();
    inputs.extend(naughty().into_iter().map(String::into_bytes));
    inputs.push(Vec::new());
    for _ in 0..rounds / 10 {
        let n = rng.below(512);
        inputs.push((0..n).map(|_| rng.next() as u8).collect());
    }
    for _ in 0..rounds {
        let seed = &seeds[rng.below(seeds.len())];
        inputs.push(mutate(&mut rng, seed));
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut crash = None;
    for input in &inputs {
        if catch_unwind(AssertUnwindSafe(|| f(input))).is_err() {
            crash = Some(input.clone());
            break;
        }
    }
    std::panic::set_hook(hook);
    if let Some(input) = crash {
        let shown: String = String::from_utf8_lossy(&input[..input.len().min(400)]).escape_debug().collect();
        // Run again outside catch_unwind so the real panic message shows up.
        eprintln!("{name}: crashed on input ({} bytes): \"{shown}\"", input.len());
        f(&input);
        panic!("{name}: crash did not repeat");
    }
}

fn s(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

pub(crate) fn seeds(list: &[&str]) -> Vec<Vec<u8>> {
    list.iter().map(|x| x.as_bytes().to_vec()).collect()
}

#[test]
fn http_responses() {
    let seeds = seeds(&[
        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhallo",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n4;ext=1\r\ndefg\r\n0\r\n\r\n",
        "HTTP/1.1 302 Found\r\nLocation: https://example.org/x\r\n\r\n",
        "HTTP/1.0 404 Not Found\r\n\r\n",
    ]);
    fuzz("http", &seeds, 6000, |d| {
        let _ = crate::net::http::parse(d);
    });
    // Found by the audit: a chunk without its closing CRLF.
    assert!(crate::net::http::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc").is_err());
    assert!(
        crate::net::http::parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nx").is_err()
    );
}

#[test]
fn smtp_reply_lines() {
    let seeds =
        seeds(&["220 mail.example.org ESMTP\r\n", "250-PIPELINING\r\n", "250 OK\r\n", "535 5.7.8 nope\r\n", "250\r\n"]);
    fuzz("smtp", &seeds, 4000, |d| {
        let _ = crate::net::smtp::reply_line(&s(d));
    });
    // Found by the audit: a multi-byte character right after the code (cleartext before STARTTLS).
    assert_eq!(crate::net::smtp::reply_line("ab\u{20ac}x"), None);
    assert_eq!(crate::net::smtp::reply_line("25\u{20ac}"), None);
    assert_eq!(crate::net::smtp::reply_line("250-x"), Some((250, false)));
    assert_eq!(crate::net::smtp::reply_line("250 x"), Some((250, true)));
}

#[test]
fn ftp_replies_and_listings() {
    let seeds = seeds(&[
        "227 Entering Passive Mode (192,168,1,2,19,137)",
        "229 Entering Extended Passive Mode (|||6446|)",
        "drwxr-xr-x    2 user  group     4096 Nov 13 12:00 Ordner mit Leerzeichen",
        "-rw-r--r--    1 user  group  1234567 Jan  1  2025 suite.ghlsuite",
        "type=file;size=1234;modify=20261113120000; datei.ghlpss",
        "type=cdir; .",
    ]);
    fuzz("ftp", &seeds, 4000, |d| {
        let t = s(d);
        let _ = crate::net::ftps::data_port(227, &t);
        let _ = crate::net::ftps::data_port(229, &t);
        let _ = crate::net::ftps::parse_list_line(&t);
        let _ = crate::net::ftps::parse_mlsd_line(&t);
    });
    // Numbers above 255 are no valid port bytes (used to overflow).
    assert_eq!(crate::net::ftps::data_port(227, "227 (1,2,3,4,300,300)"), None);
}

#[test]
fn html_mails() {
    let seeds = seeds(&[
        "<html><head><style>p{color:red}</style></head><body><p>Hallo <b>Welt</b> &amp; &#x1F600; &nbsp;<a href=\"https://example.org\">Link</a><img src=\"https://t.example/p.gif\" width=1 height=1><table><tr><td>1</td></tr></table></body></html>",
        "<div onclick=\"alert(1)\">x</div><script>alert(1)</script><!-- c --><a href=\"javascript:alert(1)\">y</a>",
        "<p>unclosed <b><i>tags <br/> &unknown; &#99999999; &#x;",
    ]);
    fuzz("html", &seeds, 4000, |d| {
        let t = s(d);
        let _ = crate::app::mailhtml::parse(&t);
        let _ = crate::app::mailhtml::decode_entities(&t);
    });
}

#[test]
fn calendar_files_and_quick_entries() {
    let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:1@x\r\nSUMMARY:Zahnarzt\\, Dr. Weber\r\nDTSTART;TZID=Europe/Berlin:20261113T100000\r\nDTEND:20261113T104500Z\r\nRRULE:FREQ=WEEKLY;COUNT=10;BYDAY=MO,WE\r\nEXDATE:20261120T100000\r\nDURATION:PT45M\r\nLOCATION:Praxis\r\nDESCRIPTION:Lange\r\n  Zeile\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Geburtstag\r\nDTSTART;VALUE=DATE:20000229\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    // Time zone rules as Outlook and tzurl.org write them: the device does not know these names,
    // so the rules of the file are parsed and used.
    let zoned = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Eastern Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:19870405T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=8,9,10,11,12,13,14;BYDAY=SU;UNTIL=20261101T060000Z\r\nRDATE:19740106T020000,19750223T020000\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nDTSTART:19181027T020000\r\nTZOFFSETFROM:-035602\r\nTZOFFSETTO:-0500\r\nRRULE:FREQ=YEARLY;BYYEARDAY=-7,-6,-5,-4,-3,-2,-1;BYDAY=SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:2@x\r\nSUMMARY:Call\r\nDTSTART;TZID=\"Eastern Standard Time\":20260308T023000\r\nDTEND;TZID=Eastern Standard Time:20260308T033000\r\nRRULE:FREQ=DAILY;UNTIL=20260401T000000Z\r\nEXDATE;TZID=Eastern Standard Time:20260310T023000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    fuzz("ics", &seeds(&[ics, zoned]), 4000, |d| {
        let _ = crate::app::caltext::ics_import(&s(d), 60);
        let _ = crate::app::caltext::parse_duration(&s(d));
    });
    let quick = seeds(&[
        "Morgen 14 Uhr Zahnarzt #Privat",
        "Fr 9:30-10 Meeting",
        "24.12. ganztägig Weihnachten",
        "in 3 Tagen 18h Kino",
    ]);
    fuzz("quick", &quick, 3000, |d| {
        let _ = crate::app::caltext::cal_quick_parse(s(d), "2026-11-13".into(), 600);
    });
    // Absurd "today" values must not crash either.
    for today in ["", "2026-02-30", "9999-12-31", "0001-01-01", "x", "2026-13-01", "2026-11-13T25:00"] {
        let _ = crate::app::caltext::cal_quick_parse("morgen 25 Uhr".into(), today.into(), 99_999);
    }
}

#[test]
fn imports_links_and_codes() {
    let csv = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\"\n\"Root\",\"Bank\",\"me\",\"s3cret\",\"https://bank.example\",\"\",\"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\"\n";
    let json = r#"{"encrypted":false,"items":[{"type":1,"name":"Site","login":{"username":"u","password":"p","uris":[{"uri":"https://s.example"}],"totp":"otpauth://totp/S:u?secret=GEZDGNBVGY3TQOJQ"}},{"type":2,"name":"Note","notes":"n"}]}"#;
    let aegis = r#"{"version":1,"header":{"slots":null,"params":null},"db":{"version":2,"entries":[{"type":"totp","name":"a","issuer":"X","info":{"secret":"GEZDGNBVGY3TQOJQ","algo":"SHA1","digits":6,"period":30}},{"type":"steam","name":"s","issuer":"Steam","info":{"secret":"GEZDGNBVGY3TQOJQ","digits":5,"period":30}}]}}"#;
    let otp = "otpauth://totp/Example:alice@example.org?secret=JBSWY3DPEHPK3PXP&issuer=Example&digits=6&period=30&algorithm=SHA256";
    let migration = "otpauth-migration://offline?data=CjEKCkhlbGxvId6tvu8SGEV4YW1wbGU6YWxpY2VAZXhhbXBsZS5vcmcaB0V4YW1wbGUgASgBMAIQARgBIAA%3D";
    for (name, seed) in [("x.csv", csv), ("x.json", json), ("aegis.json", aegis), ("x.txt", otp)] {
        fuzz(name, &seeds(&[seed]), 1500, |d| {
            let _ = crate::app::import::parse(d, name);
        });
    }
    fuzz("otpauth", &seeds(&[otp, "steam://JBSWY3DPEHPK3PXP", migration]), 3000, |d| {
        let _ = crate::app::tools::parse_otpauth_uri(s(d));
        let _ = crate::app::import::parse_migration(&s(d));
    });
}

#[test]
fn keys_codes_and_containers() {
    let cbor = {
        let v = ciborium::value::Value::Map(vec![(
            ciborium::value::Value::Text("a".into()),
            ciborium::value::Value::Array(vec![
                ciborium::value::Value::Bytes(vec![1, 2, 3]),
                ciborium::value::Value::Integer(7.into()),
            ]),
        )]);
        let mut out = Vec::new();
        ciborium::into_writer(&v, &mut out).unwrap();
        out
    };
    fuzz("cbor", &[cbor], 4000, |d| {
        let _ = crate::cbor::decode(d);
    });
    let vectors: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{}/../../docs/testvectors/container-v1.json", env!("CARGO_MANIFEST_DIR")))
            .unwrap(),
    )
    .unwrap();
    let containers: Vec<Vec<u8>> = vectors["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let h = v["file"].as_str().unwrap();
            (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
        })
        .collect();
    fuzz("container", &containers, 3000, |d| {
        if let Ok(c) = crate::vault::LockedContainer::parse(d) {
            let _ = c.fido2_requests();
            let _ = c.keyfile_slots();
        }
    });
    fuzz("recovery code", &seeds(&["ABCD-EFGH-JKLM-NPQR-STUV-WXYZ-2345-6789"]), 3000, |d| {
        let _ = crate::vault::RecoveryCode::parse(&s(d));
    });
    let kf = crate::app::suite::generate_keyfile();
    fuzz("key file", &[kf], 3000, |d| {
        let _ = crate::vault::Keyfile::parse(d);
    });
}

#[test]
fn shares_and_mail_keys() {
    let meta =
        crate::share::ShareMeta { name: "a.txt".into(), mime: "text/plain".into(), size: 6, created: 1, expires: 0 };
    let sealed = crate::share::seal(&meta, b"Inhalt", Some("pw"));
    let key: [u8; 32] = *sealed.link_key;
    // Without the password: PBKDF2 with the real iteration count would make every try slow
    // (the parser is the same; the iteration count itself is bounded by MAX_ITERATIONS).
    fuzz("share", std::slice::from_ref(&sealed.data), 2000, |d| {
        let _ = crate::share::open(d, &key, None);
        let _ = crate::share::b64url_decode(&s(d));
    });
    fuzz("autocrypt", &seeds(&["addr=alice@example.org; prefer-encrypt=mutual; keydata=mDMEZ0123456789"]), 2000, |d| {
        let _ = crate::app::mailcrypt::parse_autocrypt(&s(d));
    });
}

#[test]
fn autofill_domains() {
    let seeds = seeds(&[
        "https://user:pw@www.Example.co.uk:8443/login?x=1#y",
        "192.168.1.1",
        "[::1]:8080",
        "xn--bcher-kva.example",
        "androidapp://de.bank.app?cert=AABB",
    ]);
    fuzz("autofill", &seeds, 4000, |d| {
        let t = s(d);
        if let Some(h) = crate::app::autofill::host_of(&t) {
            let _ = crate::app::autofill::base_domain(&h);
        }
        let urls = vec![t.clone(), "https://example.org".into()];
        let _ = crate::app::autofill::match_entry(&urls, Some(&t), None, None);
        let _ = crate::app::autofill::match_entry(&urls, None, Some(&t), Some(&t));
    });
}
