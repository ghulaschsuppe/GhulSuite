// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulSuite containers (`.ghlpss`, `.ghltotp`, `.ghlsuite`) and key files (`.ghlkey`).
//! The byte-level specification is `docs/FORMAT.md`.

pub mod container;
pub mod error;
pub mod format;
pub mod keyfile;
pub mod keys;
pub mod payload;
pub mod recovery;

pub use container::{
    Fido2Registration, Fido2Request, KdfCost, LockedContainer, SecondFactor, SuiteKeys, UnlockedContainer,
};
pub use error::VaultError;
pub use format::{ContainerKind, KdfParams, Slot};
pub use keyfile::Keyfile;
pub use keys::{FactorSecret, PasswordKey};
pub use payload::{Entry, Field, Payload, SlotMeta};
pub use recovery::RecoveryCode;
#[cfg(test)]
mod tests;
