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
    // --- servers and network
    #[error("network error: {msg}")]
    Network { msg: String },
    #[error("server key not confirmed yet: {fingerprint}")]
    HostKeyUnknown { fingerprint: String },
    #[error("the server key changed to {fingerprint}")]
    HostKeyChanged { fingerprint: String },
    #[error("the server certificate is not trusted: {fingerprint}")]
    CertUntrusted { fingerprint: String },
    #[error("the server rejected the login")]
    ServerAuthFailed,
    #[error("the password was changed on another device")]
    PasswordChangedElsewhere,
    /// A key was removed and the recovery code renewed on another device: one touch of a
    /// remaining key (or the key file) takes the new protection over (`adopt_remote_protection`).
    #[error("the protection was changed on another device")]
    ProtectionChangedElsewhere,
    #[error("no server set up for this")]
    NotConfigured,
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
            VaultError::UnsupportedVersion(v) if v > crate::vault::format::FORMAT_VERSION => GhulError::UpdateRequired,
            other => GhulError::Corrupted { msg: other.to_string() },
        }
    }
}

impl From<crate::net::NetError> for GhulError {
    fn from(e: crate::net::NetError) -> Self {
        use crate::net::NetError as N;
        match e {
            N::HostKeyUnknown(fingerprint) => GhulError::HostKeyUnknown { fingerprint },
            N::HostKeyChanged(fingerprint) => GhulError::HostKeyChanged { fingerprint },
            N::AuthFailed => GhulError::ServerAuthFailed,
            N::CertUntrusted(fingerprint) => GhulError::CertUntrusted { fingerprint },
            N::Timeout => GhulError::Network { msg: "timeout".into() },
            other => GhulError::Network { msg: other.to_string() },
        }
    }
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
