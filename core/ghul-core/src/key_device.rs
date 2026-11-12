// SPDX-License-Identifier: AGPL-3.0-or-later
//! A security key as the vault code needs it: register a credential and evaluate `hmac-secret`.
//!
//! Two implementations:
//! - CTAP2 over raw HID reports (Android: the app passes the USB connection as [`HidTransport`]).
//! - The operating system's WebAuthn API (Windows: `webauthn.dll`). Windows does not let normal
//!   programs talk to FIDO keys directly; its own dialog asks for the PIN and the touch. The app
//!   selects it with [`platform_security_key`].
//!
//! Both yield the same `hmac-secret` output for the same credential and salt, so a vault created
//! on Android opens on Windows and the other way round.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::ctap2::{Authenticator, Credential};
use crate::{FidoError, HidTransport, TransportError};

/// One credential to try, with the salt for its slot.
pub(crate) struct CredRequest<'a> {
    pub rp_id: &'a str,
    pub credential_id: &'a [u8],
    /// `hmac-secret` salt = SHA-256("WebAuthn PRF" ‖ 0 ‖ prf_input).
    pub salt: &'a [u8; 32],
    #[cfg_attr(not(windows), allow(dead_code))]
    pub prf_input: &'a [u8; 32],
}

/// The first 32 bytes of `hmac-secret` for the credential at `index` of the request list.
pub(crate) struct KeyOutput {
    pub index: usize,
    pub output: Zeroizing<[u8; 32]>,
    pub user_verified: bool,
}

pub(crate) trait KeyDevice {
    /// A new non-resident credential with `hmac-secret` enabled.
    fn make_credential(&self, rp_id: &str, rp_name: &str, pin: &str) -> Result<Credential, FidoError>;
    /// Evaluates `hmac-secret` with whichever of the credentials is on the key.
    fn hmac_secret_any(&self, requests: &[CredRequest<'_>], pin: &str) -> Result<KeyOutput, FidoError>;
    /// Platform API only: switches between passing the hashed salt and the PRF input, for the
    /// case that the system ignored the "raw salt" flag. Returns false if there is nothing to try.
    fn switch_salt_mode(&self) -> bool {
        false
    }
}

impl KeyDevice for Authenticator<'_> {
    fn make_credential(&self, rp_id: &str, rp_name: &str, pin: &str) -> Result<Credential, FidoError> {
        Authenticator::make_credential(self, rp_id, rp_name, pin)
    }

    fn hmac_secret_any(&self, requests: &[CredRequest<'_>], pin: &str) -> Result<KeyOutput, FidoError> {
        for (index, r) in requests.iter().enumerate() {
            match self.hmac_secret(r.rp_id, r.credential_id, r.salt, pin) {
                Ok(out) => {
                    if out.output.len() < 32 {
                        return Err(FidoError::Protocol { msg: "hmac-secret output too short".into() });
                    }
                    let mut o = Zeroizing::new([0u8; 32]);
                    o.copy_from_slice(&out.output[..32]);
                    return Ok(KeyOutput { index, output: o, user_verified: out.user_verified });
                }
                Err(FidoError::NoCredentials) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(FidoError::NoCredentials)
    }
}

/// Window the system dialog belongs to (0 = none); set by [`platform_security_key`].
static PLATFORM_WINDOW: AtomicU64 = AtomicU64::new(0);

/// Marker passed instead of a HID connection: "use the operating system's WebAuthn API".
struct PlatformKey;

impl HidTransport for PlatformKey {
    fn report_size(&self) -> u32 {
        0
    }
    fn write_report(&self, _report: Vec<u8>) -> Result<(), TransportError> {
        Err(TransportError::Io { msg: "platform key has no HID connection".into() })
    }
    fn read_report(&self, _timeout_ms: u32) -> Result<Option<Vec<u8>>, TransportError> {
        Err(TransportError::Io { msg: "platform key has no HID connection".into() })
    }
}

/// The security key through the operating system (Windows: WebAuthn API with its own PIN and
/// touch dialog). `window_handle` is the native handle of the app window (HWND), 0 if unknown.
/// Pass the result wherever a [`HidTransport`] is expected; the PIN argument is then ignored.
#[uniffi::export]
pub fn platform_security_key(window_handle: u64) -> Arc<dyn HidTransport> {
    PLATFORM_WINDOW.store(window_handle, Ordering::Relaxed);
    Arc::new(PlatformKey)
}

/// Whether [`platform_security_key`] works here (Windows 10 1903+ with `hmac-secret` support in
/// the WebAuthn API, i.e. API version 4 or newer).
#[uniffi::export]
pub fn platform_security_key_available() -> bool {
    imp::available()
}

pub(crate) fn is_platform(transport: &dyn HidTransport) -> bool {
    transport.report_size() == 0
}

pub(crate) fn platform_device() -> Result<Box<dyn KeyDevice>, FidoError> {
    if !imp::available() {
        return Err(FidoError::Transport {
            msg: "the system's security key interface is not available (Windows 11 or a current Windows 10 needed)"
                .into(),
        });
    }
    Ok(Box::new(imp::WebAuthn::new(PLATFORM_WINDOW.load(Ordering::Relaxed))))
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn available() -> bool {
        false
    }

    pub struct WebAuthn;

    impl WebAuthn {
        pub fn new(_window: u64) -> WebAuthn {
            WebAuthn
        }
    }

    impl KeyDevice for WebAuthn {
        fn make_credential(&self, _: &str, _: &str, _: &str) -> Result<Credential, FidoError> {
            Err(FidoError::Transport { msg: "not supported on this system".into() })
        }
        fn hmac_secret_any(&self, _: &[CredRequest<'_>], _: &str) -> Result<KeyOutput, FidoError> {
            Err(FidoError::Transport { msg: "not supported on this system".into() })
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use rand_core::{OsRng, RngCore};
    use std::ptr::{null, null_mut};
    use windows_sys::core::{BOOL, HRESULT, PCWSTR};
    use windows_sys::Win32::Networking::WindowsWebServices::*;

    const TIMEOUT_MS: u32 = 120_000;
    // HRESULTs of the WebAuthn API (winerror.h).
    const NTE_NOT_FOUND: HRESULT = 0x8009_0011_u32 as i32;
    const NTE_USER_CANCELLED: HRESULT = 0x8009_0036_u32 as i32;
    const NTE_INVALID_PARAMETER: HRESULT = 0x8009_0027_u32 as i32;
    const E_INVALIDARG: HRESULT = 0x8007_0057_u32 as i32;
    const NTE_EXISTS: HRESULT = 0x8009_000F_u32 as i32;
    const NTE_NOT_SUPPORTED: HRESULT = 0x8009_0029_u32 as i32;
    const NTE_TOKEN_KEYSET_STORAGE_FULL: HRESULT = 0x8009_0023_u32 as i32;
    const NTE_DEVICE_NOT_FOUND: HRESULT = 0x8009_0035_u32 as i32;

    pub fn available() -> bool {
        // webauthn.dll exists since Windows 10 1903; hmac-secret salts need API version 4.
        unsafe { WebAuthNGetApiVersionNumber() >= WEBAUTHN_API_VERSION_4 }
    }

    pub struct WebAuthn {
        window: u64,
        /// true: hashed salt with WEBAUTHN_AUTHENTICATOR_HMAC_SECRET_VALUES_FLAG (like CTAP2).
        raw: std::cell::Cell<bool>,
        switched: std::cell::Cell<bool>,
    }

    impl WebAuthn {
        pub fn new(window: u64) -> WebAuthn {
            WebAuthn { window, raw: std::cell::Cell::new(true), switched: std::cell::Cell::new(false) }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn b64url(data: &[u8]) -> String {
        crate::share::b64url(data)
    }

    /// Client data as a browser would send it; only its hash reaches the key.
    fn client_data(kind: &str, rp_id: &str) -> Vec<u8> {
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut challenge);
        format!(
            r#"{{"type":"{kind}","challenge":"{}","origin":"https://{rp_id}","crossOrigin":false}}"#,
            b64url(&challenge)
        )
        .into_bytes()
    }

    fn error(hr: HRESULT) -> FidoError {
        match hr {
            NTE_NOT_FOUND => FidoError::NoCredentials,
            NTE_USER_CANCELLED => FidoError::NotConfirmed,
            NTE_NOT_SUPPORTED => FidoError::HmacSecretUnsupported,
            NTE_DEVICE_NOT_FOUND => FidoError::Timeout,
            NTE_EXISTS | NTE_TOKEN_KEYSET_STORAGE_FULL => {
                FidoError::Protocol { msg: format!("key refused (0x{hr:08x})") }
            }
            _ => FidoError::Transport { msg: format!("Windows WebAuthn error 0x{:08x}", hr as u32) },
        }
    }

    /// User verified (UV) flag of the authenticator data (byte 32, bit 2).
    fn uv_flag(auth_data: *const u8, len: u32) -> bool {
        if auth_data.is_null() || len < 33 {
            return false;
        }
        unsafe { *auth_data.add(32) & 0x04 != 0 }
    }

    impl KeyDevice for WebAuthn {
        fn make_credential(&self, rp_id: &str, rp_name: &str, _pin: &str) -> Result<Credential, FidoError> {
            let rp_id_w = wide(rp_id);
            let rp_name_w = wide(rp_name);
            let rp = WEBAUTHN_RP_ENTITY_INFORMATION {
                dwVersion: WEBAUTHN_RP_ENTITY_INFORMATION_CURRENT_VERSION,
                pwszId: rp_id_w.as_ptr(),
                pwszName: rp_name_w.as_ptr(),
                pwszIcon: null(),
            };
            let mut user_id = [0u8; 16];
            OsRng.fill_bytes(&mut user_id);
            let user_name = wide("GhulSuite");
            let user = WEBAUTHN_USER_ENTITY_INFORMATION {
                dwVersion: WEBAUTHN_USER_ENTITY_INFORMATION_CURRENT_VERSION,
                cbId: user_id.len() as u32,
                pbId: user_id.as_mut_ptr(),
                pwszName: user_name.as_ptr(),
                pwszIcon: null(),
                pwszDisplayName: user_name.as_ptr(),
            };
            let mut params = [
                WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
                    dwVersion: WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
                    pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
                    lAlg: WEBAUTHN_COSE_ALGORITHM_ECDSA_P256_WITH_SHA256,
                },
                WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
                    dwVersion: WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
                    pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
                    lAlg: -8, // EdDSA
                },
            ];
            let cose = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS {
                cCredentialParameters: params.len() as u32,
                pCredentialParameters: params.as_mut_ptr(),
            };
            let mut cd = client_data("webauthn.create", rp_id);
            let client = WEBAUTHN_CLIENT_DATA {
                dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
                cbClientDataJSON: cd.len() as u32,
                pbClientDataJSON: cd.as_mut_ptr(),
                pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
            };
            let mut enable: BOOL = 1;
            // credProtect 3: the key uses the credential only after PIN verification. Not
            // required, so keys without the extension still work.
            let mut protect = WEBAUTHN_CRED_PROTECT_EXTENSION_IN {
                dwCredProtect: WEBAUTHN_USER_VERIFICATION_REQUIRED,
                bRequireCredProtect: 0,
            };
            let mut ext = [
                WEBAUTHN_EXTENSION {
                    pwszExtensionIdentifier: WEBAUTHN_EXTENSIONS_IDENTIFIER_HMAC_SECRET,
                    cbExtension: std::mem::size_of::<BOOL>() as u32,
                    pvExtension: &mut enable as *mut BOOL as *mut _,
                },
                WEBAUTHN_EXTENSION {
                    pwszExtensionIdentifier: WEBAUTHN_EXTENSIONS_IDENTIFIER_CRED_PROTECT,
                    cbExtension: std::mem::size_of::<WEBAUTHN_CRED_PROTECT_EXTENSION_IN>() as u32,
                    pvExtension: &mut protect as *mut WEBAUTHN_CRED_PROTECT_EXTENSION_IN as *mut _,
                },
            ];
            let mut options: WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS = unsafe { std::mem::zeroed() };
            options.dwVersion = WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS_VERSION_3;
            options.dwTimeoutMilliseconds = TIMEOUT_MS;
            options.Extensions = WEBAUTHN_EXTENSIONS { cExtensions: ext.len() as u32, pExtensions: ext.as_mut_ptr() };
            // Only external security keys: Windows Hello has no hmac-secret.
            options.dwAuthenticatorAttachment = WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM;
            options.bRequireResidentKey = 0;
            options.dwUserVerificationRequirement = WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED;
            options.dwAttestationConveyancePreference = WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE;

            let mut att: *mut WEBAUTHN_CREDENTIAL_ATTESTATION = null_mut();
            let hr = unsafe {
                WebAuthNAuthenticatorMakeCredential(self.window as _, &rp, &user, &cose, &client, &options, &mut att)
            };
            if hr < 0 || att.is_null() {
                return Err(error(hr));
            }
            // Copy what we need, then free the API's buffer.
            let result = unsafe {
                let a = &*att;
                let id = std::slice::from_raw_parts(a.pbCredentialId, a.cbCredentialId as usize).to_vec();
                let mut hmac = false;
                for i in 0..a.Extensions.cExtensions as usize {
                    let e = &*a.Extensions.pExtensions.add(i);
                    if same(e.pwszExtensionIdentifier, WEBAUTHN_EXTENSIONS_IDENTIFIER_HMAC_SECRET)
                        && e.cbExtension as usize >= std::mem::size_of::<BOOL>()
                        && !e.pvExtension.is_null()
                    {
                        hmac = *(e.pvExtension as *const BOOL) != 0;
                    }
                }
                WebAuthNFreeCredentialAttestation(att);
                Credential { id, hmac_secret: hmac }
            };
            Ok(result)
        }

        fn hmac_secret_any(&self, requests: &[CredRequest<'_>], _pin: &str) -> Result<KeyOutput, FidoError> {
            // The hashed salt with the "raw values" flag is exactly what CTAP2 sends; builds that
            // refuse the flag get the PRF input and hash it themselves (same result).
            match self.assert(requests, self.raw.get()) {
                Err(FidoError::InvalidInput { .. }) if self.raw.get() => {
                    self.raw.set(false);
                    self.switched.set(true);
                    self.assert(requests, false)
                }
                other => other,
            }
        }

        fn switch_salt_mode(&self) -> bool {
            if self.switched.get() {
                return false;
            }
            self.switched.set(true);
            self.raw.set(!self.raw.get());
            true
        }
    }

    fn same(a: PCWSTR, b: PCWSTR) -> bool {
        if a.is_null() || b.is_null() {
            return false;
        }
        unsafe {
            let mut i = 0;
            loop {
                let (x, y) = (*a.add(i), *b.add(i));
                if x != y {
                    return false;
                }
                if x == 0 {
                    return true;
                }
                i += 1;
            }
        }
    }

    impl WebAuthn {
        fn assert(&self, requests: &[CredRequest<'_>], raw: bool) -> Result<KeyOutput, FidoError> {
            let rp_id = requests.first().map(|r| r.rp_id).ok_or(FidoError::NoCredentials)?;
            // All GhulSuite credentials share one RP ID; others are not offered in this call.
            let reqs: Vec<&CredRequest<'_>> = requests.iter().filter(|r| r.rp_id == rp_id).collect();
            let rp_id_w = wide(rp_id);
            let mut ids: Vec<Vec<u8>> = reqs.iter().map(|r| r.credential_id.to_vec()).collect();
            let mut salts: Vec<[u8; 32]> = reqs.iter().map(|r| if raw { *r.salt } else { *r.prf_input }).collect();
            let mut creds: Vec<WEBAUTHN_CREDENTIAL> = ids
                .iter_mut()
                .map(|id| WEBAUTHN_CREDENTIAL {
                    dwVersion: WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
                    cbId: id.len() as u32,
                    pbId: id.as_mut_ptr(),
                    pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
                })
                .collect();
            let mut salt_structs: Vec<WEBAUTHN_HMAC_SECRET_SALT> = salts
                .iter_mut()
                .map(|s| WEBAUTHN_HMAC_SECRET_SALT {
                    cbFirst: 32,
                    pbFirst: s.as_mut_ptr(),
                    cbSecond: 0,
                    pbSecond: null_mut(),
                })
                .collect();
            let mut with_salt: Vec<WEBAUTHN_CRED_WITH_HMAC_SECRET_SALT> = ids
                .iter_mut()
                .zip(salt_structs.iter_mut())
                .map(|(id, s)| WEBAUTHN_CRED_WITH_HMAC_SECRET_SALT {
                    cbCredID: id.len() as u32,
                    pbCredID: id.as_mut_ptr(),
                    pHmacSecretSalt: s,
                })
                .collect();
            let mut values = WEBAUTHN_HMAC_SECRET_SALT_VALUES {
                pGlobalHmacSalt: null_mut(),
                cCredWithHmacSecretSaltList: with_salt.len() as u32,
                pCredWithHmacSecretSaltList: with_salt.as_mut_ptr(),
            };
            let mut cd = client_data("webauthn.get", rp_id);
            let client = WEBAUTHN_CLIENT_DATA {
                dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
                cbClientDataJSON: cd.len() as u32,
                pbClientDataJSON: cd.as_mut_ptr(),
                pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
            };
            let mut options: WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS = unsafe { std::mem::zeroed() };
            // Version 6 brings pHmacSecretSaltValues; later fields stay unread.
            options.dwVersion = WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS_VERSION_6;
            options.dwTimeoutMilliseconds = TIMEOUT_MS;
            options.CredentialList =
                WEBAUTHN_CREDENTIALS { cCredentials: creds.len() as u32, pCredentials: creds.as_mut_ptr() };
            options.dwAuthenticatorAttachment = WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM;
            options.dwUserVerificationRequirement = WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED;
            options.dwFlags = if raw { WEBAUTHN_AUTHENTICATOR_HMAC_SECRET_VALUES_FLAG } else { 0 };
            options.pHmacSecretSaltValues = &mut values;

            let mut assertion: *mut WEBAUTHN_ASSERTION = null_mut();
            let hr = unsafe {
                WebAuthNAuthenticatorGetAssertion(self.window as _, rp_id_w.as_ptr(), &client, &options, &mut assertion)
            };
            if hr == E_INVALIDARG || hr == NTE_INVALID_PARAMETER {
                return Err(FidoError::InvalidInput { msg: format!("0x{:08x}", hr as u32) });
            }
            if hr < 0 || assertion.is_null() {
                return Err(error(hr));
            }
            let result = unsafe {
                let a = &*assertion;
                let used = std::slice::from_raw_parts(a.Credential.pbId, a.Credential.cbId as usize);
                let index = reqs.iter().position(|r| r.credential_id == used);
                let uv = uv_flag(a.pbAuthenticatorData, a.cbAuthenticatorData);
                let out = if a.dwVersion >= WEBAUTHN_ASSERTION_VERSION_3 && !a.pHmacSecret.is_null() {
                    let h = &*a.pHmacSecret;
                    if h.cbFirst >= 32 && !h.pbFirst.is_null() {
                        let mut o = Zeroizing::new([0u8; 32]);
                        o.copy_from_slice(std::slice::from_raw_parts(h.pbFirst, 32));
                        Some(o)
                    } else {
                        None
                    }
                } else {
                    None
                };
                WebAuthNFreeAssertion(assertion);
                (index, out, uv)
            };
            match result {
                (Some(i), Some(output), uv) => {
                    // Map back to the caller's list (it may contain other RP IDs).
                    let index = requests.iter().position(|r| std::ptr::eq(r, reqs[i])).unwrap_or(i);
                    Ok(KeyOutput { index, output, user_verified: uv })
                }
                (None, _, _) => Err(FidoError::NoCredentials),
                (_, None, _) => Err(FidoError::HmacSecretUnsupported),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_marker_is_recognised() {
        let t = platform_security_key(0x1234);
        assert!(is_platform(t.as_ref()));
        let usb = crate::mock::MockAuthenticator::new(crate::mock::MockConfig {
            protocols: vec![2],
            fido_2_1: true,
            pin: None,
        });
        assert!(!is_platform(usb.as_ref()));
        assert!(t.write_report(vec![0; 64]).is_err());
        #[cfg(not(windows))]
        {
            assert!(!platform_security_key_available());
            assert!(platform_device().is_err());
        }
    }
}
