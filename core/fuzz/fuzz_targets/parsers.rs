// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every parser for outside data (HTTP, ICS, imports, links, CBOR, the stream ZIP reader of
//! suite files, GhulDocs content files and indexes); the first byte chooses which.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    ghul_core::fuzzing::parse_any(data);
});
