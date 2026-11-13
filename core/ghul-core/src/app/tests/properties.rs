// SPDX-License-Identifier: AGPL-3.0-or-later
//! Generated test series: many thousands of single checks against independent references or
//! rules that must always hold (round trips, merge order, calendar arithmetic, RFC vectors).

use super::super::caldate::{days_in_month, is_leap, Date};
use super::super::tools::{base32_decode, generate_password, PasswordOptions};
use crate::totp::{hotp, totp, OtpAlgorithm};

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
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

/// Independent reference: weekday by Zeller's congruence (0 = Monday).
fn zeller(y: i32, m: u32, d: u32) -> u32 {
    let (y, m) = if m < 3 { (y - 1, m + 12) } else { (y, m) };
    let k = y.rem_euclid(100);
    let j = y.div_euclid(100);
    let h = (d as i32 + (13 * (m as i32 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7);
    // h: 0 = Saturday … 6 = Friday → 0 = Monday
    ((h + 5) % 7) as u32
}

#[test]
fn every_day_from_1600_to_2400() {
    let mut checked = 0u64;
    let mut expected_days = Date::new(1600, 1, 1).unwrap().days();
    for y in 1600..=2400 {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        assert_eq!(is_leap(y), leap, "{y}");
        for m in 1..=12u32 {
            let len = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m as usize - 1];
            assert_eq!(days_in_month(y, m), len, "{y}-{m}");
            assert!(Date::new(y, m, len + 1).is_none());
            for d in 1..=len {
                let date = Date::new(y, m, d).unwrap();
                assert_eq!(date.days(), expected_days, "{date:?}");
                assert_eq!(Date::from_days(expected_days), date);
                assert_eq!(date.weekday(), zeller(y, m, d), "{date:?}");
                assert_eq!(Date::parse(&date.to_string()), Some(date));
                expected_days += 1;
                checked += 1;
            }
        }
    }
    assert!(checked > 290_000, "{checked}");
}

#[test]
fn totp_and_hotp_rfc_vectors() {
    // RFC 4226, appendix D (secret "12345678901234567890").
    let s = b"12345678901234567890";
    let hotp_codes =
        ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"];
    for (c, want) in hotp_codes.iter().enumerate() {
        assert_eq!(hotp(s, c as u64, 6, OtpAlgorithm::Sha1).unwrap(), *want);
    }
    // RFC 6238, appendix B.
    let s256 = b"12345678901234567890123456789012";
    let s512 = b"1234567890123456789012345678901234567890123456789012345678901234";
    let table: [(u64, &str, &str, &str); 6] = [
        (59, "94287082", "46119246", "90693936"),
        (1111111109, "07081804", "68084774", "25091201"),
        (1111111111, "14050471", "67062674", "99943326"),
        (1234567890, "89005924", "91819424", "93441116"),
        (2000000000, "69279037", "90698825", "38618901"),
        (20000000000, "65353130", "77737706", "47863826"),
    ];
    for (t, a, b, c) in table {
        assert_eq!(totp(s, t, 30, 8, OtpAlgorithm::Sha1).unwrap(), a, "{t}");
        assert_eq!(totp(s256, t, 30, 8, OtpAlgorithm::Sha256).unwrap(), b, "{t}");
        assert_eq!(totp(s512, t, 30, 8, OtpAlgorithm::Sha512).unwrap(), c, "{t}");
    }
    // Independent truncation (RFC 4226 §5.3) against random secrets and counters.
    use hmac::{Hmac, Mac};
    let mut rng = Rng(77);
    for _ in 0..5000 {
        let len = 10 + rng.below(54) as usize;
        let secret = rng.bytes(len);
        let counter = rng.next();
        let digits = 6 + rng.below(3) as u32;
        let mut m = Hmac::<sha1::Sha1>::new_from_slice(&secret).unwrap();
        m.update(&counter.to_be_bytes());
        let h = m.finalize().into_bytes();
        let o = (h[19] & 0x0f) as usize;
        let bin = u32::from_be_bytes([h[o] & 0x7f, h[o + 1], h[o + 2], h[o + 3]]) % 10u32.pow(digits);
        assert_eq!(
            hotp(&secret, counter, digits, OtpAlgorithm::Sha1).unwrap(),
            format!("{bin:0w$}", w = digits as usize)
        );
    }
    // Period edges.
    for p in [0u64, 3601, u64::MAX] {
        assert!(totp(s, 59, p, 6, OtpAlgorithm::Sha1).is_err(), "{p}");
    }
    for d in [0u32, 5, 9, 10, u32::MAX] {
        assert!(hotp(s, 0, d, OtpAlgorithm::Sha1).is_err(), "{d}");
    }
    assert!(hotp(b"", 0, 6, OtpAlgorithm::Sha1).is_err());
}

/// Independent Base32 encoder (RFC 4648).
fn b32(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(A[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

#[test]
fn base32_round_trips_in_every_spelling() {
    let mut rng = Rng(5);
    for _ in 0..5000 {
        let len = 1 + rng.below(64) as usize;
        let data = rng.bytes(len);
        let e = b32(&data);
        let spaced: String =
            e.chars().enumerate().flat_map(|(i, c)| if i % 4 == 3 { vec![c, ' '] } else { vec![c] }).collect();
        let padded = format!("{e}{}", "=".repeat((8 - e.len() % 8) % 8));
        for variant in [e.clone(), e.to_lowercase(), spaced, padded] {
            assert_eq!(base32_decode(&variant).as_deref().map(|v| v.as_slice()), Some(data.as_slice()), "{variant}");
        }
    }
    for bad in ["1", "0", "8", "9", "!", "AAAA AAA@", "ä"] {
        assert!(base32_decode(bad).is_none(), "{bad}");
    }
}

#[test]
fn recovery_codes_in_every_spelling() {
    use crate::vault::RecoveryCode;
    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(99);
    use rand_core::SeedableRng;
    for i in 0..2000 {
        let code = RecoveryCode::generate(&mut rng);
        let shown = code.to_display().to_string();
        let compact = shown.replace('-', "");
        let spaced = shown.replace('-', " ");
        let variants = [
            shown.clone(),
            shown.to_lowercase(),
            compact.clone(),
            spaced,
            format!("  {shown}\n"),
            shown.replace('0', "O"),
        ];
        for v in variants {
            let back = RecoveryCode::parse(&v).unwrap_or_else(|e| panic!("#{i} {v:?}: {e:?}"));
            assert_eq!(back.to_display().as_str(), shown.as_str());
        }
        // One wrong character is noticed (checksum), not a different code.
        let mut wrong: Vec<char> = compact.chars().collect();
        let at = i % wrong.len();
        wrong[at] = if wrong[at] == 'A' { 'B' } else { 'A' };
        let wrong: String = wrong.into_iter().collect();
        if let Ok(other) = RecoveryCode::parse(&wrong) {
            assert_ne!(other.to_display().as_str(), shown.as_str());
        }
    }
}

#[test]
fn merging_is_independent_of_order_and_repetition() {
    use super::super::sync::merge_entries;
    use crate::vault::{Entry, Field};
    let mut rng = Rng(2026);
    let entry = |rng: &mut Rng, id: u8, rev: u64, deleted: bool| {
        let mut e = Entry::new([id; 16], "login", (rng.below(10_000) + 1) as i64);
        e.revision = rev;
        e.deleted = deleted;
        if !deleted {
            e.fields.insert("title".into(), Field::Text(format!("T{id}-{rev}-{}", rng.below(3))));
        }
        e
    };
    let key = |v: &[Entry]| {
        let mut k: Vec<(Vec<u8>, u64, bool, i64, Vec<String>)> = v
            .iter()
            .map(|e| (e.id.clone(), e.revision, e.deleted, e.modified, e.fields.keys().cloned().collect()))
            .collect();
        k.sort();
        k
    };
    for case in 0..3000 {
        let n = 1 + rng.below(12) as u8;
        let mut a = Vec::new();
        let mut b = Vec::new();
        for id in 0..n {
            let (r1, d1, r2, d2) = (1 + rng.below(5), rng.below(4) == 0, 1 + rng.below(5), rng.below(5) == 0);
            match rng.below(4) {
                0 => a.push(entry(&mut rng, id, r1, d1)),
                1 => b.push(entry(&mut rng, id, r1, d1)),
                _ => {
                    a.push(entry(&mut rng, id, r1, d2));
                    b.push(entry(&mut rng, id, r2, d2));
                }
            }
        }
        let (ab, _, _) = merge_entries(&a, &b);
        let (ba, _, _) = merge_entries(&b, &a);
        assert_eq!(key(&ab), key(&ba), "case {case}: order matters");
        let (again, l, r) = merge_entries(&ab, &ab);
        assert_eq!(key(&again), key(&ab), "case {case}");
        assert!(!l && !r, "case {case}: merging with itself reports a change");
        let (with_a, _, _) = merge_entries(&ab, &a);
        assert_eq!(key(&with_a), key(&ab), "case {case}: merging an older side changes the result");
        // Nothing is lost: every id of either side is in the result.
        for e in a.iter().chain(&b) {
            assert!(ab.iter().any(|x| x.id == e.id), "case {case}: lost an entry");
        }
    }
}

#[test]
fn password_generator_rules_for_every_option() {
    let mut rng = Rng(8);
    for _ in 0..5000 {
        let o = PasswordOptions {
            length: rng.below(140) as u32,
            lowercase: rng.below(2) == 0,
            uppercase: rng.below(2) == 0,
            digits: rng.below(2) == 0,
            symbols: rng.below(2) == 0,
            avoid_ambiguous: rng.below(2) == 0,
        };
        let any = o.lowercase || o.uppercase || o.digits || o.symbols;
        match generate_password(o.clone()) {
            Ok(p) => {
                assert!(any && (8..=128).contains(&o.length));
                assert_eq!(p.chars().count() as u32, o.length);
                assert_eq!(o.lowercase, p.chars().any(|c| c.is_ascii_lowercase()));
                assert_eq!(o.uppercase, p.chars().any(|c| c.is_ascii_uppercase()));
                assert_eq!(o.digits, p.chars().any(|c| c.is_ascii_digit()));
                assert_eq!(o.symbols, p.chars().any(|c| !c.is_ascii_alphanumeric()));
                if o.avoid_ambiguous {
                    assert!(!p.chars().any(|c| "0O1lI|`'\"".contains(c)), "{p}");
                }
            }
            Err(_) => assert!(!any || !(8..=128).contains(&o.length)),
        }
    }
}

#[test]
fn autofill_never_fills_a_lookalike() {
    use super::super::autofill::{match_entry, Match};
    let sites = [
        "bank.de",
        "sparkasse.de",
        "paypal.com",
        "amazon.de",
        "google.com",
        "github.com",
        "microsoft.com",
        "apple.com",
        "ebay.de",
        "dkb.de",
        "ing.de",
        "comdirect.de",
        "web.de",
        "gmx.net",
        "t-online.de",
        "posteo.de",
        "mailbox.org",
        "netflix.com",
        "steampowered.com",
        "dropbox.com",
        "nextcloud.example.org",
        "alice.github.io",
        "nas.synology.me",
        "box.myfritz.net",
        "shop.co.uk",
        "bbc.co.uk",
        "gov.uk",
        "example.com.au",
        "router.local.example",
        "10.0.0.1",
    ];
    let attacks = |site: &str| -> Vec<String> {
        let mut v = vec![
            format!("{site}.evil.com"),
            format!("evil-{site}"),
            format!("{site}-login.com"),
            format!("{}x", site),
            format!("x{site}"),
            site.replacen('.', "-", 1) + ".com",
            site.replace('o', "0"),
            site.replace('l', "1"),
            site.replace('a', "\u{430}"),
            format!("evil.{}", site.split('.').skip(1).collect::<Vec<_>>().join(".")),
            format!("{site}.de"),
            format!("{site}.com"),
            format!("{}.{}", site.split('.').next().unwrap(), "evil"),
        ];
        v.retain(|a| a != site && super::super::autofill::host_of(a).is_some());
        v
    };
    let mut checked = 0;
    for site in sites {
        let urls = vec![format!("https://{site}/login")];
        assert_eq!(match_entry(&urls, Some(site), None, None), Match::Direct, "{site}");
        assert_eq!(match_entry(&urls, Some(&format!("www.{site}")), None, None), Match::Direct, "www.{site}");
        for a in attacks(site) {
            let same = super::super::autofill::base_domain(&super::super::autofill::host_of(&a).unwrap())
                == super::super::autofill::base_domain(site);
            // Only another name under the very same registrable domain may fill.
            assert_eq!(match_entry(&urls, Some(&a), None, None) == Match::Direct, same, "{site} vs {a}");
            // (A name under the same registrable domain, e.g. evil-nextcloud.example.org next to
            // nextcloud.example.org, belongs to the same owner and may fill – that is the rule.)
            checked += 1;
        }
    }
    assert!(checked > 300, "{checked}");
}

#[test]
fn calendar_files_round_trip() {
    use super::super::cal::{CalEvent, CalRepeat};
    use super::super::caltext::{ics_export, ics_import};
    let mut rng = Rng(31);
    let mut events = Vec::new();
    for i in 0..600 {
        let y = 1990 + rng.below(60) as i32;
        let m = 1 + rng.below(12) as u32;
        let d = 1 + rng.below(days_in_month(y, m) as u64) as u32;
        let all_day = rng.below(4) == 0;
        let (start, end) = if all_day {
            let s = Date::new(y, m, d).unwrap();
            let e = Date::from_days(s.days() + rng.below(5) as i64);
            (s.to_string(), e.to_string())
        } else {
            let h = rng.below(23);
            (
                format!("{y:04}-{m:02}-{d:02}T{h:02}:{:02}", rng.below(4) * 15),
                format!("{y:04}-{m:02}-{d:02}T{:02}:00", h + 1),
            )
        };
        events.push(CalEvent {
            id: format!("{i:032x}"),
            title: format!("Termin {i}; mit, Sonderzeichen \\ und \"Zitat\" – äöü 😀"),
            location: if i % 3 == 0 { "Ort, Straße 1".into() } else { String::new() },
            notes: if i % 5 == 0 { "Zeile 1\nZeile 2".into() } else { String::new() },
            all_day,
            start,
            end,
            repeat: [CalRepeat::None, CalRepeat::Daily, CalRepeat::Weekly, CalRepeat::Monthly, CalRepeat::Yearly]
                [rng.below(5) as usize],
            ..Default::default()
        });
    }
    let text = ics_export(&events, "Test");
    let back = ics_import(&text, 0).unwrap();
    assert_eq!(back.len(), events.len());
    for (a, b) in events.iter().zip(&back) {
        assert_eq!(
            (&a.title, &a.location, &a.notes, a.all_day, &a.start, &a.end, a.repeat),
            (&b.title, &b.location, &b.notes, b.all_day, &b.start, &b.end, b.repeat),
            "{a:?}"
        );
    }
}
