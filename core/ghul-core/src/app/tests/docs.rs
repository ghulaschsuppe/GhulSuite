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
    archive_inspect, check_room, stored_limit, suite_limit, ArchiveContent, DocsItem, Room, ShareDocsOptions, Usage,
    DOCS_LIMIT, DOCS_MAX_FILES, DOCS_SUITE_LIMIT,
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
    let usage = s.docs_usage(v.clone()).unwrap();
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
    assert_eq!(s.docs_usage(v.clone()).unwrap().used, 6000);

    // The folder goes to the trash with everything in it; it still counts.
    s.docs_to_trash(v.clone(), f.id.clone()).unwrap();
    assert_eq!(names(&s.docs_list(v.clone(), None).unwrap()), vec!["c.pdf"]);
    assert_eq!(names(&s.docs_trash(None).unwrap()), vec!["Alt"]);
    assert!(s.docs_search(None, "b.pdf".into()).unwrap().is_empty());
    assert_eq!(s.docs_usage(v.clone()).unwrap().used, 6000, "the trash counts");
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
    assert_eq!(s.docs_usage(v.clone()).unwrap().used, 3000);
    assert!(matches!(s.docs_item(v.clone(), c.id.clone()), Err(GhulError::NotFound)));
    // For good, without the trash: a folder with everything in it.
    s.docs_delete_forever(v.clone(), f.id.clone()).unwrap();
    assert_eq!(content_files(&d, &v).len(), 1);
    assert!(matches!(s.docs_item(v.clone(), a.id.clone()), Err(GhulError::NotFound)));
    assert!(matches!(s.docs_delete_forever(v.clone(), a.id.clone()), Err(GhulError::NotFound)));
    assert_eq!(
        s.docs_usage(v.clone()).unwrap(),
        super::super::docs::DocsUsage {
            used: 2000,
            limit: DOCS_LIMIT,
            files: 1,
            suite_used: 2000,
            suite_limit: DOCS_SUITE_LIMIT,
            suite_files: 1,
            max_files: DOCS_MAX_FILES as u32,
        }
    );
}

fn add_into_trashed(s: &GhulSuite, v: &str, folder: &DocsItem) -> Result<DocsItem, GhulError> {
    s.docs_add_file(v.into(), Some(folder.id.clone()), "x".into(), "".into(), source(b"x".to_vec()), None)
}

#[test]
fn the_limit_holds_with_the_trash() {
    // The real numbers: 1 GB of content per vault, also as one document; 3 GB in all vaults
    // together; and it fits into a suite file.
    let vault = |u: Usage| Room { vault: u, suite: u };
    let none = Room::default();
    assert_eq!(DOCS_SUITE_LIMIT, suite_limit(DOCS_LIMIT));
    assert!(check_room(none, Usage::file(DOCS_LIMIT), DOCS_LIMIT).is_ok());
    assert!(matches!(check_room(none, Usage::file(DOCS_LIMIT + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { .. })));
    let half = Usage::file(DOCS_LIMIT / 2);
    assert!(check_room(vault(half), half, DOCS_LIMIT).is_ok());
    assert!(
        matches!(check_room(vault(half), Usage::file(DOCS_LIMIT / 2 + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { used, limit, needed, suite: false }) if used == DOCS_LIMIT / 2 && limit == DOCS_LIMIT && needed == DOCS_LIMIT / 2 + 1)
    );
    // Other vaults do not count towards this vault's limit, only towards the suite's.
    let others = Usage::file(DOCS_LIMIT).plus(Usage::file(DOCS_LIMIT / 2));
    let r = Room { vault: half, suite: half.plus(others) };
    assert!(check_room(r, half, DOCS_LIMIT).is_ok());
    assert!(
        matches!(check_room(r, Usage::file(DOCS_LIMIT / 2 + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { limit, .. }) if limit == DOCS_LIMIT),
        "the vault's limit comes first"
    );
    let full =
        Room { vault: Usage::default(), suite: Usage::file(DOCS_LIMIT).plus(Usage::file(DOCS_LIMIT)).plus(half) };
    assert!(check_room(full, half, DOCS_LIMIT).is_ok());
    assert!(
        matches!(check_room(full, Usage::file(DOCS_LIMIT / 2 + 1), DOCS_LIMIT), Err(GhulError::DocsLimit { used, limit, .. }) if limit == DOCS_SUITE_LIMIT && used == full.suite.used),
        "all vaults together: the suite's numbers"
    );
    assert!(blob::file_len(DOCS_LIMIT) <= stored_limit(DOCS_LIMIT));
    assert!(stored_limit(DOCS_SUITE_LIMIT) + 64 * 1024 * 1024 < super::super::archive::MAX_ARCHIVE);
    // Many small documents: each takes at least 64 KiB in the suite file, so their rounding
    // counts as well (about 16 700 of the smallest fit into a vault), and their number.
    let small = Usage::file(10);
    let fit = stored_limit(DOCS_LIMIT) / blob::file_len(10);
    let many = Usage { used: 10 * (fit - 1), stored: blob::file_len(10) * (fit - 1), files: fit - 1 };
    assert!(check_room(vault(many), small, DOCS_LIMIT).is_ok());
    assert!(matches!(check_room(vault(many.plus(small)), small, DOCS_LIMIT), Err(GhulError::DocsLimit { .. })));
    // The number of documents counts for all vaults together.
    let counted = Room { vault: Usage::default(), suite: Usage { used: 0, stored: 0, files: DOCS_MAX_FILES - 1 } };
    assert!(check_room(counted, Usage { files: 1, ..Usage::default() }, DOCS_LIMIT).is_ok());
    assert!(matches!(
        check_room(counted, Usage { files: 2, ..Usage::default() }, DOCS_LIMIT),
        Err(GhulError::DocsLimit { .. })
    ));

    // A small limit, so it is reached quickly: the trash counts.
    let (d, s, _, _, v) = docs_suite("docs-limit");
    s.set_docs_limit(10_000_000);
    let a = add(&s, &v, None, "a", &content(6_000_000, 1));
    let before = content_files(&d, &v);
    let r = s.docs_add_file(v.clone(), None, "b".into(), "".into(), generated(5_000_000, true), None);
    assert!(
        matches!(r, Err(GhulError::DocsLimit { used: 6_000_000, limit: 10_000_000, needed: 5_000_000, suite: false })),
        "{r:?}"
    );
    // Size not known before: stopped as soon as it is too much, nothing stays.
    let r = s.docs_add_file(v.clone(), None, "b".into(), "".into(), generated(5_000_000, false), None);
    assert!(matches!(r, Err(GhulError::DocsLimit { .. })), "{r:?}");
    assert_eq!(content_files(&d, &v), before, "nothing written, no temporary file left");
    s.docs_add_file(v.clone(), None, "b".into(), "".into(), generated(3_900_000, false), None).unwrap();
    // In the trash it still counts; emptied, there is room again.
    s.docs_to_trash(v.clone(), a.id.clone()).unwrap();
    assert!(matches!(
        s.docs_add_file(v.clone(), None, "c".into(), "".into(), generated(1_000_000, true), None),
        Err(GhulError::DocsLimit { .. })
    ));
    s.docs_empty_trash(Some(v.clone())).unwrap();
    s.docs_add_file(v.clone(), None, "c".into(), "".into(), generated(6_000_000, true), None).unwrap();
    assert_eq!(s.docs_usage(v.clone()).unwrap().used, 9_900_000);
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
    assert_eq!(b.docs_usage(String::new()).unwrap(), a.docs_usage(String::new()).unwrap());
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

    // Added to another suite (another protection) that has a GhulDocs vault: a vault of its
    // own next to it, with that suite's protection; its documents count towards the limits.
    let (db, b, kfb, _, vb) = docs_suite("docs-single-b");
    let mine = add(&b, &vb, None, "Von A", b"meins");
    b.set_docs_limit(1000);
    let files_before = content_files(&db, &vb);
    let r =
        b.docs_adopt_vault(source(exported.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    // The error says how much the package needs (all its documents, the trash included): a new
    // vault, so nothing is used in it yet.
    let needed = (SECTION + 5 + 300 + 30) as u64;
    assert!(
        matches!(r, Err(GhulError::DocsLimit { used: 0, limit: 1000, needed: n, suite: false }) if n == needed),
        "{:?}",
        r.err()
    );
    // Refused, nothing changed: no new vault, no content file and no staging folder left.
    assert_eq!(docs_vaults(&b), vec![vb.clone()]);
    assert_eq!(content_files(&db, &vb), files_before);
    assert!(!docs_dir(&db, &v).exists());
    assert_eq!(staging_left(&db), 0);
    b.set_docs_limit(DOCS_LIMIT);
    let r = b.docs_adopt_vault(source(save(&b)), "".into(), PW.into(), None, None, None, Some(kfb.clone()), None);
    assert!(
        matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "this is a suite file, not a GhulDocs package"),
        "{r:?}"
    );
    let id = b
        .docs_adopt_vault(source(exported.clone()), "Von A".into(), PW.into(), None, None, None, Some(kf.clone()), None)
        .unwrap();
    assert_eq!(id, v);
    assert_eq!(docs_vaults(&b).len(), 2);
    assert!(b.list_vaults().unwrap().iter().any(|x| x.id == v && x.name == "Von A"));
    // Everything as in the package: folders, trash, documents; the own vault untouched.
    assert_eq!(names(&b.docs_list(v.clone(), None).unwrap()), vec!["Ordner", "y.pdf"]);
    assert_eq!(read(&b, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    assert_eq!(read(&b, &v, &y.id).unwrap(), content(300, 2));
    assert_eq!(names(&b.docs_trash(Some(v.clone())).unwrap()), vec!["trash.pdf"]);
    assert_eq!(names(&b.docs_list(vb.clone(), None).unwrap()), vec!["Von A"]);
    assert_eq!(read(&b, &vb, &mine.id).unwrap(), b"meins");
    let u = b.docs_usage(v.clone()).unwrap();
    assert_eq!((u.files, u.suite_files), (3, 4));
    assert_eq!(content_files(&db, &v).len(), 3);
    assert_eq!(content_files(&db, &vb).len(), 1);
    // The same package again: the vault is there already.
    let r =
        b.docs_adopt_vault(source(exported.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "{r:?}");
    assert_eq!(docs_vaults(&b).len(), 2);
    // It all has b's protection: b's key file opens it after a lock, and it is in b's suite file.
    b.lock();
    b.unlock(PW.into(), None, None, None, Some(kfb.clone())).unwrap();
    assert_eq!(read(&b, &v, &y.id).unwrap(), content(300, 2));
    assert!(b.docs_verify(v.clone()).unwrap().is_empty());
    let dc = Dirs::new("docs-single-c");
    let c = dc.suite();
    c.archive_open_bytes(save(&b), PW.into(), None, None, None, Some(kfb)).unwrap();
    c.archive_accept().unwrap();
    assert_eq!(read(&c, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    assert_eq!(read(&c, &vb, &mine.id).unwrap(), b"meins");

    // A suite without a GhulDocs vault: the package becomes it (with this suite's protection).
    let de = Dirs::new("docs-single-e");
    let e = de.suite();
    let kfe = generate_keyfile();
    e.create(options(PW, Some(kfe.clone()), &[(VaultKind::Pass, "Privat")]), None, None, None).unwrap();
    let id = e
        .docs_adopt_vault(source(exported.clone()), "Von A".into(), PW.into(), None, None, None, Some(kf.clone()), None)
        .unwrap();
    assert_eq!(id, v);
    assert_eq!(read(&e, &v, &x.id).unwrap(), content(SECTION + 5, 1));
    let r =
        e.docs_adopt_vault(source(exported.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "{r:?}");
    e.lock();
    e.unlock(PW.into(), None, None, None, Some(kfe)).unwrap();
    assert_eq!(read(&e, &v, &y.id).unwrap(), content(300, 2));

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
    assert!(matches!(s.docs_usage(v.clone()), Err(GhulError::NotFound)));
    assert_eq!(s.docs_usage(String::new()).unwrap().files, 0);
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
    assert_eq!(b.docs_usage(String::new()).unwrap().used, SIZE);
    assert!(b.docs_verify(v).unwrap().is_empty());
}

// --- review G9: regressions ---------------------------------------------------------------------

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn refused_as_docs<T: std::fmt::Debug>(r: Result<T, GhulError>) -> bool {
    matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "a GhulDocs vault goes with its documents")
}

/// Opens a suite file on a new device and accepts it.
fn take(name: &str, file: Vec<u8>, kf: &[u8]) -> (Dirs, Arc<GhulSuite>) {
    let d = Dirs::new(name);
    let s = d.suite();
    s.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.to_vec())).unwrap();
    s.archive_accept().unwrap();
    (d, s)
}

/// Sets the modification time of everything directly in `dir` (files and folders) to three days
/// ago.
fn backdate(dir: &std::path::Path) {
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 24 * 3600);
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        open_for_times(&e.path()).unwrap().set_modified(then).unwrap();
    }
}

/// Opens a file or folder so its times can be set. Windows opens a folder only with
/// FILE_FLAG_BACKUP_SEMANTICS and sets times only with FILE_WRITE_ATTRIBUTES access; a plain
/// `File::open` is refused there ("Access is denied").
fn open_for_times(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open(path)
    }
}

/// The generic entry functions (GhulPass, GhulAuth) never touch a GhulDocs index: a copy would
/// share its content file, a move would leave it behind (G9 4).
#[test]
fn generic_entry_functions_refuse_docs_vaults() {
    let (_d, a, kf, _, v) = docs_suite("g9-entry");
    let v2 = vault_id(&a, "Privat");
    let x = add(&a, &v, None, "x.pdf", b"hello");
    let id = unhex(&x.id);
    assert!(refused_as_docs(a.duplicate_entry(v.clone(), id.clone(), " (Kopie)".into())));
    assert!(refused_as_docs(a.move_entry(v.clone(), id.clone(), v2.clone())));
    assert!(refused_as_docs(a.set_entry_favorite(v.clone(), id.clone(), true)));
    assert!(refused_as_docs(a.add_entry_attachment(v.clone(), id.clone(), "a.txt".into(), b"a".to_vec())));
    assert!(refused_as_docs(a.remove_entry_attachment(v.clone(), id.clone(), "a.txt".into())));
    assert!(refused_as_docs(a.entry_attachment(v.clone(), id.clone(), "a.txt".into())));
    assert!(refused_as_docs(a.entry_extras(v.clone(), id.clone())));
    assert!(refused_as_docs(a.get_entry(v.clone(), id.clone())));
    assert!(refused_as_docs(a.delete_entry(v.clone(), id.clone())));
    assert!(refused_as_docs(a.rename_totp(v.clone(), id.clone(), "i".into(), "a".into())));
    assert!(refused_as_docs(a.totp_uri(v.clone(), id.clone())));
    // Nothing changed: one document, the suite file saves and opens elsewhere with it.
    assert_eq!(a.docs_usage(String::new()).unwrap().files, 1);
    let file = save(&a);
    let (_db, b) = take("g9-entry-b", file, &kf);
    assert_eq!(read(&b, &v, &x.id).unwrap(), b"hello");
}

/// A check waiting for "accept" keeps its documents however long it waits (also over a jump of
/// the clock): tidying leaves its staging folder alone (G9 2).
#[test]
fn a_waiting_check_keeps_its_documents_however_old() {
    let (_d, a, kf, _, v) = docs_suite("g9-stage-a");
    let x = add(&a, &v, None, "x.pdf", b"hello");
    let file = save(&a);
    let db = Dirs::new("g9-stage-b");
    let b = db.suite();
    b.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    backdate(&db.root.join("vault").join(".staging"));
    // Any later staging (another open, an adopt, accept's own) tidies old folders.
    drop(super::super::docs::Staging::new(&db.root.join("vault")).unwrap());
    b.archive_accept().unwrap();
    assert_eq!(read(&b, &v, &x.id).unwrap(), b"hello");
    assert_eq!(staging_left(&db), 0);
}

/// A checked document missing before "accept" (removed by anything else): nothing is replaced,
/// the documents of the device stay (G9 2).
#[test]
fn accept_with_a_checked_document_missing_changes_nothing() {
    let (_da, a, kf_a, _, va) = docs_suite("g9-miss-a");
    add(&a, &va, None, "x.pdf", b"hello");
    let file = save(&a);
    let (db, b, _kf_b, _, vb) = docs_suite("g9-miss-b");
    let y = add(&b, &vb, None, "y.pdf", b"mine");
    b.archive_open_bytes(file, PW.into(), None, None, None, Some(kf_a)).unwrap();
    let staging = db.root.join("vault").join(".staging");
    let mut gone = 0;
    for s in std::fs::read_dir(&staging).unwrap().flatten() {
        for v in std::fs::read_dir(s.path()).unwrap().flatten() {
            for f in std::fs::read_dir(v.path()).unwrap().flatten() {
                std::fs::remove_file(f.path()).unwrap();
                gone += 1;
            }
        }
    }
    assert_eq!(gone, 1);
    let r = b.archive_accept();
    assert!(matches!(&r, Err(GhulError::Corrupted { msg }) if msg == "a document is damaged or missing"), "{r:?}");
    assert!(b.is_unlocked(), "failed before anything was replaced");
    assert!(!b.archive_device_data_wiped());
    assert_eq!(read(&b, &vb, &y.id).unwrap(), b"mine");
    assert_eq!(staging_left(&db), 0);
}

/// A suite file larger than is read is never written: the save is refused with a clear error
/// before anything reaches the sink, and the working copy keeps its state (G9 3).
#[test]
fn a_suite_file_too_large_to_open_is_never_written() {
    use super::super::archive::{ArchiveTarget, OTHER_LIMIT, TEST_LIMITS};
    let (_d, a, _kf, _, v) = docs_suite("g9-big");
    let first = save(&a);
    add(&a, &v, None, "big.bin", &content(300_000, 1));
    TEST_LIMITS.with(|l| l.set(Some((300_000, OTHER_LIMIT))));
    let sink = Arc::new(BytesSink::default());
    let r = a.archive_build("Test".into(), sink.clone(), None);
    TEST_LIMITS.with(|l| l.set(None));
    assert!(
        matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "the suite file would be too large"),
        "{:?}",
        r.map(|_| ())
    );
    assert!(sink.0.lock().unwrap().is_empty(), "nothing written");
    assert_eq!(a.archive_target_bytes(first).unwrap(), ArchiveTarget::Same, "the state before");
    save(&a);
}

/// Attachments stop before the vaults besides GhulDocs could make the suite file too large
/// (G9 3).
#[test]
fn attachments_stop_before_the_suite_file_gets_too_large() {
    use super::super::archive::{MAX_ARCHIVE, TEST_LIMITS};
    let (_d, a, _kf, _, _v) = docs_suite("g9-other");
    let p = vault_id(&a, "Privat");
    let id = a.save_entry(p.clone(), login("Bank", "u", "1")).unwrap();
    TEST_LIMITS.with(|l| l.set(Some((MAX_ARCHIVE, 600_000))));
    let mut added = 0;
    let refused = loop {
        match a.add_entry_attachment(p.clone(), id.clone(), format!("{added}.bin"), content(100_000, added as u8)) {
            Ok(()) => added += 1,
            Err(e) => break e,
        }
        assert!(added < 10);
    };
    let dup = a.duplicate_entry(p.clone(), id.clone(), " (Kopie)".into());
    TEST_LIMITS.with(|l| l.set(None));
    assert!(matches!(&refused, GhulError::InvalidInput { msg } if msg == "the vaults besides GhulDocs are full"));
    assert!((3..=4).contains(&added), "{added}");
    assert!(matches!(&dup, Err(GhulError::InvalidInput { msg }) if msg == "the vaults besides GhulDocs are full"));
}

/// A GhulDocs vault whose adoption cannot be saved is not half added: not in the open suite, not
/// in the folder, no content folder; the suite still saves, and adopting again works (G9 5).
#[test]
fn an_adopted_docs_vault_that_cannot_be_saved_is_not_half_added() {
    let (_da, a, kf_a, _, va) = docs_suite("g9-adopt-a");
    let x = add(&a, &va, None, "x.pdf", b"hello");
    let sink = Arc::new(BytesSink::default());
    a.docs_export_vault(va.clone(), sink.clone(), None).unwrap();
    let file = std::mem::take(&mut *sink.0.lock().unwrap());
    // Into a suite with a GhulDocs vault and into one without.
    for (name, kinds) in
        [("g9-adopt-b", vec![(VaultKind::Docs, "Eigene")]), ("g9-adopt-c", vec![(VaultKind::Pass, "P")])]
    {
        let db = Dirs::new(name);
        let flaky = Arc::new(super::archive::FailOnce { inner: db.storage(), armed: false.into() });
        let b = GhulSuite::new(flaky.clone(), db.local());
        let kf_b = generate_keyfile();
        b.create(options(PW, Some(kf_b), &kinds), None, None, None).unwrap();
        let own = b.list_vaults().unwrap().into_iter().find(|i| i.kind == VaultKind::Docs).map(|i| i.id);
        flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
        let adopt = || {
            b.docs_adopt_vault(
                source(file.clone()),
                "Übernommen".into(),
                PW.into(),
                None,
                None,
                None,
                Some(kf_a.clone()),
                None,
            )
        };
        assert!(adopt().is_err());
        assert!(!b.list_vaults().unwrap().iter().any(|i| i.id == va));
        assert!(!flaky.list().unwrap().iter().any(|f| f.contains(&va)), "no vault file");
        assert!(!docs_dir(&db, &va).exists(), "no content folder");
        if let Some(vb) = &own {
            assert!(b.docs_list(vb.clone(), None).unwrap().is_empty(), "the index as it was");
            assert!(content_files(&db, vb).is_empty(), "no content file moved in");
        }
        assert_eq!(b.docs_usage(String::new()).unwrap().files, 0);
        b.archive_build_bytes("Test".into()).unwrap();
        // A vault of its own, next to the suite's GhulDocs vault if it has one.
        let id = adopt().unwrap();
        assert_eq!(id, va);
        assert_eq!(read(&b, &va, &x.id).unwrap(), b"hello");
        assert!(b.list_vaults().unwrap().iter().any(|i| i.id == va && i.name == "Übernommen"));
        if let Some(vb) = &own {
            assert!(b.docs_list(vb.clone(), None).unwrap().is_empty(), "the own vault untouched");
        }
    }
}

/// Deletes a document for good when the first piece of the suite file is written.
struct DeletingSink {
    suite: Arc<GhulSuite>,
    vault: String,
    id: String,
    done: std::sync::atomic::AtomicBool,
    data: Mutex<Vec<u8>>,
}

impl ArchiveSink for DeletingSink {
    fn write(&self, data: Vec<u8>) -> Result<(), StorageError> {
        if !self.done.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.suite.docs_delete_forever(self.vault.clone(), self.id.clone()).unwrap();
        }
        self.data.lock().unwrap().extend_from_slice(&data);
        Ok(())
    }
}

/// A document deleted for good (or the trash emptied) while the suite file is written: its
/// content file stays until the file is complete (on Android the file is overwritten in place,
/// a failure there loses the copy before), and goes afterwards (G9 6).
#[test]
fn deleting_a_document_while_saving_keeps_the_file_complete() {
    let (d, a, kf, _, v) = docs_suite("g9-del");
    let x = add(&a, &v, None, "x.bin", &content(3 * 1024 * 1024, 3));
    let y = add(&a, &v, None, "y.pdf", b"second");
    let sink = Arc::new(DeletingSink {
        suite: a.clone(),
        vault: v.clone(),
        id: y.id.clone(),
        done: false.into(),
        data: Mutex::new(Vec::new()),
    });
    let built = a.archive_build("Test".into(), sink.clone(), None).unwrap();
    assert!(sink.done.load(std::sync::atomic::Ordering::SeqCst), "deleted while it was written");
    assert_eq!(content_files(&d, &v).len(), 1, "deleted once the file was complete");
    let data = std::mem::take(&mut *sink.data.lock().unwrap());
    assert!(
        super::super::archive::archive_written_matches(source(data.clone()), built.size, built.sha256, None).unwrap()
    );
    // The file holds the state it was built from, with both documents.
    let (_db, b) = take("g9-del-b", data, &kf);
    assert_eq!(read(&b, &v, &y.id).unwrap(), b"second");
    assert_eq!(read(&b, &v, &x.id).unwrap().len(), 3 * 1024 * 1024);
    // The next unlock tidies nothing it should not.
    a.lock();
    a.finish_lock();
    a.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(read(&a, &v, &x.id).unwrap().len(), 3 * 1024 * 1024);
}

/// A file of an older development version (an older format) says so, instead of a generic
/// error: the working copy and a suite file (G9 13).
#[test]
fn files_of_an_older_development_version_say_so() {
    let (d, a, _kf, _, _v) = docs_suite("g9-old");
    let mut suite = std::fs::read(d.root.join("vault").join("suite.ghlsuite")).unwrap();
    suite[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert!(matches!(LockedContainer::parse(&suite).map_err(GhulError::from), Err(GhulError::OldFormat)));
    let r = a.archive_protection_bytes(zip_of(&[("export.ghlexp".into(), suite.clone())]));
    assert!(matches!(r, Err(GhulError::OldFormat)), "{r:?}");
    a.lock();
    a.finish_lock();
    std::fs::write(d.root.join("vault").join("suite.ghlsuite"), &suite).unwrap();
    let r = a.unlock(PW.into(), None, None, None, Some(_kf));
    assert!(matches!(r, Err(GhulError::OldFormat)), "{r:?}");
}

// --- several GhulDocs vaults per suite, and the GhulDocs package ---------------------------------

/// GhulDocs vaults are like the other vaults: several of them, each on its own (its documents,
/// folders, trash and limit), all in the suite file.
#[test]
fn several_docs_vaults_each_on_their_own() {
    let (d, s, kf, _, v) = docs_suite("many-docs");
    let w = s.create_vault(VaultKind::Docs, "Steuer 2025".into()).unwrap();
    assert_ne!(w, v);
    assert_eq!(docs_vaults(&s).len(), 2);
    let a = add(&s, &v, None, "a.pdf", &content(1000, 1));
    let b = add(&s, &w, None, "b.pdf", &content(2000, 2));
    assert_eq!(names(&s.docs_list(v.clone(), None).unwrap()), vec!["a.pdf"]);
    assert_eq!(names(&s.docs_list(w.clone(), None).unwrap()), vec!["b.pdf"]);
    assert!(matches!(s.docs_item(w.clone(), a.id.clone()), Err(GhulError::NotFound)), "not in the other vault");
    let u = s.docs_usage(w.clone()).unwrap();
    assert_eq!((u.used, u.files, u.suite_used, u.suite_files), (2000, 1, 3000, 2));
    let all = s.docs_usage(String::new()).unwrap();
    assert_eq!((all.used, all.files), (3000, 2));
    // Each vault has its own limit, all of them together the suite's (three times as much).
    s.set_docs_limit(10_000_000);
    add(&s, &v, None, "a2.pdf", &content(8_000_000, 3));
    let r = s.docs_add_file(v.clone(), None, "voll".into(), "".into(), generated(2_000_000, true), None);
    assert!(
        matches!(r, Err(GhulError::DocsLimit { used: 8_001_000, limit: 10_000_000, needed: 2_000_000, suite: false })),
        "{r:?}"
    );
    add(&s, &w, None, "b2.pdf", &content(8_000_000, 4));
    let x = s.create_vault(VaultKind::Docs, "Dritter".into()).unwrap();
    add(&s, &x, None, "c.pdf", &content(9_000_000, 5));
    // The third vault has room for 1 MB: its own limit comes first.
    let r = s.docs_add_file(x.clone(), None, "zu viel".into(), "".into(), generated(1_500_000, true), None);
    assert!(matches!(r, Err(GhulError::DocsLimit { used: 9_000_000, limit: 10_000_000, suite: false, .. })), "{r:?}");
    // At most three GhulDocs vaults: a fourth is refused wherever it would come in.
    let full = |r: Result<String, GhulError>| matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "a suite has at most 3 GhulDocs vaults");
    assert!(full(s.create_vault(VaultKind::Docs, "Vierter".into())));
    assert!(s.add_area(VaultKind::Docs, "".into()).is_ok(), "the area is there: nothing new");
    assert_eq!(docs_vaults(&s).len(), 3);
    s.create_vault(VaultKind::Pass, "Noch ein Passwort-Tresor".into()).unwrap();
    // Removed, one may come again.
    s.remove_vault(x.clone()).unwrap();
    let y = s.create_vault(VaultKind::Docs, "Vierter".into()).unwrap();
    add(&s, &y, None, "passt.pdf", &content(4_000_000, 6));
    s.set_docs_limit(DOCS_LIMIT);
    // Removing one vault takes only its documents.
    assert!(!docs_dir(&d, &x).exists());
    assert_eq!(read(&s, &w, &b.id).unwrap(), content(2000, 2));
    // In the suite file and back, after a lock, on another device.
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(docs_vaults(&s).len(), 3);
    let file = save(&s);
    let d2 = Dirs::new("many-docs-b");
    let s2 = d2.suite();
    s2.archive_open_bytes(file, PW.into(), None, None, None, Some(kf.clone())).unwrap();
    s2.archive_accept().unwrap();
    assert_eq!(docs_vaults(&s2).len(), 3);
    assert_eq!(read(&s2, &v, &a.id).unwrap(), content(1000, 1));
    assert_eq!(read(&s2, &w, &b.id).unwrap(), content(2000, 2));
    assert_eq!(names(&s2.docs_list(y.clone(), None).unwrap()), vec!["passt.pdf"]);
    // Set up with two at once.
    let d3 = Dirs::new("many-docs-setup");
    let s3 = d3.suite();
    s3.create(
        options(PW, Some(generate_keyfile()), &[(VaultKind::Docs, "A"), (VaultKind::Docs, "B")]),
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(docs_vaults(&s3).len(), 2);
}

/// A GhulDocs package is not a suite file and a suite file no GhulDocs package: each says what
/// it is.
#[test]
fn a_docs_package_and_a_suite_file_are_told_apart() {
    let (_d, a, kf, _, v) = docs_suite("package-kind");
    add(&a, &v, None, "x.pdf", b"x");
    let sink = Arc::new(BytesSink::default());
    a.docs_export_vault(v.clone(), sink.clone(), None).unwrap();
    let package = std::mem::take(&mut *sink.0.lock().unwrap());
    let suite_file = save(&a);
    let is_package = |r: Result<(), GhulError>| matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "this is a GhulDocs package, not a suite file");
    let is_suite = |r: Result<String, GhulError>| matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "this is a suite file, not a GhulDocs package");
    let db = Dirs::new("package-kind-b");
    let b = db.suite();
    assert!(is_package(b.archive_protection(source(package.clone())).map(|_| ())));
    assert!(is_package(
        b.archive_open_bytes(package.clone(), PW.into(), None, None, None, Some(kf.clone())).map(|_| ())
    ));
    assert!(is_suite(b.open_single_docs(
        source(suite_file.clone()),
        PW.into(),
        None,
        None,
        None,
        Some(kf.clone()),
        None
    )));
    assert!(!b.exists().unwrap());
    assert!(is_suite(a.docs_adopt_vault(
        source(suite_file),
        "".into(),
        PW.into(),
        None,
        None,
        None,
        Some(kf.clone()),
        None
    )));
    // Something else entirely stays "not a GhulSuite file".
    let r = b.archive_protection(source(b"PK\x03\x04 nothing".to_vec()));
    assert!(matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "not a GhulSuite file"), "{r:?}");
}

fn docs_vaults(s: &GhulSuite) -> Vec<String> {
    s.list_vaults().unwrap().into_iter().filter(|v| v.kind == VaultKind::Docs).map(|v| v.id).collect()
}

#[test]
fn deleting_the_working_copy_leaves_nothing_of_the_suite() {
    let (d, s, kf, _, v) = docs_suite("delete-working-copy");
    let x = add(&s, &v, None, "x.pdf", &content(300, 1));
    s.quick_unlock_enable("pin".into(), Some("123456".into())).unwrap();
    let file = save(&s);
    add(&s, &v, None, "unsaved.pdf", b"u");
    assert!(s.archive_unsaved());
    let vault = d.root.join("vault");
    let local = d.root.join("local");
    // What an earlier failure may have left: a staging folder kept on purpose, rescued documents.
    std::fs::create_dir_all(vault.join(".staging").join("kept")).unwrap();
    std::fs::write(vault.join(".staging").join("kept").join("KEEP"), b"").unwrap();
    std::fs::create_dir_all(vault.join("rescued-documents-1")).unwrap();
    assert!(std::fs::read_dir(&local)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("quick-")));
    let seen = std::fs::read_to_string(local.join("archive-seen.json")).unwrap();
    let suite_id = seen.split('"').nth(1).unwrap().to_owned();

    s.delete_working_copy().unwrap();
    assert!(!s.is_unlocked());
    assert!(!s.exists().unwrap());
    let left: Vec<String> =
        std::fs::read_dir(&vault).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(left.is_empty(), "nothing of the suite is left: {left:?}");
    let left: Vec<String> = std::fs::read_dir(&local)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("quick-") || n == "archive-unsaved")
        .collect();
    assert!(left.is_empty(), "no quick unlock, no unsaved mark: {left:?}");
    let seen = std::fs::read_to_string(local.join("archive-seen.json")).unwrap_or_default();
    assert!(!seen.contains(&suite_id), "what was seen of its suite files is forgotten: {seen}");
    assert!(!s.quick_unlock_exists("pin".into()));
    // Deleting again finds nothing to delete.
    s.delete_working_copy().unwrap();

    // A new suite in its place; the saved suite file still opens on another device with the old data.
    s.create(options("neues-pw-123", Some(generate_keyfile()), &[(VaultKind::Pass, "Neu")]), None, None, None).unwrap();
    assert_eq!(s.list_vaults().unwrap().iter().map(|v| v.name.clone()).collect::<Vec<_>>(), vec!["Neu"]);
    let other = Dirs::new("delete-working-copy-other");
    let o = other.suite();
    o.archive_open_bytes(file, PW.into(), None, None, None, Some(kf)).unwrap();
    o.archive_accept().unwrap();
    assert_eq!(read(&o, &v, &x.id).unwrap(), content(300, 1));
}

// --- review of several GhulDocs vaults: adding never overwrites another vault -------------------

/// a GhulDocs vault that cannot be opened (damaged file) keeps its folder at unlock;
/// adding a package with the same CONTAINER_ID must not remove that folder or overwrite the file.
#[test]
fn adopting_never_removes_a_damaged_docs_vault() {
    let (d, s, kf, _, v) = docs_suite("audit-p3-damaged");
    let _old = add(&s, &v, None, "alt.pdf", b"alt");
    let sink = Arc::new(BytesSink::default());
    s.docs_export_vault(v.clone(), sink.clone(), None).unwrap();
    let exported = std::mem::take(&mut *sink.0.lock().unwrap());
    let newer = add(&s, &v, None, "neu.pdf", b"neu, nicht im Export");
    let newer_blob_files = content_files(&d, &v);
    assert_eq!(newer_blob_files.len(), 2);
    s.lock();
    // Damage the vault file (one byte in the sealed payload).
    let file = d.root.join("vault").join(format!("{}.ghldoc", &v[..16]));
    let mut bytes = std::fs::read(&file).unwrap();
    let n = bytes.len();
    bytes[n - 40] ^= 1;
    std::fs::write(&file, &bytes).unwrap();
    let damaged_copy = bytes.clone();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(content_files(&d, &v).len(), 2, "unlock keeps the folder of the damaged vault");
    let r = s.docs_adopt_vault(source(exported), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    let after = content_files(&d, &v);
    assert_eq!(after, newer_blob_files, "its documents are all still there");
    assert_eq!(std::fs::read(&file).unwrap(), damaged_copy, "the damaged vault file is kept as it was");
    let _ = newer;
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "adopted over a damaged vault: {r:?}, files {after:?}");
}

/// Gives `pre` first, then OS randomness.
struct Prefixed(Vec<u8>);
impl rand_core::RngCore for Prefixed {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let n = dest.len().min(self.0.len());
        dest[..n].copy_from_slice(&self.0[..n]);
        self.0.drain(..n);
        rand_core::OsRng.fill_bytes(&mut dest[n..]);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
impl rand_core::CryptoRng for Prefixed {}

/// a package whose CONTAINER_ID shares the first 8 bytes with a vault of the suite
/// (the file name has only 16 hex characters) is added next to it and writes the same file.
#[test]
fn adopting_never_overwrites_a_vault_with_the_same_file_name() {
    use crate::vault::container::{KdfCost, SuiteKeys};
    use crate::vault::{ContainerKind, Keyfile};
    use rand_core::OsRng;
    let (d, s, kf, _, v) = docs_suite("audit-p3-collide");
    let doc = add(&s, &v, None, "wichtig.pdf", b"wichtiger Inhalt");
    let vfile = d.root.join("vault").join(format!("{}.ghldoc", &v[..16]));
    let before = std::fs::read(&vfile).unwrap();
    // The attacker knows v (it is in every suite file: docs-<v>/...): same first 8 bytes.
    let mut id = unhex(&v);
    id[15] ^= 0xff;
    let akf = generate_keyfile();
    let keys = SuiteKeys::create(
        "attacker-pw-1",
        KdfCost { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 },
        &mut OsRng,
    )
    .unwrap();
    let parsed = Keyfile::parse(&zeroize::Zeroizing::new(akf.clone())).unwrap();
    let slot = keys.keyfile_slot(&parsed, &mut OsRng);
    let mut c = keys.create_container(ContainerKind::Docs, vec![slot], &mut Prefixed(id.clone())).unwrap();
    c.add_recovery_slot("x", 0, &mut OsRng).unwrap();
    let vid = crate::kdf::hex(&c.container_id());
    assert_ne!(vid, v);
    assert_eq!(&vid[..16], &v[..16]);
    let data = c.seal(&mut OsRng).unwrap();
    let pkg = zip_of(&[(format!("{}.ghldoc", &vid[..16]), data)]);
    let r =
        s.docs_adopt_vault(source(pkg), "Geschenk".into(), "attacker-pw-1".into(), None, None, None, Some(akf), None);
    assert_eq!(std::fs::read(&vfile).unwrap(), before, "the vault file is not overwritten");
    s.lock();
    s.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    assert_eq!(read(&s, &v, &doc.id).unwrap(), b"wichtiger Inhalt");
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "a package with a colliding file name was added: {r:?}");
}

/// A vault removed while a suite file is written keeps its folder until the copy ended: added
/// again meanwhile, it is refused (nothing would be left of it afterwards); once the copy ended,
/// it comes back with its documents and keeps them.
#[test]
fn a_vault_added_again_while_saving_never_loses_its_documents() {
    let (_d, s, kf, _, v) = docs_suite("readopt-while-saving");
    let x = add(&s, &v, None, "x.pdf", b"hello");
    let sink = Arc::new(BytesSink::default());
    s.docs_export_vault(v.clone(), sink.clone(), None).unwrap();
    let pkg = std::mem::take(&mut *sink.0.lock().unwrap());
    let copy = s.content_copy();
    s.remove_vault(v.clone()).unwrap();
    let r = s.docs_adopt_vault(source(pkg.clone()), "".into(), PW.into(), None, None, None, Some(kf.clone()), None);
    assert!(matches!(r, Err(GhulError::AlreadyExists)), "{r:?}");
    drop(copy);
    let id = s.docs_adopt_vault(source(pkg), "".into(), PW.into(), None, None, None, Some(kf), None).unwrap();
    assert_eq!(id, v);
    assert_eq!(read(&s, &v, &x.id).unwrap(), b"hello");
}

/// A suite file with more GhulDocs vaults than a suite may have (this app never writes one) is
/// not opened.
#[test]
fn a_suite_file_with_four_docs_vaults_is_refused() {
    let (_d, a, kf, _, _) = docs_suite("four-docs-a");
    a.create_vault(VaultKind::Docs, "Zwei".into()).unwrap();
    a.create_vault(VaultKind::Docs, "Drei".into()).unwrap();
    // Past the check, as another program might have done it.
    a.with_open(|o| {
        let mut c = o.new_container(crate::vault::ContainerKind::Docs)?;
        super::super::suite::set_vault_name(&mut c, "Vier");
        let id = crate::kdf::hex(&c.container_id());
        let file = super::super::suite::file_name(VaultKind::Docs, &id);
        let reference =
            super::super::suite::VaultRef { id: id.clone(), kind: "docs".into(), name: "Vier".into(), file };
        o.vaults.push(super::super::suite::OpenVault { reference, container: Some(c) });
        a.save_vault(o, &id)
    })
    .unwrap();
    let file = save(&a);
    let db = Dirs::new("four-docs-b");
    let b = db.suite();
    let r = b.archive_open_bytes(file, PW.into(), None, None, None, Some(kf));
    assert!(
        matches!(&r, Err(GhulError::InvalidInput { msg }) if msg == "a suite file holds at most 3 GhulDocs vaults"),
        "{:?}",
        r.map(|_| ())
    );
    assert!(!b.exists().unwrap());
}

/// A document whose add failed after its vault file was written (the suite file failed): the
/// index on the disk is written back, the document is not there and nothing is damaged; the
/// suite file can still be saved.
#[test]
fn a_failed_add_leaves_no_document_without_content() {
    let (d, s, kf, _, v) = docs_suite("docs-add-fail");
    add(&s, &v, None, "a.pdf", b"a");
    s.lock();
    s.finish_lock();
    let flaky =
        Arc::new(super::archive::FailOnce { inner: d.storage(), armed: std::sync::atomic::AtomicBool::new(false) });
    let t = GhulSuite::new(flaky.clone(), d.local());
    t.unlock(PW.into(), None, None, None, Some(kf.clone())).unwrap();
    flaky.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(t
        .docs_add_file(v.clone(), None, "b.pdf".into(), "application/pdf".into(), source(b"b".to_vec()), None)
        .is_err());
    t.lock();
    t.finish_lock();
    t.unlock(PW.into(), None, None, None, Some(kf)).unwrap();
    assert_eq!(names(&t.docs_list(v.clone(), None).unwrap()), vec!["a.pdf"]);
    assert_eq!(content_files(&d, &v).len(), 1, "the content of the failed add is tidied up");
    assert!(t.archive_build_bytes("Test".into()).is_ok());
}
