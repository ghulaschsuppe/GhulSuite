// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network tests against real servers on the internet; they only run at night (`--ignored`).

/// Real servers with broken TLS (badssl.com): GhulSuite must refuse every one of them. Runs at
/// night (needs the internet): `cargo test -p ghul-core -- --ignored badssl`.
#[test]
#[ignore = "needs the internet: cargo test -p ghul-core -- --ignored badssl"]
fn badssl_broken_tls_is_refused() {
    use super::http::{request, Url};
    let get = |url: &str| {
        let u = Url::parse(url).unwrap();
        super::block_on(request("GET", &u, &[], b"", None), std::time::Duration::from_secs(30))
    };
    // Without the reference site the test says nothing (network down, site gone).
    if get("https://badssl.com/").is_err() {
        eprintln!("badssl.com not reachable, skipped");
        return;
    }
    assert!(get("https://tls-v1-2.badssl.com:1012/").is_ok(), "a good TLS 1.2 server must work");
    for host in [
        "expired.badssl.com",
        "wrong.host.badssl.com",
        "self-signed.badssl.com",
        "untrusted-root.badssl.com",
        "no-common-name.badssl.com",
        "no-subject.badssl.com",
        "incomplete-chain.badssl.com",
        "tls-v1-0.badssl.com:1010",
        "tls-v1-1.badssl.com:1011",
        "rc4.badssl.com",
        "rc4-md5.badssl.com",
        "3des.badssl.com",
        "null.badssl.com",
        "dh480.badssl.com",
        "dh512.badssl.com",
        "dh1024.badssl.com",
        "superfish.badssl.com",
        "edellroot.badssl.com",
        "dsdtestprovider.badssl.com",
        "preact-cli.badssl.com",
        "webpack-dev-server.badssl.com",
    ] {
        let r = get(&format!("https://{host}/"));
        assert!(r.is_err(), "{host} was accepted");
    }
}
