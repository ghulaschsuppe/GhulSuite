// SPDX-License-Identifier: AGPL-3.0-or-later
//! Software authenticator speaking CTAPHID + CTAP2, used to test the client end to end.
//! It implements the authenticator side from the specification independently of the
//! client code paths (only shared primitives are reused).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use ciborium::value::Value;
use hmac::{Hmac, Mac};
use p256::SecretKey;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::cbor::{as_bytes, as_u64, decode, map_get_int, map_get_text, Cbor};
use crate::pin::{cose_to_public_key, public_key_to_cose, PinProtocol, SharedSecret};
#[cfg(test)]
use crate::{FidoListener, FidoStatus};
use crate::{HidTransport, TransportError};

const CID: u32 = 0x0102_0304;

pub struct MockConfig {
    pub protocols: Vec<i64>,
    pub fido_2_1: bool,
    pub pin: Option<String>,
}

struct State {
    cfg: MockConfig,
    key_agreement: SecretKey,
    pin_hash: Option<[u8; 16]>,
    retries: u32,
    token: Vec<u8>,
    creds: HashMap<Vec<u8>, [u8; 32]>,
    incoming: Vec<u8>,
    expected_len: usize,
    expected_cmd: u8,
    outgoing: VecDeque<Vec<u8>>,
}

pub struct MockAuthenticator {
    state: Mutex<State>,
}

impl MockAuthenticator {
    pub fn new(cfg: MockConfig) -> Arc<Self> {
        let pin_hash = cfg.pin.as_ref().map(|p| first16(&Sha256::digest(p.as_bytes())));
        Arc::new(MockAuthenticator {
            state: Mutex::new(State {
                cfg,
                key_agreement: crate::pin::random_secret_key(),
                pin_hash,
                retries: 8,
                token: vec![],
                creds: HashMap::new(),
                incoming: vec![],
                expected_len: 0,
                expected_cmd: 0,
                outgoing: VecDeque::new(),
            }),
        })
    }
}

fn first16(d: &[u8]) -> [u8; 16] {
    let mut o = [0u8; 16];
    o.copy_from_slice(&d[..16]);
    o
}

fn proto_of(id: i64) -> PinProtocol {
    if id == 2 {
        PinProtocol::V2
    } else {
        PinProtocol::V1
    }
}

fn queue_message(out: &mut VecDeque<Vec<u8>>, cid: u32, cmd: u8, data: &[u8]) {
    let mut p = vec![0u8; 64];
    p[..4].copy_from_slice(&cid.to_be_bytes());
    p[4] = 0x80 | cmd;
    p[5..7].copy_from_slice(&(data.len() as u16).to_be_bytes());
    let n = data.len().min(57);
    p[7..7 + n].copy_from_slice(&data[..n]);
    out.push_back(p);
    let mut off = n;
    let mut seq = 0u8;
    while off < data.len() {
        let mut p = vec![0u8; 64];
        p[..4].copy_from_slice(&cid.to_be_bytes());
        p[4] = seq;
        let n = (data.len() - off).min(59);
        p[5..5 + n].copy_from_slice(&data[off..off + n]);
        out.push_back(p);
        off += n;
        seq += 1;
    }
}

impl State {
    fn decapsulate(&self, proto: PinProtocol, platform_key: &Value) -> SharedSecret {
        let platform = cose_to_public_key(platform_key).expect("valid platform key");
        let z = p256::ecdh::diffie_hellman(self.key_agreement.to_nonzero_scalar(), platform.as_affine());
        SharedSecret::from_z(proto, z.raw_secret_bytes())
    }

    fn handle_cbor(&mut self, req: &[u8]) -> Vec<u8> {
        let cmd = req[0];
        let params = if req.len() > 1 { decode(&req[1..]).expect("valid CBOR request") } else { Value::Null };
        match self.dispatch(cmd, &params) {
            Ok(Some(resp)) => {
                let mut v = vec![0x00];
                v.extend_from_slice(&resp.encode());
                v
            }
            Ok(None) => vec![0x00],
            Err(code) => vec![code],
        }
    }

    fn dispatch(&mut self, cmd: u8, p: &Value) -> Result<Option<Cbor>, u8> {
        match cmd {
            0x04 => {
                let mut versions = vec![Cbor::text("FIDO_2_0")];
                if self.cfg.fido_2_1 {
                    versions.push(Cbor::text("FIDO_2_1"));
                }
                Ok(Some(Cbor::Map(vec![
                    (Cbor::Int(1), Cbor::Array(versions)),
                    (Cbor::Int(2), Cbor::Array(vec![Cbor::text("hmac-secret")])),
                    (Cbor::Int(3), Cbor::bytes(&[0xAA; 16])),
                    (
                        Cbor::Int(4),
                        Cbor::Map(vec![
                            (Cbor::text("clientPin"), Cbor::Bool(self.pin_hash.is_some())),
                            (Cbor::text("up"), Cbor::Bool(true)),
                            (Cbor::text("pinUvAuthToken"), Cbor::Bool(self.cfg.fido_2_1)),
                        ]),
                    ),
                    (Cbor::Int(6), Cbor::Array(self.cfg.protocols.iter().map(|p| Cbor::Int(*p)).collect())),
                ])))
            }
            0x06 => self.client_pin(p),
            0x01 => self.make_credential(p),
            0x02 => self.get_assertion(p),
            _ => Err(0x01),
        }
    }

    fn client_pin(&mut self, p: &Value) -> Result<Option<Cbor>, u8> {
        let proto_id = map_get_int(p, 1).and_then(as_u64).ok_or(0x14)? as i64;
        if !self.cfg.protocols.contains(&proto_id) {
            return Err(0x02);
        }
        let proto = proto_of(proto_id);
        match map_get_int(p, 2).and_then(as_u64).ok_or(0x14)? {
            1 => Ok(Some(Cbor::Map(vec![(Cbor::Int(3), Cbor::Int(self.retries as i64))]))),
            2 => Ok(Some(Cbor::Map(vec![(Cbor::Int(1), public_key_to_cose(&self.key_agreement.public_key()))]))),
            3 => {
                let ss = self.decapsulate(proto, map_get_int(p, 3).ok_or(0x14)?);
                let enc = map_get_int(p, 5).and_then(as_bytes).ok_or(0x14)?;
                let auth = map_get_int(p, 4).and_then(as_bytes).ok_or(0x14)?;
                if ss.authenticate(enc) != auth {
                    return Err(0x33);
                }
                let padded = ss.decrypt(enc).map_err(|_| 0x02)?;
                let pin: Vec<u8> = padded.iter().copied().take_while(|b| *b != 0).collect();
                self.pin_hash = Some(first16(&Sha256::digest(&pin)));
                Ok(None)
            }
            sub @ (5 | 9) => {
                if sub == 9 && !self.cfg.fido_2_1 {
                    return Err(0x3E);
                }
                let expected = self.pin_hash.ok_or(0x35)?;
                let ss = self.decapsulate(proto, map_get_int(p, 3).ok_or(0x14)?);
                let enc = map_get_int(p, 6).and_then(as_bytes).ok_or(0x14)?;
                let hash = ss.decrypt(enc).map_err(|_| 0x02)?;
                if hash.as_slice() != expected {
                    self.retries -= 1;
                    return Err(0x31);
                }
                self.retries = 8;
                self.token = vec![0u8; 32];
                OsRng.fill_bytes(&mut self.token);
                let enc_token = ss.encrypt(&self.token).unwrap();
                Ok(Some(Cbor::Map(vec![(Cbor::Int(2), Cbor::Bytes(enc_token))])))
            }
            _ => Err(0x3E),
        }
    }

    fn check_pin_auth(&self, p: &Value, auth_key: i64, proto_key: i64) -> Result<(), u8> {
        let cdh = map_get_int(p, 1).and_then(as_bytes);
        let cdh = match cdh {
            Some(c) if c.len() == 32 => c,
            _ => map_get_int(p, 2).and_then(as_bytes).ok_or(0x14)?,
        };
        let proto = proto_of(map_get_int(p, proto_key).and_then(as_u64).ok_or(0x14)? as i64);
        let auth = map_get_int(p, auth_key).and_then(as_bytes).ok_or(0x36)?;
        if self.token.is_empty() || proto.authenticate(&self.token, cdh) != auth {
            return Err(0x33);
        }
        Ok(())
    }

    fn make_credential(&mut self, p: &Value) -> Result<Option<Cbor>, u8> {
        self.check_pin_auth(p, 8, 9)?;
        let rp = map_get_int(p, 2).ok_or(0x14)?;
        let rp_id = map_get_text(rp, "id").and_then(Value::as_text).ok_or(0x14)?;
        let ext = map_get_int(p, 6).and_then(|e| map_get_text(e, "hmac-secret")).and_then(Value::as_bool);
        let mut cred_id = vec![0u8; 48];
        OsRng.fill_bytes(&mut cred_id);
        let mut cred_random = [0u8; 32];
        OsRng.fill_bytes(&mut cred_random);
        self.creds.insert(cred_id.clone(), cred_random);

        let mut ad = Sha256::digest(rp_id.as_bytes()).to_vec();
        ad.push(0x01 | 0x04 | 0x40 | 0x80);
        ad.extend_from_slice(&[0, 0, 0, 1]);
        ad.extend_from_slice(&[0xAA; 16]);
        ad.extend_from_slice(&(cred_id.len() as u16).to_be_bytes());
        ad.extend_from_slice(&cred_id);
        ad.extend_from_slice(&public_key_to_cose(&crate::pin::random_secret_key().public_key()).encode());
        ad.extend_from_slice(&Cbor::Map(vec![(Cbor::text("hmac-secret"), Cbor::Bool(ext == Some(true)))]).encode());
        Ok(Some(Cbor::Map(vec![
            (Cbor::Int(1), Cbor::text("none")),
            (Cbor::Int(2), Cbor::Bytes(ad)),
            (Cbor::Int(3), Cbor::Map(vec![])),
        ])))
    }

    fn get_assertion(&mut self, p: &Value) -> Result<Option<Cbor>, u8> {
        self.check_pin_auth(p, 6, 7)?;
        let rp_id = map_get_int(p, 1).and_then(Value::as_text).ok_or(0x14)?;
        let allow = map_get_int(p, 3).and_then(Value::as_array).ok_or(0x14)?;
        let cred_id = allow
            .iter()
            .filter_map(|c| map_get_text(c, "id").and_then(as_bytes))
            .find(|id| self.creds.contains_key(*id))
            .ok_or(0x2E)?
            .to_vec();
        let cred_random = self.creds[&cred_id];
        let hs = map_get_int(p, 4).and_then(|e| map_get_text(e, "hmac-secret")).ok_or(0x14)?;
        let proto = proto_of(map_get_int(hs, 4).and_then(as_u64).unwrap_or(1) as i64);
        let ss = self.decapsulate(proto, map_get_int(hs, 1).ok_or(0x14)?);
        let salt_enc = map_get_int(hs, 2).and_then(as_bytes).ok_or(0x14)?;
        let salt_auth = map_get_int(hs, 3).and_then(as_bytes).ok_or(0x14)?;
        if ss.authenticate(salt_enc) != salt_auth {
            return Err(0x02);
        }
        let salt = ss.decrypt(salt_enc).map_err(|_| 0x02)?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&cred_random).unwrap();
        mac.update(&salt[..32]);
        let output = mac.finalize().into_bytes();
        let out_enc = ss.encrypt(&output).unwrap();

        let mut ad = Sha256::digest(rp_id.as_bytes()).to_vec();
        ad.push(0x01 | 0x04 | 0x80);
        ad.extend_from_slice(&[0, 0, 0, 2]);
        ad.extend_from_slice(&Cbor::Map(vec![(Cbor::text("hmac-secret"), Cbor::Bytes(out_enc))]).encode());
        Ok(Some(Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Map(vec![(Cbor::text("id"), Cbor::Bytes(cred_id))])),
            (Cbor::Int(2), Cbor::Bytes(ad)),
            (Cbor::Int(3), Cbor::bytes(&[0x30; 70])),
        ])))
    }
}

impl HidTransport for MockAuthenticator {
    fn report_size(&self) -> u32 {
        64
    }

    fn write_report(&self, report: Vec<u8>) -> Result<(), TransportError> {
        let mut s = self.state.lock().unwrap();
        let cid = u32::from_be_bytes([report[0], report[1], report[2], report[3]]);
        if report[4] & 0x80 != 0 {
            s.expected_cmd = report[4] & 0x7f;
            s.expected_len = u16::from_be_bytes([report[5], report[6]]) as usize;
            s.incoming = report[7..(7 + s.expected_len).min(64)].to_vec();
        } else {
            let n = (s.expected_len - s.incoming.len()).min(59);
            let chunk = report[5..5 + n].to_vec();
            s.incoming.extend_from_slice(&chunk);
        }
        if s.incoming.len() < s.expected_len {
            return Ok(());
        }
        let msg = std::mem::take(&mut s.incoming);
        match s.expected_cmd {
            0x06 => {
                let mut resp = msg[..8].to_vec();
                resp.extend_from_slice(&CID.to_be_bytes());
                resp.extend_from_slice(&[2, 5, 4, 3, 0x05]);
                queue_message(&mut s.outgoing, cid, 0x06, &resp);
            }
            0x10 => {
                assert_eq!(cid, CID, "CBOR must use the allocated channel");
                if matches!(msg[0], 0x01 | 0x02) {
                    queue_message(&mut s.outgoing, CID, 0x3b, &[2]); // UPNEEDED keepalive
                }
                let resp = s.handle_cbor(&msg);
                queue_message(&mut s.outgoing, CID, 0x10, &resp);
            }
            0x11 => {}
            other => panic!("unexpected CTAPHID command {other:#x}"),
        }
        Ok(())
    }

    fn read_report(&self, _timeout_ms: u32) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(self.state.lock().unwrap().outgoing.pop_front())
    }
}

/// Software authenticator for FFI tests (feature `mock` only).
#[cfg(feature = "mock")]
#[uniffi::export]
pub fn mock_authenticator(pin: Option<String>) -> Arc<dyn HidTransport> {
    MockAuthenticator::new(MockConfig { protocols: vec![2, 1], fido_2_1: true, pin })
}

#[cfg(test)]
#[derive(Default)]
pub struct RecordingListener {
    pub events: Mutex<Vec<FidoStatus>>,
}

#[cfg(test)]
impl FidoListener for RecordingListener {
    fn on_status(&self, status: FidoStatus) {
        self.events.lock().unwrap().push(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fido_get_info, fido_register, fido_set_pin, fido_unlock_test, FidoError};

    fn run_flow(protocols: Vec<i64>, fido_2_1: bool) {
        let mock = MockAuthenticator::new(MockConfig { protocols, fido_2_1, pin: Some("1234".into()) });
        let listener = Arc::new(RecordingListener::default());

        let info = fido_get_info(mock.clone()).unwrap();
        assert!(info.supports_hmac_secret && info.pin_set);
        assert_eq!(info.pin_retries, Some(8));

        let reg = fido_register(mock.clone(), listener.clone(), "1234".into()).unwrap();
        assert!(reg.hmac_secret_enabled);
        assert_eq!(reg.credential_id.len(), 48);

        let a = fido_unlock_test(mock.clone(), listener.clone(), "1234".into(), reg.credential_id.clone(), "pw".into())
            .unwrap();
        let b = fido_unlock_test(mock.clone(), listener.clone(), "1234".into(), reg.credential_id.clone(), "pw".into())
            .unwrap();
        let c =
            fido_unlock_test(mock.clone(), listener.clone(), "1234".into(), reg.credential_id.clone(), "other".into())
                .unwrap();
        assert!(a.user_verified && a.user_present);
        assert_eq!(a.key_fingerprint, b.key_fingerprint, "hmac-secret must be deterministic");
        assert_eq!(a.kek_fingerprint, b.kek_fingerprint);
        assert_eq!(a.key_fingerprint, c.key_fingerprint);
        assert_ne!(a.kek_fingerprint, c.kek_fingerprint, "password must change the KEK");
        assert!(listener.events.lock().unwrap().contains(&FidoStatus::TouchNeeded));

        match fido_unlock_test(mock.clone(), listener.clone(), "9999".into(), reg.credential_id.clone(), "pw".into()) {
            Err(FidoError::PinInvalid { retries: 7 }) => {}
            other => panic!("expected PinInvalid, got {other:?}"),
        }
        match fido_unlock_test(mock, listener, "1234".into(), vec![1, 2, 3], "pw".into()) {
            Err(FidoError::NoCredentials) => {}
            other => panic!("expected NoCredentials, got {other:?}"),
        }
    }

    #[test]
    fn full_flow_pin_protocol_2_ctap21() {
        run_flow(vec![2, 1], true);
    }

    #[test]
    fn full_flow_pin_protocol_1_ctap20() {
        run_flow(vec![1], false);
    }

    #[test]
    fn set_pin_then_register() {
        for protocols in [vec![2], vec![1]] {
            let mock = MockAuthenticator::new(MockConfig { protocols, fido_2_1: true, pin: None });
            let listener = Arc::new(RecordingListener::default());
            assert!(!fido_get_info(mock.clone()).unwrap().pin_set);
            assert!(matches!(fido_register(mock.clone(), listener.clone(), "1234".into()), Err(FidoError::PinNotSet)));
            assert!(matches!(fido_set_pin(mock.clone(), "12".into()), Err(FidoError::InvalidInput { .. })));
            fido_set_pin(mock.clone(), "246810".into()).unwrap();
            assert!(fido_get_info(mock.clone()).unwrap().pin_set);
            fido_register(mock, listener, "246810".into()).unwrap();
        }
    }
}
