// SPDX-License-Identifier: AGPL-3.0-or-later
//! Matching of logins for autofill: which entries fit a website or an app.
//!
//! Websites match by their registrable domain after the Public Suffix List (`login.example.com`
//! fits `example.com`; `example.com.evil.net`, `evil.github.io` for `alice.github.io` and other IP
//! addresses do not) and are filled directly.
//!
//! Apps are filled directly only if the entry links them with their signing certificate
//! (`androidapp://<package>?cert=<sha256 hex>`), which the app sets when the user picks a login by
//! hand. A link without certificate (e.g. imported) or a package name that only fits the entry's
//! domain (`com.github.android` → `github.com`) is merely *suggested*, because any app can choose
//! such a package name. A link with a different certificate excludes the entry.

/// Lower-case host of a URL or bare domain, without `www.`, port, path and credentials.
pub(crate) fn host_of(url: &str) -> Option<String> {
    // Like a browser (WHATWG URL standard): tabs and line breaks inside are ignored, and a
    // backslash ends the host just like "/", "https://evil.com\\@bank.de" goes to evil.com.
    let cleaned: String = url.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let s = cleaned.trim_matches(|c: char| c <= ' ');
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.trim_start_matches(['/', '\\']);
    let s = s.split(['/', '\\', '?', '#']).next()?;
    let s = s.rsplit_once('@').map_or(s, |(_, h)| h);
    let s = s.split(':').next()?;
    let host = s.trim_end_matches('.').to_lowercase();
    // Numbers in any spelling a browser accepts (0xC0A80001, 192.168.1, 0300.0250.0.1) are the
    // same address as 192.168.0.1.
    if let Some(ip) = ipv4_like_a_browser(&host) {
        return ip;
    }
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    if host.is_empty() || !host.contains('.') || host.contains(' ') {
        return None;
    }
    Some(host)
}

/// IPv4 parsing of the WHATWG URL standard: `None` if the host is no IPv4 address at all,
/// `Some(None)` if it looks like one but is invalid, else the dotted address.
fn ipv4_like_a_browser(host: &str) -> Option<Option<String>> {
    let parts: Vec<&str> = host.split('.').collect();
    let number = |p: &str| -> Option<u64> {
        let (digits, radix) = if let Some(h) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
            (h, 16)
        } else if p.len() > 1 && p.starts_with('0') {
            (&p[1..], 8)
        } else {
            (p, 10)
        };
        if digits.is_empty() {
            return Some(0);
        }
        u64::from_str_radix(digits, radix).ok().filter(|_| digits.len() <= 20)
    };
    // Only when the last part is a number is the host an IPv4 address.
    let last = parts.last()?;
    if last.is_empty() || number(last).is_none() {
        return None;
    }
    if parts.len() > 4 || parts.iter().any(|p| p.is_empty()) {
        return Some(None);
    }
    let nums: Option<Vec<u64>> = parts.iter().map(|p| number(p)).collect();
    let Some(nums) = nums else { return Some(None) };
    let (head, tail) = nums.split_at(nums.len() - 1);
    if head.iter().any(|&n| n > 255) {
        return Some(None);
    }
    let limit = 256u64.pow(5 - nums.len() as u32);
    if tail[0] >= limit {
        return Some(None);
    }
    let mut value = tail[0];
    for (i, &n) in head.iter().enumerate() {
        value += n * 256u64.pow(3 - i as u32);
    }
    let b = (value as u32).to_be_bytes();
    Some(Some(format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])))
}

/// The registrable part of a host, after the Public Suffix List: `a.b.example.com` →
/// `example.com`, `x.example.co.uk` → `example.co.uk`, `alice.github.io` → `alice.github.io`
/// (github.io is a suffix where everyone gets their own name). IP addresses stay exactly as they
/// are, as do hosts without a registrable part.
pub(crate) fn base_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_lowercase();
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    match psl::domain_str(&host) {
        Some(d) => d.to_owned(),
        None => host,
    }
}

/// Guesses a website from an Android package name (`com.github.android` → `github.com`).
pub(crate) fn domain_of_package(package: &str) -> Option<String> {
    let parts: Vec<&str> = package.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!("{}.{}", parts[1], parts[0]).to_lowercase())
}

/// How well an entry fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Match {
    /// Fill directly: website with the same base domain, or app linked with its certificate.
    Direct,
    /// Offer only in the picker (needs a deliberate choice).
    Suggested,
    None,
}

/// The `androidapp://` link of an app with its signing certificate.
pub(crate) fn app_link(package: &str, cert: Option<&str>) -> String {
    match cert {
        Some(c) if !c.is_empty() => format!("androidapp://{}?cert={}", package.trim(), c.trim().to_lowercase()),
        _ => format!("androidapp://{}", package.trim()),
    }
}

/// Package and certificate of an `androidapp://` URL.
fn parse_app_link(url: &str) -> Option<(String, Option<String>)> {
    let rest = url.trim().strip_prefix("androidapp://")?;
    let (pkg, query) = rest.split_once('?').unwrap_or((rest, ""));
    let cert = query.split('&').find_map(|kv| kv.strip_prefix("cert=")).map(|c| c.to_lowercase());
    Some((pkg.to_owned(), cert))
}

/// How an entry with `urls` fits the website `domain` or the app `package` with certificate `cert`.
pub(crate) fn match_entry(urls: &[String], domain: Option<&str>, package: Option<&str>, cert: Option<&str>) -> Match {
    if let Some(host) = domain.and_then(host_of) {
        let target = base_domain(&host);
        let hit = urls
            .iter()
            .filter(|u| !u.starts_with("androidapp://"))
            .filter_map(|u| host_of(u))
            .any(|h| base_domain(&h) == target);
        return if hit { Match::Direct } else { Match::None };
    }
    let Some(package) = package else { return Match::None };
    let cert = cert.map(str::to_lowercase);
    let mut result = Match::None;
    for (pkg, link_cert) in urls.iter().filter_map(|u| parse_app_link(u)) {
        if pkg != package {
            continue;
        }
        match (link_cert, &cert) {
            (Some(l), Some(c)) if &l == c => return Match::Direct,
            // Linked to another signing certificate: possibly a fake app.
            (Some(_), _) => return Match::None,
            (None, _) => result = Match::Suggested,
        }
    }
    if result == Match::None {
        if let Some(d) = domain_of_package(package) {
            if urls.iter().filter_map(|u| host_of(u)).any(|h| base_domain(&h) == d) {
                result = Match::Suggested;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn hosts_and_domains() {
        assert_eq!(host_of("https://user:pw@www.Example.com:8443/login?x=1").as_deref(), Some("example.com"));
        assert_eq!(host_of("example.org").as_deref(), Some("example.org"));
        assert_eq!(host_of("not a url"), None);
        assert_eq!(base_domain("a.b.example.com"), "example.com");
        assert_eq!(base_domain("login.example.co.uk"), "example.co.uk");
        assert_eq!(domain_of_package("com.github.android").as_deref(), Some("github.com"));
    }

    /// The official test cases of the Public Suffix List (tests/data/test_psl.txt).
    #[test]
    fn public_suffix_list_test_cases() {
        let text = std::fs::read_to_string(format!("{}/tests/data/test_psl.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let mut checked = 0;
        for line in text.lines().map(str::trim).filter(|l| l.starts_with("checkPublicSuffix(")) {
            let args = line.trim_start_matches("checkPublicSuffix(").trim_end_matches(");");
            let (input, want) = args.split_once(", ").unwrap();
            let unquote = |x: &str| (x != "null").then(|| x.trim_matches('\'').to_owned());
            let (Some(input), want) = (unquote(input), unquote(want)) else { continue };
            // Leading dots and non-ASCII names are not hosts the app ever sees in this form.
            if input.starts_with('.') || !input.is_ascii() {
                continue;
            }
            let got = super::base_domain(&input);
            match want {
                Some(w) => assert_eq!(got, w, "{input}"),
                // No registrable part: the host itself (only an exact match fills).
                None => assert_eq!(got, input.to_lowercase(), "{input}"),
            }
            checked += 1;
        }
        assert!(checked > 40, "{checked}");
    }

    /// Who may get a login for `https://login.bank.example`, `alice.github.io`, a router ...
    #[test]
    fn phishing_matrix() {
        let cases: &[(&str, &str, Match)] = &[
            ("https://login.bank.de", "bank.de", Match::Direct),
            ("https://login.bank.de", "www.bank.de", Match::Direct),
            ("https://login.bank.de", "BANK.DE.", Match::Direct),
            ("https://login.bank.de", "bank.de.evil.com", Match::None),
            ("https://login.bank.de", "bank-de.com", Match::None),
            ("https://login.bank.de", "xn--bank-9na.de", Match::None),
            ("https://login.bank.de", "evil.com", Match::None),
            ("https://alice.github.io", "evil.github.io", Match::None),
            ("https://alice.github.io", "alice.github.io", Match::Direct),
            ("https://nas.synology.me", "evil.synology.me", Match::None),
            ("https://box.myfritz.net", "other.myfritz.net", Match::None),
            ("https://shop.co.uk", "evil.co.uk", Match::None),
            ("https://a.example.co.uk", "b.example.co.uk", Match::Direct),
            ("http://192.168.1.1/", "192.168.1.1", Match::Direct),
            ("http://192.168.1.1/", "10.0.1.1", Match::None),
            ("http://192.168.1.1/", "203.0.1.1", Match::None),
            ("https://user:pw@bank.de:8443/x", "bank.de", Match::Direct),
            ("https://bank.de", "", Match::None),
            ("bank.de", "bank.de", Match::Direct),
        ];
        for (url, domain, want) in cases {
            assert_eq!(match_entry(&u(&[url]), Some(domain), None, None), *want, "{url} vs {domain}");
        }
    }

    #[test]
    fn matching() {
        let gh = u(&["https://github.com/login"]);
        assert_eq!(match_entry(&gh, Some("gist.github.com"), None, None), Match::Direct);
        assert_eq!(match_entry(&gh, Some("github.com.evil.net"), None, None), Match::None);
        assert_eq!(match_entry(&gh, Some("notgithub.com"), None, None), Match::None);
        // A package name alone is never enough to fill directly.
        assert_eq!(match_entry(&gh, None, Some("com.github.android"), Some("aa")), Match::Suggested);
        assert_eq!(match_entry(&gh, None, Some("com.example.app"), None), Match::None);
        let linked = u(&["https://bank.example", "androidapp://de.bank.app?cert=AABB"]);
        assert_eq!(match_entry(&linked, None, Some("de.bank.app"), Some("aabb")), Match::Direct);
        assert_eq!(match_entry(&linked, None, Some("de.bank.app"), Some("ccdd")), Match::None);
        assert_eq!(match_entry(&linked, None, Some("de.bank.app"), None), Match::None);
        assert_eq!(
            match_entry(&u(&["androidapp://de.bank.app"]), None, Some("de.bank.app"), Some("aabb")),
            Match::Suggested
        );
        assert_eq!(match_entry(&gh, None, None, None), Match::None);
        assert_eq!(app_link("de.bank.app", Some("AABB")), "androidapp://de.bank.app?cert=aabb");
    }
}
