// SPDX-License-Identifier: AGPL-3.0-or-later
//! Matching of logins for autofill: which entries fit a website or an app.
//!
//! Websites match by their base domain (`login.example.com` fits `example.com`, but
//! `example.com.evil.net` does not) and are filled directly.
//!
//! Apps are filled directly only if the entry links them with their signing certificate
//! (`androidapp://<package>?cert=<sha256 hex>`), which the app sets when the user picks a login by
//! hand. A link without certificate (e.g. imported) or a package name that only fits the entry's
//! domain (`com.github.android` → `github.com`) is merely *suggested*, because any app can choose
//! such a package name. A link with a different certificate excludes the entry.

/// Second-level labels under which registrations happen one level deeper (`example.co.uk`).
const SECOND_LEVEL: &[&str] = &["co", "com", "org", "net", "ac", "gov", "edu", "or", "ne", "go"];

/// Lower-case host of a URL or bare domain, without `www.`, port, path and credentials.
pub(crate) fn host_of(url: &str) -> Option<String> {
    let s = url.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next()?;
    let s = s.rsplit_once('@').map_or(s, |(_, h)| h);
    let s = s.split(':').next()?;
    let host = s.trim_end_matches('.').to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    if host.is_empty() || !host.contains('.') || host.contains(' ') {
        return None;
    }
    Some(host)
}

/// The registrable part of a host: `a.b.example.com` → `example.com`, `x.example.co.uk` → `example.co.uk`.
pub(crate) fn base_domain(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }
    let n = labels.len();
    let take = if labels[n - 1].len() == 2 && SECOND_LEVEL.contains(&labels[n - 2]) { 3 } else { 2 };
    labels[n - take.min(n)..].join(".")
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
