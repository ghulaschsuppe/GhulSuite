// SPDX-License-Identifier: AGPL-3.0-or-later
//! Network access: HTTPS for the update check (and the optional password leak check of GhulPass).
//!
//! The app calls blocking functions on a background thread; inside, one shared Tokio runtime
//! drives the asynchronous I/O. Everything is pure Rust (rustls with ring), so it builds the same
//! for Android and Windows.

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

pub mod http;
pub mod tls;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_limbo;

/// Errors of network operations (mapped to `GhulError` at the API boundary).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NetError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("timeout")]
    Timeout,
    /// The TLS certificate is not trusted; the user may confirm its fingerprint.
    #[error("untrusted certificate {0}")]
    CertUntrusted(String),
    #[error("server error: {0}")]
    Protocol(String),
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
