// SPDX-License-Identifier: AGPL-3.0-or-later
//! Error types exposed to the app.

/// Error raised by the platform transport (USB HID on Android).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TransportError {
    #[error("{msg}")]
    Io { msg: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for TransportError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        TransportError::Io { msg: e.reason }
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FidoError {
    #[error("transport: {msg}")]
    Transport { msg: String },
    #[error("CTAPHID error 0x{code:02x}")]
    Hid { code: u8 },
    #[error("CTAP2 error 0x{code:02x} ({name})")]
    Ctap { code: u8, name: String },
    #[error("protocol: {msg}")]
    Protocol { msg: String },
    #[error("timeout")]
    Timeout,
    #[error("no PIN set on the key")]
    PinNotSet,
    #[error("wrong PIN ({retries} attempts left)")]
    PinInvalid { retries: u32 },
    #[error("PIN blocked")]
    PinBlocked,
    #[error("PIN temporarily blocked, reconnect the key")]
    PinAuthBlocked,
    #[error("PIN does not meet the key's policy")]
    PinPolicyViolation,
    #[error("key does not support hmac-secret")]
    HmacSecretUnsupported,
    #[error("credential not found on this key")]
    NoCredentials,
    #[error("operation denied or not confirmed on the key")]
    NotConfirmed,
    #[error("invalid input: {msg}")]
    InvalidInput { msg: String },
}

impl From<TransportError> for FidoError {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Io { msg } => FidoError::Transport { msg },
        }
    }
}

impl FidoError {
    pub fn protocol(msg: impl Into<String>) -> Self {
        FidoError::Protocol { msg: msg.into() }
    }

    /// Maps a CTAP2 status code to an error.
    pub fn from_ctap(code: u8) -> Self {
        match code {
            0x2E => FidoError::NoCredentials,
            0x27 | 0x2F => FidoError::NotConfirmed,
            0x32 => FidoError::PinBlocked,
            0x34 => FidoError::PinAuthBlocked,
            0x35 => FidoError::PinNotSet,
            0x37 => FidoError::PinPolicyViolation,
            _ => FidoError::Ctap { code, name: ctap_status_name(code).to_owned() },
        }
    }
}

pub fn ctap_status_name(code: u8) -> &'static str {
    match code {
        0x00 => "SUCCESS",
        0x01 => "INVALID_COMMAND",
        0x02 => "INVALID_PARAMETER",
        0x03 => "INVALID_LENGTH",
        0x04 => "INVALID_SEQ",
        0x05 => "TIMEOUT",
        0x06 => "CHANNEL_BUSY",
        0x0A => "LOCK_REQUIRED",
        0x0B => "INVALID_CHANNEL",
        0x11 => "CBOR_UNEXPECTED_TYPE",
        0x12 => "INVALID_CBOR",
        0x14 => "MISSING_PARAMETER",
        0x15 => "LIMIT_EXCEEDED",
        0x19 => "CREDENTIAL_EXCLUDED",
        0x21 => "PROCESSING",
        0x22 => "INVALID_CREDENTIAL",
        0x23 => "USER_ACTION_PENDING",
        0x24 => "OPERATION_PENDING",
        0x25 => "NO_OPERATIONS",
        0x26 => "UNSUPPORTED_ALGORITHM",
        0x27 => "OPERATION_DENIED",
        0x28 => "KEY_STORE_FULL",
        0x2B => "UNSUPPORTED_OPTION",
        0x2C => "INVALID_OPTION",
        0x2D => "KEEPALIVE_CANCEL",
        0x2E => "NO_CREDENTIALS",
        0x2F => "USER_ACTION_TIMEOUT",
        0x30 => "NOT_ALLOWED",
        0x31 => "PIN_INVALID",
        0x32 => "PIN_BLOCKED",
        0x33 => "PIN_AUTH_INVALID",
        0x34 => "PIN_AUTH_BLOCKED",
        0x35 => "PIN_NOT_SET",
        0x36 => "PUAT_REQUIRED",
        0x37 => "PIN_POLICY_VIOLATION",
        0x39 => "REQUEST_TOO_LARGE",
        0x3A => "ACTION_TIMEOUT",
        0x3B => "UP_REQUIRED",
        0x3C => "UV_BLOCKED",
        0x3D => "INTEGRITY_FAILURE",
        0x3E => "INVALID_SUBCOMMAND",
        0x3F => "UV_INVALID",
        0x40 => "UNAUTHORIZED_PERMISSION",
        _ => "UNKNOWN",
    }
}
