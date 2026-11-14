// SPDX-License-Identifier: AGPL-3.0-or-later
//! The TLS certificate verifier against x509-limbo (https://github.com/C2SP/x509-limbo), a large
//! suite of certificate path validation test cases. Runs at night:
//!
//! `GHUL_LIMBO=limbo.json cargo test -p ghul-core --lib -- --ignored --nocapture limbo`
//!
//! Every SERVER test case with a DNS or IP peer name goes through the same verifier the app uses
//! (`tls::Verifier`, only with the test case's roots instead of the Mozilla ones). The test fails
//! only when a certificate that must be refused is accepted and the case is not listed in
//! `tests/limbo-known.txt`. Wrong rejects (too strict) are only reported. The summary goes to
//! stdout and to `GHUL_LIMBO_REPORT` (default: `limbo-report.txt` in the temp directory).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use serde_json::Value;

use super::tls::verifier_with_roots;
use crate::app::caldate::Date;

/// RFC 3339 time as used by limbo ("2024-01-01T00:00:00Z" or with "+00:00"), to Unix seconds.
fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let days = Date::parse(&s[..10])?.days();
    let (h, m, sec): (i64, i64, i64) = (s[11..13].parse().ok()?, s[14..16].parse().ok()?, s[17..19].parse().ok()?);
    let mut rest = &s[19..];
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ if rest.len() == 6 && rest.as_bytes()[3] == b':' => {
            let sign = match rest.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            sign * (rest[1..3].parse::<i64>().ok()? * 3600 + rest[4..6].parse::<i64>().ok()? * 60)
        }
        _ => return None,
    };
    u64::try_from(days * 86400 + h * 3600 + m * 60 + sec - offset).ok()
}

/// IDs of known wrong accepts with their reason (`id  # reason`, `#` starts a comment).
fn known_wrong_accepts() -> BTreeMap<String, String> {
    let text = include_str!("../../tests/limbo-known.txt");
    text.lines()
        .filter_map(|l| {
            let (id, reason) = l.split_once('#').unwrap_or((l, ""));
            let id = id.trim();
            (!id.is_empty()).then(|| (id.to_owned(), reason.trim().to_owned()))
        })
        .collect()
}

enum Outcome {
    Skip(&'static str),
    Accept,
    Reject(String),
}

fn certs(v: &Value) -> Vec<CertificateDer<'static>> {
    v.as_array()
        .map(|a| a.iter().filter_map(|p| CertificateDer::from_pem_slice(p.as_str()?.as_bytes()).ok()).collect())
        .unwrap_or_default()
}

fn run_case(t: &Value) -> Outcome {
    if t["validation_kind"] != "SERVER" {
        return Outcome::Skip("not a server certificate");
    }
    let features: Vec<&str> = t["features"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if features.contains(&"has-crl") {
        // GhulSuite does not check revocation (like most TLS clients without OCSP stapling).
        return Outcome::Skip("needs CRL checking");
    }
    if !t["max_chain_depth"].is_null() {
        return Outcome::Skip("needs a configured maximum chain depth");
    }
    let peer = &t["expected_peer_name"];
    let value = peer["value"].as_str().unwrap_or_default();
    let name = match peer["kind"].as_str() {
        Some("DNS") => match ServerName::try_from(value.to_owned()) {
            Ok(n) => n,
            Err(e) => return Outcome::Reject(format!("peer name: {e}")),
        },
        Some("IP") => match value.parse::<std::net::IpAddr>() {
            Ok(ip) => ServerName::IpAddress(ip.into()),
            Err(e) => return Outcome::Reject(format!("peer IP: {e}")),
        },
        _ => return Outcome::Skip("peer name is not DNS or IP"),
    };
    let now = match t["validation_time"].as_str() {
        None => UnixTime::now(),
        Some(s) => match parse_rfc3339(s) {
            Some(secs) => UnixTime::since_unix_epoch(Duration::from_secs(secs)),
            None => return Outcome::Skip("unreadable validation time"),
        },
    };
    let mut roots = rustls::RootCertStore::empty();
    for c in certs(&t["trusted_certs"]) {
        // An anchor webpki cannot read is simply not trusted.
        let _ = roots.add(c);
    }
    let intermediates = certs(&t["untrusted_intermediates"]);
    let leaf = match t["peer_certificate"].as_str().map(|p| CertificateDer::from_pem_slice(p.as_bytes())) {
        Some(Ok(c)) => c,
        _ => return Outcome::Reject("peer certificate is no PEM certificate".into()),
    };
    let verifier = match verifier_with_roots(roots, None) {
        Ok(v) => v,
        Err(e) => return Outcome::Reject(format!("verifier: {e}")),
    };
    match verifier.verify_server_cert(&leaf, &intermediates, &name, &[], now) {
        Ok(_) => Outcome::Accept,
        Err(e) => Outcome::Reject(e.to_string()),
    }
}

#[test]
fn rfc3339_times_are_read() {
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_rfc3339("2024-01-01T00:00:00+00:00"), Some(1_704_067_200));
    assert_eq!(parse_rfc3339("2024-01-01T01:30:00.5+01:30"), Some(1_704_067_200));
    assert_eq!(parse_rfc3339("2024-01-01 00:00:00Z"), None);
    assert!(known_wrong_accepts().keys().all(|id| !id.contains(char::is_whitespace)));
}

#[test]
#[ignore = "needs limbo.json: GHUL_LIMBO=limbo.json cargo test -p ghul-core -- --ignored limbo"]
fn limbo_certificate_validation() {
    let Ok(path) = std::env::var("GHUL_LIMBO") else {
        eprintln!("GHUL_LIMBO is not set (path to x509-limbo's limbo.json), skipped");
        return;
    };
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let limbo: Value = serde_json::from_slice(&data).expect("limbo.json is not JSON");
    let cases = limbo["testcases"].as_array().expect("limbo.json has no testcases");
    let known = known_wrong_accepts();

    let (mut passed, mut known_accepts) = (0usize, 0usize);
    let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut wrong_accepts, mut wrong_rejects, mut slow, mut panics) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut known_used = std::collections::BTreeSet::new();
    for t in cases {
        let id = t["id"].as_str().unwrap_or("?").to_owned();
        let expect_ok = t["expected_result"] == "SUCCESS";
        let start = Instant::now();
        let outcome = match std::panic::catch_unwind(|| run_case(t)) {
            Ok(o) => o,
            Err(_) => {
                panics.push(id.clone());
                continue;
            }
        };
        if start.elapsed() > Duration::from_secs(1) {
            slow.push(format!("{id} ({} ms)", start.elapsed().as_millis()));
        }
        match (outcome, expect_ok) {
            (Outcome::Skip(why), _) => *skipped.entry(why).or_default() += 1,
            (Outcome::Accept, true) => passed += 1,
            (Outcome::Reject(_), false) => passed += 1,
            (Outcome::Accept, false) if known.contains_key(&id) => {
                known_accepts += 1;
                known_used.insert(id);
            }
            (Outcome::Accept, false) => wrong_accepts.push(id),
            (Outcome::Reject(e), true) => wrong_rejects.push(format!("{id}: {e}")),
        }
    }

    let mut r = String::new();
    let total_skipped: usize = skipped.values().sum();
    let _ = writeln!(r, "x509-limbo: {} test cases", cases.len());
    let _ = writeln!(r, "passed:        {passed}");
    let _ = writeln!(r, "wrong accept:  {} (+ {known_accepts} known, tests/limbo-known.txt)", wrong_accepts.len());
    let _ = writeln!(r, "wrong reject:  {}", wrong_rejects.len());
    let _ = writeln!(r, "panics:        {}", panics.len());
    let _ = writeln!(r, "skipped:       {total_skipped}");
    for (why, n) in &skipped {
        let _ = writeln!(r, "  {n:5}  {why}");
    }
    let mut section = |title: &str, items: &[String]| {
        if !items.is_empty() {
            let _ = writeln!(r, "\n{title}:");
            for i in items {
                let _ = writeln!(r, "  {i}");
            }
        }
    };
    section("WRONG ACCEPT (must be refused)", &wrong_accepts);
    section("PANIC", &panics);
    section("Wrong reject (too strict, only reported)", &wrong_rejects);
    section("Slow (over 1 s)", &slow);
    let unused: Vec<String> = known.keys().filter(|k| !known_used.contains(*k)).cloned().collect();
    section("Known entries no longer accepted (can be removed from limbo-known.txt)", &unused);
    println!("{r}");
    let report = std::env::var("GHUL_LIMBO_REPORT")
        .unwrap_or_else(|_| std::env::temp_dir().join("limbo-report.txt").display().to_string());
    std::fs::write(&report, &r).unwrap_or_else(|e| panic!("{report}: {e}"));

    assert!(
        wrong_accepts.is_empty() && panics.is_empty(),
        "{} certificate(s) accepted that must be refused, {} panic(s): see {report}",
        wrong_accepts.len(),
        panics.len()
    );
}
