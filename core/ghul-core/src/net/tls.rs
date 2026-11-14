// SPDX-License-Identifier: AGPL-3.0-or-later
//! TLS for IMAP and SMTP (rustls with ring and the Mozilla root certificates).
//!
//! Certificates are checked normally. For own servers with a self-signed certificate the user
//! can confirm the certificate's SHA-256 fingerprint once; exactly that certificate is then
//! trusted (pinning). An untrusted certificate is reported with its fingerprint.

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use super::{NetError, NetResult};

/// Lower-case hex SHA-256 of a certificate.
pub fn fingerprint(der: &[u8]) -> String {
    crate::kdf::hex(&Sha256::digest(der))
}

#[derive(Debug)]
struct Verifier {
    inner: Arc<rustls::client::WebPkiServerVerifier>,
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl Verifier {
    fn new(
        roots: rustls::RootCertStore,
        provider: Arc<rustls::crypto::CryptoProvider>,
        pinned: Option<&str>,
    ) -> NetResult<Verifier> {
        let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        Ok(Verifier { inner, pinned: pinned.map(str::to_lowercase), seen: Arc::new(Mutex::new(None)) })
    }
}

/// The certificate verifier of [`Client`], but trusting only the given roots instead of the
/// Mozilla ones. For tests against certificate test suites (x509-limbo).
#[cfg(test)]
pub(crate) fn verifier_with_roots(
    roots: rustls::RootCertStore,
    pinned: Option<&str>,
) -> NetResult<Arc<dyn ServerCertVerifier>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Ok(Arc::new(Verifier::new(roots, provider, pinned)?))
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity);
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = Some(fp.clone());
        if self.pinned.as_deref() == Some(fp.as_str()) {
            return Ok(ServerCertVerified::assertion());
        }
        self.inner.verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// A TLS client configuration that remembers the certificate it saw. One configuration can be
/// used for several connections, which lets TLS sessions resume (needed for FTPS data channels).
pub struct Client {
    config: Arc<rustls::ClientConfig>,
    seen: Arc<Mutex<Option<String>>>,
    pinned: Option<String>,
}

impl Client {
    /// `pinned`: a confirmed certificate fingerprint (self-signed own servers).
    pub fn new(pinned: Option<&str>) -> NetResult<Client> {
        Self::with_versions(pinned, rustls::DEFAULT_VERSIONS)
    }

    /// FTPS: TLS 1.2 only. FTPS servers demand that every data connection resumes the control
    /// connection's TLS session; TLS 1.3 tickets are single-use, TLS 1.2 sessions can be resumed
    /// again and again. Still forward secret (ECDHE) with AEAD ciphers only.
    pub fn for_ftps(pinned: Option<&str>) -> NetResult<Client> {
        Self::with_versions(pinned, &[&rustls::version::TLS12])
    }

    fn with_versions(
        pinned: Option<&str>,
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> NetResult<Client> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let verifier = Verifier::new(roots, provider.clone(), pinned)?;
        let (seen, pinned) = (verifier.seen.clone(), verifier.pinned.clone());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(versions)
            .map_err(|e| NetError::Protocol(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        Ok(Client { config: Arc::new(config), seen, pinned })
    }

    /// Starts TLS on a connected socket.
    pub async fn connect<S>(&self, tcp: S, host: &str) -> NetResult<tokio_rustls::client::TlsStream<S>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let name =
            ServerName::try_from(host.to_owned()).map_err(|_| NetError::Connect(format!("invalid host {host}")))?;
        match tokio_rustls::TlsConnector::from(self.config.clone()).connect(name, tcp).await {
            Ok(s) => Ok(s),
            Err(e) => {
                let fp = self.seen.lock().unwrap_or_else(|p| p.into_inner()).clone();
                Err(match fp {
                    Some(fp) if self.pinned.as_deref() != Some(fp.as_str()) => NetError::CertUntrusted(fp),
                    _ => NetError::Connect(e.to_string()),
                })
            }
        }
    }
}

/// Starts TLS on a connected socket. `pinned`: a confirmed certificate fingerprint.
pub async fn start(tcp: TcpStream, host: &str, pinned: Option<&str>) -> NetResult<TlsStream<TcpStream>> {
    Client::new(pinned)?.connect(tcp, host).await
}

/// Base64 (standard, with padding), e.g. for SMTP AUTH.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = c.iter().enumerate().fold(0u32, |a, (i, b)| a | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= c.len() { T[((n >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
        }
    }
    out
}
