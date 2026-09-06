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

/// An incremental SHA-256 computation.
///
/// Bytes are supplied in as many pieces as the caller has them, and the
/// digest and the byte count come out together. A caller streaming evidence
/// it cannot hold in memory uses this; a caller holding a reader uses
/// [`hash_reader`], which is written over it.
///
/// The byte count is kept here rather than by the caller, so the number of
/// bytes reported can only ever be the number of bytes hashed.
///
/// ADR-0004 section 5 conditions 1 and 2: `sha2` is named in this module and
/// nowhere else, and none of its types appears in this one's signatures.
pub struct Sha256Hasher {
    inner: Sha256,
    bytes_read: u64,
}

impl Sha256Hasher {
    /// Starts a computation over no bytes.
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
            bytes_read: 0,
        }
    }

    /// Adds `bytes` to the computation.
    ///
    /// How the input is divided across calls does not affect the result.
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
        self.bytes_read = self
            .bytes_read
            .checked_add(bytes.len() as u64)
            .expect("byte count cannot exceed u64::MAX");
    }

    /// Finishes the computation, returning the digest and the byte count.
    pub fn finish(self) -> HashResult {
        let output = self.inner.finalize();
        let mut digest = [0u8; DIGEST_LEN];
        digest.copy_from_slice(&output);

        HashResult {
            digest: Sha256Digest(digest),
            bytes_read: self.bytes_read,
        }
    }
}

impl Default for Sha256Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Computes the SHA-256 digest of everything the reader yields.
///
/// Reads in fixed-size chunks. Memory use does not scale with input size.
///
/// `bytes_read` is counted here rather than taken from filesystem metadata,
/// because the number of bytes that were actually read is the number the
/// digest covers. A short read is not silently treated as a full one.
pub fn hash_reader<R: Read>(reader: &mut R) -> io::Result<HashResult> {
    let mut hasher = Sha256Hasher::new();
    let mut buffer = [0u8; BUFFER_LEN];

    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finish())
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

    /// Dividing the input differently must not change the result.
    ///
    /// This compares two call patterns through one implementation, so it
    /// establishes that `update` accumulates correctly and does not
    /// establish that the digest is SHA-256. `tests/nist_vectors.rs` does
    /// that, and does it through this type now that `hash_reader` is written
    /// over it.
    #[test]
    fn many_small_updates_match_one_pass() {
        let data = vec![0x5au8; BUFFER_LEN + 1234];

        let mut whole: &[u8] = &data;
        let expected = hash_reader(&mut whole).expect("hashing in one pass");

        let mut hasher = Sha256Hasher::new();
        for piece in data.chunks(7) {
            hasher.update(piece);
        }

        assert_eq!(hasher.finish(), expected);
    }

    /// A hasher given nothing is a computation over zero bytes, not an
    /// error. The digest is the published value for the empty input.
    #[test]
    fn a_hasher_given_nothing_yields_the_empty_digest() {
        let result = Sha256Hasher::new().finish();

        assert_eq!(result.bytes_read, 0);
        assert_eq!(
            result.digest.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
