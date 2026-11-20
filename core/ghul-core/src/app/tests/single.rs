// SPDX-License-Identifier: AGPL-3.0-or-later
//! One vault opened on its own (the own suite untouched) and picked entries handed on as a
//! vault with a protection of its own.

use super::super::single::ShareVaultOptions;
use super::*;
use crate::vault::{LockedContainer, RecoveryCode};

const PW: &str = "pw-123456";
const NEW_PW: &str = "weiter-98765";

/// A suite with a GhulPass vault "Privat" (Bank, Kino, Forum), a GhulAuth and a calendar vault.
fn suite_with_entries(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, String) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    let res = s
        .create(
            options(
                PW,
                Some(kf.clone()),
                &[(VaultKind::Pass, "Privat"), (VaultKind::Totp, "Codes"), (VaultKind::Cal, "Kalender")],
            ),
            None,
            None,
            None,
        )
        .unwrap();
    let v = vault_id(&s, "Privat");
    let bank = s.save_entry(v.clone(), login("Bank", "u", "alt-1")).unwrap();
    // A second password: the first goes into the history.
    let mut changed = login("Bank", "u", "neu-2");
    changed.id = bank;
    s.save_entry(v.clone(), changed).unwrap();
    s.save_entry(v.clone(), login("Kino", "m", "3")).unwrap();
    s.save_entry(v, login("Forum", "f", "4")).unwrap();
    s.add_totp(vault_id(&s, "Codes"), totp_input()).unwrap();
    (d, s, kf, res.recovery_code.unwrap())
}

fn entry_id(s: &GhulSuite, title: &str) -> Vec<u8> {
    s.list_entries(VaultKind::Pass, None).unwrap().into_iter().find(|e| e.title == title).unwrap().id
}

fn share_opts(s: &GhulSuite, ids: Vec<Vec<u8>>, keyfile: Option<Vec<u8>>) -> ShareVaultOptions {
    ShareVaultOptions {
        vault_id: vault_id(s, "Privat"),
        ids,
        name: "Für Anna".into(),
        master_password: PW.into(),
        password: NEW_PW.into(),
        keyfile,
        key_label: "Key".into(),
        keyfile_label: "Datei".into(),
        recovery_label: "Blatt".into(),
    }
}

#[test]
fn a_vault_opens_on_its_own_and_is_written_back() {
    let (_d, s, kf, _) = suite_with_entries("single-open");
    let file = s.export_vault(vault_id(&s, "Privat")).unwrap();
    s.lock();
    s.finish_lock();

    // A second, temporary suite: only this vault, the own suite stays locked.
    let t_dirs = Dirs::new("single-temp");
    let t = t_dirs.suite();
    let wrong = t.open_single_vault(file.clone(), "falsch".into(), None, None, None, Some(kf.clone()));
    assert!(matches!(wrong, Err(GhulError::WrongPassword)), "{wrong:?}");
    assert!(!t.is_unlocked());
    assert_eq!(
        t.open_single_vault(file.clone(), PW.into(), None, None, None, Some(kf.clone())).unwrap(),
        VaultKind::Pass
    );
    assert!(t.is_single() && !s.is_unlocked());
    assert_eq!(t.list_vaults().unwrap().len(), 1);
    assert_eq!(titles(&t), vec!["Bank", "Forum", "Kino"]);
    assert!(t.recovery_confirmed().unwrap(), "no reminder about a code that is not ours");
    assert!(!t.archive_unsaved());

    // A change is noted; the file written back opens with the same protection as before.
    t.save_entry(vault_id(&t, "Privat"), login("Neu", "n", "5")).unwrap();
    assert!(t.archive_unsaved());
    let back = t.single_vault_file().unwrap();
    // A change while the file is written is not in it: it stays unsaved.
    t.save_entry(vault_id(&t, "Privat"), login("Dazwischen", "d", "6")).unwrap();
    t.single_saved(back.change);
    assert!(t.archive_unsaved(), "the change in between is not in the file");
    let back = t.single_vault_file().unwrap();
    t.single_saved(back.change);
    assert!(!t.archive_unsaved());
    let (l1, l2) = (LockedContainer::parse(&file).unwrap(), LockedContainer::parse(&back.data).unwrap());
    assert_eq!(l1.header().suite_id, l2.header().suite_id);
    assert_eq!(l1.header().kdf, l2.header().kdf);
    let again_dirs = Dirs::new("single-again");
    let again = again_dirs.suite();
    again.open_single_vault(back.data, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&again), vec!["Bank", "Dazwischen", "Forum", "Kino", "Neu"]);

    // The own suite did not change.
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(titles(&s), vec!["Bank", "Forum", "Kino"]);
    assert!(!s.is_single());
    assert!(matches!(s.single_vault_file(), Err(GhulError::InvalidInput { .. })));
}

#[test]
fn what_cannot_be_opened_on_its_own() {
    let (d, s, kf, _) = suite_with_entries("single-refuse");
    let pass = s.export_vault(vault_id(&s, "Privat")).unwrap();

    // Not into a folder that holds a suite.
    s.lock();
    s.finish_lock();
    let busy = d.suite();
    let r = busy.open_single_vault(pass.clone(), PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "{r:?}");

    // No suite file.
    let suite_file = d.storage().read(super::super::suite::SUITE_FILE.into()).unwrap().unwrap();
    let t_dirs = Dirs::new("single-refuse-t");
    let t = t_dirs.suite();
    let r = t.open_single_vault(suite_file, PW.into(), None, None, None, Some(kf.clone()));
    assert!(matches!(r, Err(GhulError::InvalidInput { .. })), "{r:?}");
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();

    // A file changed by someone without the keys is refused.
    let mut forged = pass.clone();
    let n = forged.len();
    forged[n - 40] ^= 1;
    assert!(t.open_single_vault(forged, PW.into(), None, None, None, Some(kf.clone())).is_err());
    assert!(!t.is_unlocked());

    // A calendar opens on its own.
    let cal = s.export_vault(vault_id(&s, "Kalender")).unwrap();
    assert_eq!(t.open_single_vault(cal, PW.into(), None, None, None, Some(kf)).unwrap(), VaultKind::Cal);
}

#[test]
fn picked_entries_go_into_a_vault_of_their_own() {
    let (_d, s, kf, code) = suite_with_entries("share-vault");
    let ids = vec![entry_id(&s, "Bank"), entry_id(&s, "Kino")];
    let new_kf = generate_keyfile();

    // The own master password is asked again; the new one must be long enough; one factor.
    let mut o = share_opts(&s, ids.clone(), Some(new_kf.clone()));
    o.master_password = "falsch".into();
    assert!(matches!(s.share_as_vault(o, None, None, None), Err(GhulError::WrongPassword)));
    let mut o = share_opts(&s, ids.clone(), Some(new_kf.clone()));
    o.password = "kurz".into();
    assert!(matches!(s.share_as_vault(o, None, None, None), Err(GhulError::InvalidInput { .. })));
    let o = share_opts(&s, ids.clone(), Some(new_kf.clone()));
    assert!(matches!(s.share_as_vault(o, Some(key()), None, Some("1234".into())), Err(GhulError::InvalidInput { .. })));
    let o = share_opts(&s, vec![vec![9; 16]], Some(new_kf.clone()));
    assert!(matches!(s.share_as_vault(o, None, None, None), Err(GhulError::NotFound)));

    let shared = s.share_as_vault(share_opts(&s, ids, Some(new_kf.clone())), None, None, None).unwrap();
    assert_eq!(shared.kind, VaultKind::Pass);

    // Nothing of the own protection: another suite, salt and recovery code.
    let mine = LockedContainer::parse(&s.export_vault(vault_id(&s, "Privat")).unwrap()).unwrap();
    let theirs = LockedContainer::parse(&shared.data).unwrap();
    assert_ne!(mine.header().suite_id, theirs.header().suite_id);
    assert_ne!(mine.header().kdf.salt, theirs.header().kdf.salt);
    assert_ne!(shared.recovery_code, code);
    assert!(LockedContainer::parse(&shared.data).unwrap().recover(&RecoveryCode::parse(&code).unwrap()).is_err());
    assert!(theirs.recover(&RecoveryCode::parse(&shared.recovery_code).unwrap()).is_ok());

    let t_dirs = Dirs::new("share-vault-t");
    let t = t_dirs.suite();
    let old = t.open_single_vault(shared.data.clone(), PW.into(), None, None, None, Some(kf));
    assert!(old.is_err(), "the own password and key file do not open it");
    t.open_single_vault(shared.data.clone(), NEW_PW.into(), None, None, None, Some(new_kf)).unwrap();
    assert_eq!(titles(&t), vec!["Bank", "Kino"]);
    assert_eq!(t.list_vaults().unwrap()[0].name, "Für Anna");
    // New ids: nothing points back to the own vault.
    let theirs: Vec<Vec<u8>> = t.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.id).collect();
    let mine: Vec<Vec<u8>> = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().map(|e| e.id).collect();
    assert!(theirs.iter().all(|i| !mine.contains(i)));
    // Old passwords stay behind.
    let bank = t.list_entries(VaultKind::Pass, None).unwrap().into_iter().find(|e| e.title == "Bank").unwrap();
    assert!(t.entry_extras(bank.vault_id, bank.id).unwrap().history.is_empty());

    // The own suite still has everything, history included.
    assert_eq!(titles(&s), vec!["Bank", "Forum", "Kino"]);
    let bank = s.list_entries(VaultKind::Pass, None).unwrap().into_iter().find(|e| e.title == "Bank").unwrap();
    assert_eq!(s.entry_extras(bank.vault_id, bank.id).unwrap().history.len(), 1);
}

#[test]
fn handed_on_with_a_security_key() {
    let (_d, s, _, _) = suite_with_entries("share-key");
    let k = key();
    let codes = vault_id(&s, "Codes");
    let ids: Vec<Vec<u8>> = s.list_entries(VaultKind::Totp, None).unwrap().into_iter().map(|e| e.id).collect();
    let mut o = share_opts(&s, ids, None);
    o.vault_id = codes;
    let shared = s.share_as_vault(o, Some(k.clone()), Some(listener()), Some("1234".into())).unwrap();
    assert_eq!(shared.kind, VaultKind::Totp);
    let t_dirs = Dirs::new("share-key-t");
    let t = t_dirs.suite();
    let without = t.open_single_vault(shared.data.clone(), NEW_PW.into(), None, None, None, Some(generate_keyfile()));
    assert!(without.is_err());
    t.open_single_vault(shared.data, NEW_PW.into(), Some(k), Some(listener()), Some("1234".into()), None).unwrap();
    assert_eq!(t.list_entries(VaultKind::Totp, None).unwrap().len(), 1);

    // Calendar entries are not handed on this way.
    let mut o = share_opts(&s, vec![vec![1; 16]], Some(generate_keyfile()));
    o.vault_id = vault_id(&s, "Kalender");
    assert!(matches!(s.share_as_vault(o, None, None, None), Err(GhulError::InvalidInput { .. })));
}

/// "Hand on" from a vault's menu: no ids picked, the whole vault; a calendar too, with its
/// events still in their calendars.
#[test]
fn a_whole_vault_is_handed_on() {
    let (_d, s, _, _) = suite_with_entries("share-whole");
    let new_kf = generate_keyfile();
    let shared = s.share_as_vault(share_opts(&s, Vec::new(), Some(new_kf.clone())), None, None, None).unwrap();
    assert_eq!(shared.kind, VaultKind::Pass);
    let t_dirs = Dirs::new("share-whole-t");
    let t = t_dirs.suite();
    t.open_single_vault(shared.data, NEW_PW.into(), None, None, None, Some(new_kf.clone())).unwrap();
    assert_eq!(titles(&t), vec!["Bank", "Forum", "Kino"]);

    let cal = s
        .cal_save_calendar(super::super::cal::CalCalendar {
            id: String::new(),
            vault_id: vault_id(&s, "Kalender"),
            name: "Familie".into(),
            color: 1,
            default_reminder: 0,
            events: 0,
        })
        .unwrap();
    s.cal_save_event(super::super::cal::CalEvent {
        calendar_id: cal.clone(),
        title: "Geburtstag".into(),
        start: "2026-12-01T10:00".into(),
        end: "2026-12-01T11:00".into(),
        ..Default::default()
    })
    .unwrap();
    let mut o = share_opts(&s, Vec::new(), Some(new_kf.clone()));
    o.vault_id = vault_id(&s, "Kalender");
    let shared = s.share_as_vault(o, None, None, None).unwrap();
    assert_eq!(shared.kind, VaultKind::Cal);
    let c_dirs = Dirs::new("share-whole-c");
    let c = c_dirs.suite();
    c.open_single_vault(shared.data.clone(), NEW_PW.into(), None, None, None, Some(new_kf.clone())).unwrap();
    let cals = c.cal_calendars().unwrap();
    let fam = cals.iter().find(|x| x.name == "Familie").expect("the calendar came along");
    assert_eq!(fam.events, 1, "the event is still in its calendar");
    assert_ne!(fam.id, cal, "a new id: nothing points back to the own vault");

    // Handed back into the same suite: two calendars "Familie", each with its own id; deleting
    // the one that came back leaves the own one and its event.
    s.adopt_vault(shared.data, "Zurück".into(), NEW_PW.into(), None, None, None, Some(new_kf)).unwrap();
    let both: Vec<_> = s.cal_calendars().unwrap().into_iter().filter(|x| x.name == "Familie").collect();
    assert_eq!(both.len(), 2);
    assert_ne!(both[0].id, both[1].id);
    let back = both.iter().find(|x| x.id != cal).unwrap();
    s.cal_delete_calendar(back.id.clone()).unwrap();
    let own = s.cal_calendars().unwrap().into_iter().find(|x| x.id == cal).expect("the own calendar is still there");
    assert_eq!(own.events, 1);

    // An empty vault is handed on as an empty vault.
    let empty = s.create_vault(VaultKind::Pass, "Leer".into()).unwrap();
    let mut o = share_opts(&s, Vec::new(), Some(generate_keyfile()));
    o.vault_id = empty;
    assert_eq!(s.share_as_vault(o, None, None, None).unwrap().kind, VaultKind::Pass);
}

#[test]
fn a_vault_on_its_own_stays_one_vault() {
    let (_d, s, kf, _) = suite_with_entries("single-one");
    let file = s.export_vault(vault_id(&s, "Privat")).unwrap();
    let other = s.export_vault(vault_id(&s, "Codes")).unwrap();
    let t_dirs = Dirs::new("single-one-t");
    let t = t_dirs.suite();
    t.open_single_vault(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    let id = vault_id(&t, "Privat");
    let refused = |r: Result<(), GhulError>| assert!(matches!(r, Err(GhulError::InvalidInput { .. })), "{r:?}");
    // No second vault (it could end up in the file), none removed, no other protection.
    refused(t.create_vault(VaultKind::Pass, "Neu".into()).map(|_| ()));
    refused(t.add_vault_file(other, "Codes".into()).map(|_| ()));
    refused(t.remove_vault(id.clone()));
    refused(t.add_keyfile(generate_keyfile(), "Zweite".into()));
    refused(t.change_password(PW.into(), "ganz-neu-123".into()));
    refused(t.new_recovery_code("Neu".into()).map(|_| ()));
    refused(t.archive_build_bytes("Test".into()).map(|_| ()));
    // The device's quick unlock belongs to the own suite.
    refused(t.quick_unlock_enable("pin".into(), Some("1234".into())).map(|_| ()));
    assert_eq!(t.list_vaults().unwrap().len(), 1);
    assert_eq!(t.single_vault_name().unwrap(), t.list_vaults().unwrap()[0].file);
    // Entries change as usual.
    t.save_entry(id, login("Neu", "n", "1")).unwrap();
    assert_eq!(titles(&t), vec!["Bank", "Forum", "Kino", "Neu"]);
}
