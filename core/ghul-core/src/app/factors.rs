// SPDX-License-Identifier: AGPL-3.0-or-later
//! Changing the second factors in one step: a new key or key file in place of one or more
//! others ("Austauschen", "Schutz wechseln"). One change of the protection: the new slot is
//! added and the old ones removed in memory, then everything is written once through the
//! journaled save (`commit_protection`): a stop leaves the state before or the new one.
//!
//! The rules, checked before a key is touched and again under the lock:
//! - at least one second factor stays;
//! - a security key always keeps a backup (a second key or a key file); a key file alone is
//!   enough (decision of the owner);
//! - the recovery code is never touched (it is replaced with `new_recovery_code`).

use std::sync::Arc;

use rand_core::OsRng;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::suite::{now_ms, register_key, with_authenticator, FactorProof, GhulSuite, Open};
use crate::vault::container::Fido2Registration;
use crate::vault::{Keyfile, Slot};
use crate::{FidoListener, HidTransport, RP_ID};

/// The new second factor of [`GhulSuite::change_factors`].
#[derive(uniffi::Enum, Clone)]
pub enum NewFactor {
    /// A key file (from `generate_keyfile`).
    Keyfile { keyfile: Vec<u8> },
    /// A security key, registered with this PIN (two touches, through `new_transport`).
    SecurityKey { pin: String },
}

super::redact::redacted_debug!(NewFactor {});

/// What kind of second factor a slot gives (for the rules).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Key,
    Keyfile,
}

/// Checks the rules for the second factors after the change: `remove` taken away, `add`
/// added.
fn check_rules(o: &Open, remove: &[[u8; 16]], add: Option<Kind>) -> Result<()> {
    let slots = o.suite.slots();
    for id in remove {
        let slot = slots.iter().find(|s| s.id() == id).ok_or(GhulError::NotFound)?;
        if !slot.provides_factor() {
            return Err(GhulError::InvalidInput { msg: "recovery codes are replaced, not removed".into() });
        }
    }
    let mut left: Vec<Kind> = slots
        .iter()
        .filter(|s| s.provides_factor() && !remove.contains(s.id()))
        .map(|s| if matches!(s, Slot::Fido2(_)) { Kind::Key } else { Kind::Keyfile })
        .collect();
    left.extend(add);
    if left.is_empty() {
        return Err(GhulError::LastFactor);
    }
    // A security key keeps a backup: a second key or a key file.
    if left.len() < 2 && left.contains(&Kind::Key) {
        return Err(GhulError::BackupFactorRequired);
    }
    Ok(())
}

#[uniffi::export]
impl GhulSuite {
    /// Changes the second factors in one step (one change of the protection, see the module):
    /// `new_factor` (if given) is added, the slots in `remove` (their `slot_id` from `slots`)
    /// go. Replacing a factor is both at once; the factor that proves (`proof`) may be one of
    /// those removed (prove with the old key file, replace it with a new one).
    ///
    /// - `proof`: both factors of now; a key of now through `proof_transport`.
    /// - A new security key is touched through `new_transport` (twice: register, then its
    ///   output), after the rules were checked; `listener` gets the prompts of both keys.
    /// - `label`: the name of the new factor.
    ///
    /// Errors: `LastFactor` (none would stay), `BackupFactorRequired` (a key without a second
    /// key or a key file), `InvalidInput` (nothing to do, a recovery code in `remove`, a bad
    /// slot ID, a security key without `new_transport`), `NotFound` (an unknown slot),
    /// `AlreadyExists` (the key file is a factor already and stays), `CredentialExcluded`
    /// (the new key is registered already), plus those of the proof and of the key.
    #[allow(clippy::too_many_arguments)]
    pub fn change_factors(
        &self,
        proof: FactorProof,
        proof_transport: Option<Arc<dyn HidTransport>>,
        new_factor: Option<NewFactor>,
        new_transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        label: String,
        remove: Vec<Vec<u8>>,
    ) -> Result<()> {
        let mut ids: Vec<[u8; 16]> = Vec::new();
        for r in &remove {
            let id: [u8; 16] =
                r.as_slice().try_into().map_err(|_| GhulError::InvalidInput { msg: "invalid slot id".into() })?;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if new_factor.is_none() && ids.is_empty() {
            return Err(GhulError::InvalidInput { msg: "nothing to change".into() });
        }
        let keyfile = match &new_factor {
            Some(NewFactor::Keyfile { keyfile }) => Some(Keyfile::parse(&Zeroizing::new(keyfile.clone()))?),
            _ => None,
        };
        let pin = match &new_factor {
            Some(NewFactor::SecurityKey { pin }) => Some(Zeroizing::new(pin.clone())),
            _ => None,
        };
        let add = new_factor.as_ref().map(|f| match f {
            NewFactor::Keyfile { .. } => Kind::Keyfile,
            NewFactor::SecurityKey { .. } => Kind::Key,
        });
        if pin.is_some() && new_transport.is_none() {
            return Err(GhulError::InvalidInput { msg: "a new security key needs its connection".into() });
        }
        // Refused before any key is touched: the proof, the rules, a key file there already.
        self.require_factors(&proof, proof_transport.as_ref(), listener.as_ref())?;
        let known: Vec<Vec<u8>> = self.with_open(|o| {
            super::single::refuse_single(o)?;
            check_rules(o, &ids, add)?;
            if let Some(kf) = &keyfile {
                if o.suite
                    .slots()
                    .iter()
                    .any(|s| matches!(s, Slot::Keyfile(k) if k.keyfile_id == kf.id() && !ids.contains(s.id())))
                {
                    return Err(GhulError::AlreadyExists);
                }
            }
            Ok(o.suite
                .slots()
                .iter()
                .filter_map(|s| if let Slot::Fido2(f) = s { Some(f.credential_id.clone()) } else { None })
                .collect())
        })?;
        // The new key: registered before the lock is taken (it waits for touches).
        let new_key = match (&pin, &new_transport) {
            (Some(pin), Some(t)) => Some(with_authenticator(t, listener.as_ref(), |a| register_key(a, pin, &known))?),
            _ => None,
        };
        self.change_protection(|o, began| {
            super::single::refuse_single(o)?;
            // Again under the lock: the factors may have changed meanwhile.
            check_rules(o, &ids, add)?;
            let now = now_ms();
            *began = true;
            let slot = if let Some((cred_id, prf_input, output)) = &new_key {
                let reg = Fido2Registration {
                    rp_id: RP_ID,
                    credential_id: cred_id,
                    prf_input: *prf_input,
                    hmac_output: output,
                };
                Some(o.suite.add_fido2_slot(&reg, &label, now, &mut OsRng)?)
            } else if let Some(kf) = &keyfile {
                Some(o.suite.add_keyfile_slot(kf, &label, now, &mut OsRng)?)
            } else {
                None
            };
            if let Some(slot) = slot {
                for v in &mut o.vaults {
                    if let Some(c) = v.container.as_mut() {
                        c.insert_slot(slot.clone(), &label, now)?;
                    }
                }
            }
            for id in &ids {
                for (c, _) in o.containers_mut() {
                    match c.remove_slot(id) {
                        Ok(()) | Err(crate::vault::VaultError::SlotNotFound) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            if !ids.is_empty() {
                // A removed key or key file with an old copy of a file must not open what is
                // written now: a new factor secret for those that stay, new container keys.
                o.renew_keys(true, now)?;
            }
            self.commit_protection(o)?;
            if !ids.is_empty() {
                super::quick::delete_all(&self.local_dir);
            }
            Ok(())
        })
    }
}
