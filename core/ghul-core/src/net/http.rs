// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal HTTP/1.1 client over rustls, for the share self-check and the PHP server module.
//!
//! Only `https://` with publicly trusted certificates (or a pinned fingerprint). Plain `http://` is
//! accepted for loopback addresses only, so tests can talk to `php -S` on the same machine.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use super::{tls, NetError, NetResult};

/// Largest response body accepted (protects against a hostile server).
const MAX_BODY: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// Path and query, always starting with `/`.
    pub path: String,
}

impl Url {
    pub fn parse(url: &str) -> NetResult<Url> {
        let url = url.trim();
        let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = url.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(NetError::Protocol("only https addresses are supported".into()));
        };
        let rest = rest.split('#').next().unwrap_or_default();
        let (authority, path) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], rest[i..].to_owned()),
            None => (rest, "/".to_owned()),
        };
        let path = if path.starts_with('?') { format!("/{path}") } else { path };
        if authority.contains('@') || authority.is_empty() {
            return Err(NetError::Protocol("invalid address".into()));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
                (h.to_owned(), p.parse().map_err(|_| NetError::Protocol("invalid port".into()))?)
            }
            _ => (authority.to_owned(), if tls { 443 } else { 80 }),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']').to_lowercase();
        if !tls && !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
            return Err(NetError::Protocol("unencrypted http is only allowed for this device".into()));
        }
        Ok(Url { tls, host, port, path })
    }

    /// The URL with `rel` appended to its path (no `..`, no new host).
    pub fn join(&self, rel: &str) -> Url {
        let base = self.path.split('?').next().unwrap_or("/");
        let base = if base.ends_with('/') { base.to_owned() } else { format!("{base}/") };
        Url { path: format!("{base}{}", rel.trim_start_matches('/')), ..self.clone() }
    }

    pub fn to_string_without_path(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        let default = if self.tls { 443 } else { 80 };
        if self.port == default {
            format!("{scheme}://{}", self.host)
        } else {
            format!("{scheme}://{}:{}", self.host, self.port)
        }
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.to_string_without_path(), self.path)
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Sends one request (`Connection: close`) and reads the whole response.
pub async fn request(
    method: &str,
    url: &Url,
    headers: &[(&str, String)],
    body: &[u8],
    pinned: Option<&str>,
) -> NetResult<Response> {
    let tcp = TcpStream::connect((url.host.as_str(), url.port)).await.map_err(|e| NetError::Connect(e.to_string()))?;
    if url.tls {
        let s = tls::start(tcp, &url.host, pinned).await?;
        exchange(s, method, url, headers, body).await
    } else {
        exchange(tcp, method, url, headers, body).await
    }
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut s: S,
    method: &str,
    url: &Url,
    headers: &[(&str, String)],
    body: &[u8],
) -> NetResult<Response> {
    let host = if (url.tls && url.port == 443) || (!url.tls && url.port == 80) {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    };
    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: GhulSuite\r\nConnection: close\r\nContent-Length: {}\r\n",
        url.path,
        body.len()
    );
    for (k, v) in headers {
        if k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']) {
            return Err(NetError::Protocol("invalid header".into()));
        }
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let io = |e: std::io::Error| NetError::Connect(e.to_string());
    s.write_all(head.as_bytes()).await.map_err(io)?;
    s.write_all(body).await.map_err(io)?;
    s.flush().await.map_err(io)?;

    let mut raw = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match s.read(&mut buf).await {
            Ok(n) => n,
            // Some servers close TLS without close_notify; the body length tells us if we have all.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(io(e)),
        };
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_BODY + 64 * 1024 {
            return Err(NetError::Protocol("response too large".into()));
        }
    }
    parse(&raw)
}

fn parse(raw: &[u8]) -> NetResult<Response> {
    let bad = || NetError::Protocol("invalid HTTP response".into());
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(bad)?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| bad())?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or_else(bad)?;
    let status: u16 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or_else(bad)?;
    let headers: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))).collect();
    let rest = &raw[end + 4..];
    let get = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());
    let body = if get("Transfer-Encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        let mut out = Vec::new();
        let mut pos = 0;
        loop {
            let line_end = rest[pos..].windows(2).position(|w| w == b"\r\n").ok_or_else(bad)? + pos;
            let size_str = std::str::from_utf8(&rest[pos..line_end]).map_err(|_| bad())?;
            let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("").trim(), 16).map_err(|_| bad())?;
            pos = line_end + 2;
            if size == 0 {
                break;
            }
            if pos + size > rest.len() || out.len() + size > MAX_BODY {
                return Err(bad());
            }
            out.extend_from_slice(&rest[pos..pos + size]);
            pos += size + 2;
        }
        out
    } else if let Some(len) = get("Content-Length") {
        let len: usize = len.parse().map_err(|_| bad())?;
        if len > rest.len() {
            return Err(NetError::Protocol("connection closed early".into()));
        }
        rest[..len].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(Response { status, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = Url::parse("https://Max.Example.org/teilen?x=1#secret").unwrap();
        assert_eq!((u.host.as_str(), u.port, u.path.as_str()), ("max.example.org", 443, "/teilen?x=1"));
        assert_eq!(Url::parse("https://h:8443").unwrap().path, "/");
        assert_eq!(Url::parse("https://h/a/").unwrap().join("s/x").path, "/a/s/x");
        assert_eq!(Url::parse("https://h/a").unwrap().join("/b").to_string(), "https://h/a/b");
        assert!(Url::parse("http://example.org/").is_err());
        assert!(Url::parse("http://127.0.0.1:8080/x.php").is_ok());
        assert!(Url::parse("ftp://x").is_err());
        assert!(Url::parse("https://user@evil/").is_err());
    }

    #[test]
    fn responses() {
        let r = parse(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nX-A: b\r\n\r\nabcdef").unwrap();
        assert_eq!((r.status, r.body.as_slice(), r.header("x-a")), (200, &b"abc"[..], Some("b")));
        let c = parse(b"HTTP/1.1 404 Not Found\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n")
            .unwrap();
        assert_eq!((c.status, c.body.as_slice()), (404, &b"abcde"[..]));
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nab").is_err());
        assert!(parse(b"garbage").is_err());
    }
}
