// SPDX-License-Identifier: AGPL-3.0-or-later
//! Errors of the container (vault) layer.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    /// The file is not a GhulSuite container or is structurally invalid.
    #[error("invalid container: {0}")]
    Format(String),
    #[error("unsupported format version {0}")]
    UnsupportedVersion(u16),
    #[error("container has kind {found}, expected {expected}")]
    WrongKind { expected: &'static str, found: &'static str },
    /// The second factor (security key output or recovery code) does not open any slot.
    #[error("security key or recovery code does not match this container")]
    WrongSecondFactor,
    #[error("wrong password")]
    WrongPassword,
    #[error("this container has no key file slot")]
    NoKeyfileSlot,
    #[error("this is not a key file of this container")]
    WrongKeyfile,
    /// Payload authentication failed: the file was modified or damaged.
    #[error("container content is damaged or was modified")]
    Corrupted,
    #[error("payload: {0}")]
    Payload(String),
    #[error("slot not found")]
    SlotNotFound,
    #[error("the last slot cannot be removed")]
    LastSlot,
    #[error("invalid recovery code")]
    InvalidRecoveryCode,
    #[error("invalid key file")]
    InvalidKeyfile,
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl VaultError {
    pub(crate) fn format(msg: impl Into<String>) -> Self {
        VaultError::Format(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;
