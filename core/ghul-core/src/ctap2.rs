// SPDX-License-Identifier: AGPL-3.0-or-later
//! CTAP2 client: getInfo, clientPIN, makeCredential and getAssertion with `hmac-secret`.

use std::time::Duration;

use ciborium::value::Value;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::cbor::{as_bytes, as_u64, decode, decode_prefix, map_get_int, map_get_text, Cbor};
use crate::ctaphid::CtapHid;
use crate::error::FidoError;
use crate::pin::{encapsulate, PinProtocol, SharedSecret};

const CMD_MAKE_CREDENTIAL: u8 = 0x01;
const CMD_GET_ASSERTION: u8 = 0x02;
const CMD_GET_INFO: u8 = 0x04;
const CMD_CLIENT_PIN: u8 = 0x06;

const PIN_GET_RETRIES: i64 = 0x01;
const PIN_GET_KEY_AGREEMENT: i64 = 0x02;
const PIN_SET_PIN: i64 = 0x03;
const PIN_GET_TOKEN: i64 = 0x05;
const PIN_GET_TOKEN_WITH_PERMISSIONS: i64 = 0x09;

pub const PERMISSION_MAKE_CREDENTIAL: i64 = 0x01;
pub const PERMISSION_GET_ASSERTION: i64 = 0x02;

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;
const FLAG_ED: u8 = 0x80;

const SHORT: Duration = Duration::from_secs(5);
const USER_ACTION: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone)]
pub struct Info {
    pub versions: Vec<String>,
    pub extensions: Vec<String>,
    pub aaguid: Vec<u8>,
    pub options: Vec<(String, bool)>,
    pub max_msg_size: Option<u64>,
    pub pin_protocols: Vec<u64>,
    pub min_pin_length: Option<u64>,
    pub firmware_version: Option<u64>,
}

impl Info {
    pub fn option(&self, name: &str) -> Option<bool> {
        self.options.iter().find(|(k, _)| k == name).map(|(_, v)| *v)
    }

    pub fn supports_hmac_secret(&self) -> bool {
        self.extensions.iter().any(|e| e == "hmac-secret")
    }

    /// Picks the best PIN/UV auth protocol the key supports.
    pub fn pin_protocol(&self) -> Result<PinProtocol, FidoError> {
        if self.pin_protocols.contains(&2) {
            Ok(PinProtocol::V2)
        } else if self.pin_protocols.contains(&1) || self.pin_protocols.is_empty() {
            // CTAP 2.0 keys may omit the list; protocol 1 is mandatory for them.
            Ok(PinProtocol::V1)
        } else {
            Err(FidoError::protocol("no supported PIN protocol"))
        }
    }

    fn supports_token_permissions(&self) -> bool {
        self.option("pinUvAuthToken") == Some(true) || self.versions.iter().any(|v| v == "FIDO_2_1")
    }
}

pub struct Credential {
    pub id: Vec<u8>,
    pub hmac_secret: bool,
}

pub struct HmacSecretOutput {
    pub output: Zeroizing<Vec<u8>>,
    pub user_verified: bool,
    pub user_present: bool,
}

pub struct Authenticator<'a> {
    hid: CtapHid<'a>,
    pub info: Info,
}

impl<'a> Authenticator<'a> {
    pub fn new(hid: CtapHid<'a>) -> Result<Self, FidoError> {
        let mut a = Authenticator { hid, info: Info::default() };
        a.info = a.get_info()?;
        Ok(a)
    }

    pub fn hid(&self) -> &CtapHid<'a> {
        &self.hid
    }

    fn command(&self, cmd: u8, params: Option<Cbor>, timeout: Duration) -> Result<Value, FidoError> {
        let mut payload = vec![cmd];
        if let Some(p) = params {
            payload.extend_from_slice(&p.encode());
        }
        let resp = self.hid.cbor(&payload, timeout)?;
        let status = *resp.first().ok_or_else(|| FidoError::protocol("empty CTAP2 response"))?;
        if status != 0x00 {
            return Err(FidoError::from_ctap(status));
        }
        if resp.len() == 1 {
            return Ok(Value::Map(vec![]));
        }
        decode(&resp[1..]).map_err(FidoError::protocol)
    }

    fn get_info(&self) -> Result<Info, FidoError> {
        let v = self.command(CMD_GET_INFO, None, SHORT)?;
        let texts = |key| -> Vec<String> {
            map_get_int(&v, key)
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_text().map(str::to_owned)).collect())
                .unwrap_or_default()
        };
        let options = map_get_int(&v, 0x04)
            .and_then(|m| m.as_map())
            .map(|m| m.iter().filter_map(|(k, val)| Some((k.as_text()?.to_owned(), val.as_bool()?))).collect())
            .unwrap_or_default();
        Ok(Info {
            versions: texts(0x01),
            extensions: texts(0x02),
            aaguid: map_get_int(&v, 0x03).and_then(as_bytes).map(<[u8]>::to_vec).unwrap_or_default(),
            options,
            max_msg_size: map_get_int(&v, 0x05).and_then(as_u64),
            pin_protocols: map_get_int(&v, 0x06)
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(as_u64).collect())
                .unwrap_or_default(),
            min_pin_length: map_get_int(&v, 0x0D).and_then(as_u64),
            firmware_version: map_get_int(&v, 0x0E).and_then(as_u64),
        })
    }

    pub fn pin_retries(&self) -> Result<u32, FidoError> {
        let proto = self.info.pin_protocol()?;
        let v = self.command(
            CMD_CLIENT_PIN,
            Some(Cbor::Map(vec![(Cbor::Int(1), Cbor::Int(proto.id())), (Cbor::Int(2), Cbor::Int(PIN_GET_RETRIES))])),
            SHORT,
        )?;
        map_get_int(&v, 0x03)
            .and_then(as_u64)
            .map(|r| r as u32)
            .ok_or_else(|| FidoError::protocol("missing pinRetries"))
    }

    /// Runs getKeyAgreement and derives a fresh shared secret.
    fn shared_secret(&self, proto: PinProtocol) -> Result<(Cbor, SharedSecret), FidoError> {
        let v = self.command(
            CMD_CLIENT_PIN,
            Some(Cbor::Map(vec![
                (Cbor::Int(1), Cbor::Int(proto.id())),
                (Cbor::Int(2), Cbor::Int(PIN_GET_KEY_AGREEMENT)),
            ])),
            SHORT,
        )?;
        let peer = map_get_int(&v, 0x01).ok_or_else(|| FidoError::protocol("missing keyAgreement"))?;
        encapsulate(proto, peer).map_err(FidoError::protocol)
    }

    pub fn set_pin(&self, new_pin: &str) -> Result<(), FidoError> {
        let pin = new_pin.as_bytes();
        let min = self.info.min_pin_length.unwrap_or(4) as usize;
        if new_pin.chars().count() < min || pin.len() > 63 {
            return Err(FidoError::InvalidInput {
                msg: format!("PIN must have at least {min} characters and at most 63 bytes"),
            });
        }
        let proto = self.info.pin_protocol()?;
        let (platform_key, ss) = self.shared_secret(proto)?;
        let mut padded = Zeroizing::new([0u8; 64]);
        padded[..pin.len()].copy_from_slice(pin);
        let new_pin_enc = ss.encrypt(padded.as_slice()).map_err(FidoError::protocol)?;
        let auth = ss.authenticate(&new_pin_enc);
        self.command(
            CMD_CLIENT_PIN,
            Some(Cbor::Map(vec![
                (Cbor::Int(1), Cbor::Int(proto.id())),
                (Cbor::Int(2), Cbor::Int(PIN_SET_PIN)),
                (Cbor::Int(3), platform_key),
                (Cbor::Int(4), Cbor::Bytes(auth)),
                (Cbor::Int(5), Cbor::Bytes(new_pin_enc)),
            ])),
            SHORT,
        )?;
        Ok(())
    }

    /// Obtains a pinUvAuthToken using the PIN.
    pub fn pin_token(
        &self,
        pin: &str,
        permissions: i64,
        rp_id: &str,
    ) -> Result<(PinProtocol, Zeroizing<Vec<u8>>), FidoError> {
        if self.info.option("clientPin") != Some(true) {
            return Err(FidoError::PinNotSet);
        }
        let proto = self.info.pin_protocol()?;
        let (platform_key, ss) = self.shared_secret(proto)?;
        let pin_hash = Zeroizing::new(<[u8; 32]>::from(Sha256::digest(pin.as_bytes())));
        let pin_hash_enc = ss.encrypt(&pin_hash[..16]).map_err(FidoError::protocol)?;
        let mut params = vec![
            (Cbor::Int(1), Cbor::Int(proto.id())),
            (Cbor::Int(3), platform_key),
            (Cbor::Int(6), Cbor::Bytes(pin_hash_enc)),
        ];
        if self.info.supports_token_permissions() {
            params.push((Cbor::Int(2), Cbor::Int(PIN_GET_TOKEN_WITH_PERMISSIONS)));
            params.push((Cbor::Int(9), Cbor::Int(permissions)));
            params.push((Cbor::Int(10), Cbor::text(rp_id)));
        } else {
            params.push((Cbor::Int(2), Cbor::Int(PIN_GET_TOKEN)));
        }
        let v = match self.command(CMD_CLIENT_PIN, Some(Cbor::Map(params)), SHORT) {
            Ok(v) => v,
            Err(FidoError::Ctap { code: 0x31, .. }) => {
                let retries = self.pin_retries().unwrap_or(0);
                return Err(FidoError::PinInvalid { retries });
            }
            Err(e) => return Err(e),
        };
        let enc =
            map_get_int(&v, 0x02).and_then(as_bytes).ok_or_else(|| FidoError::protocol("missing pinUvAuthToken"))?;
        let token = ss.decrypt(enc).map_err(FidoError::protocol)?;
        Ok((proto, token))
    }

    /// Creates a non-discoverable credential with the `hmac-secret` extension enabled.
    pub fn make_credential(&self, rp_id: &str, rp_name: &str, pin: &str) -> Result<Credential, FidoError> {
        if !self.info.supports_hmac_secret() {
            return Err(FidoError::HmacSecretUnsupported);
        }
        let (proto, token) = self.pin_token(pin, PERMISSION_MAKE_CREDENTIAL, rp_id)?;
        let mut cdh = [0u8; 32];
        OsRng.fill_bytes(&mut cdh);
        let mut user_id = [0u8; 16];
        OsRng.fill_bytes(&mut user_id);
        let params = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::bytes(&cdh)),
            (
                Cbor::Int(2),
                Cbor::Map(vec![(Cbor::text("id"), Cbor::text(rp_id)), (Cbor::text("name"), Cbor::text(rp_name))]),
            ),
            (
                Cbor::Int(3),
                Cbor::Map(vec![
                    (Cbor::text("id"), Cbor::bytes(&user_id)),
                    (Cbor::text("name"), Cbor::text("ghulsuite")),
                    (Cbor::text("displayName"), Cbor::text("GhulSuite")),
                ]),
            ),
            (
                Cbor::Int(4),
                Cbor::Array(vec![Cbor::Map(vec![
                    (Cbor::text("alg"), Cbor::Int(-7)),
                    (Cbor::text("type"), Cbor::text("public-key")),
                ])]),
            ),
            (Cbor::Int(6), Cbor::Map(vec![(Cbor::text("hmac-secret"), Cbor::Bool(true))])),
            (Cbor::Int(7), Cbor::Map(vec![(Cbor::text("rk"), Cbor::Bool(false))])),
            (Cbor::Int(8), Cbor::Bytes(proto.authenticate(&token, &cdh))),
            (Cbor::Int(9), Cbor::Int(proto.id())),
        ]);
        let v = self.command(CMD_MAKE_CREDENTIAL, Some(params), USER_ACTION)?;
        let auth_data =
            map_get_int(&v, 0x02).and_then(as_bytes).ok_or_else(|| FidoError::protocol("missing authData"))?;
        let parsed = parse_auth_data(auth_data, rp_id)?;
        let id = parsed.credential_id.ok_or_else(|| FidoError::protocol("no attested credential data"))?;
        let hmac_secret = parsed
            .extensions
            .as_ref()
            .and_then(|e| map_get_text(e, "hmac-secret"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Credential { id, hmac_secret })
    }

    /// Evaluates `hmac-secret` for `salt` (32 bytes) with PIN verification.
    pub fn hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        salt: &[u8; 32],
        pin: &str,
    ) -> Result<HmacSecretOutput, FidoError> {
        if !self.info.supports_hmac_secret() {
            return Err(FidoError::HmacSecretUnsupported);
        }
        let (proto, token) = self.pin_token(pin, PERMISSION_GET_ASSERTION, rp_id)?;
        let (platform_key, ss) = self.shared_secret(proto)?;
        let salt_enc = ss.encrypt(salt).map_err(FidoError::protocol)?;
        let salt_auth = ss.authenticate(&salt_enc);
        let mut hmac_input = vec![
            (Cbor::Int(1), platform_key),
            (Cbor::Int(2), Cbor::Bytes(salt_enc)),
            (Cbor::Int(3), Cbor::Bytes(salt_auth)),
        ];
        if proto != PinProtocol::V1 {
            hmac_input.push((Cbor::Int(4), Cbor::Int(proto.id())));
        }
        let mut cdh = [0u8; 32];
        OsRng.fill_bytes(&mut cdh);
        let params = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::text(rp_id)),
            (Cbor::Int(2), Cbor::bytes(&cdh)),
            (
                Cbor::Int(3),
                Cbor::Array(vec![Cbor::Map(vec![
                    (Cbor::text("id"), Cbor::bytes(credential_id)),
                    (Cbor::text("type"), Cbor::text("public-key")),
                ])]),
            ),
            (Cbor::Int(4), Cbor::Map(vec![(Cbor::text("hmac-secret"), Cbor::Map(hmac_input))])),
            (Cbor::Int(6), Cbor::Bytes(proto.authenticate(&token, &cdh))),
            (Cbor::Int(7), Cbor::Int(proto.id())),
        ]);
        let v = self.command(CMD_GET_ASSERTION, Some(params), USER_ACTION)?;
        let auth_data =
            map_get_int(&v, 0x02).and_then(as_bytes).ok_or_else(|| FidoError::protocol("missing authData"))?;
        let parsed = parse_auth_data(auth_data, rp_id)?;
        let enc_out = parsed
            .extensions
            .as_ref()
            .and_then(|e| map_get_text(e, "hmac-secret"))
            .and_then(as_bytes)
            .ok_or_else(|| FidoError::protocol("no hmac-secret output in response"))?;
        let mut output = ss.decrypt(enc_out).map_err(FidoError::protocol)?;
        if output.len() < 32 {
            return Err(FidoError::protocol("hmac-secret output too short"));
        }
        output.truncate(32);
        Ok(HmacSecretOutput {
            output,
            user_verified: parsed.flags & FLAG_UV != 0,
            user_present: parsed.flags & FLAG_UP != 0,
        })
    }
}

pub struct AuthData {
    pub flags: u8,
    pub credential_id: Option<Vec<u8>>,
    pub extensions: Option<Value>,
}

/// Parses authenticator data (WebAuthn §6.1) and checks the RP ID hash.
pub fn parse_auth_data(data: &[u8], rp_id: &str) -> Result<AuthData, FidoError> {
    if data.len() < 37 {
        return Err(FidoError::protocol("authData too short"));
    }
    if data[..32] != Sha256::digest(rp_id.as_bytes())[..] {
        return Err(FidoError::protocol("rpIdHash mismatch"));
    }
    let flags = data[32];
    let mut rest = &data[37..];
    let mut credential_id = None;
    if flags & FLAG_AT != 0 {
        if rest.len() < 18 {
            return Err(FidoError::protocol("attested credential data too short"));
        }
        let len = u16::from_be_bytes([rest[16], rest[17]]) as usize;
        if rest.len() < 18 + len {
            return Err(FidoError::protocol("credential id truncated"));
        }
        credential_id = Some(rest[18..18 + len].to_vec());
        rest = &rest[18 + len..];
        let (_public_key, used) = decode_prefix(rest).map_err(FidoError::protocol)?;
        rest = &rest[used..];
    }
    let extensions = if flags & FLAG_ED != 0 {
        let (ext, used) = decode_prefix(rest).map_err(FidoError::protocol)?;
        rest = &rest[used..];
        Some(ext)
    } else {
        None
    };
    if !rest.is_empty() {
        return Err(FidoError::protocol("trailing bytes in authData"));
    }
    Ok(AuthData { flags, credential_id, extensions })
}

/// Maps a WebAuthn PRF input to the `hmac-secret` salt (WebAuthn L3 §10.1.4), so that the
/// same key yields the same secret here and in browsers.
pub fn prf_salt(input: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"WebAuthn PRF");
    h.update([0u8]);
    h.update(input);
    h.finalize().into()
}
