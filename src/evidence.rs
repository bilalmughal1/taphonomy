//! Read-only access to evidence files.
//!
//! Every rule in `docs/SAFETY.md` sections 4 and 5 reduces to one property at
//! this layer: the handle Taphonomy holds on source evidence cannot write.
//! That is enforced here by requesting read access only, and verified in
//! `tests/read_only.rs`.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::hash::{HashResult, hash_reader};

/// An evidence file opened for reading.
///
/// The handle is opened without write, append, create, or truncate access.
/// There is no method on this type that writes.
#[derive(Debug)]
pub struct EvidenceFile {
    path: PathBuf,
    file: File,
    reported_size: u64,
}

impl EvidenceFile {
    /// Opens an evidence file read-only.
    ///
    /// Fails if the path does not exist, is not a regular file, or cannot be
    /// read. It does not create the file, and does not fall back to any other
    /// access mode.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let metadata = std::fs::metadata(path).map_err(|e| Error::from_io(path, e))?;

        if !metadata.is_file() {
            return Err(Error::NotAFile {
                path: path.to_path_buf(),
            });
        }

        // Every access flag is stated explicitly. Several match the defaults.
        // They are written out because this is the project's central safety
        // property and it should be readable as such, not inferred.
        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .append(false)
            .create(false)
            .create_new(false)
            .truncate(false)
            .open(path)
            .map_err(|e| Error::from_io(path, e))?;

        Ok(Self {
            path: path.to_path_buf(),
            file,
            reported_size: metadata.len(),
        })
    }

    /// The path this evidence was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Size in bytes as reported by filesystem metadata at open time.
    ///
    /// This is what the filesystem claimed. It is not evidence that the file
    /// can be read in full. Use [`EvidenceFile::digest`] and compare
    /// `bytes_read` to establish that.
    pub fn reported_size(&self) -> u64 {
        self.reported_size
    }

    /// Reads the file in full and returns its SHA-256 digest and byte count.
    ///
    /// Reads from the start each time it is called. Streams in fixed-size
    /// chunks, so memory use does not scale with file size.
    pub fn digest(&mut self) -> Result<HashResult, Error> {
        use std::io::{Seek, SeekFrom};

        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::from_io(&self.path, e))?;

        hash_reader(&mut self.file).map_err(|e| Error::from_io(&self.path, e))
    }
}
