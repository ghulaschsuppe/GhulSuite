// SPDX-License-Identifier: AGPL-3.0-or-later
//! Untrusted input: container files, key files, recovery codes and padded payloads.
#![no_main]

use ghul_core::vault::payload::unpad;
use ghul_core::vault::{Keyfile, LockedContainer, RecoveryCode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(locked) = LockedContainer::parse(data) {
        let _ = locked.fido2_requests();
        // Re-encoding a parsed header must be possible and stable.
        let encoded = locked.header().encode().expect("parsed header must re-encode");
        let reparsed = ghul_core::vault::format::Header::parse(&encoded).expect("re-encoded header must parse");
        assert_eq!(&reparsed, locked.header());
    }
    let _ = Keyfile::parse(data);
    let _ = unpad(data);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = RecoveryCode::parse(text);
    }
});
