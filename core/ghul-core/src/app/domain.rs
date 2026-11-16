// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hosts and registrable domains of URLs, parsed like a browser does.
//!
//! The registrable domain follows the Public Suffix List (`login.example.com` belongs to
//! `example.com`; `example.com.evil.net`, `evil.github.io` for `alice.github.io` and other IP
//! addresses do not). Used to spot mail links whose shown text names another site than the target.

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether two URLs or hosts belong to the same site.
    fn same_site(a: &str, b: &str) -> bool {
        match (host_of(a), host_of(b)) {
            (Some(x), Some(y)) => base_domain(&x) == base_domain(&y),
            _ => false,
        }
    }

    #[test]
    fn hosts_and_domains() {
        assert_eq!(host_of("https://user:pw@www.Example.com:8443/login?x=1").as_deref(), Some("example.com"));
        assert_eq!(host_of("example.org").as_deref(), Some("example.org"));
        assert_eq!(host_of("not a url"), None);
        assert_eq!(base_domain("a.b.example.com"), "example.com");
        assert_eq!(base_domain("login.example.co.uk"), "example.co.uk");
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
                // No registrable part: the host itself (only an exact match counts).
                None => assert_eq!(got, input.to_lowercase(), "{input}"),
            }
            checked += 1;
        }
        assert!(checked > 40, "{checked}");
    }

    /// Which hosts count as the same site as `https://login.bank.de`, `alice.github.io`, a router ...
    #[test]
    fn phishing_matrix() {
        let cases: &[(&str, &str, bool)] = &[
            ("https://login.bank.de", "bank.de", true),
            ("https://login.bank.de", "www.bank.de", true),
            ("https://login.bank.de", "BANK.DE.", true),
            ("https://login.bank.de", "bank.de.evil.com", false),
            ("https://login.bank.de", "bank-de.com", false),
            ("https://login.bank.de", "xn--bank-9na.de", false),
            ("https://login.bank.de", "evil.com", false),
            ("https://alice.github.io", "evil.github.io", false),
            ("https://alice.github.io", "alice.github.io", true),
            ("https://nas.synology.me", "evil.synology.me", false),
            ("https://box.myfritz.net", "other.myfritz.net", false),
            ("https://shop.co.uk", "evil.co.uk", false),
            ("https://a.example.co.uk", "b.example.co.uk", true),
            ("http://192.168.1.1/", "192.168.1.1", true),
            ("http://192.168.1.1/", "10.0.1.1", false),
            ("http://192.168.1.1/", "203.0.1.1", false),
            ("https://user:pw@bank.de:8443/x", "bank.de", true),
            ("https://bank.de", "", false),
            ("bank.de", "bank.de", true),
        ];
        for (url, domain, want) in cases {
            assert_eq!(same_site(url, domain), *want, "{url} vs {domain}");
        }
    }
}
