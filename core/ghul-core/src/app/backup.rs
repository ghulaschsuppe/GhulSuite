// SPDX-License-Identifier: AGPL-3.0-or-later
//! Complete backup of a suite: all files of the folder in one ZIP. The files are encrypted
//! containers, so the backup is as safe as the suite itself (password + key or recovery code).

use std::io::{Read, Write};

use super::error::{GhulError, Result};
use super::suite::GhulSuite;

const README: &str = "GhulSuite-Sicherung\r\n\r\nAlle Dateien sind verschluesselt. Wiederherstellen: GhulSuite -> Vorhandene Suite oeffnen -> \
                      Sicherung wiederherstellen, dann mit Passwort + Key oder Wiederherstellungscode entsperren.\r\n";
const MAX_BACKUP: u64 = 512 * 1024 * 1024;

fn suite_file(name: &str) -> bool {
    !name.ends_with(".tmp")
        && !name.ends_with(".bak")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.starts_with('.')
}

#[uniffi::export]
impl GhulSuite {
    /// All files of the suite folder as one ZIP (only while unlocked, so nobody else makes one).
    pub fn suite_backup(&self) -> Result<Vec<u8>> {
        self.with_open(|_| Ok(()))?;
        let names: Vec<String> = self
            .storage
            .list()
            .map_err(|e| GhulError::Storage { msg: e.to_string() })?
            .into_iter()
            .filter(|n| suite_file(n))
            .collect();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let err = |e: zip::result::ZipError| GhulError::Storage { msg: e.to_string() };
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("LIESMICH.txt", opts).map_err(err)?;
            z.write_all(README.as_bytes()).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            for n in &names {
                let Some(data) = self.storage.read(n.clone()).map_err(|e| GhulError::Storage { msg: e.to_string() })?
                else {
                    continue;
                };
                z.start_file(n.as_str(), opts).map_err(err)?;
                z.write_all(&data).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            }
            z.finish().map_err(err)?;
        }
        Ok(out.into_inner())
    }

    /// Writes the files of a backup into the (empty) suite folder. Afterwards unlock as usual.
    /// Returns the number of files restored.
    pub fn restore_backup(&self, data: Vec<u8>) -> Result<u32> {
        if self.exists()? {
            return Err(GhulError::InvalidInput { msg: "the folder already contains a suite".into() });
        }
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|_| GhulError::InvalidInput { msg: "not a GhulSuite backup".into() })?;
        let mut files = Vec::new();
        let mut total = 0u64;
        for i in 0..z.len() {
            let mut f = z.by_index(i).map_err(|e| GhulError::InvalidInput { msg: e.to_string() })?;
            let name: String = f.name().map_err(|e| GhulError::InvalidInput { msg: e.to_string() })?.to_string();
            if name == "LIESMICH.txt" || !suite_file(&name) || f.is_dir() {
                continue;
            }
            total += f.size();
            if total > MAX_BACKUP {
                return Err(GhulError::InvalidInput { msg: "backup too large".into() });
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
            files.push((name, buf));
        }
        if !files.iter().any(|(n, _)| n == super::suite::SUITE_FILE) {
            return Err(GhulError::InvalidInput { msg: "not a GhulSuite backup".into() });
        }
        for (n, d) in &files {
            self.storage.write(n.clone(), d.clone()).map_err(|e| GhulError::Storage { msg: e.to_string() })?;
        }
        Ok(files.len() as u32)
    }
}
