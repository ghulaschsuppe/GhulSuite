// SPDX-License-Identifier: AGPL-3.0-or-later
//! FTPS (explicit TLS, `AUTH TLS` on port 21) as storage and share target, e.g. on web hosting.
//!
//! - Control and data channels are always encrypted (`PROT P`); plain FTP is refused.
//! - Data connections reuse the TLS session of the control connection (many servers demand it).
//! - Passive mode only; the data connection goes to the configured host, never to an address
//!   the server announces (protects against redirection to other hosts).
//! - Writes are atomic: upload to a temporary name, then rename.

use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use zeroize::Zeroizing;

use super::remote::{check_path, parent, RemoteEntry, RemoteFs};
use super::{block_on, tls, NetError, NetResult, TIMEOUT};

#[derive(Clone)]
pub struct FtpsConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Zeroizing<String>,
    /// Folder relative to the login folder ("" = login folder).
    pub base: String,
    /// Confirmed certificate fingerprint for self-signed certificates.
    pub cert: Option<String>,
}

type Ctrl = BufReader<TlsStream<TcpStream>>;

pub struct FtpsFs {
    cfg: FtpsConfig,
    tls: tls::Client,
    ctrl: Mutex<Option<Ctrl>>,
}

fn proto(text: &str) -> NetError {
    NetError::Protocol(text.trim().to_owned())
}

fn io(e: std::io::Error) -> NetError {
    NetError::Connect(e.to_string())
}

async fn reply<S: tokio::io::AsyncBufRead + Unpin>(s: &mut S) -> NetResult<(u16, String)> {
    let mut text = String::new();
    let mut first: Option<String> = None;
    loop {
        let mut line = String::new();
        if s.read_line(&mut line).await.map_err(io)? == 0 {
            return Err(NetError::Connect("connection closed".into()));
        }
        text.push_str(&line);
        let code = line.get(..3).filter(|c| c.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned);
        match (&first, code) {
            (None, Some(c)) if line.as_bytes().get(3) == Some(&b'-') => first = Some(c),
            (None, Some(c)) => return Ok((c.parse().unwrap_or(0), text)),
            (Some(f), Some(c)) if &c == f && line.as_bytes().get(3) == Some(&b' ') => {
                return Ok((c.parse().unwrap_or(0), text))
            }
            _ => {}
        }
    }
}

async fn send<S: tokio::io::AsyncWrite + Unpin>(s: &mut S, line: &str) -> NetResult<()> {
    if line.contains(['\r', '\n']) {
        return Err(proto("invalid file name"));
    }
    s.write_all(format!("{line}\r\n").as_bytes()).await.map_err(io)?;
    s.flush().await.map_err(io)
}

async fn cmd(c: &mut Ctrl, line: &str) -> NetResult<(u16, String)> {
    send(c.get_mut(), line).await?;
    reply(c).await
}

async fn expect(c: &mut Ctrl, line: &str, ok: &[u16]) -> NetResult<String> {
    let (code, text) = cmd(c, line).await?;
    if ok.contains(&code) {
        Ok(text)
    } else if code == 530 {
        Err(NetError::AuthFailed)
    } else {
        Err(proto(&text))
    }
}

/// Port from an EPSV reply `229 Entering Extended Passive Mode (|||6446|)` or a PASV reply.
pub(crate) fn data_port(code: u16, text: &str) -> Option<u16> {
    if code == 229 {
        let inner = text.split('(').nth(1)?.split(')').next()?;
        return inner.trim_matches('|').parse().ok();
    }
    let nums: Vec<u8> =
        text.split('(').nth(1)?.split(')').next()?.split(',').filter_map(|n| n.trim().parse().ok()).collect();
    // Each part is one byte; anything else (e.g. 300) is not a valid reply.
    (nums.len() == 6).then(|| u16::from(nums[4]) * 256 + u16::from(nums[5]))
}

/// Parses one line of a Unix `LIST` output.
pub(crate) fn parse_list_line(line: &str) -> Option<RemoteEntry> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 9 {
        return None;
    }
    // The name starts after the eighth field and may contain spaces.
    let mut rest = line;
    for f in &fields[..8] {
        rest = rest[rest.find(f)? + f.len()..].trim_start();
    }
    let name = rest.trim_end().to_owned();
    if name == "." || name == ".." {
        return None;
    }
    Some(RemoteEntry { name, is_dir: line.starts_with('d'), size: fields[4].parse().unwrap_or(0) })
}

/// Parses one line of `MLSD` output (`type=file;size=12; name`).
pub(crate) fn parse_mlsd_line(line: &str) -> Option<RemoteEntry> {
    let (facts, name) = line.split_once(' ')?;
    let name = name.trim_end_matches(['\r', '\n']).to_owned();
    let mut kind = "";
    let mut size = 0;
    for f in facts.split(';') {
        if let Some((k, v)) = f.split_once('=') {
            match k.to_ascii_lowercase().as_str() {
                "type" => kind = v,
                "size" => size = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let kind = kind.to_ascii_lowercase();
    if kind == "cdir" || kind == "pdir" || name == "." || name == ".." {
        return None;
    }
    Some(RemoteEntry { name, is_dir: kind == "dir", size })
}

impl FtpsFs {
    pub fn connect(cfg: &FtpsConfig) -> NetResult<FtpsFs> {
        let tls = tls::Client::for_ftps(cfg.cert.as_deref())?;
        let ctrl = block_on(login(cfg, &tls), TIMEOUT)?;
        Ok(FtpsFs { cfg: cfg.clone(), tls, ctrl: Mutex::new(Some(ctrl)) })
    }

    fn full(&self, path: &str) -> NetResult<String> {
        check_path(path)?;
        let base = self.cfg.base.trim_end_matches('/');
        Ok(match (base.is_empty(), path.is_empty()) {
            (true, true) => ".".to_owned(),
            (true, false) => path.to_owned(),
            (false, true) => base.to_owned(),
            (false, false) => format!("{base}/{path}"),
        })
    }

    /// Runs `f` on the control connection; reconnects once if the connection broke.
    fn with<T: Send>(
        &self,
        limit: std::time::Duration,
        f: impl for<'a> Fn(
            &'a mut Ctrl,
            &'a FtpsFs,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = NetResult<T>> + Send + 'a>>,
    ) -> NetResult<T> {
        let mut guard = self.ctrl.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            *guard = Some(block_on(login(&self.cfg, &self.tls), TIMEOUT)?);
        }
        let first = block_on(f(guard.as_mut().expect("connected"), self), limit);
        match first {
            Err(NetError::Connect(_)) | Err(NetError::Timeout) => {
                *guard = Some(block_on(login(&self.cfg, &self.tls), TIMEOUT)?);
                let r = block_on(f(guard.as_mut().expect("connected"), self), limit);
                if r.is_err() {
                    *guard = None;
                }
                r
            }
            other => other,
        }
    }

    /// Opens a passive data connection for `command`; returns the TLS data stream.
    async fn data(&self, c: &mut Ctrl, command: &str) -> NetResult<Option<TlsStream<TcpStream>>> {
        let (code, text) = cmd(c, "EPSV").await?;
        let (code, text) = if code == 229 { (code, text) } else { cmd(c, "PASV").await? };
        let port = data_port(code, &text).ok_or_else(|| proto(&text))?;
        let tcp = TcpStream::connect((self.cfg.host.as_str(), port)).await.map_err(io)?;
        let (code, text) = cmd(c, command).await?;
        if code == 550 || code == 450 {
            return Ok(None);
        }
        if code != 150 && code != 125 {
            return Err(proto(&text));
        }
        Ok(Some(self.tls.connect(tcp, &self.cfg.host).await?))
    }

    async fn finish(c: &mut Ctrl) -> NetResult<()> {
        let (code, text) = reply(c).await?;
        if code == 226 || code == 250 {
            Ok(())
        } else {
            Err(proto(&text))
        }
    }

    async fn read_all(stream: &mut TlsStream<TcpStream>) -> NetResult<Vec<u8>> {
        let mut out = Vec::new();
        let r = match stream.read_to_end(&mut out).await {
            Ok(_) => Ok(out),
            // Some servers close the data channel without TLS close_notify.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(out),
            Err(e) => Err(io(e)),
        };
        // Close our side too: servers wait for it before confirming the transfer.
        let _ = stream.shutdown().await;
        r
    }

    async fn mkdirs(&self, c: &mut Ctrl, full_dir: &str) -> NetResult<()> {
        let mut path = String::new();
        for part in full_dir.split('/').filter(|p| !p.is_empty() && *p != ".") {
            path = if path.is_empty() { part.to_owned() } else { format!("{path}/{part}") };
            // 550/521: exists already (or not allowed – the following write reports that).
            let _ = cmd(c, &format!("MKD {path}")).await?;
        }
        Ok(())
    }
}

async fn login(cfg: &FtpsConfig, tls: &tls::Client) -> NetResult<Ctrl> {
    let tcp = TcpStream::connect((cfg.host.as_str(), cfg.port)).await.map_err(io)?;
    let mut plain = BufReader::new(tcp);
    let (code, text) = reply(&mut plain).await?;
    if code != 220 {
        return Err(proto(&text));
    }
    send(plain.get_mut(), "AUTH TLS").await?;
    let (code, text) = reply(&mut plain).await?;
    if code != 234 {
        return Err(NetError::Protocol(format!("server does not offer FTPS: {}", text.trim())));
    }
    let mut c = BufReader::new(tls.connect(plain.into_inner(), &cfg.host).await?);
    let (code, text) = cmd(&mut c, &format!("USER {}", cfg.user)).await?;
    if code == 331 {
        let (code, text) = cmd(&mut c, &format!("PASS {}", cfg.password.as_str())).await?;
        if code == 530 {
            return Err(NetError::AuthFailed);
        }
        if code != 230 {
            return Err(proto(&text));
        }
    } else if code == 530 {
        return Err(NetError::AuthFailed);
    } else if code != 230 {
        return Err(proto(&text));
    }
    expect(&mut c, "PBSZ 0", &[200]).await?;
    expect(&mut c, "PROT P", &[200]).await?;
    expect(&mut c, "TYPE I", &[200]).await?;
    let _ = cmd(&mut c, "OPTS UTF8 ON").await;
    Ok(c)
}

impl RemoteFs for FtpsFs {
    fn list(&self, dir: &str) -> NetResult<Vec<RemoteEntry>> {
        let full = self.full(dir)?;
        self.with(TIMEOUT, |c, me| {
            let full = full.clone();
            Box::pin(async move {
                let mut entries = match me.data(c, &format!("MLSD {full}")).await {
                    Ok(Some(mut d)) => {
                        let raw = FtpsFs::read_all(&mut d).await?;
                        FtpsFs::finish(c).await?;
                        String::from_utf8_lossy(&raw).lines().filter_map(parse_mlsd_line).collect::<Vec<_>>()
                    }
                    Ok(None) => return Ok(Vec::new()),
                    // No MLSD (e.g. vsftpd): fall back to LIST.
                    Err(NetError::Protocol(_)) => match me.data(c, &format!("LIST -a {full}")).await? {
                        Some(mut d) => {
                            let raw = FtpsFs::read_all(&mut d).await?;
                            FtpsFs::finish(c).await?;
                            String::from_utf8_lossy(&raw).lines().filter_map(parse_list_line).collect()
                        }
                        None => return Ok(Vec::new()),
                    },
                    Err(e) => return Err(e),
                };
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(entries)
            })
        })
    }

    fn read(&self, path: &str) -> NetResult<Option<Vec<u8>>> {
        let full = self.full(path)?;
        self.with(TIMEOUT * 20, |c, me| {
            let full = full.clone();
            Box::pin(async move {
                match me.data(c, &format!("RETR {full}")).await? {
                    Some(mut d) => {
                        let data = FtpsFs::read_all(&mut d).await?;
                        FtpsFs::finish(c).await?;
                        Ok(Some(data))
                    }
                    None => Ok(None),
                }
            })
        })
    }

    fn write(&self, path: &str, data: &[u8]) -> NetResult<()> {
        let full = self.full(path)?;
        let dir = self.full(parent(path))?;
        let limit = TIMEOUT + std::time::Duration::from_secs((data.len() / (256 * 1024)) as u64);
        let data: std::sync::Arc<[u8]> = data.into();
        self.with(limit, |c, me| {
            let (full, dir, data) = (full.clone(), dir.clone(), data.clone());
            Box::pin(async move {
                me.mkdirs(c, &dir).await?;
                let tmp = format!("{full}.ghltmp");
                let mut d = me.data(c, &format!("STOR {tmp}")).await?.ok_or_else(|| proto("cannot write"))?;
                d.write_all(&data).await.map_err(io)?;
                d.shutdown().await.map_err(io)?;
                drop(d);
                FtpsFs::finish(c).await?;
                // Replace in one step where the server allows it: the old file stays until the new
                // one is complete. Only servers that refuse to overwrite get DELE first (then the
                // file is missing for a moment; synchronisation copes with that).
                expect(c, &format!("RNFR {tmp}"), &[350]).await?;
                if expect(c, &format!("RNTO {full}"), &[250]).await.is_err() {
                    let _ = cmd(c, &format!("DELE {full}")).await?;
                    expect(c, &format!("RNFR {tmp}"), &[350]).await?;
                    expect(c, &format!("RNTO {full}"), &[250]).await?;
                }
                Ok(())
            })
        })
    }

    fn delete(&self, path: &str) -> NetResult<()> {
        let full = self.full(path)?;
        self.with(TIMEOUT, |c, _| {
            let full = full.clone();
            Box::pin(async move {
                let (code, text) = cmd(c, &format!("DELE {full}")).await?;
                if code == 250 || code == 550 || code == 450 {
                    Ok(())
                } else {
                    Err(proto(&text))
                }
            })
        })
    }

    fn mkdir_all(&self, dir: &str) -> NetResult<()> {
        let full = self.full(dir)?;
        self.with(TIMEOUT, |c, me| {
            let full = full.clone();
            Box::pin(async move { me.mkdirs(c, &full).await })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        assert_eq!(data_port(229, "229 Entering Extended Passive Mode (|||6446|)"), Some(6446));
        assert_eq!(data_port(227, "227 Entering Passive Mode (127,0,0,1,39,100)."), Some(39 * 256 + 100));
        let l = parse_list_line("-rw-r--r--    1 1000     1000         1234 Nov 11 18:00 Mein Dokument.pdf").unwrap();
        assert_eq!((l.name.as_str(), l.is_dir, l.size), ("Mein Dokument.pdf", false, 1234));
        assert!(parse_list_line("drwxr-xr-x    2 1000     1000         4096 Nov 11 18:00 suite").unwrap().is_dir);
        assert!(parse_list_line("drwxr-xr-x    2 1000     1000         4096 Nov 11 18:00 ..").is_none());
        let m = parse_mlsd_line("type=file;size=12;modify=20261111; a b.txt").unwrap();
        assert_eq!((m.name.as_str(), m.size), ("a b.txt", 12));
        assert!(parse_mlsd_line("type=cdir; .").is_none());
    }
}
