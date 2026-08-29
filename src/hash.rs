//! Cryptographic hashing of evidence.
//!
//! This module is the only place in Taphonomy that names the `sha2` crate.
//! ADR-0004 section 5 conditions 1 and 2 require that the implementation stay
//! behind a project-owned type so it can be replaced without changes
//! elsewhere.
//!
//! Correctness of the dependency is verified against NIST FIPS 180-4 test
//! vectors in `tests/nist_vectors.rs`, per ADR-0004 section 5 condition 5.

use std::fmt;
use std::io::{self, Read};

use sha2::{Digest, Sha256};

/// Length of a SHA-256 digest in bytes.
pub const DIGEST_LEN: usize = 32;

/// Size of the read buffer used when hashing.
///
/// Evidence images may be larger than available memory. Hashing streams
/// through a fixed buffer and never loads an image in full.
const BUFFER_LEN: usize = 64 * 1024;

/// A SHA-256 digest.
///
/// This is a project-owned type. It does not expose the underlying
/// implementation's types to callers.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; DIGEST_LEN]);

impl Sha256Digest {
    /// Constructs a digest from raw bytes.
    ///
    /// Intended for test vectors and for reading recorded digests. It does
    /// not compute anything.
    pub const fn from_bytes(bytes: [u8; DIGEST_LEN]) -> Self {
        Self(bytes)
    }

    /// The digest as raw bytes.
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// The digest as a lowercase hexadecimal string.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(DIGEST_LEN * 2);
        for byte in self.0 {
            fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"))
                .expect("writing to a String cannot fail");
        }
        out
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Digest({})", self.to_hex())
    }
}

/// The result of hashing a byte stream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HashResult {
    /// Digest of every byte read.
    pub digest: Sha256Digest,
    /// Number of bytes actually read.
    pub bytes_read: u64,
}

/// Computes the SHA-256 digest of everything the reader yields.
///
/// Reads in fixed-size chunks. Memory use does not scale with input size.
///
/// `bytes_read` is counted here rather than taken from filesystem metadata,
/// because the number of bytes that were actually read is the number the
/// digest covers. A short read is not silently treated as a full one.
pub fn hash_reader<R: Read>(reader: &mut R) -> io::Result<HashResult> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; BUFFER_LEN];
    let mut bytes_read: u64 = 0;

    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        hasher.update(&buffer[..n]);
        bytes_read = bytes_read
            .checked_add(n as u64)
            .expect("byte count cannot exceed u64::MAX");
    }

    let output = hasher.finalize();
    let mut digest = [0u8; DIGEST_LEN];
    digest.copy_from_slice(&output);

    Ok(HashResult {
        digest: Sha256Digest(digest),
        bytes_read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_full_length() {
        let digest = Sha256Digest::from_bytes([0xab; DIGEST_LEN]);
        let hex = digest.to_hex();
        assert_eq!(hex.len(), DIGEST_LEN * 2);
        assert_eq!(hex, "ab".repeat(DIGEST_LEN));
    }

    #[test]
    fn empty_input_yields_zero_bytes_read() {
        let mut input: &[u8] = b"";
        let result = hash_reader(&mut input).expect("hashing empty input");
        assert_eq!(result.bytes_read, 0);
    }

    #[test]
    fn bytes_read_matches_input_length() {
        let data = vec![0u8; BUFFER_LEN * 2 + 17];
        let mut input: &[u8] = &data;
        let result = hash_reader(&mut input).expect("hashing multi-chunk input");
        assert_eq!(result.bytes_read, data.len() as u64);
    }
}
