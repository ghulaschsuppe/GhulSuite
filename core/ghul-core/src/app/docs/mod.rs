// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulDocs: an encrypted store for important documents and scans (FORMAT.md section 14).
//!
//! A GhulDocs vault is a vault file `<id>.ghldoc` like the others (same keys, same write seal)
//! that holds only the index: folders and files with their names, sizes, times, types,
//! favorites, trash and, per file, its own random key and the SHA-256 of its content file. The
//! content of each file is in a content file `docs-<vault id>/<content id>.ghlblob` next to the
//! vault files (see [`blob`]); it is written once and never changed. Plaintext never reaches the
//! disk: content is encrypted while it is read in and decrypted while it is written out.

// The GhulDocs API follows; until then only the tests use the content files.
#[allow(dead_code)]
pub(crate) mod blob;
