// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minimal SMTP submission client: TLS (465) or STARTTLS (587), AUTH PLAIN, one message.

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

use super::imap::Security;
use super::{tls, NetError, NetResult};

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub user: String,
    pub password: Zeroizing<String>,
    pub cert: Option<String>,
}

struct Conn<S> {
    io: BufReader<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Conn<S> {
    fn new(s: S) -> Self {
        Conn { io: BufReader::new(s) }
    }

    async fn reply(&mut self) -> NetResult<(u16, String)> {
        let mut text = String::new();
        loop {
            let mut line = String::new();
            let n = self.io.read_line(&mut line).await.map_err(|e| NetError::Connect(e.to_string()))?;
            if n == 0 || line.len() < 4 {
                return Err(NetError::Protocol("connection closed".into()));
            }
            text.push_str(line.trim_end());
            text.push('\n');
            if line.as_bytes()[3] != b'-' {
                let code = line[..3].parse().map_err(|_| NetError::Protocol(line.clone()))?;
                return Ok((code, text));
            }
        }
    }

    async fn write(&mut self, data: &[u8]) -> NetResult<()> {
        self.io.get_mut().write_all(data).await.map_err(|e| NetError::Connect(e.to_string()))?;
        self.io.get_mut().flush().await.map_err(|e| NetError::Connect(e.to_string()))
    }

    async fn cmd(&mut self, line: &str, expect: u16) -> NetResult<String> {
        self.write(format!("{line}\r\n").as_bytes()).await?;
        let (code, text) = self.reply().await?;
        if code != expect {
            return Err(if code == 535 || code == 530 { NetError::AuthFailed } else { NetError::Protocol(text) });
        }
        Ok(text)
    }

    async fn greeting(&mut self) -> NetResult<()> {
        let (code, text) = self.reply().await?;
        if code != 220 {
            return Err(NetError::Protocol(text));
        }
        Ok(())
    }
}

fn clean_addr(a: &str) -> NetResult<&str> {
    let a = a.trim();
    if a.is_empty() || a.contains(['<', '>', '\r', '\n', ' ']) {
        return Err(NetError::Protocol(format!("invalid address {a}")));
    }
    Ok(a)
}

/// Sends `message` (complete RFC 5322 text) from `from` to `to`.
pub async fn send(cfg: &SmtpConfig, from: &str, to: &[String], message: &[u8]) -> NetResult<()> {
    let tcp = TcpStream::connect((cfg.host.as_str(), cfg.port)).await.map_err(|e| NetError::Connect(e.to_string()))?;
    match cfg.security {
        Security::Tls => {
            let mut c = Conn::new(tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?);
            c.greeting().await?;
            submit(c, cfg, from, to, message).await
        }
        Security::StartTls => {
            let mut c = Conn::new(tcp);
            c.greeting().await?;
            c.cmd("EHLO ghulsuite.invalid", 250).await?;
            c.cmd("STARTTLS", 220).await?;
            // Nothing is buffered after the 220 reply: the socket goes on with TLS.
            let tcp = c.io.into_inner();
            let c = Conn::new(tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?);
            submit(c, cfg, from, to, message).await
        }
    }
}

/// Connects, starts TLS and logs in, without sending anything (account check).
pub async fn login_only(cfg: &SmtpConfig) -> NetResult<()> {
    let tcp = TcpStream::connect((cfg.host.as_str(), cfg.port)).await.map_err(|e| NetError::Connect(e.to_string()))?;
    async fn auth<S: AsyncRead + AsyncWrite + Unpin>(mut c: Conn<S>, cfg: &SmtpConfig) -> NetResult<()> {
        c.cmd("EHLO ghulsuite.invalid", 250).await?;
        let auth = Zeroizing::new(format!("\0{}\0{}", cfg.user, cfg.password.as_str()));
        c.cmd(&format!("AUTH PLAIN {}", tls::base64(auth.as_bytes())), 235).await?;
        let _ = c.cmd("QUIT", 221).await;
        Ok(())
    }
    match cfg.security {
        Security::Tls => {
            let mut c = Conn::new(tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?);
            c.greeting().await?;
            auth(c, cfg).await
        }
        Security::StartTls => {
            let mut c = Conn::new(tcp);
            c.greeting().await?;
            c.cmd("EHLO ghulsuite.invalid", 250).await?;
            c.cmd("STARTTLS", 220).await?;
            let tcp = c.io.into_inner();
            auth(Conn::new(tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?), cfg).await
        }
    }
}

async fn submit<S: AsyncRead + AsyncWrite + Unpin>(
    mut c: Conn<S>,
    cfg: &SmtpConfig,
    from: &str,
    to: &[String],
    message: &[u8],
) -> NetResult<()> {
    c.cmd("EHLO ghulsuite.invalid", 250).await?;
    let auth = Zeroizing::new(format!("\0{}\0{}", cfg.user, cfg.password.as_str()));
    c.cmd(&format!("AUTH PLAIN {}", tls::base64(auth.as_bytes())), 235).await?;
    c.cmd(&format!("MAIL FROM:<{}>", clean_addr(from)?), 250).await?;
    for r in to {
        c.cmd(&format!("RCPT TO:<{}>", clean_addr(r)?), 250).await?;
    }
    c.cmd("DATA", 354).await?;
    let mut body = Vec::with_capacity(message.len() + 64);
    let text = String::from_utf8_lossy(message).replace("\r\n", "\n");
    for line in text.split('\n') {
        if line.starts_with('.') {
            body.push(b'.');
        }
        body.extend_from_slice(line.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b".\r\n");
    c.write(&body).await?;
    let (code, text) = c.reply().await?;
    if code != 250 {
        return Err(NetError::Protocol(text));
    }
    let _ = c.cmd("QUIT", 221).await;
    Ok(())
}
