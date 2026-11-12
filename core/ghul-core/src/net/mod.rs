// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network access: SFTP for storage and GhulDrive, IMAP/SMTP for GhulMail.
//!
//! The app calls blocking functions on a background thread; inside, one shared Tokio runtime
//! drives the asynchronous protocol libraries. Everything is pure Rust (rustls with ring,
//! russh), so it builds the same for Android and Windows.

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

pub mod ftps;
pub mod http;
pub mod imap;
pub mod phpfs;
pub mod remote;
pub mod sftp;
pub mod smtp;
pub mod tls;

#[cfg(test)]
pub(crate) mod tests;

/// Errors of network operations (mapped to `GhulError` at the API boundary).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NetError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("timeout")]
    Timeout,
    /// First connection: the server's key is not yet confirmed by the user.
    #[error("unknown server key {0}")]
    HostKeyUnknown(String),
    /// The server presents another key than confirmed before (possible attack).
    #[error("server key changed to {0}")]
    HostKeyChanged(String),
    #[error("login rejected")]
    AuthFailed,
    /// The TLS certificate is not trusted; the user may confirm its fingerprint.
    #[error("untrusted certificate {0}")]
    CertUntrusted(String),
    #[error("server error: {0}")]
    Protocol(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub type NetResult<T> = std::result::Result<T, NetError>;

/// Time limit for connecting and for each single request.
pub const TIMEOUT: Duration = Duration::from_secs(30);

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ghul-net")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Runs a future to completion on the shared runtime, with a time limit.
pub(crate) fn block_on<T>(f: impl Future<Output = NetResult<T>> + Send, limit: Duration) -> NetResult<T>
where
    T: Send,
{
    runtime().block_on(async move { tokio::time::timeout(limit, f).await.map_err(|_| NetError::Timeout)? })
}
