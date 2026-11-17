// SPDX-License-Identifier: AGPL-3.0-or-later
//! One vault on its own, and picked entries handed on as a vault of their own.
//!
//! *Only this vault*: the app opens a vault file in a second [`GhulSuite`] whose folder is
//! temporary and empty; the user's own suite stays locked. Only that vault is unlocked. Its
//! protection stays as it is, and the app writes [`GhulSuite::single_vault_file`] back into the
//! file it came from.
//!
//! *Hand on as a vault*: the picked entries of a GhulPass or GhulAuth vault go into a new vault
//! file with a suite ID, password, second factor (a new key file or a security key) and recovery
//! code of its own. Nothing of the own suite's protection goes along.

use std::sync::Arc;

use rand_core::OsRng;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::passx::HISTORY;
use super::suite::{
    file_name, now_ms, parse_keyfile, put_setting, register_key, second_factor, set_vault_name, setting,
    with_authenticator, GhulSuite, Open, OpenVault, VaultKind, VaultRef,
};
use crate::vault::container::{Fido2Registration, KdfCost, SuiteKeys};
use crate::vault::{Entry, LockedContainer, PasswordKey, Slot, SlotMeta};
use crate::{FidoListener, HidTransport, RP_ID};

/// Suite setting of the temporary suite around a vault opened on its own.
const SINGLE_KEY: &str = "single";

/// What "hand on as a vault" needs besides the second factor.
#[derive(uniffi::Record, Clone)]
pub struct ShareVaultOptions {
    pub vault_id: String,
    /// The picked entries.
    pub ids: Vec<Vec<u8>>,
    /// Name of the new vault.
    pub name: String,
    /// The own master password, asked again: whoever finds the device unlocked must not
    /// carry entries off under a password of their own.
    pub master_password: String,
    /// Password of the new vault.
    pub password: String,
    /// Key file of the new vault (from `generate_keyfile`); `None` with a security key.
    pub keyfile: Option<Vec<u8>>,
    pub key_label: String,
    pub keyfile_label: String,
    pub recovery_label: String,
}

super::redact::redacted_debug!(ShareVaultOptions { vault_id, name, key_label, keyfile_label, recovery_label });

/// The new vault file and its recovery code (shown once).
#[derive(uniffi::Record, Clone)]
pub struct SharedVault {
    pub kind: VaultKind,
    pub data: Vec<u8>,
    pub recovery_code: String,
}

super::redact::redacted_debug!(SharedVault { kind });

#[uniffi::export]
impl GhulSuite {
    /// Opens one vault file on its own (this suite's folder must be empty: the app gives it a
    /// temporary one). GhulDrive and GhulMail vaults need the servers and accounts of a suite
    /// and can only be added to one.
    pub fn open_single_vault(
        &self,
        data: Vec<u8>,
        password: String,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
        keyfile: Option<Vec<u8>>,
    ) -> Result<VaultKind> {
        let password = Zeroizing::new(password);
        let pin = pin.map(Zeroizing::new);
        let data = Zeroizing::new(data);
        if self.exists()? {
            return Err(GhulError::AlreadyExists);
        }
        let locked = LockedContainer::parse(&data)?;
        let kind = VaultKind::from_container(locked.kind())
            .ok_or_else(|| GhulError::InvalidInput { msg: "this is a suite file, open its folder instead".into() })?;
        if matches!(kind, VaultKind::Drive | VaultKind::Mail) {
            return Err(GhulError::InvalidInput { msg: "this vault can only be added to a suite".into() });
        }
        let keyfile = parse_keyfile(keyfile)?;
        let s = second_factor(
            &locked,
            transport.as_ref(),
            listener.as_ref(),
            pin.as_deref().map(|p| p.as_str()),
            keyfile.as_ref(),
        )?;
        let pk = PasswordKey::derive(&password, &locked.header().kdf)?;
        let vault = locked.unlock(&pk, &s)?;
        let mut suite = self.suite_for_vault(&vault, &pk)?;
        put_setting(&mut suite, SINGLE_KEY, &true)?;
        // Nothing to confirm: the code (if any) belongs to whoever made the file.
        suite.payload.settings.remove(super::setup::CONFIRMED_KEY);
        let id = crate::kdf::hex(&vault.container_id());
        let file = file_name(kind, &id);
        let name = super::suite::vault_name(&vault, &file);
        let reference = VaultRef { id, kind: kind.container().name().into(), name, file };
        let mut open = Open { password_key: pk, suite, vaults: vec![OpenVault { reference, container: Some(vault) }] };
        self.save_all(&mut open)?;
        self.clear_unsaved();
        self.set_open(open);
        Ok(kind)
    }

    /// Whether this suite is the temporary one around a vault opened on its own.
    pub fn is_single(&self) -> bool {
        self.with_open(|o| Ok(setting::<bool>(&o.suite, SINGLE_KEY))).unwrap_or(false)
    }

    /// The vault opened on its own as it is now (to be written back into its file). Its
    /// protection is the one it came with.
    pub fn single_vault_file(&self) -> Result<Vec<u8>> {
        let file = self.with_open(|o| {
            if !setting::<bool>(&o.suite, SINGLE_KEY) {
                return Err(GhulError::InvalidInput { msg: "no vault is open on its own".into() });
            }
            let v = o.vaults.first().filter(|v| v.container.is_some()).ok_or(GhulError::NotFound)?;
            Ok(v.reference.file.clone())
        })?;
        self.storage.read(file)?.ok_or(GhulError::NotFound)
    }

    /// The vault opened on its own was written back into its file.
    pub fn single_saved(&self) {
        self.clear_unsaved();
    }

    /// Puts the picked entries of a GhulPass or GhulAuth vault into a new vault file with a
    /// protection of its own: `options.password` plus a new key file (`options.keyfile`) or a
    /// security key (`transport` and `pin`, two touches), and a new recovery code. Password
    /// histories stay behind.
    pub fn share_as_vault(
        &self,
        options: ShareVaultOptions,
        transport: Option<Arc<dyn HidTransport>>,
        listener: Option<Arc<dyn FidoListener>>,
        pin: Option<String>,
    ) -> Result<SharedVault> {
        let password = Zeroizing::new(options.password.clone());
        let pin = pin.map(Zeroizing::new);
        if options.name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        if password.chars().count() < 8 {
            return Err(GhulError::InvalidInput { msg: "password too short".into() });
        }
        let keyfile = parse_keyfile(options.keyfile.clone())?;
        if keyfile.is_some() == pin.is_some() {
            return Err(GhulError::InvalidInput { msg: "either a key file or a security key".into() });
        }
        if !self.verify_password(options.master_password.clone())? {
            return Err(GhulError::WrongPassword);
        }
        // Copied first: the key may take a while to be touched, the suite is not held meanwhile.
        let (kind, cost, entries) = self.with_open(|o| {
            let (r, c) = o.vault_mut(&options.vault_id)?;
            let kind = VaultKind::from_name(&r.kind);
            if !matches!(kind, VaultKind::Pass | VaultKind::Totp) {
                return Err(GhulError::InvalidInput {
                    msg: "only GhulPass and GhulAuth entries can be handed on".into(),
                });
            }
            let entries: Vec<Entry> = c
                .payload
                .entries
                .iter()
                .filter(|e| !e.deleted && options.ids.contains(&e.id))
                .cloned()
                .map(|mut e| {
                    e.extra.remove(HISTORY);
                    e
                })
                .collect();
            if entries.is_empty() {
                return Err(GhulError::NotFound);
            }
            let kdf = o.kdf();
            let cost = KdfCost { memory_kib: kdf.memory_kib, iterations: kdf.iterations, parallelism: kdf.parallelism };
            Ok((kind, cost, entries))
        })?;
        let keys = SuiteKeys::create(&password, cost, &mut OsRng)?;
        let now = now_ms();
        let (slot, label): (Slot, &str) = match (&keyfile, &transport, &pin) {
            (Some(kf), _, _) => (keys.keyfile_slot(kf, &mut OsRng), &options.keyfile_label),
            (None, Some(t), Some(pin)) => {
                let (cred_id, prf_input, output) = with_authenticator(t, listener.as_ref(), |a| register_key(a, pin))?;
                let slot = keys.fido2_slot(
                    &Fido2Registration { rp_id: RP_ID, credential_id: &cred_id, prf_input, hmac_output: &output },
                    &mut OsRng,
                );
                (slot, &options.key_label)
            }
            _ => return Err(GhulError::InvalidInput { msg: "key and PIN must be given together".into() }),
        };
        let mut c = keys.create_container(kind.container(), vec![slot.clone()], &mut OsRng)?;
        c.payload.slots.insert(
            crate::kdf::hex(slot.id()),
            SlotMeta { label: label.to_owned(), created: now, extra: Default::default() },
        );
        let (_, code) = c.add_recovery_slot(&options.recovery_label, now, &mut OsRng)?;
        set_vault_name(&mut c, &options.name);
        c.payload.entries = entries;
        let data = c.seal(&mut OsRng)?;
        Ok(SharedVault { kind, data, recovery_code: code.to_display().to_string() })
    }
}
