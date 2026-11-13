// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulSuite core.
//!
//! Phase 0b prototype: FIDO2 (CTAP2 over HID) with `hmac-secret` and a first key derivation
//! test combining Argon2id (password) with the security key secret. Secrets never leave the
//! core: the app only receives fingerprints.

use std::sync::Arc;
use std::time::Instant;

pub mod app;
pub mod cbor;
pub mod ctap2;
pub mod ctaphid;
pub mod error;
pub mod kdf;
pub mod net;
pub mod pin;
pub mod share;
pub mod totp;
pub mod vault;

#[cfg(any(test, feature = "mock"))]
mod mock;

#[cfg(test)]
mod wycheproof_tests;

#[cfg(test)]
mod fuzz_tests;

pub use error::{FidoError, TransportError};

use ctap2::{prf_salt, Authenticator};
use ctaphid::CtapHid;
use zeroize::Zeroizing;

pub mod key_device;

uniffi::setup_scaffolding!();

/// Relying party ID for all GhulSuite credentials. `.invalid` is a reserved TLD
/// (RFC 6761), so no website can ever request assertions for it.
pub const RP_ID: &str = "ghulsuite.invalid";
pub const RP_NAME: &str = "GhulSuite";

/// PRF input used by the prototype unlock test.
const PROTOTYPE_PRF_INPUT: &[u8] = b"GhulSuite prototype 0b";
const PROTOTYPE_ARGON2_SALT: &[u8] = b"GhulSuite prototype 0b salt";
const PROTOTYPE_ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const PROTOTYPE_ARGON2_ITERATIONS: u32 = 3;
const PROTOTYPE_ARGON2_PARALLELISM: u32 = 1;

/// Raw HID report transport implemented by the platform (Android: USB host API).
#[uniffi::export(with_foreign)]
pub trait HidTransport: Send + Sync {
    /// HID report size in bytes (64 for virtually all FIDO keys).
    fn report_size(&self) -> u32;
    /// Writes one output report (exactly `report_size` bytes, no report ID).
    fn write_report(&self, report: Vec<u8>) -> Result<(), TransportError>;
    /// Reads one input report; returns `None` if nothing arrived within `timeout_ms`.
    fn read_report(&self, timeout_ms: u32) -> Result<Option<Vec<u8>>, TransportError>;
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidoStatus {
    Processing,
    TouchNeeded,
}

/// Receives status updates while waiting for the key (e.g. "touch your key").
#[uniffi::export(with_foreign)]
pub trait FidoListener: Send + Sync {
    fn on_status(&self, status: FidoStatus);
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct DeviceInfo {
    pub ctaphid_version: String,
    pub device_version: String,
    pub versions: Vec<String>,
    pub extensions: Vec<String>,
    pub aaguid: String,
    pub options: Vec<String>,
    pub pin_protocols: Vec<u32>,
    pub max_msg_size: Option<u32>,
    pub min_pin_length: Option<u32>,
    pub firmware_version: Option<u32>,
    pub pin_set: bool,
    pub pin_retries: Option<u32>,
    pub supports_hmac_secret: bool,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct Registration {
    pub credential_id: Vec<u8>,
    pub hmac_secret_enabled: bool,
    pub rp_id: String,
    pub pin_protocol: u32,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct UnlockTest {
    /// Fingerprint of the key's hmac-secret output (not the secret itself).
    pub key_fingerprint: String,
    pub user_verified: bool,
    pub user_present: bool,
    /// Fingerprint of the derived key-encryption key (password + key).
    pub kek_fingerprint: String,
    pub argon2_ms: u64,
    pub key_ms: u64,
    pub pin_protocol: u32,
}

#[uniffi::export]
pub fn core_version() -> String {
    format!("ghul-core {} (RP-ID {RP_ID})", env!("CARGO_PKG_VERSION"))
}

fn open<'a>(
    transport: &'a dyn HidTransport,
    listener: Option<&'a dyn FidoListener>,
) -> Result<Authenticator<'a>, FidoError> {
    Authenticator::new(CtapHid::open(transport, listener)?)
}

#[uniffi::export]
pub fn fido_get_info(transport: Arc<dyn HidTransport>) -> Result<DeviceInfo, FidoError> {
    let auth = open(transport.as_ref(), None)?;
    let i = &auth.info;
    let hid = &auth.hid().info;
    let pin_set = i.option("clientPin") == Some(true);
    Ok(DeviceInfo {
        ctaphid_version: format!("{} (caps 0x{:02x})", hid.protocol_version, hid.capabilities),
        device_version: format!("{}.{}.{}", hid.device_version.0, hid.device_version.1, hid.device_version.2),
        versions: i.versions.clone(),
        extensions: i.extensions.clone(),
        aaguid: kdf::hex(&i.aaguid),
        options: i.options.iter().map(|(k, v)| format!("{k}={v}")).collect(),
        pin_protocols: i.pin_protocols.iter().map(|p| *p as u32).collect(),
        max_msg_size: i.max_msg_size.map(|v| v as u32),
        min_pin_length: i.min_pin_length.map(|v| v as u32),
        firmware_version: i.firmware_version.map(|v| v as u32),
        pin_set,
        pin_retries: if pin_set { auth.pin_retries().ok() } else { None },
        supports_hmac_secret: i.supports_hmac_secret(),
    })
}

#[uniffi::export]
pub fn fido_set_pin(transport: Arc<dyn HidTransport>, new_pin: String) -> Result<(), FidoError> {
    let new_pin = Zeroizing::new(new_pin);
    let auth = open(transport.as_ref(), None)?;
    if auth.info.option("clientPin") == Some(true) {
        return Err(FidoError::InvalidInput { msg: "a PIN is already set".into() });
    }
    auth.set_pin(&new_pin)
}

#[uniffi::export]
pub fn fido_register(
    transport: Arc<dyn HidTransport>,
    listener: Arc<dyn FidoListener>,
    pin: String,
) -> Result<Registration, FidoError> {
    let pin = Zeroizing::new(pin);
    let auth = open(transport.as_ref(), Some(listener.as_ref()))?;
    let proto = auth.info.pin_protocol()?;
    let cred = auth.make_credential(RP_ID, RP_NAME, &pin)?;
    Ok(Registration {
        credential_id: cred.id,
        hmac_secret_enabled: cred.hmac_secret,
        rp_id: RP_ID.to_owned(),
        pin_protocol: proto.id() as u32,
    })
}

/// Prototype of the unlock path: key secret via hmac-secret (with PIN) + Argon2id(password)
/// combined with HKDF. Returns only fingerprints.
#[uniffi::export]
pub fn fido_unlock_test(
    transport: Arc<dyn HidTransport>,
    listener: Arc<dyn FidoListener>,
    pin: String,
    credential_id: Vec<u8>,
    password: String,
) -> Result<UnlockTest, FidoError> {
    let pin = Zeroizing::new(pin);
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err(FidoError::InvalidInput { msg: "password must not be empty".into() });
    }
    let auth = open(transport.as_ref(), Some(listener.as_ref()))?;
    let proto = auth.info.pin_protocol()?;

    let t0 = Instant::now();
    let salt = prf_salt(PROTOTYPE_PRF_INPUT);
    let out = auth.hmac_secret(RP_ID, &credential_id, &salt, &pin)?;
    let key_ms = t0.elapsed().as_millis() as u64;

    let t1 = Instant::now();
    let pk = kdf::argon2id(
        password.as_bytes(),
        PROTOTYPE_ARGON2_SALT,
        PROTOTYPE_ARGON2_MEMORY_KIB,
        PROTOTYPE_ARGON2_ITERATIONS,
        PROTOTYPE_ARGON2_PARALLELISM,
    )
    .map_err(|msg| FidoError::InvalidInput { msg })?;
    let argon2_ms = t1.elapsed().as_millis() as u64;

    let mut ikm = Zeroizing::new(Vec::with_capacity(64));
    ikm.extend_from_slice(pk.as_ref());
    ikm.extend_from_slice(&out.output);
    let kek = kdf::hkdf32(&ikm, b"ghul/v1/kek/prototype-0b");

    Ok(UnlockTest {
        key_fingerprint: kdf::fingerprint(&out.output),
        user_verified: out.user_verified,
        user_present: out.user_present,
        kek_fingerprint: kdf::fingerprint(kek.as_ref()),
        argon2_ms,
        key_ms,
        pin_protocol: proto.id() as u32,
    })
}

/// Measures Argon2id duration on this device (milliseconds).
#[uniffi::export]
pub fn argon2_benchmark(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<u64, FidoError> {
    let t = Instant::now();
    kdf::argon2id(b"benchmark", b"ghulsuite-benchmark", memory_kib, iterations, parallelism)
        .map_err(|msg| FidoError::InvalidInput { msg })?;
    Ok(t.elapsed().as_millis() as u64)
}
