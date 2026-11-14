// SPDX-License-Identifier: AGPL-3.0-or-later
//! App updates from the project's releases on GitHub.
//!
//! A release carries the app files and two more: `SHA256SUMS` (SHA-256 of every file) and
//! `SHA256SUMS.sig`, a signature of it made in CI with the app's signing key. The app knows the
//! public half of that key (built in) and accepts a file only if:
//! 1. the signature over `SHA256SUMS` is valid for that key,
//! 2. the downloaded file has exactly the size and the SHA-256 listed there.
//!
//! So neither GitHub nor anyone in between can hand out another file. On Android the system checks
//! once more that the new APK is signed with the same key as the installed app.
//!
//! Only `https` to GitHub's own hosts is used (redirects included); a release that is a draft or
//! a pre-release is never offered.

use std::io::Write;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::app::error::{GhulError, Result};
use crate::net::http::{self, Url};
use crate::net::{block_on, tls, NetError, NetResult};

/// Largest file the updater downloads (the Windows installer carries its own Java runtime).
const MAX_DOWNLOAD: u64 = 600 * 1024 * 1024;
/// Largest `SHA256SUMS` and signature accepted.
const MAX_SMALL: usize = 64 * 1024;
const REDIRECTS: usize = 5;

/// A newer version than the running one, ready to download.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct UpdateInfo {
    /// e.g. "0.6.0".
    pub version: String,
    /// The release notes (Markdown, shown as text).
    pub notes: String,
    /// The release on GitHub (for "show in the browser").
    pub page: String,
    /// File name, e.g. "GhulSuite-0.6.0.apk".
    pub asset: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 (hex) from the signed `SHA256SUMS`.
    pub sha256: String,
}

fn bad(msg: &str) -> GhulError {
    GhulError::Corrupted { msg: msg.into() }
}

/// Looks for a newer release of `repo` ("owner/name") than `current` with a file whose name ends
/// with `suffix` (".apk", ".msi", "-portable.zip"). `public_key` is the PEM of the signing key.
/// `prerelease`: also development versions ("0.6.0-dev.3", GitHub pre-releases), whichever is
/// newest; otherwise only final releases. `None`: nothing newer.
#[uniffi::export]
pub fn update_check(
    repo: String,
    current: String,
    suffix: String,
    public_key: String,
    prerelease: bool,
) -> Result<Option<UpdateInfo>> {
    let key = PublicKey::from_pem(&public_key)?;
    if repo.is_empty() || !repo.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) || repo.contains("..")
    {
        return Err(GhulError::InvalidInput { msg: "invalid repository name".into() });
    }
    let path = if prerelease { "releases?per_page=30" } else { "releases/latest" };
    let resp = get(&format!("https://api.github.com/repos/{repo}/{path}"))?;
    if resp.status == 404 {
        return Ok(None);
    }
    if resp.status != 200 {
        return Err(GhulError::Network { msg: "the update server refused the request, please try again later".into() });
    }
    let answer: serde_json::Value =
        serde_json::from_slice(&resp.body).map_err(|_| bad("update information unreadable"))?;
    let Some((info, release)) = best(&answer, &current, &suffix, prerelease)? else { return Ok(None) };
    // The signed list of checksums.
    let asset_url = |name: &str| -> Option<String> {
        release["assets"].as_array()?.iter().find(|a| a["name"] == name)?["browser_download_url"]
            .as_str()
            .map(str::to_owned)
    };
    let sums_url = asset_url("SHA256SUMS").ok_or_else(|| bad("the release has no signed checksums"))?;
    let sig_url = asset_url("SHA256SUMS.sig").ok_or_else(|| bad("the release has no signed checksums"))?;
    let sums = get_small(&sums_url)?;
    let sig = get_small(&sig_url)?;
    if !key.verify(&sums, &sig) {
        return Err(bad("the signature of the update is not valid"));
    }
    let sha256 = sha_for(&sums, &info.asset).ok_or_else(|| bad("the update file is not in the signed checksums"))?;
    Ok(Some(UpdateInfo { sha256, ..info }))
}

/// Downloads an update into `path` (written as `path.part`, renamed only when size and SHA-256
/// match the signed checksums). Anything else is deleted.
#[uniffi::export]
pub fn update_download(info: UpdateInfo, path: String) -> Result<()> {
    if info.size == 0 || info.size > MAX_DOWNLOAD || info.sha256.len() != 64 {
        return Err(bad("update file size or checksum invalid"));
    }
    let part = format!("{path}.part");
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&part)?;
        let digest = block_on(stream(&info.url, &mut file, info.size), Duration::from_secs(3600))?;
        file.flush()?;
        drop(file);
        if !crate::kdf::hex(&digest).eq_ignore_ascii_case(&info.sha256) {
            return Err(bad("the update file does not match its checksum"));
        }
        std::fs::rename(&part, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// A version "1.2.3" or "1.2.3-dev.4" (a leading "v" and "+build" are ignored).
#[derive(Debug, PartialEq, Eq)]
struct Version {
    num: [u64; 3],
    pre: Vec<String>,
}

impl Version {
    fn parse(v: &str) -> Option<Version> {
        let v = v.trim().trim_start_matches('v');
        let v = v.split('+').next()?;
        let (num, pre) = match v.split_once('-') {
            Some((n, p)) if !p.is_empty() => (n, p.split('.').map(str::to_owned).collect::<Vec<_>>()),
            Some(_) => return None,
            None => (v, Vec::new()),
        };
        let parts: Vec<&str> = num.split('.').collect();
        if parts.len() != 3 || pre.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric())) {
            return None;
        }
        let mut out = [0u64; 3];
        for (o, p) in out.iter_mut().zip(parts) {
            *o = p.parse().ok()?;
        }
        Some(Version { num: out, pre })
    }
}

impl Ord for Version {
    /// As in semantic versioning: a pre-release ("0.6.0-dev.3") is older than its release
    /// ("0.6.0"); pre-releases compare part by part, numbers as numbers ("dev.10" > "dev.9").
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        self.num.cmp(&other.num).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Equal,
            (true, false) => Greater,
            (false, true) => Less,
            (false, false) => {
                for (a, b) in self.pre.iter().zip(&other.pre) {
                    let o = match (a.parse::<u64>(), b.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => Less,
                        (Err(_), Ok(_)) => Greater,
                        (Err(_), Err(_)) => a.cmp(b),
                    };
                    if o != Equal {
                        return o;
                    }
                }
                self.pre.len().cmp(&other.pre.len())
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Whether `latest` is newer than `current`. Anything unreadable is never newer.
pub fn newer(latest: &str, current: &str) -> bool {
    matches!((Version::parse(latest), Version::parse(current)), (Some(l), Some(c)) if l > c)
}

/// The newest suitable release in the answer (one release, or a list of them).
fn best<'a>(
    answer: &'a serde_json::Value,
    current: &str,
    suffix: &str,
    prerelease: bool,
) -> Result<Option<(UpdateInfo, &'a serde_json::Value)>> {
    let releases: Vec<&serde_json::Value> = match answer.as_array() {
        Some(list) => list.iter().collect(),
        None => vec![answer],
    };
    let mut found: Option<(Version, UpdateInfo, &serde_json::Value)> = None;
    for r in releases {
        if let Some(info) = pick(r, current, suffix, prerelease)? {
            let Some(v) = Version::parse(&info.version) else { continue };
            if !found.as_ref().is_some_and(|(best, _, _)| v <= *best) {
                found = Some((v, info, r));
            }
        }
    }
    Ok(found.map(|(_, info, r)| (info, r)))
}

/// The release, if it is a newer, published one (final, or with `prerelease` also a development
/// version) with a file for this platform.
fn pick(release: &serde_json::Value, current: &str, suffix: &str, prerelease: bool) -> Result<Option<UpdateInfo>> {
    let marked_pre = release["prerelease"].as_bool() == Some(true);
    if release["draft"].as_bool() == Some(true) || (marked_pre && !prerelease) {
        return Ok(None);
    }
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag.trim_start_matches('v').to_owned();
    // A final release must not carry a pre-release version, and the other way round.
    match Version::parse(&version) {
        Some(v) if v.pre.is_empty() == !marked_pre => {}
        _ => return Ok(None),
    }
    if !newer(&version, current) {
        return Ok(None);
    }
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let mut matching = assets.iter().filter(|a| a["name"].as_str().is_some_and(|n| n.ends_with(suffix)));
    let Some(a) = matching.next() else { return Ok(None) };
    if matching.next().is_some() {
        return Err(bad("the release has more than one file for this platform"));
    }
    Ok(Some(UpdateInfo {
        version,
        notes: release["body"].as_str().unwrap_or_default().chars().take(20_000).collect(),
        page: release["html_url"].as_str().unwrap_or_default().to_owned(),
        asset: a["name"].as_str().unwrap_or_default().to_owned(),
        url: a["browser_download_url"].as_str().unwrap_or_default().to_owned(),
        size: a["size"].as_u64().unwrap_or(0),
        sha256: String::new(),
    }))
}

/// The SHA-256 of `name` in a `sha256sum` list ("<hex>  <name>" or "<hex> *<name>").
fn sha_for(sums: &[u8], name: &str) -> Option<String> {
    std::str::from_utf8(sums).ok()?.lines().find_map(|l| {
        let (hash, file) = l.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Only GitHub's own hosts, only https (tests: a server on this computer).
fn allowed(url: &Url) -> bool {
    #[cfg(test)]
    if !url.tls && (url.host == "127.0.0.1" || url.host == "localhost") {
        return true;
    }
    let h = url.host.to_ascii_lowercase();
    url.tls && (h == "api.github.com" || h == "github.com" || h.ends_with(".githubusercontent.com"))
}

/// GET with redirects to allowed hosts only; the whole answer in memory (small answers).
fn get(url: &str) -> Result<http::Response> {
    let mut url = Url::parse(url)?;
    for _ in 0..=REDIRECTS {
        if !allowed(&url) {
            return Err(GhulError::InvalidInput { msg: "update address not allowed".into() });
        }
        let headers = [("Accept", "application/vnd.github+json".to_owned())];
        let resp = block_on(http::request("GET", &url, &headers, b"", None), Duration::from_secs(60))?;
        if (300..400).contains(&resp.status) {
            let next = resp.header("Location").ok_or_else(|| bad("redirect without address"))?;
            url = Url::parse(next)?;
            continue;
        }
        return Ok(resp);
    }
    Err(bad("too many redirects"))
}

fn get_small(url: &str) -> Result<Vec<u8>> {
    let resp = get(url)?;
    if resp.status != 200 || resp.body.len() > MAX_SMALL {
        return Err(bad("checksums of the update unreadable"));
    }
    Ok(resp.body)
}

/// Streams a GET into `out` (redirects to allowed hosts), at most `expected` bytes; returns the
/// SHA-256 of what was written. The size must match exactly.
async fn stream(url: &str, out: &mut std::fs::File, expected: u64) -> NetResult<[u8; 32]> {
    let mut url = Url::parse(url)?;
    for _ in 0..=REDIRECTS {
        if !allowed(&url) {
            return Err(NetError::Protocol("update address not allowed".into()));
        }
        let tcp =
            TcpStream::connect((url.host.as_str(), url.port)).await.map_err(|e| NetError::Connect(e.to_string()))?;
        let next = if url.tls {
            let s = tls::start(tcp, &url.host, None).await?;
            stream_over(s, &url, out, expected).await?
        } else {
            stream_over(tcp, &url, out, expected).await?
        };
        match next {
            Streamed::Done(d) => return Ok(d),
            Streamed::Redirect(to) => url = Url::parse(&to)?,
        }
    }
    Err(NetError::Protocol("too many redirects".into()))
}

enum Streamed {
    Done([u8; 32]),
    Redirect(String),
}

async fn stream_over<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut s: S,
    url: &Url,
    out: &mut std::fs::File,
    expected: u64,
) -> NetResult<Streamed> {
    let io = |e: std::io::Error| NetError::Connect(e.to_string());
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: GhulSuite\r\nConnection: close\r\n\r\n",
        url.path, url.host
    );
    s.write_all(req.as_bytes()).await.map_err(io)?;
    s.flush().await.map_err(io)?;
    // The head first (at most 64 KiB).
    let mut head = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let body_start = loop {
        let n = s.read(&mut buf).await.map_err(io)?;
        if n == 0 {
            return Err(NetError::Protocol("connection closed before the answer".into()));
        }
        head.extend_from_slice(&buf[..n]);
        if let Some(p) = head.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if head.len() > 64 * 1024 {
            return Err(NetError::Protocol("answer head too large".into()));
        }
    };
    let text = String::from_utf8_lossy(&head[..body_start]).into_owned();
    let mut lines = text.split("\r\n");
    let status: u16 = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned())))
        .collect();
    let header = |n: &str| headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    if (300..400).contains(&status) {
        return header("location")
            .map(Streamed::Redirect)
            .ok_or_else(|| NetError::Protocol("redirect without address".into()));
    }
    if status != 200 {
        return Err(NetError::Protocol(format!("download answered {status}")));
    }
    if header("transfer-encoding").is_some_and(|v| !v.eq_ignore_ascii_case("identity")) {
        return Err(NetError::Protocol("unexpected transfer encoding".into()));
    }
    if header("content-length").and_then(|v| v.parse::<u64>().ok()).is_some_and(|n| n != expected) {
        return Err(NetError::Protocol("download has another size than announced".into()));
    }
    let mut hash = Sha256::new();
    let mut written: u64 = 0;
    let mut put = |data: &[u8]| -> NetResult<()> {
        written += data.len() as u64;
        if written > expected {
            return Err(NetError::Protocol("download larger than announced".into()));
        }
        hash.update(data);
        out.write_all(data).map_err(|e| NetError::Protocol(e.to_string()))
    };
    put(&head[body_start..])?;
    loop {
        let n = match s.read(&mut buf).await {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(io(e)),
        };
        if n == 0 {
            break;
        }
        put(&buf[..n])?;
    }
    if written != expected {
        return Err(NetError::Protocol("download incomplete".into()));
    }
    Ok(Streamed::Done(hash.finalize().into()))
}

/// The public half of the app's signing key (RSA or P-256), from PEM.
pub(crate) enum PublicKey {
    Rsa(Vec<u8>),
    P256(Vec<u8>),
}

impl PublicKey {
    pub(crate) fn from_pem(pem: &str) -> Result<PublicKey> {
        use base64::Engine;
        let none = || GhulError::InvalidInput { msg: "this build has no update key".into() };
        let body: String = pem.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("-----")).collect();
        if body.is_empty() {
            return Err(none());
        }
        let der = base64::engine::general_purpose::STANDARD.decode(body).map_err(|_| none())?;
        Self::from_spki(&der).ok_or_else(none)
    }

    /// SubjectPublicKeyInfo: SEQUENCE { SEQUENCE { OID, params }, BIT STRING key }.
    fn from_spki(der: &[u8]) -> Option<PublicKey> {
        const RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
        const EC: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
        const P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
        let (seq, rest) = der_read(der, 0x30)?;
        if !rest.is_empty() {
            return None;
        }
        let (alg, rest) = der_read(seq, 0x30)?;
        let (bits, rest) = der_read(rest, 0x03)?;
        if !rest.is_empty() || bits.first() != Some(&0) {
            return None;
        }
        let key = bits[1..].to_vec();
        let (oid, params) = der_read(alg, 0x06)?;
        if oid == RSA {
            Some(PublicKey::Rsa(key))
        } else if oid == EC && der_read(params, 0x06).is_some_and(|(c, r)| c == P256 && r.is_empty()) {
            Some(PublicKey::P256(key))
        } else {
            None
        }
    }

    pub(crate) fn verify(&self, data: &[u8], sig: &[u8]) -> bool {
        use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1, RSA_PKCS1_2048_8192_SHA256};
        match self {
            PublicKey::Rsa(k) => UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, k).verify(data, sig).is_ok(),
            PublicKey::P256(k) => UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, k).verify(data, sig).is_ok(),
        }
    }
}

/// One DER element with `tag`: (its content, what follows).
fn der_read(data: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    if data.first() != Some(&tag) {
        return None;
    }
    let first = *data.get(1)?;
    let (len, start) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for b in data.get(2..2 + n)? {
            len = (len << 8) | *b as usize;
        }
        (len, 2 + n)
    };
    let end = start.checked_add(len)?;
    Some((data.get(start..end)?, data.get(end..)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &[u8] = b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  GhulSuite-0.6.0.apk\n";
    const RSA_PUB: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA2r6rCUQrNxnH+ALbqsvQ
xVhnRvWJ5PD/H9NnQ/ZZIGJDwa54o3e6+ikj//YjtQCzo+XmYVxLwGjrdX840l97
QzIoM4r2WE9m7SZE3y5jcIAXg1v78N9JnIua7OJz+9kyZl/5CRAFpEfMf1lh4GjW
EwelmU3lgodXhDhuephSLUa3u6wIkJoAbixBZQuqfsO//qGv4D4YEA0TkvvPLmKF
hk9sgVeYhVsVQGZZ1uKeUJXpdaMxyAuVYddW++GR4NhS3mNUfeT7GnhL+JdZMWkU
LWQ7Ev4PE9hH/frHS7Y9YVfGE42KgVDRqT6/Fu9tuyzsmoksyH/xxJMWnJw/cGai
sQIDAQAB
-----END PUBLIC KEY-----";
    const RSA_SIG: &str = "t3rqv3Y8uZ63EmCK5KhLTIoSdADsMSWZWMluGvSCIxQGZPh0cdxBoh1c+wPhgWx6NCH9IvT4j8RN/EBRP9QdnNAsEyTV8o8qpfX8fG5DQhl4JDoVqZKDXnZ6xxFGudETPqSi1RI+CbWCZtRRA/fFGbOViKFAr3y7a8yYcJ5z6+pNgau0k2v5FRRYQz/Z3ScxnehjUVk2owuAkQqEW3PSD44FhFobWwdPfMRAmiq8Wyfzctv/QbjmlCGP5v+gZhGHi3gaW4ltnDgGBnfd9RWVeaAyPXKx74CSpAalzqEn+yVggQRoeUqFcAa+X3chzJe3cUXe3mpNMqS1ixuAG5tWnQ==";
    const EC_PUB: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAExTw0yQ1jRyede+GkIHVxLNhgKBWe
f3j2T3ZQ17VauB9iOzX7I9V6neeaHdUFnkjcMgaHR5uant+ua9qb79Qfig==
-----END PUBLIC KEY-----";
    const EC_SIG: &str =
        "MEUCIF41hh44EAGAyubV3wPzrr2UZEWt3AOM1nU495AyHzo/AiEAtqwBV/2GahgtvXG5nU3ED1FN4vpjAR9YGkVLcVsMmLw=";

    fn b64(s: &str) -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(s).unwrap()
    }

    #[test]
    fn signatures_of_rsa_and_p256_keys_as_openssl_makes_them() {
        for (pem, sig) in [(RSA_PUB, RSA_SIG), (EC_PUB, EC_SIG)] {
            let key = PublicKey::from_pem(pem).unwrap();
            assert!(key.verify(SUMS, &b64(sig)));
            // Any change to the list or the signature is refused.
            let mut other = SUMS.to_vec();
            other[0] = b'f';
            assert!(!key.verify(&other, &b64(sig)));
            let mut s = b64(sig);
            let n = s.len() - 2;
            s[n] ^= 1;
            assert!(!key.verify(SUMS, &s));
        }
        // As the release workflow builds it in: the key as one line of base64, without PEM lines.
        let one_line: String = RSA_PUB.lines().filter(|l| !l.starts_with("-----")).collect();
        assert!(PublicKey::from_pem(&one_line).unwrap().verify(SUMS, &b64(RSA_SIG)));
        // The signature of one key is not valid for the other.
        assert!(!PublicKey::from_pem(EC_PUB).unwrap().verify(SUMS, &b64(RSA_SIG)));
        // Without a key (local builds) there are no updates.
        assert!(PublicKey::from_pem("").is_err());
        assert!(PublicKey::from_pem("-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----").is_err());
    }

    #[test]
    fn versions_and_checksum_lists() {
        assert!(newer("0.6.0", "0.5.0"));
        assert!(newer("v0.6.0", "0.5.0-dev"));
        assert!(newer("0.5.0", "0.5.0-dev"), "a release is newer than its development version");
        assert!(newer("1.0.0", "0.99.99"));
        assert!(newer("0.10.0", "0.9.0"), "numbers, not text");
        assert!(!newer("0.5.0", "0.5.0"));
        assert!(!newer("0.4.9", "0.5.0"));
        assert!(!newer("0.6.0-beta", "0.6.0"));
        assert!(!newer("rubbish", "0.5.0"));
        assert!(!newer("1.2.3.4", "0.5.0"));
        assert!(!newer("0.6", "0.5.0"));
        // Development versions: numbers as numbers, the release beats all of them.
        assert!(newer("0.6.0-dev.10", "0.6.0-dev.9"));
        assert!(newer("0.6.0-dev.1", "0.5.0"));
        assert!(newer("0.6.0", "0.6.0-dev.99"));
        assert!(!newer("0.6.0-dev.3", "0.6.0-dev.3"));
        assert!(!newer("0.6.0-dev.2", "0.6.0-dev.10"));
        assert!(newer("0.6.0-dev.2.1", "0.6.0-dev.2"));
        assert!(newer("0.6.0-dev.1", "0.6.0-beta.1"), "text parts compare as text");
        assert!(!newer("0.6.0-", "0.5.0"));
        assert!(!newer("0.6.0-dev..1", "0.5.0"));
        assert_eq!(
            sha_for(SUMS, "GhulSuite-0.6.0.apk").unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha_for(b"E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855 *x.msi\n", "x.msi")
                .unwrap()
                .len(),
            64
        );
        assert!(sha_for(SUMS, "GhulSuite-0.6.0.ap").is_none());
        assert!(sha_for(b"abc  x.apk\n", "x.apk").is_none());
    }

    #[test]
    fn only_final_newer_releases_with_one_file_for_the_platform() {
        let release = |tag: &str, draft: bool, pre: bool, names: &[&str]| {
            serde_json::json!({
                "tag_name": tag, "draft": draft, "prerelease": pre, "body": "Neu", "html_url": "https://github.com/x/y/releases/v",
                "assets": names.iter().map(|n| serde_json::json!({"name": n, "size": 10, "browser_download_url": format!("https://github.com/x/y/releases/download/v/{n}")})).collect::<Vec<_>>(),
            })
        };
        let files = [
            "GhulSuite-0.6.0.apk",
            "GhulSuite-0.6.0.msi",
            "GhulSuite-0.6.0-portable.zip",
            "SHA256SUMS",
            "SHA256SUMS.sig",
        ];
        let r = pick(&release("v0.6.0", false, false, &files), "0.5.0", ".apk", false).unwrap().unwrap();
        assert_eq!((r.version.as_str(), r.asset.as_str(), r.size), ("0.6.0", "GhulSuite-0.6.0.apk", 10));
        assert_eq!(
            pick(&release("v0.6.0", false, false, &files), "0.5.0", "-portable.zip", false).unwrap().unwrap().asset,
            "GhulSuite-0.6.0-portable.zip"
        );
        assert!(pick(&release("v0.6.0", true, false, &files), "0.5.0", ".apk", true).unwrap().is_none(), "draft");
        assert!(
            pick(&release("v0.6.0-dev.1", false, true, &files), "0.5.0", ".apk", false).unwrap().is_none(),
            "pre-release on the stable channel"
        );
        assert!(
            pick(&release("v0.6.0-dev.1", false, true, &files), "0.5.0", ".apk", true).unwrap().is_some(),
            "pre-release on the dev channel"
        );
        assert!(
            pick(&release("v0.6.0-dev.1", false, false, &files), "0.5.0", ".apk", true).unwrap().is_none(),
            "dev version not marked as pre-release"
        );
        assert!(
            pick(&release("v0.6.0", false, true, &files), "0.5.0", ".apk", true).unwrap().is_none(),
            "final version marked as pre-release"
        );
        assert!(pick(&release("v0.5.0", false, false, &files), "0.5.0", ".apk", false).unwrap().is_none(), "not newer");
        assert!(
            pick(&release("v0.6.0", false, false, &["a.apk", "b.apk"]), "0.5.0", ".apk", false).is_err(),
            "ambiguous"
        );
        // The list of the dev channel: the newest wins, whether release or pre-release.
        let list = serde_json::json!([
            release("v0.6.0-dev.9", false, true, &files),
            release("v0.6.0-dev.10", false, true, &files),
            release("v0.5.1", false, false, &files),
            release("v0.7.0", true, false, &files),
        ]);
        assert_eq!(best(&list, "0.5.0", ".apk", true).unwrap().unwrap().0.version, "0.6.0-dev.10");
        assert_eq!(best(&list, "0.5.0", ".apk", false).unwrap().unwrap().0.version, "0.5.1");
        assert!(best(&list, "0.6.0-dev.10", ".apk", true).unwrap().is_none());
        let list =
            serde_json::json!([release("v0.6.0-dev.10", false, true, &files), release("v0.6.0", false, false, &files)]);
        assert_eq!(
            best(&list, "0.6.0-dev.10", ".apk", true).unwrap().unwrap().0.version,
            "0.6.0",
            "back to stable once released"
        );
    }

    #[test]
    fn only_https_to_github() {
        for (u, ok) in [
            ("https://api.github.com/repos/x/y/releases/latest", true),
            ("https://github.com/x/y/releases/download/v1/a.apk", true),
            ("https://objects.githubusercontent.com/a", true),
            ("https://release-assets.githubusercontent.com/a", true),
            ("https://github.com.evil.example/x", false),
            ("https://evilgithubusercontent.com/x", false),
            ("https://example.org/a.apk", false),
        ] {
            assert_eq!(allowed(&Url::parse(u).unwrap()), ok, "{u}");
        }
        // Plain http to anything but this device is refused before any connection.
        assert!(Url::parse("http://github.com/x").is_err());
    }

    /// A small HTTP server on this computer: a redirect, then the file (or a wrong one).
    fn serve(body: Vec<u8>, announced: usize) -> String {
        use std::io::{Read, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for (i, conn) in listener.incoming().take(2).enumerate() {
                let mut c = conn.unwrap();
                let mut req = [0u8; 2048];
                let _ = c.read(&mut req);
                if i == 0 {
                    let _ = write!(
                        c,
                        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/file\r\nContent-Length: 0\r\n\r\n"
                    );
                } else {
                    let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Length: {announced}\r\n\r\n");
                    let _ = c.write_all(&body);
                }
            }
        });
        format!("http://127.0.0.1:{port}/start")
    }

    #[test]
    fn downloads_follow_redirects_and_keep_only_matching_files() {
        let dir = std::env::temp_dir().join(format!("ghul-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("u.apk").to_string_lossy().into_owned();
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let sha = crate::kdf::hex(&Sha256::digest(&body));
        let info = |url: String, size: usize, sha: &str| UpdateInfo {
            version: "0.6.0".into(),
            notes: String::new(),
            page: String::new(),
            asset: "u.apk".into(),
            url,
            size: size as u64,
            sha256: sha.into(),
        };
        // The right file: downloaded, checked, in place.
        update_download(info(serve(body.clone(), body.len()), body.len(), &sha), path.clone()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        std::fs::remove_file(&path).unwrap();
        // Another file with the same size: refused, nothing left behind.
        let mut other = body.clone();
        other[1000] ^= 1;
        assert!(update_download(info(serve(other, body.len()), body.len(), &sha), path.clone()).is_err());
        assert!(!std::path::Path::new(&path).exists() && !std::path::Path::new(&format!("{path}.part")).exists());
        // Cut short, or larger than the signed size: refused.
        assert!(
            update_download(info(serve(body[..1000].to_vec(), body.len()), body.len(), &sha), path.clone()).is_err()
        );
        assert!(update_download(info(serve(body.clone(), body.len()), 1000, &sha), path.clone()).is_err());
        assert!(!std::path::Path::new(&path).exists());
        // Addresses outside GitHub are never fetched.
        assert!(update_download(info("https://example.org/u.apk".into(), body.len(), &sha), path.clone()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
