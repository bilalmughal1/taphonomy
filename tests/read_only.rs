//! Evidence immutability tests.
//!
//! `docs/SAFETY.md` sections 4 and 5 require that source evidence is treated
//! as immutable and opened read-only. This file establishes that by
//! observation rather than by inspecting the code.
//!
//! These tests are Unix-specific because they rely on file permission bits.
//! Windows behaviour must be established separately before any Windows
//! support is claimed.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use taphonomy::EvidenceFile;

/// A fixture file removed when the test ends, including on panic.
struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(name: &str, contents: &[u8]) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "taphonomy-test-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("system clock is after the unix epoch")
                .as_nanos()
        ));

        let mut file = fs::File::create(&path).expect("creating fixture");
        file.write_all(contents).expect("writing fixture");
        file.sync_all().expect("flushing fixture");

        Self { path }
    }

    fn set_mode(&self, mode: u32) {
        fs::set_permissions(&self.path, fs::Permissions::from_mode(mode))
            .expect("setting fixture permissions");
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600));
        let _ = fs::remove_file(&self.path);
    }
}

/// The central safety test.
///
/// The fixture is made read-only at the filesystem level. Any attempt to open
/// it with write access fails with `EACCES`. If `EvidenceFile::open` succeeds
/// here, it did not request write access.
///
/// The control assertion below proves the fixture genuinely rejects writes,
/// so a pass cannot come from a permission change that silently did nothing.
#[test]
fn opening_a_read_only_file_succeeds() {
    let fixture = Fixture::new("readonly", b"evidence bytes");
    fixture.set_mode(0o444);

    let write_attempt = fs::OpenOptions::new().write(true).open(fixture.path());
    assert!(
        write_attempt.is_err(),
        "control failed: fixture accepted a write handle, so this test proves nothing"
    );

    let evidence = EvidenceFile::open(fixture.path());
    assert!(
        evidence.is_ok(),
        "opening read-only evidence failed: {:?}",
        evidence.err()
    );
}

/// Hashing must not alter the source.
///
/// Compares size, modification time, and full contents before and after.
#[test]
fn hashing_does_not_modify_the_source() {
    let contents = b"the quick brown fox jumps over the lazy dog";
    let fixture = Fixture::new("unmodified", contents);

    let before_meta = fs::metadata(fixture.path()).expect("metadata before");
    let before_mtime = before_meta.modified().expect("mtime before");
    let before_bytes = fs::read(fixture.path()).expect("contents before");

    let mut evidence = EvidenceFile::open(fixture.path()).expect("opening evidence");
    let result = evidence.digest().expect("hashing evidence");
    assert_eq!(result.bytes_read, contents.len() as u64);

    let after_meta = fs::metadata(fixture.path()).expect("metadata after");
    let after_bytes = fs::read(fixture.path()).expect("contents after");

    assert_eq!(before_meta.len(), after_meta.len(), "size changed");
    assert_eq!(
        before_mtime,
        after_meta.modified().expect("mtime after"),
        "modification time changed"
    );
    assert_eq!(before_bytes, after_bytes, "contents changed");
}

/// Repeated hashing of unchanged evidence must produce the same digest.
///
/// `PROJECT.md` section 6.4 requires deterministic behaviour.
#[test]
fn digest_is_repeatable() {
    let fixture = Fixture::new("repeatable", b"deterministic evidence");
    let mut evidence = EvidenceFile::open(fixture.path()).expect("opening evidence");

    let first = evidence.digest().expect("first digest");
    let second = evidence.digest().expect("second digest");

    assert_eq!(first, second, "digest is not repeatable");
}

/// A directory is not evidence. Fail closed rather than producing a
/// meaningless result. See `docs/SAFETY.md` section 12.
#[test]
fn a_directory_is_rejected() {
    let dir = std::env::temp_dir();
    let result = EvidenceFile::open(&dir);
    assert!(result.is_err(), "a directory was accepted as evidence");
}

/// A missing path must fail, never be created.
#[test]
fn a_missing_path_is_not_created() {
    let mut path = std::env::temp_dir();
    path.push(format!("taphonomy-absent-{}", std::process::id()));
    let _ = fs::remove_file(&path);

    let result = EvidenceFile::open(&path);
    assert!(result.is_err(), "a missing path was accepted");
    assert!(!path.exists(), "opening a missing path created it");
}

/// An empty file is valid evidence of zero bytes, not an error.
#[test]
fn an_empty_file_hashes_to_the_empty_digest() {
    let fixture = Fixture::new("empty", b"");
    let mut evidence = EvidenceFile::open(fixture.path()).expect("opening empty evidence");

    let result = evidence.digest().expect("hashing empty evidence");

    assert_eq!(result.bytes_read, 0);
    assert_eq!(
        result.digest.to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
