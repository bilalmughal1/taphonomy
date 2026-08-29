//! Error types for evidence access.
//!
//! Errors preserve the path they relate to, because a recovery failure that
//! does not identify what failed is not diagnosable.
//!
//! See `docs/development/DEVELOPMENT_ENVIRONMENT.md` section 19.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// An error arising from evidence access.
#[derive(Debug)]
pub enum Error {
    /// The evidence path does not exist.
    NotFound { path: PathBuf },

    /// The evidence path exists but is not a regular file.
    NotAFile { path: PathBuf },

    /// The evidence path exists but could not be opened for reading.
    PermissionDenied { path: PathBuf },

    /// An I/O failure occurred while accessing evidence.
    Io { path: PathBuf, source: io::Error },
}

impl Error {
    /// Classifies an [`io::Error`] against a known path.
    pub(crate) fn from_io(path: &std::path::Path, source: io::Error) -> Self {
        let path = path.to_path_buf();
        match source.kind() {
            io::ErrorKind::NotFound => Error::NotFound { path },
            io::ErrorKind::PermissionDenied => Error::PermissionDenied { path },
            _ => Error::Io { path, source },
        }
    }

    /// The path this error relates to.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Error::NotFound { path }
            | Error::NotAFile { path }
            | Error::PermissionDenied { path }
            | Error::Io { path, .. } => path,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound { path } => {
                write!(f, "evidence not found: {}", path.display())
            }
            Error::NotAFile { path } => {
                write!(f, "evidence is not a regular file: {}", path.display())
            }
            Error::PermissionDenied { path } => {
                write!(f, "permission denied reading evidence: {}", path.display())
            }
            Error::Io { path, source } => {
                write!(f, "i/o failure reading {}: {}", path.display(), source)
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
