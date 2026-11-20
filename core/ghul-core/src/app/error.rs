// SPDX-License-Identifier: AGPL-3.0-or-later
//! Error type of the app API.

use crate::error::FidoError;
use crate::vault::VaultError;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GhulError {
    // --- security key
    #[error("USB communication failed: {msg}")]
    Transport { msg: String },
    #[error("timeout while waiting for the security key")]
    Timeout,
    #[error("no PIN set on the security key")]
    PinNotSet,
    #[error("wrong PIN ({retries} attempts left)")]
    PinInvalid { retries: u32 },
    #[error("PIN blocked")]
    PinBlocked,
    #[error("PIN temporarily blocked, reconnect the key")]
    PinAuthBlocked,
    #[error("PIN does not meet the key's policy")]
    PinPolicyViolation,
    #[error("security key does not support hmac-secret")]
    HmacSecretUnsupported,
    #[error("this security key is not registered for this vault")]
    KeyNotRegistered,
    #[error("not confirmed on the security key")]
    NotConfirmed,
    #[error("security key protocol error: {msg}")]
    KeyProtocol { msg: String },
    // --- vault
    #[error("wrong password")]
    WrongPassword,
    #[error("security key or recovery code does not match")]
    WrongSecondFactor,
    #[error("invalid recovery code")]
    InvalidRecoveryCode,
    #[error("vault is locked")]
    Locked,
    #[error("a vault already exists")]
    AlreadyExists,
    #[error("no vault found")]
    NoVault,
    #[error("entry not found")]
    NotFound,
    #[error("vault files are damaged or were modified: {msg}")]
    Corrupted { msg: String },
    #[error("storage error: {msg}")]
    Storage { msg: String },
    #[error("invalid input: {msg}")]
    InvalidInput { msg: String },
    #[error("this vault needs a security key")]
    SecurityKeyRequired,
    #[error("this vault needs its key file")]
    KeyfileRequired,
    #[error("this is not a key file of this vault")]
    WrongKeyfile,
    #[error("the last security key or key file cannot be removed")]
    LastFactor,
    /// The file was made by a newer GhulSuite (new file kind or format): update the app.
    #[error("please update GhulSuite to open this file")]
    UpdateRequired,
    /// The file was made by an older development version of GhulSuite (an older file format,
    /// before the first stable version): it is not read any more.
    #[error("this file was made by an older development version of GhulSuite and is not supported any more")]
    OldFormat,
    /// A file opens with the password and the second factor, but its write seal is wrong: it was
    /// made or changed by someone without the password (e.g. from an old copy and an old
    /// recovery code). It is not used.
    #[error("the file was changed without the password")]
    Tampered,
    /// A suite file with an older protection than this device saw (before a change of the
    /// password, a key or the recovery code): it is not opened.
    #[error("the file has an older protection")]
    ProtectionOutdated,
    #[error("this area has not been set up")]
    NotConfigured,
    /// GhulDocs holds as much as it may (the trash included): nothing
    /// was added. `needed`: bytes of what was to be added (0: not known, e.g. a source without a
    /// size that turned out too large while it was read).
    /// `suite`: all GhulDocs vaults together are full (`used` and `limit`, 3 GB, are the
    /// suite's); otherwise the vault is (its `used`, `limit` 1 GB).
    #[error("GhulDocs is full ({used} of {limit} bytes used, {needed} needed)")]
    DocsLimit { used: u64, limit: u64, needed: u64, suite: bool },
    // --- network (update check)
    #[error("network error: {msg}")]
    Network { msg: String },
    #[error("the server certificate is not trusted: {fingerprint}")]
    CertUntrusted { fingerprint: String },
    // --- import
    #[error("this export is encrypted; export it without encryption")]
    ImportEncrypted,
    #[error("unknown file format")]
    ImportUnknownFormat,
    #[error("the vault uses a different protection than this suite")]
    DifferentProtection,
    // --- quick unlock
    #[error("quick unlock is not set up")]
    QuickUnlockUnavailable,
    #[error("quick unlock has expired, a full unlock is required")]
    QuickUnlockExpired,
    #[error("wrong PIN or quick unlock key")]
    QuickUnlockInvalid,
}

impl From<FidoError> for GhulError {
    fn from(e: FidoError) -> Self {
        match e {
            FidoError::Transport { msg } => GhulError::Transport { msg },
            FidoError::Timeout => GhulError::Timeout,
            FidoError::PinNotSet => GhulError::PinNotSet,
            FidoError::PinInvalid { retries } => GhulError::PinInvalid { retries },
            FidoError::PinBlocked => GhulError::PinBlocked,
            FidoError::PinAuthBlocked => GhulError::PinAuthBlocked,
            FidoError::PinPolicyViolation => GhulError::PinPolicyViolation,
            FidoError::HmacSecretUnsupported => GhulError::HmacSecretUnsupported,
            FidoError::NoCredentials => GhulError::KeyNotRegistered,
            FidoError::NotConfirmed => GhulError::NotConfirmed,
            FidoError::InvalidInput { msg } => GhulError::InvalidInput { msg },
            other => GhulError::KeyProtocol { msg: other.to_string() },
        }
    }
}

impl From<VaultError> for GhulError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::WrongPassword => GhulError::WrongPassword,
            VaultError::WrongSecondFactor | VaultError::SlotNotFound => GhulError::WrongSecondFactor,
            VaultError::InvalidRecoveryCode => GhulError::InvalidRecoveryCode,
            VaultError::WrongKeyfile | VaultError::NoKeyfileSlot | VaultError::InvalidKeyfile => {
                GhulError::WrongKeyfile
            }
            VaultError::InvalidInput(msg) => GhulError::InvalidInput { msg },
            VaultError::LastSlot => GhulError::LastFactor,
            VaultError::NewerKind => GhulError::UpdateRequired,
            VaultError::Tampered => GhulError::Tampered,
            VaultError::UnsupportedVersion(v) if v > crate::vault::format::FORMAT_VERSION => GhulError::UpdateRequired,
            VaultError::UnsupportedVersion(_) => GhulError::OldFormat,
            other => GhulError::Corrupted { msg: other.to_string() },
        }
    }
}

impl From<crate::net::NetError> for GhulError {
    fn from(e: crate::net::NetError) -> Self {
        use crate::net::NetError as N;
        match e {
            N::CertUntrusted(fingerprint) => GhulError::CertUntrusted { fingerprint },
            N::Timeout => GhulError::Network { msg: "timeout".into() },
            N::Connect(m) if host_not_found(&m) => GhulError::Network { msg: "server address not found".into() },
            N::Connect(m) if refused(&m) => GhulError::Network { msg: "connection refused".into() },
            other => GhulError::Network { msg: other.to_string() },
        }
    }
}

/// The message of a failed name lookup (a misspelt server name or no network), so the app shows
/// a translated text instead of the system's: on Unix and Android `getaddrinfo` ("failed to
/// lookup address information: ..."), on Windows WSAHOST_NOT_FOUND (11001), WSATRY_AGAIN (11002)
/// and WSANO_DATA (11004).
fn host_not_found(m: &str) -> bool {
    m.contains("failed to lookup address information")
        || (cfg!(windows) && ["os error 11001)", "os error 11002)", "os error 11004)"].iter().any(|c| m.contains(c)))
}

/// ECONNREFUSED: nothing listens on the port. The number differs per system (and 61 means
/// something else on Linux), so only this system's number counts.
fn refused(m: &str) -> bool {
    let code = if cfg!(windows) {
        "os error 10061)"
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        "os error 61)"
    } else {
        "os error 111)"
    };
    m.contains(code)
}

impl From<std::io::Error> for GhulError {
    fn from(e: std::io::Error) -> Self {
        GhulError::Storage { msg: e.to_string() }
    }
}

impl From<crate::totp::OtpError> for GhulError {
    fn from(e: crate::totp::OtpError) -> Self {
        GhulError::InvalidInput { msg: e.to_string() }
    }
}

pub type Result<T> = std::result::Result<T, GhulError>;
