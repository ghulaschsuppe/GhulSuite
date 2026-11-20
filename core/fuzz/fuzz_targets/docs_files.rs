// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a suite file brings besides the containers: the stream ZIP reader, GhulDocs content
//! files and GhulDocs indexes; the first byte chooses which.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&which, rest)) = data.split_first() else { return };
    ghul_core::fuzzing::parse_any(&[8 + which % 3].iter().chain(rest).copied().collect::<Vec<u8>>());
});
