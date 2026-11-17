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
use crate::net::sftp::{generate_key, public_key_of, SftpFs, SshAuth};
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

/// Logs in to `s` with exactly `key` (no other one), in the login folder.
fn login(o: &mut Open, s: &StoredServer, key: &Zeroizing<String>) -> std::result::Result<SftpFs, NetError> {
    let mut cfg = sftp_config(o, s).map_err(|_| NetError::AuthFailed)?;
    cfg.auth = SshAuth::Key(key.clone());
    cfg.base = authorized_keys_place().0;
    SftpFs::connect(&cfg)
}

impl GhulSuite {
    /// Starts a key change (called with every change of the protection): a new SSH key for the
    /// suite and a new key for GhulDrive, the earlier ones kept until the change is through.
    pub(super) fn begin_key_change(&self, o: &mut Open) -> Result<()> {
        let current: String = setting(&o.suite, SSH_KEY);
        if !current.is_empty() && !all_servers(o).is_empty() {
            let mut c: KeyChange = setting(&o.suite, CHANGE_KEY);
            c.ssh_old.insert(0, current);
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
            all_servers(o)
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
        Ok(KeyChangeStatus { running: ssh_running || !drive_waiting.is_empty(), drive_waiting, servers, public_key })
    }

    /// Moves `s` to the new key as far as it goes (see the module).
    fn switch_server(&self, o: &mut Open, s: &StoredServer) -> KeyChangeState {
        let Ok(new) = ssh_private_key(o) else { return KeyChangeState::Waiting };
        let Ok(new_public) = public_key_of(&new) else { return KeyChangeState::Waiting };
        let earlier = earlier_ssh_keys(o);
        let earlier_bodies: Vec<String> =
            earlier.iter().filter_map(|k| public_key_of(k).ok()).map(|p| body(&p)).collect();
        let path = authorized_keys_place().1;
        let fs = match login(o, s, &new) {
            Ok(fs) => fs,
            Err(NetError::AuthFailed) => {
                // In with an earlier key, add the new one, check it.
                let Some(old_fs) = earlier.iter().find_map(|k| login(o, s, k).ok()) else {
                    return KeyChangeState::Waiting;
                };
                let had = old_fs.read(&path).ok().flatten().unwrap_or_default();
                if !String::from_utf8_lossy(&had).contains(&body(&new_public))
                    && old_fs.append_line(&path, &new_public).is_err()
                {
                    return KeyChangeState::AddByHand;
                }
                match login(o, s, &new) {
                    Ok(fs) => fs,
                    // The hoster does not use that file: by hand in its panel.
                    Err(NetError::AuthFailed) => return KeyChangeState::AddByHand,
                    Err(_) => return KeyChangeState::Waiting,
                }
            }
            Err(_) => return KeyChangeState::Waiting,
        };
        // The earlier keys out of the file (every other line stays).
        if let Ok(Some(had)) = fs.read(&path) {
            let text = String::from_utf8_lossy(&had);
            let kept: Vec<&str> = text
                .lines()
                .filter(|l| !earlier_bodies.iter().any(|b| !b.is_empty() && l.contains(b.as_str())))
                .collect();
            if kept.len() != text.lines().count() {
                let mut out = kept.join("\n");
                out.push('\n');
                if fs.write(&path, out.as_bytes()).is_err() {
                    return KeyChangeState::RemoveByHand;
                }
            }
        }
        // None of them may log in any more.
        let still = earlier.iter().any(|k| login(o, s, k).is_ok());
        if still {
            KeyChangeState::RemoveByHand
        } else {
            KeyChangeState::Done
        }
    }

    /// Saves where the SSH part stands; once every server is through, the earlier keys go.
    fn finish_key_change_if_done(&self, o: &mut Open) -> Result<()> {
        let c: KeyChange = setting(&o.suite, CHANGE_KEY);
        if c.ssh_old.is_empty() {
            return Ok(());
        }
        if all_servers(o).iter().all(|s| c.done.contains(&s.id)) {
            put_setting(&mut o.suite, CHANGE_KEY, &KeyChange::default())?;
        }
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
    /// every server to the new SSH key. Returns where it stands.
    pub fn key_change_continue(&self) -> Result<KeyChangeStatus> {
        self.with_open(|o| {
            super::single::refuse_single(o)?;
            let _ = self.drive_rekey_all(o);
            let mut states: Vec<(String, KeyChangeState)> = Vec::new();
            let mut c: KeyChange = setting(&o.suite, CHANGE_KEY);
            let ssh = !c.ssh_old.is_empty();
            if ssh {
                for s in all_servers(o) {
                    if c.done.contains(&s.id) {
                        continue;
                    }
                    let st = self.switch_server(o, &s);
                    if st == KeyChangeState::Done {
                        c.done.push(s.id.clone());
                    }
                    states.push((s.id.clone(), st));
                }
                put_setting(&mut o.suite, CHANGE_KEY, &c)?;
            }
            self.finish_key_change_if_done(o)?;
            if ssh {
                // The connection kept so far may still use an earlier key.
                self.net.lock().unwrap_or_else(|p| p.into_inner()).conn = None;
            }
            self.key_change_status_in(o, &states)
        })
    }
}
