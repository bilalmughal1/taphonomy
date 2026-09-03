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

    /// Reads exactly `buf.len()` bytes starting at `offset`.
    ///
    /// Fails if the evidence ends before the buffer is filled. A short read
    /// is never silently treated as a complete one.
    pub fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error> {
        use std::io::{Read, Seek, SeekFrom};

        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Error::from_io(&self.path, e))?;

        self.file
            .read_exact(buf)
            .map_err(|e| Error::from_io(&self.path, e))
    }
}

/// Positional read access to evidence.
///
/// One method, because that is all a parser needs: an offset and a buffer,
/// filled or failed. There is no cursor to leave in the wrong place between
/// calls, and no method that writes.
///
/// The receiver is `&mut self` because the implementation below seeks before
/// reading. That is invisible across this boundary — a caller cannot observe
/// a cursor, move one, or leave one anywhere — but it does mean one reader
/// cannot be shared between two callers. ADR-0008 section 3 records why the
/// `pread` alternative was rejected, and section 10 the condition under
/// which it should be revisited.
///
/// Implementors must fail rather than pad when the evidence ends before the
/// buffer is full. A short read reported as a complete one produces zeroed
/// structures that parse.
pub trait EvidenceReader {
    /// Reads exactly `buf.len()` bytes starting at `offset`.
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error>;
}

impl EvidenceReader for EvidenceFile {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error> {
        // Fully qualified deliberately. An unqualified call resolves to this
        // trait method rather than the inherent one and recurses until the
        // stack is exhausted.
        EvidenceFile::read_exact_at(self, offset, buf)
    }
}
