// SPDX-License-Identifier: AGPL-3.0-or-later
//! The key change after a change of the protection (concept, section 6.4).
//!
//! A new password, a new recovery code, a key or key file removed: from then on an older copy
//! of the suite file must not reach the servers any more. So the suite gets a new SSH key and
//! GhulDrive a new key, in addition to the earlier ones, with the note "change running":
//!
//! 1. every Drive location's index is written with the new key (see `drive_rekey_here`);
//! 2. on every server the new SSH key is added to `~/.ssh/authorized_keys`, the login with it is
//!    checked, then the earlier keys are removed there and it is checked that they no longer
//!    log in. Where the hoster does not use that file (keys are managed in its web panel), the
//!    user adds the new key there by hand and the app checks it;
//! 3. once everything is through, the earlier keys go.
//!
//! Every step can stop (no network, the app is closed): it continues where it stopped. Until
//! then, servers are reached with an earlier key where the new one does not log in yet.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::servers::{all_servers, sftp_config, ssh_private_key, StoredServer, SSH_KEY};
use super::suite::{put_setting, setting, GhulSuite, Open};
use crate::net::remote::RemoteFs;
use crate::net::sftp::{generate_key, public_key_of, SftpConfig, SftpFs, SshAuth};
use crate::net::NetError;

/// Suite setting: the running change (absent when none runs).
const CHANGE_KEY: &str = "key_change";

#[derive(Serialize, Deserialize, Default, Clone)]
struct KeyChange {
    /// The earlier private SSH keys (OpenSSH format), newest first.
    #[serde(default)]
    ssh_old: Vec<String>,
    /// Servers that take the new key and no earlier one any more.
    #[serde(default)]
    done: Vec<String>,
}

impl Drop for KeyChange {
    fn drop(&mut self) {
        for k in &mut self.ssh_old {
            zeroize::Zeroize::zeroize(k);
        }
    }
}

/// Where a server is in the key change.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyChangeState {
    /// Not reached yet (no network, server down): tried again later.
    Waiting,
    /// The new key logs in, the earlier ones do not.
    Done,
    /// The new key does not log in: the hoster manages keys in its web panel. Add the new
    /// public key there, then check again.
    AddByHand,
    /// The new key logs in, but an earlier one still does: remove it in the hoster's web panel,
    /// then check again.
    RemoveByHand,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct KeyChangeServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub state: KeyChangeState,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct KeyChangeStatus {
    /// A change runs (also when everything waits for the network).
    pub running: bool,
    /// GhulDrive locations whose index does not have the new key yet (names).
    pub drive_waiting: Vec<String>,
    pub servers: Vec<KeyChangeServer>,
    /// The new public key (to add by hand where needed).
    pub public_key: String,
    /// Nothing moves to the new keys before the suite file with the new protection is saved:
    /// until then the earlier copy is the only one that opens GhulDrive and the servers.
    pub waiting_for_save: bool,
}

/// Where the server keeps the keys allowed to log in: the login folder and the file in it.
fn authorized_keys_place() -> (String, String) {
    #[cfg(test)]
    if let Some(place) = AUTH_KEYS_FOR_TESTS.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        return place;
    }
    (String::new(), ".ssh/authorized_keys".to_owned())
}

/// Tests: the test server reads its keys from another file.
#[cfg(test)]
pub(crate) static AUTH_KEYS_FOR_TESTS: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

/// The earlier SSH keys of a running change (to reach servers that do not take the new one).
pub(super) fn earlier_ssh_keys(o: &Open) -> Vec<Zeroizing<String>> {
    let c: KeyChange = setting(&o.suite, CHANGE_KEY);
    c.ssh_old.iter().map(|k| Zeroizing::new(k.clone())).collect()
}

/// The key part of a public key line ("ssh-ed25519 AAAA…", without the comment).
fn body(public: &str) -> String {
    public.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

/// Every server once (GhulDrive's location can be a copy of a suite server, with its id).
fn servers_once(o: &Open) -> Vec<StoredServer> {
    let mut seen = std::collections::HashSet::new();
    all_servers(o).into_iter().filter(|s| seen.insert(s.id.clone())).collect()
}

/// `text` without the lines that hold one of `bodies`; every other line stays as it is (also
/// bytes that are no UTF-8).
fn without_lines(text: &[u8], bodies: &[String]) -> Vec<u8> {
    let has = |line: &[u8], b: &str| !b.is_empty() && line.windows(b.len()).any(|w| w == b.as_bytes());
    let mut out = Vec::with_capacity(text.len());
    for line in text.split_inclusive(|c| *c == b'\n') {
        if !bodies.iter().any(|b| has(line, b)) {
            out.extend_from_slice(line);
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn without_lines_for_tests(text: &[u8], bodies: &[String]) -> Vec<u8> {
    without_lines(text, bodies)
}

/// One server's part of the change: prepared with the suite, done without holding it (the
/// logins wait for the network).
struct Job {
    id: String,
    cfg: SftpConfig,
    new: Zeroizing<String>,
    new_public: String,
    earlier: Vec<Zeroizing<String>>,
}

impl Job {
    fn login(&self, key: &Zeroizing<String>) -> std::result::Result<SftpFs, NetError> {
        let mut cfg = self.cfg.clone();
        cfg.auth = SshAuth::Key(key.clone());
        SftpFs::connect(&cfg)
    }

    /// Moves the server to the new key as far as it goes (see the module). Only a clear answer
    /// counts: a login refused is refused, no network is "try again later".
    fn run(&self) -> KeyChangeState {
        use KeyChangeState::{AddByHand, Done, RemoveByHand, Waiting};
        // A failure that may pass (no network, time out) waits; any other one needs the user.
        let or_wait = |e: NetError, otherwise: KeyChangeState| match e {
            NetError::Connect(_) | NetError::Timeout => Waiting,
            _ => otherwise,
        };
        let path = authorized_keys_place().1;
        let earlier_bodies: Vec<String> =
            self.earlier.iter().filter_map(|k| public_key_of(k).ok()).map(|p| body(&p)).collect();
        let fs = match self.login(&self.new) {
            Ok(fs) => fs,
            Err(NetError::AuthFailed) => {
                // In with an earlier key, add the new one, check it.
                let mut unsure = false;
                let mut old_fs = None;
                for k in &self.earlier {
                    match self.login(k) {
                        Ok(fs) => {
                            old_fs = Some(fs);
                            break;
                        }
                        Err(NetError::AuthFailed) => {}
                        Err(_) => unsure = true,
                    }
                }
                // No key logs in: the user adds the new one (e.g. the old one was removed in
                // the hoster's panel already).
                let Some(old_fs) = old_fs else { return if unsure { Waiting } else { AddByHand } };
                let had = match old_fs.read(&path) {
                    Ok(had) => had.unwrap_or_default(),
                    Err(e) => return or_wait(e, AddByHand),
                };
                let new_body = body(&self.new_public);
                if !had.windows(new_body.len()).any(|w| w == new_body.as_bytes()) {
                    if let Err(e) = old_fs.append_line(&path, &self.new_public) {
                        return or_wait(e, AddByHand);
                    }
                }
                match self.login(&self.new) {
                    Ok(fs) => fs,
                    // The hoster does not use that file: by hand in its panel.
                    Err(NetError::AuthFailed) => return AddByHand,
                    Err(_) => return Waiting,
                }
            }
            Err(_) => return Waiting,
        };
        // The earlier keys out of the file (every other line stays).
        match fs.read(&path) {
            Err(e) => return or_wait(e, RemoveByHand),
            Ok(None) => {}
            Ok(Some(had)) => {
                let kept = without_lines(&had, &earlier_bodies);
                if kept != had {
                    // Rewritten, the file gets the server's default rights: only where they can
                    // be set to 0600 (else sshd may ignore it, and both keys are out).
                    if let Err(e) = fs.set_private(&path) {
                        return or_wait(e, RemoveByHand);
                    }
                    if let Err(e) = fs.write(&path, &kept) {
                        return or_wait(e, RemoveByHand);
                    }
                    if fs.set_private(&path).is_err() {
                        let _ = fs.write(&path, &had);
                        let _ = fs.set_private(&path);
                        return RemoveByHand;
                    }
                    // The new key must still log in; otherwise the file goes back as it was.
                    match self.login(&self.new) {
                        Ok(_) => {}
                        Err(NetError::AuthFailed) => {
                            let _ = fs.write(&path, &had);
                            let _ = fs.set_private(&path);
                            return RemoveByHand;
                        }
                        Err(_) => return Waiting,
                    }
                }
            }
        }
        // None of the earlier keys may log in any more: only a refused login counts.
        for k in &self.earlier {
            match self.login(k) {
                Ok(_) => return RemoveByHand,
                Err(NetError::AuthFailed) => {}
                Err(_) => return Waiting,
            }
        }
        Done
    }
}

impl GhulSuite {
    /// Starts a key change (called with every change of the protection that must shut out older
    /// copies): a new SSH key for the suite and a new key for GhulDrive, the earlier ones kept
    /// until the change is through.
    pub(super) fn begin_key_change(&self, o: &mut Open) -> Result<()> {
        let current: Zeroizing<String> = Zeroizing::new(setting(&o.suite, SSH_KEY));
        if !current.is_empty() && !all_servers(o).is_empty() {
            let mut c: KeyChange = setting(&o.suite, CHANGE_KEY);
            c.ssh_old.insert(0, current.to_string());
            c.done.clear();
            let (private, _) = generate_key("GhulSuite").map_err(GhulError::from)?;
            put_setting(&mut o.suite, SSH_KEY, &private.as_str())?;
            put_setting(&mut o.suite, CHANGE_KEY, &c)?;
        }
        self.drive_begin_rekey(o)
    }

    fn key_change_status_in(&self, o: &mut Open, states: &[(String, KeyChangeState)]) -> Result<KeyChangeStatus> {
        let c: KeyChange = setting(&o.suite, CHANGE_KEY);
        let drive_waiting: Vec<String> = self.drive_rekey_waiting(o).into_iter().map(|(_, n)| n).collect();
        let ssh_running = !c.ssh_old.is_empty();
        let servers: Vec<KeyChangeServer> = if ssh_running {
            servers_once(o)
                .into_iter()
                .map(|s| {
                    let state = if c.done.contains(&s.id) {
                        KeyChangeState::Done
                    } else {
                        states.iter().find(|(id, _)| *id == s.id).map_or(KeyChangeState::Waiting, |(_, st)| *st)
                    };
                    KeyChangeServer { id: s.id.clone(), name: s.name.clone(), host: s.host.clone(), state }
                })
                .collect()
        } else {
            Vec::new()
        };
        let public_key = if ssh_running { public_key_of(&ssh_private_key(o)?)? } else { String::new() };
        let running = ssh_running || !drive_waiting.is_empty();
        let waiting_for_save = running && !self.key_change_saved(o);
        Ok(KeyChangeStatus { running, drive_waiting, servers, public_key, waiting_for_save })
    }

    /// Whether the suite file with the current protection was saved (or opened) on this device:
    /// only then the key change goes on. Before, the earlier suite file is the only copy, and
    /// it must still reach GhulDrive and the servers (e.g. when this device is lost meanwhile).
    pub(super) fn key_change_saved(&self, o: &Open) -> bool {
        self.saved_epoch(&o.suite.header().suite_id) >= o.epoch()
    }

    /// The servers still to move to the new key.
    fn key_change_jobs(&self, o: &mut Open) -> Result<Vec<Job>> {
        let c: KeyChange = setting(&o.suite, CHANGE_KEY);
        if c.ssh_old.is_empty() || !self.key_change_saved(o) {
            return Ok(Vec::new());
        }
        let new = ssh_private_key(o)?;
        let new_public = public_key_of(&new)?;
        let earlier = earlier_ssh_keys(o);
        let mut jobs = Vec::new();
        for s in servers_once(o).into_iter().filter(|s| !c.done.contains(&s.id)) {
            let mut cfg = sftp_config(o, &s)?;
            cfg.base = authorized_keys_place().0;
            jobs.push(Job {
                id: s.id,
                cfg,
                new: new.clone(),
                new_public: new_public.clone(),
                earlier: earlier.clone(),
            });
        }
        Ok(jobs)
    }

    /// Notes the servers that are through; once all are, the earlier SSH keys go. Results of a
    /// change that another change of the protection replaced meanwhile are not taken.
    fn note_key_change(&self, o: &mut Open, new_public: &str, states: &[(String, KeyChangeState)]) -> Result<()> {
        let mut c: KeyChange = setting(&o.suite, CHANGE_KEY);
        if c.ssh_old.is_empty() || public_key_of(&ssh_private_key(o)?)? != new_public {
            return Ok(());
        }
        for (id, st) in states {
            if *st == KeyChangeState::Done && !c.done.contains(id) {
                c.done.push(id.clone());
            }
        }
        if servers_once(o).iter().all(|s| c.done.contains(&s.id)) {
            c = KeyChange::default();
        }
        put_setting(&mut o.suite, CHANGE_KEY, &c)?;
        self.save_container(&mut o.suite, super::suite::SUITE_FILE)
    }
}

#[uniffi::export]
impl GhulSuite {
    /// Where a running key change is (nothing tried).
    pub fn key_change_status(&self) -> Result<KeyChangeStatus> {
        self.with_open(|o| self.key_change_status_in(o, &[]))
    }

    /// Continues a running key change as far as it goes now: Drive's indexes with the new key,
    /// every server to the new SSH key. Returns where it stands. The servers are worked on
    /// without holding the suite (other calls do not wait for their logins).
    pub fn key_change_continue(&self) -> Result<KeyChangeStatus> {
        let jobs = self.with_open(|o| {
            super::single::refuse_single(o)?;
            // Drive like every Drive call: under the suite.
            let _ = self.drive_rekey_all(o);
            self.key_change_jobs(o)
        })?;
        let states: Vec<(String, KeyChangeState)> = jobs.iter().map(|j| (j.id.clone(), j.run())).collect();
        self.with_open(|o| {
            // Also without jobs: e.g. the only server still waiting was removed meanwhile.
            let worked_with = match jobs.first() {
                Some(j) => j.new_public.clone(),
                None => {
                    let current: Zeroizing<String> = Zeroizing::new(setting(&o.suite, SSH_KEY));
                    if current.is_empty() {
                        String::new()
                    } else {
                        public_key_of(&current)?
                    }
                }
            };
            self.note_key_change(o, &worked_with, &states)?;
            if !jobs.is_empty() {
                // The connection kept so far may still use an earlier key.
                self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            }
            self.key_change_status_in(o, &states)
        })
    }
}
