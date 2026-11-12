// SPDX-License-Identifier: AGPL-3.0-or-later
//! API for the GhulSuite app (exported via UniFFI).

pub mod authx;
pub mod autofill;
pub mod backup;
pub mod drive;
pub mod error;
pub mod import;
pub mod mail;
pub(crate) mod mailcrypt;
pub mod mailhtml;
pub mod mailx;
pub mod passx;
pub mod quick;
pub mod servers;
pub mod shares;
pub mod storage;
pub mod suite;
pub mod sync;
pub mod tools;

#[cfg(test)]
mod tests;
