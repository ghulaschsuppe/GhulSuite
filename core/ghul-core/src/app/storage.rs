// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where the suite's files live. The app passes an implementation: the app's private storage
//! (Rust implementation below) or a folder chosen by the user (Android: Storage Access Framework,
//! implemented in Kotlin). Later, sync adapters (SFTP, WebDAV, …) implement the same interface.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::error::{GhulError, Result};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum StorageError {
    #[error("{msg}")]
    Io { msg: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for StorageError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        StorageError::Io { msg: e.reason }
    }
}

impl From<StorageError> for GhulError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Io { msg } => GhulError::Storage { msg },
        }
    }
}

/// A flat folder of files. `write` must replace the file atomically (no half-written files).
#[uniffi::export(with_foreign)]
pub trait VaultStorage: Send + Sync {
    /// Returns `None` if the file does not exist.
    fn read(&self, name: String) -> std::result::Result<Option<Vec<u8>>, StorageError>;
    fn write(&self, name: String, data: Vec<u8>) -> std::result::Result<(), StorageError>;
    fn delete(&self, name: String) -> std::result::Result<(), StorageError>;
    fn list(&self) -> std::result::Result<Vec<String>, StorageError>;
    /// Human-readable location, e.g. "App storage" or a folder name.
    fn describe(&self) -> String;
}

/// Writes atomically: temporary file, fsync, keep the previous version as `.bak`, rename.
pub(crate) fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<()> {
    let tmp = dir.join(format!("{name}.tmp"));
    let target = dir.join(name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if target.exists() {
        fs::rename(&target, dir.join(format!("{name}.bak")))?;
    }
    fs::rename(&tmp, &target)?;
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn valid_name(name: &str) -> std::result::Result<(), StorageError> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(StorageError::Io { msg: format!("invalid file name {name:?}") });
    }
    Ok(())
}

/// A directory on the local file system (Android: the app's private files).
pub struct LocalDirStorage {
    dir: PathBuf,
}

impl LocalDirStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        LocalDirStorage { dir: dir.into() }
    }
}

fn io(e: impl std::fmt::Display) -> StorageError {
    StorageError::Io { msg: e.to_string() }
}

impl VaultStorage for LocalDirStorage {
    fn read(&self, name: String) -> std::result::Result<Option<Vec<u8>>, StorageError> {
        valid_name(&name)?;
        match fs::read(self.dir.join(&name)) {
            Ok(d) => Ok(Some(d)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn write(&self, name: String, data: Vec<u8>) -> std::result::Result<(), StorageError> {
        valid_name(&name)?;
        fs::create_dir_all(&self.dir).map_err(io)?;
        write_atomic(&self.dir, &name, &data).map_err(io)
    }

    fn delete(&self, name: String) -> std::result::Result<(), StorageError> {
        valid_name(&name)?;
        match fs::remove_file(self.dir.join(&name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        let _ = fs::remove_file(self.dir.join(format!("{name}.bak")));
        Ok(())
    }

    fn list(&self) -> std::result::Result<Vec<String>, StorageError> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if !n.ends_with(".tmp") && !n.ends_with(".bak") && !n.starts_with('.') {
                    out.push(n);
                }
            }
        }
        out.sort();
        Ok(out)
    }

    fn describe(&self) -> String {
        "app".into()
    }
}

/// Storage in a local directory (Android: the app's private files directory).
#[uniffi::export]
pub fn local_storage(dir: String) -> Arc<dyn VaultStorage> {
    Arc::new(LocalDirStorage::new(dir))
}
