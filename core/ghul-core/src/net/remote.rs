// SPDX-License-Identifier: AGPL-3.0-or-later
//! A remote folder: what synchronisation and GhulDrive need from a server.

use std::path::{Path, PathBuf};

use super::{NetError, NetResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Paths are relative to the configured base folder and use `/`.
pub trait RemoteFs: Send + Sync {
    fn list(&self, dir: &str) -> NetResult<Vec<RemoteEntry>>;
    /// `None` if the file does not exist.
    fn read(&self, path: &str) -> NetResult<Option<Vec<u8>>>;
    /// Writes atomically (temporary file, then rename), creating parent folders.
    fn write(&self, path: &str, data: &[u8]) -> NetResult<()>;
    /// Deleting a missing file is not an error.
    fn delete(&self, path: &str) -> NetResult<()>;
    fn mkdir_all(&self, dir: &str) -> NetResult<()>;
}

/// Rejects paths that could leave the base folder.
pub(crate) fn check_path(path: &str) -> NetResult<()> {
    if path.split('/').any(|seg| seg == ".." || seg == ".") || path.starts_with('/') || path.contains('\\') {
        return Err(NetError::Protocol(format!("invalid path {path}")));
    }
    Ok(())
}

pub(crate) fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

/// A local folder as "server" (tests, USB stick as mirror).
pub struct LocalFs {
    base: PathBuf,
}

impl LocalFs {
    pub fn new(base: impl AsRef<Path>) -> Self {
        LocalFs { base: base.as_ref().to_path_buf() }
    }

    fn full(&self, path: &str) -> NetResult<PathBuf> {
        check_path(path)?;
        Ok(if path.is_empty() { self.base.clone() } else { self.base.join(path) })
    }
}

fn io(e: std::io::Error) -> NetError {
    NetError::Protocol(e.to_string())
}

impl RemoteFs for LocalFs {
    fn list(&self, dir: &str) -> NetResult<Vec<RemoteEntry>> {
        let p = self.full(dir)?;
        if !p.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for e in std::fs::read_dir(p).map_err(io)? {
            let e = e.map_err(io)?;
            let m = e.metadata().map_err(io)?;
            out.push(RemoteEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: m.is_dir(),
                size: m.len(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn read(&self, path: &str) -> NetResult<Option<Vec<u8>>> {
        match std::fs::read(self.full(path)?) {
            Ok(d) => Ok(Some(d)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn write(&self, path: &str, data: &[u8]) -> NetResult<()> {
        let full = self.full(path)?;
        self.mkdir_all(parent(path))?;
        let tmp = full.with_extension("ghltmp");
        std::fs::write(&tmp, data).map_err(io)?;
        std::fs::rename(&tmp, &full).map_err(io)
    }

    fn delete(&self, path: &str) -> NetResult<()> {
        match std::fs::remove_file(self.full(path)?) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }

    fn mkdir_all(&self, dir: &str) -> NetResult<()> {
        std::fs::create_dir_all(self.full(dir)?).map_err(io)
    }
}
