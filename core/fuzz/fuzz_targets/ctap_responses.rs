// SPDX-License-Identifier: AGPL-3.0-or-later
//! Untrusted input from a (possibly malicious) USB device: authenticator data and CBOR.
#![no_main]

use ghul_core::cbor::decode_prefix;
use ghul_core::ctap2::parse_auth_data;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_auth_data(data, "ghulsuite.invalid");
    let _ = decode_prefix(data);
});
