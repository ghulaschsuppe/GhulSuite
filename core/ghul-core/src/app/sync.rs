// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synchronisation of the suite folder with a server folder (`<base>/suite/`).
//!
//! Files stay encrypted on the server. Each vault is matched by its container ID and merged
//! **entry by entry**: the higher entry revision wins (then the later change), deletions travel
//! as tombstones. So two devices can change different entries of the same vault at the same
//! time without losing anything. Vaults deleted on one device are deleted on the others (the
//! suite remembers which vaults were already synchronised).
//!
//! Protection (password, keys, key files, recovery code) is taken from the side changed last
//! (`protection_changed` in the settings). A newer password from another device must be typed
//! once (`adopt_remote_password`); new keys and codes are taken over without asking.

use std::collections::{BTreeMap, BTreeSet};

use ciborium::value::Value;

use super::error::{GhulError, Result};
use super::servers;
use super::suite::{
    is_vault_file, now_ms, put_setting, setting, vault_name, GhulSuite, Open, OpenVault, VaultKind, VaultRef,
    SUITE_FILE,
};
use crate::net::remote::RemoteFs;
use crate::vault::{Entry, FactorSecret, LockedContainer, PasswordKey, Slot, SlotMeta, UnlockedContainer};

const SYNC_KEY: &str = "sync";
const KNOWN_KEY: &str = "sync_known";
/// Ids of vaults deleted on purpose (synchronised). Only these are ever deleted by a sync: a
/// file that is missing, cut off or unreadable on one side is repaired from the other side.
pub(super) const DELETED_VAULTS_KEY: &str = "deleted_vaults";
pub(super) const PROTECTION_CHANGED: &str = "protection_changed";
const DIR: &str = "suite";
/// Settings that belong to one device and are never merged from another.
const DEVICE_KEYS: &[&str] = &[SYNC_KEY, KNOWN_KEY];

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct SyncSettings {
    #[serde(default)]
    server: String,
    #[serde(default)]
    last: i64,
}

#[derive(uniffi::Record, Clone, Debug, Default, PartialEq)]
pub struct SyncReport {
    pub uploaded: u32,
    pub downloaded: u32,
    /// Vaults changed on both sides and merged.
    pub merged: u32,
    pub deleted: u32,
    /// Files on the server that could not be opened (other suite, other code).
    pub skipped: u32,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SyncStatus {
    /// `None`: synchronisation is off.
    pub server_id: Option<String>,
    /// Unix time in milliseconds of the last successful synchronisation (0: never).
    pub last_sync: i64,
}

/// Marks a change of the protection (password, keys, code) for the synchronisation.
pub(super) fn mark_protection_changed(o: &mut Open) -> Result<()> {
    put_setting(&mut o.suite, PROTECTION_CHANGED, &now_ms())
}

/// The server this device synchronises with, if any.
pub(super) fn sync_server(o: &Open) -> Option<String> {
    let s: SyncSettings = setting(&o.suite, SYNC_KEY);
    (!s.server.is_empty()).then_some(s.server)
}

pub(super) fn forget_server(o: &mut Open, id: &str) -> Result<()> {
    let s: SyncSettings = setting(&o.suite, SYNC_KEY);
    if s.server == id {
        put_setting(&mut o.suite, SYNC_KEY, &SyncSettings::default())?;
    }
    Ok(())
}

/// Merges two versions of the entry list. Returns the result and whether it differs from the
/// local and from the remote list.
pub(crate) fn merge_entries(local: &[Entry], remote: &[Entry]) -> (Vec<Entry>, bool, bool) {
    let mut out: Vec<Entry> = Vec::with_capacity(local.len().max(remote.len()));
    let mut local_changed = false;
    let mut remote_changed = false;
    let remote_by_id: BTreeMap<&[u8], &Entry> = remote.iter().map(|e| (e.id.as_slice(), e)).collect();
    let local_ids: BTreeSet<&[u8]> = local.iter().map(|e| e.id.as_slice()).collect();
    for l in local {
        match remote_by_id.get(l.id.as_slice()) {
            None => {
                remote_changed = true;
                out.push(l.clone());
            }
            Some(r) => {
                let take_remote = (r.revision, r.modified) > (l.revision, l.modified);
                if take_remote {
                    local_changed = true;
                    out.push((*r).clone());
                } else {
                    if *r != l {
                        remote_changed = true;
                    }
                    out.push(l.clone());
                }
            }
        }
    }
    for r in remote.iter().filter(|r| !local_ids.contains(r.id.as_slice())) {
        local_changed = true;
        out.push(r.clone());
    }
    (out, local_changed, remote_changed)
}

/// Merges settings maps: missing keys are added, maps merge recursively, lists of objects with
/// an `id` are united by id (local wins), other lists are united, numbers take the maximum,
/// otherwise the local value stays. Returns whether `local` changed.
pub(crate) fn merge_settings(local: &mut BTreeMap<String, Value>, remote: &BTreeMap<String, Value>) -> bool {
    let mut changed = false;
    for (k, rv) in remote {
        match local.get_mut(k) {
            None => {
                local.insert(k.clone(), rv.clone());
                changed = true;
            }
            Some(lv) => changed |= merge_value(lv, rv),
        }
    }
    // What was deleted on any device stays deleted (lists would otherwise unite it back).
    changed |= prune_deleted(local);
    changed
}

/// Deletion marks of settings list items ("list/id" → Unix seconds), synchronised like the rest.
pub(crate) const DELETED_KEY: &str = "deleted";

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Identity of a list item: its `id`, else its `email` (contact keys).
fn item_key(v: &Value) -> Option<String> {
    let map = v.as_map()?;
    for name in ["id", "email"] {
        if let Some((_, Value::Text(t))) = map.iter().find(|(k, _)| k.as_text() == Some(name)) {
            return Some(t.clone());
        }
    }
    None
}

/// When an item was last renewed (`updated`, Unix seconds), if it says so.
fn item_time(v: &Value) -> Option<i128> {
    v.as_map()?.iter().find(|(k, _)| k.as_text() == Some("updated")).and_then(|(_, v)| match v {
        Value::Integer(i) => Some(i128::from(*i)),
        _ => None,
    })
}

/// Marks an item of the list `list` as deleted on every device and removes it here.
pub(crate) fn mark_deleted(settings: &mut BTreeMap<String, Value>, list: &str, id: &str) {
    let key = Value::Text(format!("{list}/{id}"));
    let now = Value::Integer(now_secs().into());
    let entry = settings.entry(DELETED_KEY.to_owned()).or_insert_with(|| Value::Map(Vec::new()));
    if !matches!(entry, Value::Map(_)) {
        *entry = Value::Map(Vec::new());
    }
    if let Value::Map(m) = entry {
        match m.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v = now,
            None => m.push((key, now)),
        }
    }
    prune_deleted(settings);
}

/// Removes list items that carry a deletion mark, unless they were renewed after it (a contact
/// key learned again after "forget"). Returns whether anything was removed.
pub(crate) fn prune_deleted(settings: &mut BTreeMap<String, Value>) -> bool {
    let Some(Value::Map(marks)) = settings.get(DELETED_KEY) else { return false };
    let marks: BTreeMap<String, i128> = marks
        .iter()
        .filter_map(|(k, v)| match (k, v) {
            (Value::Text(k), Value::Integer(t)) => Some((k.clone(), i128::from(*t))),
            _ => None,
        })
        .collect();
    let mut changed = false;
    for (list, value) in settings.iter_mut() {
        if list == DELETED_KEY {
            continue;
        }
        if let Value::Array(items) = value {
            let before = items.len();
            items.retain(|item| match item_key(item).and_then(|id| marks.get(&format!("{list}/{id}"))) {
                Some(deleted) => item_time(item).is_some_and(|t| t > *deleted),
                None => true,
            });
            changed |= items.len() != before;
        }
    }
    changed
}

fn value_id(v: &Value) -> Option<Value> {
    v.as_map()?.iter().find(|(k, _)| k.as_text() == Some("id")).map(|(_, v)| v.clone())
}

/// The time of a stamped value: a map of exactly `v` and `at` (integer).
fn stamp_time(v: &Value) -> Option<i128> {
    let m = v.as_map()?;
    if m.len() != 2 || !m.iter().any(|(k, _)| k.as_text() == Some("v")) {
        return None;
    }
    m.iter().find(|(k, _)| k.as_text() == Some("at")).and_then(|(_, t)| match t {
        Value::Integer(i) => Some(i128::from(*i)),
        _ => None,
    })
}

fn merge_value(lv: &mut Value, rv: &Value) -> bool {
    if lv == rv {
        return false;
    }
    // A stamped value ({v, at}): the later change wins as a whole.
    if let (Some(lt), Some(rt)) = (stamp_time(lv), stamp_time(rv)) {
        if rt > lt {
            *lv = rv.clone();
            return true;
        }
        return false;
    }
    match (lv, rv) {
        (Value::Map(l), Value::Map(r)) => {
            let mut changed = false;
            for (rk, rval) in r {
                match l.iter_mut().find(|(lk, _)| lk == rk) {
                    None => {
                        l.push((rk.clone(), rval.clone()));
                        changed = true;
                    }
                    Some((_, lval)) => changed |= merge_value(lval, rval),
                }
            }
            changed
        }
        (Value::Array(l), Value::Array(r)) => {
            let mut changed = false;
            for item in r {
                let present = match value_id(item) {
                    Some(id) => l.iter().any(|x| value_id(x).as_ref() == Some(&id)),
                    None => l.contains(item),
                };
                if !present {
                    l.push(item.clone());
                    changed = true;
                }
            }
            changed
        }
        (Value::Integer(l), Value::Integer(r)) if i128::from(*r) > i128::from(*l) => {
            *l = *r;
            true
        }
        _ => false,
    }
}

fn slot_label(c: &UnlockedContainer, s: &Slot) -> (String, i64) {
    c.payload.slots.get(&crate::kdf::hex(s.id())).map(|m| (m.label.clone(), m.created)).unwrap_or_default()
}

/// Opens a file from the server: with password and factor secret, or with the recovery code.
fn open_remote(o: &Open, data: &[u8]) -> Option<UnlockedContainer> {
    let locked = LockedContainer::parse(data).ok()?;
    if locked.header().suite_id != o.suite.header().suite_id {
        return None;
    }
    if locked.header().kdf == o.kdf() {
        let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
        if let Ok(c) = locked.unlock(&o.password_key, &s) {
            return Some(c);
        }
        return None;
    }
    let code = o.suite.recovery_code()?;
    LockedContainer::parse(data).ok()?.recover(&code).ok()
}

impl GhulSuite {
    fn sync_with(&self, o: &mut Open, fs: &dyn RemoteFs) -> Result<SyncReport> {
        let mut rep = SyncReport::default();
        fs.mkdir_all(DIR)?;
        let remote_files: Vec<String> =
            fs.list(DIR)?.into_iter().filter(|e| !e.is_dir).map(|e| e.name).filter(|n| is_vault_file(n)).collect();

        // 1. Settings and protection from the remote suite file.
        // A damaged suite file (cut off by a dropped upload) is replaced by this device's one at
        // the end; one from a newer app stops the sync instead of being overwritten.
        let remote_suite = match fs.read(&format!("{DIR}/{SUITE_FILE}"))? {
            Some(data) => match LockedContainer::parse(&data) {
                Ok(locked) => Some((data, locked)),
                Err(e) => match GhulError::from(e) {
                    GhulError::UpdateRequired => return Err(GhulError::UpdateRequired),
                    _ => None,
                },
            },
            None => None,
        };
        if let Some((data, locked)) = remote_suite {
            if locked.header().suite_id != o.suite.header().suite_id {
                return Err(GhulError::InvalidInput { msg: "the server folder belongs to another suite".into() });
            }
            let local_ts: i64 = setting(&o.suite, PROTECTION_CHANGED);
            match open_remote(o, &data) {
                Some(r) => {
                    let remote_ts: i64 = setting(&r, PROTECTION_CHANGED);
                    if remote_ts > local_ts {
                        if r.header().kdf != o.kdf() {
                            return Err(GhulError::PasswordChangedElsewhere);
                        }
                        self.adopt_protection(o, &r)?;
                    }
                    // What this device has synchronised (and with which server) stays its own.
                    let mut remote_settings = r.payload.settings.clone();
                    for k in DEVICE_KEYS {
                        remote_settings.remove(*k);
                    }
                    merge_settings(&mut o.suite.payload.settings, &remote_settings);
                }
                // Unreadable with our password and code: the protection changed elsewhere.
                None if locked.header().kdf != o.kdf() => return Err(GhulError::PasswordChangedElsewhere),
                // Same password, but the content does not open: damaged, replaced below.
                None => {}
            }
        }

        // 2. Vaults.
        // Deletions travel only as explicit marks (from this device or merged from the server
        // suite above). A vault missing or unreadable on one side is never taken as deleted.
        let deleted: BTreeSet<String> = setting::<Vec<String>>(&o.suite, DELETED_VAULTS_KEY).into_iter().collect();
        let mut known: BTreeSet<String> = setting::<Vec<String>>(&o.suite, KNOWN_KEY).into_iter().collect();
        let mut seen_remote: BTreeSet<String> = BTreeSet::new();
        // Server files this app must not overwrite: from a newer app or from another suite.
        let mut untouchable: BTreeSet<String> = BTreeSet::new();
        let now = now_ms();
        for name in &remote_files {
            let Some(data) = fs.read(&format!("{DIR}/{name}"))? else { continue };
            let locked = match LockedContainer::parse(&data) {
                Ok(l) => l,
                Err(e) => {
                    if matches!(GhulError::from(e), GhulError::UpdateRequired) {
                        untouchable.insert(name.clone());
                    }
                    // Otherwise damaged (cut off, broken upload): repaired from a local copy below.
                    rep.skipped += 1;
                    continue;
                }
            };
            if locked.header().suite_id != o.suite.header().suite_id {
                untouchable.insert(name.clone());
                rep.skipped += 1;
                continue;
            }
            let id = crate::kdf::hex(&locked.header().container_id);
            if deleted.contains(&id) {
                // Deleted on purpose on some device: away from the server as well.
                fs.delete(&format!("{DIR}/{name}"))?;
                known.remove(&id);
                rep.deleted += 1;
                continue;
            }
            let Some(remote) = open_remote(o, &data) else {
                // Same suite, but damaged: repaired from a local copy below.
                rep.skipped += 1;
                continue;
            };
            seen_remote.insert(id.clone());
            let local_idx = o.vaults.iter().position(|v| v.reference.id == id && v.container.is_some());
            match local_idx {
                None => {
                    // New on the server, or this device lost or broke its copy: take it over
                    // (with this device's protection). A broken local file of the same name is replaced.
                    let kind = VaultKind::from_container(remote.kind()).ok_or(GhulError::NotFound)?;
                    o.vaults.retain(|v| !(v.container.is_none() && &v.reference.file == name));
                    let local_name = if o.vaults.iter().any(|v| &v.reference.file == name) {
                        super::suite::file_name(kind, &vault_name(&remote, name), &id)
                    } else {
                        name.clone()
                    };
                    let reference = VaultRef {
                        id: id.clone(),
                        kind: kind.container().name().into(),
                        name: vault_name(&remote, &local_name),
                        file: local_name,
                    };
                    // Same bytes as on the server; harmonised below if its protection is older.
                    self.storage.write(reference.file.clone(), data.clone())?;
                    o.vaults.push(OpenVault { reference, container: Some(remote), stale: false });
                    known.insert(id.clone());
                    rep.downloaded += 1;
                }
                Some(i) => {
                    known.insert(id.clone());
                    let v = &mut o.vaults[i];
                    let local = v.container.as_mut().expect("checked above");
                    let (merged, local_changed, remote_changed) =
                        merge_entries(&local.payload.entries, &remote.payload.entries);
                    let mut settings_changed = false;
                    if remote.payload.revision > local.payload.revision {
                        let mut s = remote.payload.settings.clone();
                        merge_settings(&mut s, &local.payload.settings);
                        if s != local.payload.settings {
                            local.payload.settings = s;
                            settings_changed = true;
                        }
                    } else if merge_settings(&mut local.payload.settings, &remote.payload.settings) {
                        settings_changed = true;
                    }
                    // Settings changed only here (e.g. an area option) must reach the server too.
                    let settings_newer_here = local.payload.settings != remote.payload.settings;
                    let protection_differs =
                        local.header().slots != remote.header().slots || local.header().kdf != remote.header().kdf;
                    if local_changed || remote_changed || settings_changed || settings_newer_here || protection_differs
                    {
                        if local_changed && remote_changed {
                            rep.merged += 1;
                        } else if local_changed {
                            rep.downloaded += 1;
                        }
                        local.payload.entries = merged;
                        local.payload.revision = local.payload.revision.max(remote.payload.revision);
                        v.reference.name = vault_name(local, &v.reference.file);
                        v.stale = false;
                        // Written below, locally and to the server, under the server's name.
                        if remote_changed || settings_changed || settings_newer_here || protection_differs {
                            rep.uploaded += 1;
                        }
                        let file = v.reference.file.clone();
                        let bytes = local.seal(&mut rand_core::OsRng)?;
                        self.storage.write(file, bytes.clone())?;
                        fs.write(&format!("{DIR}/{name}"), &bytes)?;
                    }
                }
            }
        }
        // 3. Local vaults the server does not have (or has only damaged): uploaded, unless they
        // were deleted on purpose. A server file from a newer app is never overwritten.
        let mut gone = Vec::new();
        for (i, v) in o.vaults.iter_mut().enumerate() {
            let Some(c) = v.container.as_mut() else { continue };
            if seen_remote.contains(&v.reference.id) {
                continue;
            }
            if deleted.contains(&v.reference.id) {
                gone.push(i);
                continue;
            }
            if untouchable.contains(&v.reference.file) {
                continue;
            }
            let bytes = c.seal(&mut rand_core::OsRng)?;
            self.storage.write(v.reference.file.clone(), bytes.clone())?;
            fs.write(&format!("{DIR}/{}", v.reference.file), &bytes)?;
            known.insert(v.reference.id.clone());
            rep.uploaded += 1;
        }
        for i in gone.into_iter().rev() {
            let v = o.vaults.remove(i);
            self.storage.delete(v.reference.file.clone())?;
            known.remove(&v.reference.id);
            rep.deleted += 1;
        }
        // Files with older protection (e.g. downloaded from a device that missed a new key)
        // get the suite's protection, here and on the server.
        if o.harmonize(now)? {
            for v in &mut o.vaults {
                if let Some(c) = v.container.as_mut() {
                    let bytes = c.seal(&mut rand_core::OsRng)?;
                    self.storage.write(v.reference.file.clone(), bytes.clone())?;
                    fs.write(&format!("{DIR}/{}", v.reference.file), &bytes)?;
                }
            }
        }

        // 4. Settings file last.
        put_setting(&mut o.suite, KNOWN_KEY, &known.into_iter().collect::<Vec<_>>())?;
        let mut sync: SyncSettings = setting(&o.suite, SYNC_KEY);
        sync.last = now_ms();
        put_setting(&mut o.suite, SYNC_KEY, &sync)?;
        o.record_revisions()?;
        let bytes = o.suite.seal(&mut rand_core::OsRng)?;
        self.storage.write(SUITE_FILE.to_owned(), bytes.clone())?;
        fs.write(&format!("{DIR}/{SUITE_FILE}"), &bytes)?;
        Ok(rep)
    }

    /// Takes over keys, key files and recovery code of a newer remote suite (same password).
    fn adopt_protection(&self, o: &mut Open, remote: &UnlockedContainer) -> Result<()> {
        if remote.header().kdf != o.kdf() {
            return Err(GhulError::PasswordChangedElsewhere);
        }
        let slots: Vec<Slot> = remote.slots().iter().filter(|s| s.provides_factor()).cloned().collect();
        let s = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
        let pk = PasswordKey::from_bytes(*o.password_key.expose());
        let kdf = o.kdf();
        let suite_id = o.suite.header().suite_id;
        o.suite.reprotect(suite_id, s, slots.clone(), &pk, kdf, &mut rand_core::OsRng)?;
        for sl in &slots {
            let (label, created) = slot_label(remote, sl);
            o.suite
                .payload
                .slots
                .insert(crate::kdf::hex(sl.id()), SlotMeta { label, created, extra: Default::default() });
        }
        if let Some(code) = remote.recovery_code() {
            let label = remote
                .slots()
                .iter()
                .find(|s| matches!(s, Slot::Recovery(_)))
                .map(|s| slot_label(remote, s).0)
                .unwrap_or_default();
            o.suite.set_recovery_code(&code, &label, now_ms(), &mut rand_core::OsRng)?;
        }
        o.harmonize(now_ms())?;
        Ok(())
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Turns synchronisation with a stored server on (`Some`) or off (`None`).
    pub fn set_sync_server(&self, server_id: Option<String>) -> Result<()> {
        self.with_open(|o| {
            if let Some(id) = &server_id {
                if !servers::servers(o).iter().any(|s| &s.id == id) {
                    return Err(GhulError::NotFound);
                }
            }
            put_setting(&mut o.suite, SYNC_KEY, &SyncSettings { server: server_id.unwrap_or_default(), last: 0 })?;
            self.save_container(&mut o.suite, SUITE_FILE)
        })
    }

    pub fn sync_status(&self) -> Result<SyncStatus> {
        self.with_open(|o| {
            let s: SyncSettings = setting(&o.suite, SYNC_KEY);
            Ok(SyncStatus { server_id: (!s.server.is_empty()).then_some(s.server), last_sync: s.last })
        })
    }

    /// Synchronises with the configured server now.
    pub fn sync_now(&self) -> Result<SyncReport> {
        self.with_open(|o| {
            let s: SyncSettings = setting(&o.suite, SYNC_KEY);
            if s.server.is_empty() {
                return Err(GhulError::NotConfigured);
            }
            let fs = servers::connect(o, &s.server)?;
            self.sync_with(o, fs.as_ref())
        })
    }

    /// After a password change on another device: checks the new password against the server's
    /// suite file and switches this device to it. Then synchronises.
    pub fn adopt_remote_password(&self, password: String) -> Result<SyncReport> {
        self.with_open(|o| {
            let s: SyncSettings = setting(&o.suite, SYNC_KEY);
            let fs = servers::connect(o, &s.server)?;
            self.adopt_password_with(o, fs.as_ref(), password)
        })
    }
}

impl GhulSuite {
    fn adopt_password_with(&self, o: &mut Open, fs: &dyn RemoteFs, password: String) -> Result<SyncReport> {
        let password = zeroize::Zeroizing::new(password);
        {
            let data = fs.read(&format!("{DIR}/{SUITE_FILE}"))?.ok_or(GhulError::NotFound)?;
            let locked = LockedContainer::parse(&data)?;
            let kdf = locked.header().kdf.clone();
            let pk = PasswordKey::derive(&password, &kdf)?;
            let sec = FactorSecret::from_bytes(*o.suite.factor_secret().expose());
            let remote = locked.unlock(&pk, &sec).map_err(|_| GhulError::WrongPassword)?;
            for (c, _) in o.containers_mut() {
                c.rekey(&pk, kdf.clone(), &mut rand_core::OsRng)?;
            }
            o.password_key = pk;
            let remote_ts: i64 = setting(&remote, PROTECTION_CHANGED);
            put_setting(&mut o.suite, PROTECTION_CHANGED, &remote_ts)?;
            self.adopt_protection(o, &remote)?;
            super::quick::delete_all(&self.local_dir);
            self.sync_with(o, fs)
        }
    }
}

/// Synchronises with an arbitrary folder (tests, later USB mirror).
#[cfg(test)]
impl GhulSuite {
    pub(crate) fn sync_folder(&self, fs: &dyn RemoteFs) -> Result<SyncReport> {
        self.with_open(|o| self.sync_with(o, fs))
    }

    pub(crate) fn adopt_password_folder(&self, fs: &dyn RemoteFs, password: &str) -> Result<SyncReport> {
        self.with_open(|o| self.adopt_password_with(o, fs, password.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, updated: Option<i64>) -> Value {
        let mut m = vec![(Value::Text("id".into()), Value::Text(id.into()))];
        if let Some(u) = updated {
            m.push((Value::Text("updated".into()), Value::Integer(u.into())));
        }
        Value::Map(m)
    }

    fn ids(s: &BTreeMap<String, Value>, list: &str) -> Vec<String> {
        match s.get(list) {
            Some(Value::Array(a)) => a.iter().filter_map(item_key).collect(),
            _ => vec![],
        }
    }

    /// A share, server or account deleted on one device must not come back with the next sync.
    #[test]
    fn deletions_survive_merging() {
        let both = Value::Array(vec![item("a", None), item("b", None)]);
        let mut phone = BTreeMap::from([("shares".to_string(), both.clone())]);
        let mut pc = BTreeMap::from([("shares".to_string(), both)]);
        // The phone withdraws "b".
        if let Some(Value::Array(a)) = phone.get_mut("shares") {
            a.retain(|x| item_key(x).as_deref() != Some("b"));
        }
        mark_deleted(&mut phone, "shares", "b");
        // Phone gets the PC's (old) settings: "b" stays gone.
        assert!(!merge_settings(&mut phone, &pc.clone()) || ids(&phone, "shares") == vec!["a"]);
        assert_eq!(ids(&phone, "shares"), vec!["a"]);
        // The PC gets the phone's settings: "b" disappears there too.
        assert!(merge_settings(&mut pc, &phone));
        assert_eq!(ids(&pc, "shares"), vec!["a"]);
        // Nothing else is touched.
        mark_deleted(&mut pc, "servers", "x");
        assert_eq!(ids(&pc, "shares"), vec!["a"]);
    }

    /// "Forget key" does not block a key that is learned again later.
    #[test]
    fn renewed_items_survive_an_older_deletion() {
        let mut s = BTreeMap::new();
        mark_deleted(&mut s, "mail_keys", "a");
        let marked = match s.get(DELETED_KEY) {
            Some(Value::Map(m)) => match &m[0].1 {
                Value::Integer(i) => i64::try_from(i128::from(*i)).unwrap(),
                _ => 0,
            },
            _ => 0,
        };
        s.insert("mail_keys".into(), Value::Array(vec![item("a", Some(marked - 5)), item("a2", None)]));
        assert!(prune_deleted(&mut s));
        assert_eq!(ids(&s, "mail_keys"), vec!["a2"]);
        s.insert("mail_keys".into(), Value::Array(vec![item("a", Some(marked + 5))]));
        assert!(!prune_deleted(&mut s));
        assert_eq!(ids(&s, "mail_keys"), vec!["a"]);
    }

    fn e(id: u8, rev: u64, modified: i64, title: &str) -> Entry {
        let mut x = Entry::new([id; 16], "login", modified);
        x.revision = rev;
        x.fields.insert("title".into(), crate::vault::Field::Text(title.into()));
        x
    }

    #[test]
    fn entries_merge_by_revision() {
        let local = vec![e(1, 2, 10, "A2"), e(2, 1, 5, "B"), e(3, 1, 5, "only local")];
        let remote = vec![e(1, 1, 9, "A1"), e(2, 3, 20, "B3"), e(4, 1, 7, "only remote")];
        let (m, lc, rc) = merge_entries(&local, &remote);
        let titles: Vec<String> = m
            .iter()
            .map(|x| match &x.fields["title"] {
                crate::vault::Field::Text(t) => t.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(titles, vec!["A2", "B3", "only local", "only remote"]);
        assert!(lc && rc);
        let (_, lc, rc) = merge_entries(&local, &local);
        assert!(!lc && !rc);
    }

    #[test]
    fn settings_merge() {
        let mut l: BTreeMap<String, Value> = BTreeMap::new();
        l.insert("theme".into(), Value::Text("dark".into()));
        l.insert("n".into(), Value::Integer(3.into()));
        let mut r = BTreeMap::new();
        r.insert("theme".into(), Value::Text("light".into()));
        r.insert("n".into(), Value::Integer(7.into()));
        r.insert("new".into(), Value::Bool(true));
        r.insert(
            "servers".into(),
            Value::Array(vec![Value::Map(vec![(Value::Text("id".into()), Value::Text("s1".into()))])]),
        );
        assert!(merge_settings(&mut l, &r));
        assert_eq!(l["theme"], Value::Text("dark".into()));
        assert_eq!(l["n"], Value::Integer(7.into()));
        assert!(l.contains_key("new") && l.contains_key("servers"));
        assert!(!merge_settings(&mut l, &r));
    }
}
