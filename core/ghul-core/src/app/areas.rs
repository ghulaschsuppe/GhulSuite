// SPDX-License-Identifier: AGPL-3.0-or-later
//! One area, one file: GhulDrive (`.ghldrv`) and GhulMail (`.ghlmail`) keep their data and their
//! settings in their own vault file. The suite file only holds the protection, the sync server,
//! the sync state and the shares.
//!
//! Suites from before keep these settings in the suite file. They move into the area files on the
//! first unlock (and again whenever a sync brings them back from a device with an older app).

use std::collections::BTreeMap;

use ciborium::value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::{GhulError, Result};
use super::suite::{put_setting, setting, GhulSuite, Open, VaultKind, SUITE_FILE};
use super::sync::{merge_settings, DELETED_KEY};
use crate::vault::UnlockedContainer;

/// Settings that belong to GhulDrive.
pub(super) const DRIVE_KEYS: &[&str] = &["drive"];
/// Settings that belong to GhulMail.
pub(super) const MAIL_KEYS: &[&str] =
    &["mail_accounts", "mail_keys", "mail_contacts", "mail_cache_key", "mail_drafts", "mail_outbox"];

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

/// Marks a list item of an area's settings as deleted on every device.
pub(super) fn mark_area_deleted(o: &mut Open, kind: VaultKind, list: &str, id: &str) -> Result<()> {
    super::sync::mark_deleted(&mut o.area_mut(kind)?.payload.settings, list, id);
    Ok(())
}

/// Moves `keys` (and their deletion marks) from `from` into `to`. Returns whether anything moved.
fn move_settings(from: &mut BTreeMap<String, Value>, to: &mut BTreeMap<String, Value>, keys: &[&str]) -> bool {
    let mut moved = BTreeMap::new();
    for k in keys {
        if let Some(v) = from.remove(*k) {
            moved.insert((*k).to_owned(), v);
        }
    }
    if let Some(Value::Map(marks)) = from.get_mut(DELETED_KEY) {
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(marks).into_iter().partition(
            |(k, _)| matches!(k, Value::Text(t) if keys.iter().any(|key| t.starts_with(&format!("{key}/")))),
        );
        *marks = rest;
        if !mine.is_empty() {
            moved.insert(DELETED_KEY.to_owned(), Value::Map(mine));
        }
    }
    if moved.is_empty() {
        return false;
    }
    merge_settings(to, &moved);
    super::sync::prune_deleted(to);
    true
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

    /// Moves Drive and Mail settings that still live in the suite file into their area files.
    pub(super) fn migrate_areas(&self, o: &mut Open) -> Result<()> {
        for (kind, keys) in [(VaultKind::Drive, DRIVE_KEYS), (VaultKind::Mail, MAIL_KEYS)] {
            if !keys.iter().any(|k| o.suite.payload.settings.contains_key(*k)) {
                continue;
            }
            let id = self.ensure_area(o, kind)?;
            let mut suite_settings = std::mem::take(&mut o.suite.payload.settings);
            let (_, area) = o.vault_mut(&id)?;
            let moved = move_settings(&mut suite_settings, &mut area.payload.settings, keys);
            o.suite.payload.settings = suite_settings;
            if moved {
                self.save_vault(o, &id)?;
                self.save_container(&mut o.suite, SUITE_FILE)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_keys_and_their_deletion_marks() {
        let mut suite = BTreeMap::new();
        suite.insert("mail_accounts".to_owned(), Value::Array(vec![Value::Text("a".into())]));
        suite.insert("servers".to_owned(), Value::Array(vec![]));
        suite.insert(
            DELETED_KEY.to_owned(),
            Value::Map(vec![
                (Value::Text("mail_drafts/x".into()), Value::Integer(5.into())),
                (Value::Text("shares/y".into()), Value::Integer(6.into())),
            ]),
        );
        let mut area = BTreeMap::new();
        assert!(move_settings(&mut suite, &mut area, MAIL_KEYS));
        assert!(area.contains_key("mail_accounts") && !suite.contains_key("mail_accounts"));
        assert!(suite.contains_key("servers"));
        let marks = |m: &BTreeMap<String, Value>| match m.get(DELETED_KEY) {
            Some(Value::Map(v)) => v.iter().filter_map(|(k, _)| k.as_text().map(str::to_owned)).collect::<Vec<_>>(),
            _ => vec![],
        };
        assert_eq!(marks(&suite), vec!["shares/y"]);
        assert_eq!(marks(&area), vec!["mail_drafts/x"]);
        assert!(!move_settings(&mut suite, &mut area, MAIL_KEYS));
    }
}
