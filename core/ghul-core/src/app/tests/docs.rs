// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulDocs: documents in folders, trash, the limit, damaged and changed content files, a
//! change of the protection, the suite file and a GhulDocs vault on its own.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

use super::super::archive::in_memory::{source, BytesSink};
use super::super::archive::{ArchiveSink, ArchiveSource};
use super::super::docs::blob::{self, SECTION};
use super::super::docs::{
    archive_inspect, check_room, stored_limit, ArchiveContent, DocsItem, ShareDocsOptions, Usage, DOCS_LIMIT,
    DOCS_MAX_FILES,
};
use super::super::storage::StorageError;
use super::super::zipstream::ZipWriter;
use super::*;
use crate::vault::{Keyfile, LockedContainer, PasswordKey, SecondFactor};

const PW: &str = "pw-123456";

/// Deterministic content: byte `i` is `(i * 7 + seed) % 251`.
fn content(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| ((i * 7 + seed as usize) % 251) as u8).collect()
}

/// Content made while it is read (nothing of it is held), its size known or not.
struct Generated {
    len: u64,
    known: bool,
    pos: Mutex<u64>,
}

fn generated(len: u64, known: bool) -> Arc<dyn ArchiveSource> {
    Arc::new(Generated { len, known, pos: Mutex::new(0) })
}

impl ArchiveSource for Generated {
    fn size(&self) -> u64 {
        if self.known {
            self.len
        } else {
            0
        }
    }
    fn read(&self, max: u32) -> Result<Vec<u8>, StorageError> {
        let mut pos = self.pos.lock().unwrap();
        let n = (self.len - *pos).min(u64::from(max));
        let out = (*pos..*pos + n).map(|i| (i % 251) as u8).collect();
        *pos += n;
        Ok(out)
    }
}

/// Counts and hashes what is written, keeps nothing.
#[derive(Default)]
struct Counting(Mutex<(u64, sha2::Sha256)>);

impl ArchiveSink for Counting {
    fn write(&self, data: Vec<u8>) -> Result<(), StorageError> {
        use sha2::Digest;
        let mut g = self.0.lock().unwrap();
        g.0 += data.len() as u64;
        g.1.update(&data);
        Ok(())
    }
}

/// Writes into a file.
struct FileSink(Mutex<std::fs::File>);

impl ArchiveSink for FileSink {
    fn write(&self, data: Vec<u8>) -> Result<(), StorageError> {
        std::io::Write::write_all(&mut *self.0.lock().unwrap(), &data)
            .map_err(|e| StorageError::Io { msg: e.to_string() })
    }
}

/// Reads from a file.
struct FileSource(Mutex<std::fs::File>, u64);

impl ArchiveSource for FileSource {
    fn size(&self) -> u64 {
        self.1
    }
    fn read(&self, max: u32) -> Result<Vec<u8>, StorageError> {
        let mut buf = vec![0u8; max as usize];
        let n = self.0.lock().unwrap().read(&mut buf).map_err(|e| StorageError::Io { msg: e.to_string() })?;
        buf.truncate(n);
        Ok(buf)
    }
}

/// A suite with a GhulDocs vault "Dokumente" (and a GhulPass vault), its key file and code.
fn docs_suite(name: &str) -> (Dirs, Arc<GhulSuite>, Vec<u8>, String, String) {
    let d = Dirs::new(name);
    let s = d.suite();
    let kf = generate_keyfile();
    let res = s
        .create(
            options(PW, Some(kf.clone()), &[(VaultKind::Pass, "Privat"), (VaultKind::Docs, "Dokumente")]),
            None,
            None,
            None,
        )
        .unwrap();
    let v = vault_id(&s, "Dokumente");
    (d, s, kf, res.recovery_code.unwrap(), v)
}

fn add(s: &GhulSuite, v: &str, folder: Option<&DocsItem>, name: &str, data: &[u8]) -> DocsItem {
    s.docs_add_file(
        v.into(),
        folder.map(|f| f.id.clone()),
        name.into(),
        "application/pdf".into(),
        source(data.to_vec()),
        None,
    )
    .unwrap()
}

fn read(s: &GhulSuite, v: &str, id: &str) -> Result<Vec<u8>, GhulError> {
    let sink = Arc::new(BytesSink::default());
    s.docs_read_file(v.into(), id.into(), sink.clone(), None)?;
    let out = std::mem::take(&mut *sink.0.lock().unwrap());
    Ok(out)
}

fn names(list: &[DocsItem]) -> Vec<String> {
    list.iter().map(|i| i.name.clone()).collect()
}

fn docs_dir(d: &Dirs, v: &str) -> PathBuf {
    d.root.join("vault").join(format!("docs-{v}"))
}

fn content_files(d: &Dirs, v: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(docs_dir(d, v))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn save(s: &GhulSuite) -> Vec<u8> {
    let built = s.archive_build_bytes("Test".into()).unwrap();
    s.archive_saved(built.state, built.change).unwrap();
    built.data
}

fn zip_entries(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data.to_vec())).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().to_string(), b)
        })
        .collect()
}

fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut z = ZipWriter::new(Vec::new(), |_| {});
    for (n, d) in files {
        z.add_bytes(n, d).unwrap();
    }
    z.finish().unwrap()
}

fn staging_left(d: &Dirs) -> usize {
    std::fs::read_dir(d.root.join("vault").join(".staging")).into_iter().flatten().count()
}

#[test]
fn documents_of_every_size_come_back_exactly() {
    let (d, s, kf, _, v) = docs_suite("docs-sizes");
    for (i, size) in [0, 1, 1000, SECTION - 1, SECTION, SECTION + 1, 3 * SECTION + 7].into_iter().enumerate() {
        let data = content(size, i as u8);
        let it = add(&s, &v, None, &format!("f{i}.bin"), &data);
        assert_eq!((it.size, it.is_folder, it.mime.as_str()), (size as u64, false, "application/pdf"));
        assert_eq!(read(&s, &v, &it.id).unwrap(), data, "{size}");
    }
    let mut all = Vec::new();
    let usage = s.docs_usage().unwrap();
    assert_eq!(usage.files, 7);
    assert_eq!(usage.used, (1 + 1000 + SECTION - 1 + SECTION + SECTION + 1 + 3 * SECTION + 7) as u64);
    assert_eq!(usage.limit, DOCS_LIMIT);
    let files = content_files(&d, &v);
    assert_eq!(files.len(), 7);
    assert!(files.iter().all(|f| f.len() == 32 + ".ghlblob".len() && f.ends_with(".ghlblob")), "{files:?}");
    for f in &files {
        let len = std::fs::metadata(docs_dir(&d, &v).join(f)).unwrap().len();
        assert_eq!(len % (64 * 1024), 0, "rounded up to 64 KiB");
    }
    // After locking, on a new session, everything is still there.
    for it in s.docs_list(v.clone(), None).unwrap() {
        all.push((it.id.clone(), read(&s, &v, &it.id).unwrap()));
    }
    s.lock();
    assert!(matches!(s.docs_list(v.clone(), None), Err(GhulError::Locked)));
    let s = d.suite();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(content_files(&d, &v).len(), 7);
    for (id, data) in all {
        assert_eq!(read(&s, &v, &id).unwrap(), data);
    }
}

#[test]
fn folders_names_search_and_favorites() {
    let (_d, s, kf, _, v) = docs_suite("docs-folders");
    let steuer = s.docs_create_folder(v.clone(), None, " Steuer ".into()).unwrap();
    assert_eq!((steuer.name.as_str(), steuer.is_folder, steuer.items), ("Steuer", true, 0));
    let y2025 = s.docs_create_folder(v.clone(), Some(steuer.id.clone()), "2025".into()).unwrap();
    assert_eq!(y2025.path, "Steuer");
    assert!(matches!(s.docs_create_folder(v.clone(), None, "steuer".into()), Err(GhulError::AlreadyExists)));
    for bad in ["", "  ", "a/b", "a\\b", "..", "x\u{0}y", "x\u{202e}y"] {
        assert!(
            matches!(s.docs_create_folder(v.clone(), None, bad.into()), Err(GhulError::InvalidInput { .. })),
            "{bad:?}"
        );
    }
    let a = add(&s, &v, Some(&y2025), "Bescheid.pdf", b"one");
    let b = add(&s, &v, Some(&y2025), "bescheid.pdf", b"two");
    assert_eq!(b.name, "bescheid (2).pdf", "a taken name gets a number");
    assert_eq!(a.path, "Steuer/2025");
    add(&s, &v, None, "Zeugnis.pdf", b"z");
    add(&s, &v, None, "alpha.txt", b"a");
    s.docs_create_folder(v.clone(), None, "Arzt".into()).unwrap();
    // Folders first, then by name.
    assert_eq!(names(&s.docs_list(v.clone(), None).unwrap()), vec!["Arzt", "Steuer", "alpha.txt", "Zeugnis.pdf"]);
    assert_eq!(s.docs_list(v.clone(), None).unwrap()[1].items, 1);
    assert_eq!(
        names(&s.docs_list(v.clone(), Some(y2025.id.clone())).unwrap()),
        vec!["bescheid (2).pdf", "Bescheid.pdf"]
    );

    // Rename and move.
    assert!(matches!(s.docs_rename(v.clone(), b.id.clone(), "BESCHEID.pdf".into()), Err(GhulError::AlreadyExists)));
    let b = s.docs_rename(v.clone(), b.id.clone(), "Vorauszahlung.pdf".into()).unwrap();
    assert_eq!(b.name, "Vorauszahlung.pdf");
    let moved = s.docs_move(v.clone(), b.id.clone(), None).unwrap();
    assert_eq!((moved.parent, moved.path.as_str()), (None, ""));
    assert!(matches!(
        s.docs_move(v.clone(), steuer.id.clone(), Some(y2025.id.clone())),
        Err(GhulError::InvalidInput { .. })
    ));
    assert!(matches!(
        s.docs_move(v.clone(), steuer.id.clone(), Some(steuer.id.clone())),
        Err(GhulError::InvalidInput { .. })
    ));
    let arzt = s.docs_list(v.clone(), None).unwrap().into_iter().find(|i| i.name == "Arzt").unwrap();
    let st = s.docs_move(v.clone(), steuer.id.clone(), Some(arzt.id.clone())).unwrap();
    assert_eq!(st.path, "Arzt");
    assert_eq!(s.docs_item(v.clone(), a.id.clone()).unwrap().path, "Arzt/Steuer/2025");
    assert!(matches!(s.docs_move(v.clone(), a.id.clone(), Some("00".repeat(16))), Err(GhulError::NotFound)));
    assert!(matches!(s.docs_item(v.clone(), "zz".into()), Err(GhulError::InvalidInput { .. })));

    // Search and favorites.
    assert_eq!(names(&s.docs_search(None, "BESCHEID".into()).unwrap()), vec!["Bescheid.pdf"]);
    assert_eq!(
        names(&s.docs_search(Some(v.clone()), "U".into()).unwrap()),
        vec!["Steuer", "Vorauszahlung.pdf", "Zeugnis.pdf"]
    );
    assert!(s.docs_search(None, "  ".into()).unwrap().is_empty());
    s.docs_set_favorite(v.clone(), a.id.clone(), true).unwrap();
    s.docs_set_favorite(v.clone(), arzt.id.clone(), true).unwrap();
    assert_eq!(names(&s.docs_favorites(None).unwrap()), vec!["Arzt", "Bescheid.pdf"]);
    s.docs_set_favorite(v.clone(), arzt.id.clone(), false).unwrap();
    assert_eq!(names(&s.docs_favorites(Some(v.clone())).unwrap()), vec!["Bescheid.pdf"]);
    assert!(s.docs_item(v.clone(), a.id.clone()).unwrap().favorite);

    // A GhulPass vault is no GhulDocs vault, and the other way round.
    let pass = vault_id(&s, "Privat");
    assert!(matches!(s.docs_list(pass, None), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(s.save_entry(v.clone(), login("x", "y", "z")), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(s.export_vault(v.clone()), Err(GhulError::InvalidInput { .. })));
    assert!(matches!(s.export_plain(v.clone(), PW.into()), Err(GhulError::InvalidInput { .. })));
    let info = s.list_vaults().unwrap().into_iter().find(|i| i.id == v).unwrap();
    assert_eq!((info.kind, info.entries, info.file.ends_with(".ghldoc")), (VaultKind::Docs, 4, true));

    // All of it is still there after locking.
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(s.docs_item(v.clone(), a.id.clone()).unwrap().path, "Arzt/Steuer/2025");
    assert_eq!(read(&s, &v, &a.id).unwrap(), b"one");
}

#[test]
fn trash_restore_and_delete_for_good() {
    let (d, s, _, _, v) = docs_suite("docs-trash");
    let f = s.docs_create_folder(v.clone(), None, "Alt".into()).unwrap();
    let sub = s.docs_create_folder(v.clone(), Some(f.id.clone()), "Innen".into()).unwrap();
    let a = add(&s, &v, Some(&f), "a.pdf", &content(1000, 1));
    let b = add(&s, &v, Some(&sub), "b.pdf", &content(2000, 2));
    let c = add(&s, &v, None, "c.pdf", &content(3000, 3));
    assert_eq!(s.docs_usage().unwrap().used, 6000);

    // The folder goes to the trash with everything in it; it still counts.
    s.docs_to_trash(v.clone(), f.id.clone()).unwrap();
    assert_eq!(names(&s.docs_list(v.clone(), None).unwrap()), vec!["c.pdf"]);
    assert_eq!(names(&s.docs_trash(None).unwrap()), vec!["Alt"]);
    assert!(s.docs_search(None, "b.pdf".into()).unwrap().is_empty());
    assert_eq!(s.docs_usage().unwrap().used, 6000, "the trash counts");
    assert_eq!(read(&s, &v, &b.id).unwrap(), content(2000, 2), "readable in the trash");
    assert!(matches!(s.docs_rename(v.clone(), a.id.clone(), "x".into()), Err(GhulError::NotFound)));
    assert!(matches!(add_into_trashed(&s, &v, &sub), Err(GhulError::NotFound)));

    // Restored with everything; a name taken meanwhile gets a number.
    s.docs_create_folder(v.clone(), None, "Alt".into()).unwrap();
    let back = s.docs_restore(v.clone(), f.id.clone()).unwrap();
    assert_eq!(back.name, "Alt (2)");
    assert_eq!(s.docs_item(v.clone(), b.id.clone()).unwrap().path, "Alt (2)/Innen");
    assert!(s.docs_trash(None).unwrap().is_empty());

    // A file out of a folder in the trash goes to the top.
    s.docs_to_trash(v.clone(), b.id.clone()).unwrap();
    s.docs_to_trash(v.clone(), sub.id.clone()).unwrap();
    // Inside a folder in the trash it goes with the folder.
    assert_eq!(names(&s.docs_trash(Some(v.clone())).unwrap()), vec!["Innen"]);
    let b2 = s.docs_restore(v.clone(), b.id.clone()).unwrap();
    assert_eq!((b2.parent, b2.path.as_str()), (None, ""));

    // Emptying deletes the content files.
    s.docs_to_trash(v.clone(), c.id.clone()).unwrap();
    assert_eq!(content_files(&d, &v).len(), 3);
    s.docs_empty_trash(None).unwrap();
    assert!(s.docs_trash(None).unwrap().is_empty());
    assert_eq!(content_files(&d, &v).len(), 2);
    assert_eq!(s.docs_usage().unwrap().used, 3000);
    assert!(matches!(s.docs_item(v.clone(), c.id.clone()), Err(GhulError::NotFound)));
    // For good, without the trash: a folder with everything in it.
    s.docs_delete_forever(v.clone(), f.id.clone()).unwrap();
    assert_eq!(content_files(&d, &v).len(), 1);
    assert!(matches!(s.docs_item(v.clone(), a.id.clone()), Err(GhulError::NotFound)));
    assert!(matches!(s.docs_delete_forever(v.clone(), a.id.clone()), Err(GhulError::NotFound)));
    assert_eq!(s.docs_usage().unwrap(), super::super::docs::DocsUsage { used: 2000, limit: DOCS_LIMIT, files: 1 });
}

fn add_into_trashed(s: &GhulSuite, v: &str, folder: &DocsItem) -> Result<DocsItem, GhulError> {
    s.docs_add_file(v.into(), Some(folder.id.clone()), "x".into(), "".into(), source(b"x".to_vec()), None)
}

#[test]
fn the_limit_holds_for_all_vaults_and_the_trash() {
    // The real numbers: 1 GB of content in all, also as one document, and it fits into a suite file.
    let none = Usage::default();
    assert!(check_room(none, Usage::file(DOCS_LIMIT), DOCS_LIMIT).is_ok());
    assert!(matches!(check_room(none, Usage::file(DOCS_LIMIT + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { .. })));
    let half = Usage::file(DOCS_LIMIT / 2);
    assert!(check_room(half, half, DOCS_LIMIT).is_ok());
    assert!(
        matches!(check_room(half, Usage::file(DOCS_LIMIT / 2 + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { used, limit }) if used == DOCS_LIMIT / 2 && limit == DOCS_LIMIT)
    );
    assert!(blob::file_len(DOCS_LIMIT) <= stored_limit(DOCS_LIMIT));
    assert!(stored_limit(DOCS_LIMIT) + 64 * 1024 * 1024 < super::super::archive::MAX_ARCHIVE);
    // Many small documents: each takes at least 64 KiB in the suite file, so their rounding
    // counts as well (about 16 700 of the smallest fit), and their number.
    let small = Usage::file(10);
    let fit = stored_limit(DOCS_LIMIT) / blob::file_len(10);
    let many = Usage { used: 10 * (fit - 1), stored: blob::file_len(10) * (fit - 1), files: fit - 1 };
    assert!(check_room(many, small, DOCS_LIMIT).is_ok());
    assert!(matches!(check_room(many.plus(small), small, DOCS_LIMIT), Err(GhulError::DocsLimit { .. })));
    let counted = Usage { used: 0, stored: 0, files: DOCS_MAX_FILES - 1 };
    assert!(check_room(counted, Usage { files: 1, ..Usage::default() }, DOCS_LIMIT).is_ok());
    assert!(matches!(
        check_room(counted, Usage { files: 2, ..Usage::default() }, DOCS_LIMIT),
        Err(GhulError::DocsLimit { .. })
    ));

    // A small limit, so it is reached quickly: two vaults share it, the trash counts.
    let (d, s, _, _, v) = docs_suite("docs-limit");
    let w = s.create_vault(VaultKind::Docs, "Zweiter".into()).unwrap();
    s.set_docs_limit(10_000_000);
    let a = add(&s, &v, None, "a", &content(6_000_000, 1));
    let before = (content_files(&d, &v), content_files(&d, &w));
    let r = s.docs_add_file(w.clone(), None, "b".into(), "".into(), generated(5_000_000, true), None);
    assert!(matches!(r, Err(GhulError::DocsLimit { used: 6_000_000, limit: 10_000_000 })), "{r:?}");
    // Size not known before: stopped as soon as it is too much, nothing stays.
    let r = s.docs_add_file(w.clone(), None, "b".into(), "".into(), generated(5_000_000, false), None);
    assert!(matches!(r, Err(GhulError::DocsLimit { .. })), "{r:?}");
    assert_eq!((content_files(&d, &v), content_files(&d, &w)), before, "nothing written, no temporary file left");
    s.docs_add_file(w.clone(), None, "b".into(), "".into(), generated(3_900_000, false), None).unwrap();
    // In the trash it still counts; emptied, there is room again.
    s.docs_to_trash(v.clone(), a.id.clone()).unwrap();
    assert!(matches!(
        s.docs_add_file(w.clone(), None, "c".into(), "".into(), generated(1_000_000, true), None),
        Err(GhulError::DocsLimit { .. })
    ));
    s.docs_empty_trash(Some(v.clone())).unwrap();
    s.docs_add_file(w.clone(), None, "c".into(), "".into(), generated(6_000_000, true), None).unwrap();
    assert_eq!(s.docs_usage().unwrap().used, 9_900_000);
}

#[test]
fn damaged_missing_and_swapped_content_files_are_noticed() {
    let (d, s, _, _, v) = docs_suite("docs-damage");
    let a = add(&s, &v, None, "a.pdf", &content(SECTION + 100, 1));
    let b = add(&s, &v, None, "b.pdf", &content(500, 2));
    let c = add(&s, &v, None, "c.pdf", &content(500, 3));
    let dir = docs_dir(&d, &v);
    assert_eq!(content_files(&d, &v).len(), 3);
    assert!(s.docs_verify(v.clone()).unwrap().is_empty());
    // Which file is whose: the one that breaks reading `a` when changed.
    let file_of = |id: &str| -> PathBuf {
        for f in content_files(&d, &v) {
            let p = dir.join(&f);
            let keep = std::fs::read(&p).unwrap();
            std::fs::write(&p, b"x").unwrap();
            let broken = read(&s, &v, id).is_err();
            std::fs::write(&p, &keep).unwrap();
            if broken {
                return p;
            }
        }
        panic!("no content file for {id}");
    };
    let (pa, pb, pc) = (file_of(&a.id), file_of(&b.id), file_of(&c.id));
    // A flipped byte in each section.
    for at in [30, 26 + 40 + SECTION + 30] {
        let keep = std::fs::read(&pa).unwrap();
        let mut bad = keep.clone();
        bad[at] ^= 1;
        std::fs::write(&pa, &bad).unwrap();
        assert!(matches!(read(&s, &v, &a.id), Err(GhulError::Corrupted { .. })), "{at}");
        assert_eq!(names(&s.docs_verify(v.clone()).unwrap()), vec!["a.pdf"]);
        std::fs::write(&pa, &keep).unwrap();
    }
    // Another document's content file in its place (same size): its id does not match.
    let keep_b = std::fs::read(&pb).unwrap();
    std::fs::copy(&pc, &pb).unwrap();
    assert!(matches!(read(&s, &v, &b.id), Err(GhulError::Corrupted { .. })));
    std::fs::write(&pb, &keep_b).unwrap();
    // Missing.
    std::fs::remove_file(&pc).unwrap();
    assert!(matches!(read(&s, &v, &c.id), Err(GhulError::Corrupted { .. })));
    assert_eq!(names(&s.docs_verify(v.clone()).unwrap()), vec!["c.pdf"]);
    // A suite with a missing document is not written into a suite file (it would be gone).
    assert!(s.archive_build_bytes("x".into()).is_err());
    // Deleting it for good is still possible; then saving works again.
    s.docs_delete_forever(v.clone(), c.id.clone()).unwrap();
    assert!(s.archive_build_bytes("x".into()).is_ok());
    assert_eq!(read(&s, &v, &a.id).unwrap(), content(SECTION + 100, 1));
}

/// What an index key (container key) of a GhulDocs vault opened with: password and key file.
fn index_key(d: &Dirs, v: &str, password: &str, kf: &[u8]) -> [u8; 32] {
    let data = std::fs::read(d.root.join("vault").join(format!("{}.ghldoc", &v[..16]))).unwrap();
    let l = LockedContainer::parse(&data).unwrap();
    let s = l.factor_secret(&SecondFactor::Keyfile(&Keyfile::parse(kf).unwrap())).unwrap();
    let pk = PasswordKey::derive(password, &l.header().kdf).unwrap();
    *l.unlock(&pk, &s).unwrap().vmk()
}

#[test]
fn a_change_of_the_protection_keeps_every_document_and_renews_the_index_key() {
    let (d, s, kf, _, v) = docs_suite("docs-protect");
    let a = add(&s, &v, None, "a.pdf", &content(SECTION + 1, 1));
    let b = add(&s, &v, None, "b.pdf", &content(10, 2));
    let files_before: Vec<(String, Vec<u8>)> = content_files(&d, &v)
        .into_iter()
        .map(|f| (f.clone(), std::fs::read(docs_dir(&d, &v).join(f)).unwrap()))
        .collect();
    let old_key = index_key(&d, &v, PW, &kf);
    s.change_password(PW.into(), "neu-654321".into()).unwrap();
    let code = s.new_recovery_code("Blatt".into()).unwrap();
    assert_eq!(read(&s, &v, &a.id).unwrap(), content(SECTION + 1, 1));
    assert_eq!(read(&s, &v, &b.id).unwrap(), content(10, 2));
    // The content files are untouched (their keys are in the index), the index key is new.
    let files_after: Vec<(String, Vec<u8>)> = content_files(&d, &v)
        .into_iter()
        .map(|f| (f.clone(), std::fs::read(docs_dir(&d, &v).join(f)).unwrap()))
        .collect();
    assert_eq!(files_before, files_after);
    let data = std::fs::read(d.root.join("vault").join(format!("{}.ghldoc", &v[..16]))).unwrap();
    assert!(
        LockedContainer::parse(&data).unwrap().unlock_with_vmk(&old_key).is_err(),
        "an old copy's key opens nothing new"
    );
    assert_ne!(index_key(&d, &v, "neu-654321", &kf), old_key);
    // Also after unlocking anew, and with the new recovery code.
    s.lock();
    s.unlock_with_recovery_code(code, "dritt-13579".into()).unwrap();
    assert_eq!(read(&s, &v, &a.id).unwrap(), content(SECTION + 1, 1));
}

#[test]
fn documents_travel_in_the_suite_file() {
    let (_da, a, kf, code, v) = docs_suite("docs-arch-a");
    let f = a.docs_create_folder(v.clone(), None, "Ordner".into()).unwrap();
    let x = add(&a, &v, Some(&f), "x.pdf", &content(SECTION + 5, 1));
    let y = add(&a, &v, None, "y.pdf", &content(0, 2));
    let z = add(&a, &v, None, "z.pdf", &content(70_000, 3));
    a.docs_to_trash(v.clone(), z.id.clone()).unwrap();
    let file = save(&a);
    let list = zip_entries(&file);
    let names_in: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names_in[0], "export.ghlexp");
    let docs_entries: Vec<&str> = names_in.iter().copied().filter(|n| n.starts_with("docs-")).collect();
    assert_eq!(docs_entries.len(), 3, "the trash goes along: {names_in:?}");
    assert!(docs_entries.iter().all(|n| n.starts_with(&format!("docs-{v}/"))));
    let first_doc = names_in.iter().position(|n| n.starts_with("docs-")).unwrap();
    assert!(names_in[..first_doc].iter().any(|n| n.ends_with(".ghldoc")), "the vault comes before its documents");

    // Another device: everything comes along.
    let db = Dirs::new("docs-arch-b");
    let b = db.suite();
    let check = b.archive_open_bytes(file.clone(), PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(check.vaults, 2);
    assert_eq!(staging_left(&db), 1, "checked documents wait on the disk");
    b.archive_accept().unwrap();
    assert_eq!(staging_left(&db), 0);
    assert_eq!(read(&b, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    assert_eq!(read(&b, &v, &y.id).unwrap(), Vec::<u8>::new());
    assert_eq!(names(&b.docs_trash(None).unwrap()), vec!["z.pdf"]);
    assert_eq!(b.docs_usage().unwrap(), a.docs_usage().unwrap());
    assert_eq!(content_files(&db, &v).len(), 3);

    // With the recovery code as well.
    let dc = Dirs::new("docs-arch-c");
    let c = dc.suite();
    c.archive_open_with_code_bytes(file.clone(), code, "neu-24680".into()).unwrap();
    c.archive_accept().unwrap();
    assert_eq!(read(&c, &v, &z.id).unwrap(), content(70_000, 3));

    // A device with documents of its own: the file replaces them.
    let (dd, d2, _, _, own) = docs_suite("docs-arch-d");
    add(&d2, &own, None, "own.pdf", b"mine");
    d2.archive_open_bytes(file.clone(), PW.into(), None, None, None, Some(kf.clone())).unwrap();
    d2.archive_accept().unwrap();
    assert!(!docs_dir(&dd, &own).exists(), "the old documents are gone");
    assert_eq!(read(&d2, &v, &x.id).unwrap(), content(SECTION + 5, 1));
}

#[test]
fn suite_files_with_missing_extra_or_changed_documents_are_refused() {
    let (_da, a, kf, _, v) = docs_suite("docs-bad-a");
    let x = add(&a, &v, None, "x.pdf", &content(SECTION + 5, 1));
    add(&a, &v, None, "y.pdf", &content(100, 2));
    let file = save(&a);
    let list = zip_entries(&file);
    let docs_at: Vec<usize> = (0..list.len()).filter(|&i| list[i].0.starts_with("docs-")).collect();
    let vault_at = list.iter().position(|(n, _)| n.ends_with(".ghldoc")).unwrap();

    let (db, b, _, _, own) = docs_suite("docs-bad-b");
    let mine = add(&b, &own, None, "mine.pdf", b"meins");
    let refused = |data: Vec<u8>, what: &str| {
        let r = b.archive_open_bytes(data, PW.into(), None, None, None, Some(kf.clone()));
        assert!(
            matches!(r, Err(GhulError::Corrupted { .. }) | Err(GhulError::InvalidInput { .. })),
            "{what}: {:?}",
            r.err()
        );
        assert_eq!(staging_left(&db), 0, "{what}: nothing checked stays");
    };
    // Missing.
    let mut l = list.clone();
    l.remove(docs_at[1]);
    refused(zip_of(&l), "missing");
    // An extra one (unknown id).
    let mut l = list.clone();
    l.push((format!("docs-{v}/{}.ghlblob", "ab".repeat(16)), l[docs_at[0]].1.clone()));
    refused(zip_of(&l), "extra");
    // Twice.
    let mut l = list.clone();
    l.push(l[docs_at[0]].clone());
    refused(zip_of(&l), "twice");
    // Changed (a flipped byte, still the right size).
    let mut l = list.clone();
    l[docs_at[0]].1[100] ^= 1;
    refused(zip_of(&l), "changed");
    // Swapped content: each document under the other's name.
    let mut l = list.clone();
    let (n0, n1) = (l[docs_at[0]].0.clone(), l[docs_at[1]].0.clone());
    l[docs_at[0]].0 = n1;
    l[docs_at[1]].0 = n0;
    refused(zip_of(&l), "swapped");
    // Before its vault.
    let mut l = list.clone();
    let doc = l.remove(docs_at[0]);
    l.insert(vault_at, doc);
    refused(zip_of(&l), "before its vault");
    // Of a vault that is not in the file, in a folder of its own, odd names.
    for name in [
        format!("docs-{}/{}.ghlblob", "cd".repeat(16), "ab".repeat(16)),
        format!("docs-{}/x/{}.ghlblob", v, "ab".repeat(16)),
        format!("docs-{}/{}.ghlblob", v.to_uppercase(), "ab".repeat(16)),
        format!("docs-{v}/"),
        format!("../docs-{v}/{}.ghlblob", "ab".repeat(16)),
    ] {
        let mut l = list.clone();
        l.push((name.clone(), vec![0u8; 65536]));
        refused(zip_of(&l), &name);
    }
    // Nothing of it touched the working copy.
    assert_eq!(read(&b, &own, &mine.id).unwrap(), b"meins");
    // The real file opens.
    b.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    b.archive_accept().unwrap();
    assert_eq!(read(&b, &v, &x.id).unwrap(), content(SECTION + 5, 1));
}

#[test]
fn a_docs_vault_on_its_own_export_open_write_back_adopt_and_share() {
    let (_da, a, kf, _, v) = docs_suite("docs-single-a");
    let f = a.docs_create_folder(v.clone(), None, "Ordner".into()).unwrap();
    let x = add(&a, &v, Some(&f), "x.pdf", &content(SECTION + 5, 1));
    let y = add(&a, &v, None, "y.pdf", &content(300, 2));
    let t = add(&a, &v, None, "trash.pdf", &content(30, 3));
    a.docs_to_trash(v.clone(), t.id.clone()).unwrap();
    let sink = Arc::new(BytesSink::default());
    let out = a.docs_export_vault(v.clone(), sink.clone(), None).unwrap();
    let exported = std::mem::take(&mut *sink.0.lock().unwrap());
    assert_eq!(out.size, exported.len() as u64);
    let names_in: Vec<String> = zip_entries(&exported).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names_in.len(), 4, "{names_in:?}");
    assert!(names_in[0].ends_with(".ghldoc"));
    let info = archive_inspect(source(exported.clone())).unwrap();
    assert_eq!((info.content, info.protection.keyfiles), (ArchiveContent::DocsVault, 1));
    assert_eq!(archive_inspect(source(save(&a))).unwrap().content, ArchiveContent::Suite);
    // The bare vault file is no use without its documents.
    let raw = zip_entries(&exported)[0].1.clone();
    let tmp = Dirs::new("docs-single-raw");
    assert!(matches!(
        tmp.suite().open_single_vault(raw, PW.into(), None, None, None, Some(kf.clone())),
        Err(GhulError::InvalidInput { .. })
    ));

    // Opened on its own, changed, written back, opened again.
    let t1 = Dirs::new("docs-single-t1");
    let s1 = t1.suite();
    let r = s1.open_single_docs(source(exported.clone()), "falsch".into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::WrongPassword)));
    assert_eq!(
        s1.open_single_docs(source(exported.clone()), PW.into(), None, None, None, Some(kf.clone()), None).unwrap(),
        v
    );
    assert!(s1.is_single());
    assert_eq!(read(&s1, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    let n = add(&s1, &v, None, "neu.pdf", b"neu");
    assert!(s1.archive_unsaved());
    assert!(matches!(s1.single_vault_file(), Err(GhulError::InvalidInput { .. })));
    let sink = Arc::new(BytesSink::default());
    let w = s1.single_docs_write(sink.clone(), None).unwrap();
    s1.single_saved(w.change);
    assert!(!s1.archive_unsaved());
    let written = std::mem::take(&mut *sink.0.lock().unwrap());
    let t2 = Dirs::new("docs-single-t2");
    let s2 = t2.suite();
    s2.open_single_docs(source(written.clone()), PW.into(), None, None, None, Some(kf.clone()), None).unwrap();
    assert_eq!(read(&s2, &v, &n.id).unwrap(), b"neu");
    assert_eq!(read(&s2, &v, &y.id).unwrap(), content(300, 2));
    // A changed document in it: refused, nothing stays.
    let mut l = zip_entries(&written);
    let i = l.iter().position(|(n, _)| n.starts_with("docs-")).unwrap();
    l[i].1[200] ^= 1;
    let t3 = Dirs::new("docs-single-t3");
    let r = t3.suite().open_single_docs(source(zip_of(&l)), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::Corrupted { .. })), "{:?}", r.err());
    assert!(!t3.suite().exists().unwrap());

    // Added to another suite (another protection): its documents count towards the limit.
    let (_db, b, kfb, _, _) = docs_suite("docs-single-b");
    b.set_docs_limit(1000);
    let r =
        b.docs_adopt_vault(source(exported.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::DocsLimit { .. })), "{:?}", r.err());
    b.set_docs_limit(DOCS_LIMIT);
    let id = b
        .docs_adopt_vault(source(exported.clone()), "Von A".into(), PW.into(), None, None, None, Some(kf.clone()), None)
        .unwrap();
    assert_eq!(id, v);
    assert_eq!(read(&b, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    assert_eq!(names(&b.docs_trash(Some(v.clone())).unwrap()), vec!["trash.pdf"]);
    let r =
        b.docs_adopt_vault(source(exported.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::AlreadyExists)));
    // It has b's protection now: b's key file opens it after a lock.
    b.lock();
    b.unlock(PW.into(), None, None, None, Some(kfb)).unwrap();
    assert_eq!(read(&b, &v, &y.id).unwrap(), content(300, 2));

    // Handed on with a protection of its own: only the picked folder (and nothing from the trash).
    let new_kf = generate_keyfile();
    let opts = ShareDocsOptions {
        vault_id: v.clone(),
        ids: vec![f.id.clone(), t.id.clone()],
        name: "Für Anna".into(),
        master_password: PW.into(),
        password: "anna-24680".into(),
        keyfile: Some(new_kf.clone()),
        key_label: "Key".into(),
        keyfile_label: "Datei".into(),
        recovery_label: "Blatt".into(),
    };
    let sink = Arc::new(BytesSink::default());
    let wrong = ShareDocsOptions { master_password: "falsch".into(), ..opts.clone() };
    assert!(matches!(a.docs_share_vault(wrong, None, None, None, sink.clone(), None), Err(GhulError::WrongPassword)));
    let shared = a.docs_share_vault(opts, None, None, None, sink.clone(), None).unwrap();
    assert_eq!(shared.recovery_code.split('-').count(), 11);
    let data = std::mem::take(&mut *sink.0.lock().unwrap());
    let t4 = Dirs::new("docs-single-t4");
    let s4 = t4.suite();
    assert!(s4.open_single_docs(source(data.clone()), PW.into(), None, None, None, Some(kf.clone()), None).is_err());
    let nv = s4.open_single_docs(source(data), "anna-24680".into(), None, None, None, Some(new_kf), None).unwrap();
    assert_ne!(nv, v);
    assert_eq!(names(&s4.docs_list(nv.clone(), None).unwrap()), vec!["Ordner"]);
    assert_eq!(read(&s4, &nv, &x.id).unwrap(), content(SECTION + 5, 1));
    assert!(s4.docs_trash(None).unwrap().is_empty());
    assert_eq!(s4.list_vaults().unwrap()[0].name, "Für Anna");
}

#[test]
fn removing_a_vault_takes_its_documents_and_unlocking_tidies_up() {
    let (d, s, kf, _, v) = docs_suite("docs-remove");
    add(&s, &v, None, "a.pdf", b"a");
    // Left behind by an add that did not end, and by a vault that is gone.
    std::fs::write(docs_dir(&d, &v).join(format!("{}.ghlblob", "ab".repeat(16))), b"x").unwrap();
    std::fs::write(docs_dir(&d, &v).join("y.ghlblob.tmp"), b"x").unwrap();
    let gone = "cd".repeat(16);
    std::fs::create_dir_all(docs_dir(&d, &gone)).unwrap();
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(content_files(&d, &v).len(), 1);
    assert!(!docs_dir(&d, &gone).exists());
    s.remove_vault(v.clone()).unwrap();
    assert!(!docs_dir(&d, &v).exists());
    assert_eq!(s.docs_usage().unwrap().files, 0);
}

/// A document of 300 MB goes in, out, into a suite file and out of it again through a few
/// buffers: the memory used stays small.
#[test]
fn a_large_document_needs_little_memory() {
    const SIZE: u64 = 300 * 1024 * 1024;
    const MAX: u64 = 24 * 1024 * 1024;
    let (_d, s, kf, _, v) = docs_suite("docs-large");
    let start = crate::alloc_meter::start();
    let it = s.docs_add_file(v.clone(), None, "gross.bin".into(), "".into(), generated(SIZE, true), None).unwrap();
    let peak = crate::alloc_meter::peak_since(start);
    assert!(peak < MAX, "adding needed {peak} bytes");

    let start = crate::alloc_meter::start();
    let sink = Arc::new(Counting::default());
    s.docs_read_file(v.clone(), it.id.clone(), sink.clone(), None).unwrap();
    let peak = crate::alloc_meter::peak_since(start);
    assert!(peak < MAX, "reading needed {peak} bytes");
    let (n, sha) = std::mem::take(&mut *sink.0.lock().unwrap());
    assert_eq!(n, SIZE);
    let mut expected = sha2::Sha256::default();
    let mut pos = 0u64;
    while pos < SIZE {
        let k = (SIZE - pos).min(1 << 20);
        sha2::Digest::update(&mut expected, (pos..pos + k).map(|i| (i % 251) as u8).collect::<Vec<u8>>());
        pos += k;
    }
    assert_eq!(sha2::Digest::finalize(sha), sha2::Digest::finalize(expected));

    // Into a suite file on the disk and out of it on another device.
    let dir = Dirs::new("docs-large-file");
    std::fs::create_dir_all(&dir.root).unwrap();
    let path = dir.root.join("suite.ghlzip");
    let start = crate::alloc_meter::start();
    let sink = Arc::new(FileSink(Mutex::new(std::fs::File::create(&path).unwrap())));
    let built = s.archive_build("Test".into(), sink, None).unwrap();
    let peak = crate::alloc_meter::peak_since(start);
    assert!(peak < MAX, "saving needed {peak} bytes");
    assert!(built.size > SIZE);
    let db = Dirs::new("docs-large-b");
    let b = db.suite();
    let start = crate::alloc_meter::start();
    let src = Arc::new(FileSource(Mutex::new(std::fs::File::open(&path).unwrap()), built.size));
    b.archive_open(src, PW.into(), None, None, None, Some(kf), None).unwrap();
    let peak = crate::alloc_meter::peak_since(start);
    assert!(peak < MAX + 70 * 1024 * 1024, "opening needed {peak} bytes (Argon2id takes 64 MiB)");
    std::fs::remove_file(&path).unwrap();
    b.archive_accept().unwrap();
    assert_eq!(b.docs_usage().unwrap().used, SIZE);
    assert!(b.docs_verify(v).unwrap().is_empty());
}
