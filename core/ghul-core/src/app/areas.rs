// SPDX-License-Identifier: AGPL-3.0-or-later
//! One area, one file: GhulDrive (`.ghldrv`) and GhulMail (`.ghlmail`) keep their data and their
//! settings in their own vault file. The suite file only holds the protection, the servers and
//! the shares.

use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::{GhulError, Result};
use super::suite::{put_setting, setting, GhulSuite, Open, VaultKind};
use crate::vault::UnlockedContainer;

/// Options of an area that travel with its file (e.g. GhulCal's holiday region).
const OPTIONS_KEY: &str = "options";

/// Name of an area file created without asking for one.
fn default_name(kind: VaultKind) -> &'static str {
    match kind {
        VaultKind::Drive => "Drive",
        VaultKind::Mail => "Mail",
        VaultKind::Cal => "Kalender",
        VaultKind::Totp => "2FA",
        VaultKind::Pass => "Passwörter",
    }
}

impl Open {
    /// Id of the area's file (the first readable one of that kind).
    pub(super) fn area_id(&self, kind: VaultKind) -> Option<String> {
        let name = kind.container().name();
        self.vaults.iter().find(|v| v.reference.kind == name && v.container.is_some()).map(|v| v.reference.id.clone())
    }

    fn area(&self, kind: VaultKind) -> Option<&UnlockedContainer> {
        let name = kind.container().name();
        self.vaults.iter().find(|v| v.reference.kind == name).and_then(|v| v.container.as_ref())
    }

    /// The area's file; `NotConfigured` when the area has not been set up.
    pub(super) fn area_mut(&mut self, kind: VaultKind) -> Result<&mut UnlockedContainer> {
        let id = self.area_id(kind).ok_or(GhulError::NotConfigured)?;
        Ok(self.vault_mut(&id)?.1)
    }
}

/// A setting of an area; the default when the area or the setting does not exist.
pub(super) fn area_setting<T: DeserializeOwned + Default>(o: &Open, kind: VaultKind, key: &str) -> T {
    o.area(kind).map(|c| setting(c, key)).unwrap_or_default()
}

/// Stores a setting in the area's file (the area must exist).
pub(super) fn put_area_setting<T: Serialize>(o: &mut Open, kind: VaultKind, key: &str, value: &T) -> Result<()> {
    put_setting(o.area_mut(kind)?, key, value)
}

impl GhulSuite {
    /// The area's file id, created (with the suite's protection) if the area does not exist yet.
    pub(super) fn ensure_area(&self, o: &mut Open, kind: VaultKind) -> Result<String> {
        if let Some(id) = o.area_id(kind) {
            return Ok(id);
        }
        let id = self.add_vault(o, kind, default_name(kind))?;
        self.save_vault(o, &id)?;
        Ok(id)
    }

    /// Saves the area's file (and the suite's record of it); nothing when the area does not exist.
    pub(super) fn save_area(&self, o: &mut Open, kind: VaultKind) -> Result<()> {
        match o.area_id(kind) {
            Some(id) => self.save_vault(o, &id),
            None => Ok(()),
        }
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Options stored in the area's file (empty when the area has not been set up). Keys and
    /// values are chosen by the app, e.g. `holiday_region` → `DE-NW`.
    pub fn area_options(&self, kind: VaultKind) -> Result<HashMap<String, String>> {
        self.with_open(|o| {
            let map: BTreeMap<String, String> = area_setting(o, kind, OPTIONS_KEY);
            Ok(map.into_iter().collect())
        })
    }

    /// Stores options in the area's file and saves it (`NotConfigured` without the area).
    pub fn set_area_options(&self, kind: VaultKind, options: HashMap<String, String>) -> Result<()> {
        self.with_open(|o| {
            let mut map: BTreeMap<String, String> = area_setting(o, kind, OPTIONS_KEY);
            let mut changed = false;
            for (k, v) in options {
                if map.get(&k) != Some(&v) {
                    map.insert(k, v);
                    changed = true;
                }
            }
            if !changed {
                return Ok(());
            }
            put_area_setting(o, kind, OPTIONS_KEY, &map)?;
            self.save_area(o, kind)
        })
    }

    /// Removes an area: its files are deleted. Hiding an area on one device is a setting of the
    /// app instead.
    pub fn remove_area(&self, kind: VaultKind) -> Result<()> {
        let name = kind.container().name();
        let ids: Vec<String> = self.with_open(|o| {
            Ok(o.vaults.iter().filter(|v| v.reference.kind == name).map(|v| v.reference.id.clone()).collect())
        })?;
        for id in ids {
            self.remove_vault(id)?;
        }
        if kind == VaultKind::Drive {
            self.net.lock().unwrap_or_else(|p| p.into_inner()).drive = None;
        }
        Ok(())
    }

    /// Creates the area's file now (when the area is switched on later) and returns its id;
    /// an existing area keeps its file. `name` empty: the usual name.
    pub fn add_area(&self, kind: VaultKind, name: String) -> Result<String> {
        self.with_open(|o| {
            if let Some(id) = o.area_id(kind) {
                return Ok(id);
            }
            let name = if name.trim().is_empty() { default_name(kind).to_owned() } else { name.trim().to_owned() };
            let id = self.add_vault(o, kind, &name)?;
            self.save_vault(o, &id)?;
            Ok(id)
        })
    }
}
