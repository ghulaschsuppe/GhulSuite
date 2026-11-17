// SPDX-License-Identifier: AGPL-3.0-or-later
//! API for the GhulSuite app (exported via UniFFI).

pub mod archive;
pub mod areas;
pub mod authx;
pub mod cal;
pub mod caldate;
pub mod caltext;
pub mod error;
pub mod import;
pub mod passx;
pub mod quick;
pub(crate) mod redact;
pub mod setup;
pub mod single;
pub mod storage;
pub mod suite;

pub mod tools;

#[cfg(test)]
mod tests;
