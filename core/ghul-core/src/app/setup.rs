// SPDX-License-Identifier: AGPL-3.0-or-later
//! A setup that survives a crash, and a recovery code that was really written down.
//!
//! - The suite remembers whether its recovery code was confirmed (one group typed back). A code
//!   that was never confirmed (the app was closed during setup, or a new code was made) shows up
//!   in `recovery_confirmed`; the app then asks for a new code right after unlocking.
//! - Files are written vault by vault and the suite file last, each one atomically. A setup that
//!   stopped in between leaves vault files without the suite file: `setup_state` reports that and
//!   unlocking with password and second factor completes the suite.
//! - Security keys hold nothing to clean up: GhulSuite registers non-resident credentials
//!   (`rk = false`); the key derives its secret from data stored in the files.

use sha2::{Digest, Sha256};

use super::areas::{stamp, Stamped};
use super::error::{GhulError, Result};
use super::suite::{put_setting, setting, GhulSuite, Open, SUITE_FILE};
use crate::vault::{RecoveryCode, UnlockedContainer};

/// Fingerprint of the confirmed recovery code (stamped, merged by the latest change).
pub(super) const CONFIRMED_KEY: &str = "recovery_confirmed";

/// What the app finds in the suite folder.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupState {
    /// Nothing there: start the setup.
    Empty,
    /// A suite: unlock it.
    Complete,
    /// Vault files without the suite file (a setup that stopped, or files copied in by hand).
    /// Unlocking with the password and second factor completes the suite.
    Incomplete,
}

fn fingerprint(code: &RecoveryCode) -> String {
    let mut h = Sha256::new();
    h.update(b"ghul/recovery-confirmed/v1");
    h.update(code.0.as_ref());
    crate::kdf::hex(&h.finalize()[..8])
}

/// Marks the suite's recovery code as not yet confirmed (new suites).
pub(super) fn mark_unconfirmed(suite: &mut UnlockedContainer) -> Result<()> {
    // Time 0: any real confirmation from another device wins over this mark.
    put_setting(suite, CONFIRMED_KEY, &Stamped { v: String::new(), at: 0 })
}

/// Normalises a typed group: upper case, no spaces or dashes, O read as 0.
fn normalise(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect::<String>()
        .replace('O', "0")
}

impl GhulSuite {
    pub(super) fn recovery_confirmed_in(o: &Open) -> bool {
        if !o.suite.payload.settings.contains_key(CONFIRMED_KEY) {
            // Suites from before this check.
            return true;
        }
        let s: Stamped<String> = setting(&o.suite, CONFIRMED_KEY);
        match o.suite.recovery_code() {
            Some(code) => s.v == fingerprint(&code),
            None => false,
        }
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Whether this folder holds a complete suite, an incomplete one, or nothing.
    pub fn setup_state(&self) -> Result<SetupState> {
        Ok(if self.storage.read(SUITE_FILE.to_owned())?.is_some() {
            SetupState::Complete
        } else if self.exists()? {
            SetupState::Incomplete
        } else {
            SetupState::Empty
        })
    }

    /// Whether the current recovery code was confirmed. When not, the app asks right after
    /// unlocking for a new code (shown, written down and confirmed like during setup).
    pub fn recovery_confirmed(&self) -> Result<bool> {
        self.with_open(|o| Ok(Self::recovery_confirmed_in(o)))
    }

    /// Checks one group of the recovery code as the user typed it back (`group`: 0-based
    /// position of the group). Only a correct group marks the code as confirmed; returns
    /// whether it was correct.
    pub fn confirm_recovery_code(&self, group: u32, typed: String) -> Result<bool> {
        self.with_open(|o| {
            let code = o.suite.recovery_code().ok_or(GhulError::NotFound)?;
            let display = code.to_display();
            let Some(expected) = display.split('-').nth(group as usize) else {
                return Err(GhulError::InvalidInput { msg: "no such group".into() });
            };
            if normalise(&typed) != normalise(expected) {
                return Ok(false);
            }
            put_setting(&mut o.suite, CONFIRMED_KEY, &stamp(fingerprint(&code)))?;
            self.save_container(&mut o.suite, SUITE_FILE)?;
            Ok(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn typed_groups_are_normalised() {
        assert_eq!(normalise(" ab0o-1 "), "AB001");
    }
}
