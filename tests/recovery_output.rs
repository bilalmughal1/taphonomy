//! Writing a recovered artifact to a destination.
//!
//! `ADR-0015` section 13's six conditions before M10 may claim to write a
//! recovered file, and one destination failure section 13 did not list.
//! Every test runs the binary rather than calling into the crate, because
//! three of the six are decisions `main` makes about `argv` or about the
//! destination, and none of those is observable from the library.
//!
//! The fixtures are generated and not committed. Run
//! `./scripts/generate-fixtures.sh` first.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use taphonomy::Sha256Digest;
use taphonomy::hash::hash_reader;

/// Digests of the three files inside `fat32-deleted-subtree.img`, taken
/// from the content `scripts/generate-fixtures.sh` writes: each file holds
/// its own name and a newline.
const ALPHA_DIGEST_HEX: &str = "7144bb30418262f6995f3ad55b7e3147213c088fd9c65a166242a1b873cc10f3";
const BETA_DIGEST_HEX: &str = "aa58025d4c86a81175aa87ef17f1806b29a021e7483cac0b9654a6f95fa9f2c5";
const GAMMA_DIGEST_HEX: &str = "87ff853631aedb5277ccab38be53e6e8418e14d3902718a30e552964a8699ac4";

/// The content both deleted entries of `fat32-slot-collision.img` name:
/// cluster 5, which still holds `X.TXT`'s content.
const COLLISION_DIGEST_HEX: &str =
    "9c9d337b37bdea6963d8c43e1869bc9e7983d3a98f52d804e220b45057709b7d";

/// `BIG.TXT`'s content digest, as `ADR-0013` section 16.1 records it.
const BIG_DIGEST_HEX: &str = "5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a91";

/// What `FRAG.BIN` actually held, measured in `EXP-0004`.
///
/// The fragmented fixture's slot 2 does not recover this, which is the
/// point of the last test below.
const FRAG_CONTENT_HEX: &str = "a27a7e9556749147a521e0f133a9a1a84375861885f883b404c125f4719f3bfd";

fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_taphonomy"));
    command.args(args);
    command.output().expect("running the binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A generated fixture by name, so a missing one fails as a missing fixture
/// rather than deep inside a later assertion.
fn fixture(name: &str) -> String {
    let path = format!("fixtures/partition/{name}");
    assert!(
        Path::new(&path).exists(),
        "fixture missing: {path}\nrun ./scripts/generate-fixtures.sh first"
    );
    path
}

/// An empty directory of this test's own, removed first so a previous run
/// cannot make this one pass or fail.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("taphonomy-output-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

/// The digest of a file on disk, computed without the tool's reporting.
fn digest_of_file(path: &Path) -> Sha256Digest {
    let mut file = fs::File::open(path).expect("opening the written file");
    hash_reader(&mut file)
        .expect("hashing the written file")
        .digest
}

fn written_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("reading the destination")
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    paths.sort();
    paths
}

/// Condition 1. The file on disk is the digest the run reported.
///
/// `ADR-0015` Decision H. The run says it read back what it wrote; this
/// hashes the file independently of that claim and compares against the
/// digest the run printed, which is the only assertion that does not take
/// the tool's word for its own output.
#[test]
fn a_written_artifact_carries_the_digest_the_run_reported() {
    let dir = scratch("digest");
    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    assert!(
        text.contains("matches what was read"),
        "the run did not verify its own write: {text}"
    );

    let written = written_files(&dir);
    assert_eq!(written.len(), 2, "two deleted files recover here");

    let big = written
        .iter()
        .find(|path| path.ends_with("c2-s1-first-3.bin"))
        .expect("BIG.TXT's artifact");

    assert_eq!(digest_of_file(big).to_string(), BIG_DIGEST_HEX);
    assert!(text.contains(BIG_DIGEST_HEX), "{text}");
}

/// Condition 2. A destination holding the evidence stops the run.
///
/// `ADR-0015` Decision B as Appendix A.3 corrects it. The check runs before
/// the evidence is opened, so this asserts the argument exit status rather
/// than a file error.
#[test]
fn a_destination_holding_the_evidence_is_refused() {
    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--output",
        "fixtures/partition",
    ]);

    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("holds the evidence"),
        "{}",
        stderr(&output)
    );
}

/// Condition 3. An existing file is preserved and the run continues.
///
/// `SAFETY.md` section 15: the default behaviour preserves existing output.
/// The digest is still reported, because `ADR-0015` section 9 keeps a
/// finding about the evidence separate from a failure at the destination.
#[test]
fn an_existing_artifact_is_preserved_and_the_run_continues() {
    let dir = scratch("exists");
    let taken = dir.join("c2-s1-first-3.bin");
    fs::write(&taken, b"not this tool's").expect("planting a file");

    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    assert!(text.contains("NOT WRITTEN"), "{text}");
    assert!(text.contains("artifacts not written"), "{text}");

    // Untouched.
    assert_eq!(
        fs::read(&taken).expect("reading the planted file"),
        b"not this tool's"
    );

    // The run carried on: the second entry was written, and both digests
    // were still reported.
    assert!(dir.join("c2-s4-first-9.bin").exists(), "{text}");
    assert!(text.contains(BIG_DIGEST_HEX), "{text}");
}

/// Condition 4. The file holds the declared size, and no slack.
///
/// `ADR-0015` Decision F. `BIG.TXT` declares 1,600 bytes in a volume of
/// 512-byte clusters, so a file carrying its run's slack would be 2,048.
#[test]
fn a_written_artifact_holds_the_declared_size_without_slack() {
    let dir = scratch("slack");
    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let big = fs::metadata(dir.join("c2-s1-first-3.bin")).expect("the artifact");
    assert_eq!(big.len(), 1_600, "slack would make this 2048");

    let small = fs::metadata(dir.join("c2-s4-first-9.bin")).expect("the artifact");
    assert_eq!(small.len(), 29, "slack would make this 512");
}

/// Condition 5. A refused entry writes nothing.
///
/// On `fat32-fragmented-live-gap.img` slot 2's implied run reaches cluster
/// 5, which the live `S2.BIN` holds, so `ADR-0010` Decision C refuses it
/// before any content is read. Two other entries recover, so this asserts
/// which files appear rather than that none do.
#[test]
fn a_refused_entry_writes_no_artifact() {
    let dir = scratch("refused");
    let output = run(&[
        &fixture("fat32-fragmented-live-gap.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    assert!(text.contains("REFUSED: cluster 5 is in use"), "{text}");

    for path in written_files(&dir) {
        assert!(
            !path.ends_with("c2-s2-first-4.bin"),
            "the refused entry produced a file: {}",
            path.display()
        );
    }
}

/// Condition 6. The tool writes a wrong file, correctly.
///
/// `CLAUDE.md` section 26 requires incorrect recovery to be measured.
/// `FRAG.BIN` occupied clusters 4, 6 and 8; its entry implies 4 to 6, all
/// of which read as free on this fixture. So an artifact is written, its
/// digest matches what was read, and its content is not the file's.
///
/// This is the project's characteristic false positive with a file on disk
/// behind it, which is the form an operator would actually be handed.
#[test]
fn a_fragmented_file_is_written_whole_and_is_not_the_file() {
    let dir = scratch("fragmented");
    let output = run(&[
        &fixture("fat32-fragmented-deleted.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let artifact = dir.join("c2-s2-first-4.bin");
    assert!(artifact.exists(), "{}", stdout(&output));

    let written = digest_of_file(&artifact);

    // The write is faithful to what was read.
    assert!(
        stdout(&output).contains("matches what was read"),
        "{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains(&written.to_string()));

    // And what was read is not the file.
    assert_ne!(
        written.to_string(),
        FRAG_CONTENT_HEX,
        "the fragmented file recovered correctly, which EXP-0004 says it \
         cannot"
    );

    // Whole, and wrong: the declared size was written, so nothing about the
    // artifact's size tells an operator it is the wrong content.
    let size = fs::metadata(&artifact).expect("the artifact").len();
    assert_eq!(size, 1_536);
}

/// A destination that cannot be written reports each artifact as not
/// created, and claims no partial file.
///
/// Not one of section 13's conditions. `ADR-0015` Decision G distinguishes
/// the failures, and before `7c127a2` this run printed "partial file left"
/// for files that were never created. The digest still stands, because the
/// destination has no say in what the evidence holds.
///
/// Unix-specific because it relies on permission bits, as
/// `tests/read_only.rs` does. The control assertion below proves the
/// directory genuinely rejects a new file, so a pass cannot come from a
/// permission change that did nothing, which is what running as root
/// would produce.
#[cfg(unix)]
#[test]
fn an_unwritable_destination_is_reported_without_a_partial_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch("unwritable");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555))
        .expect("making the destination read-only");

    let control = fs::File::create(dir.join("probe"));
    let accepted = control.is_ok();
    if accepted {
        let _ = fs::remove_file(dir.join("probe"));
    }

    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    // Restored before any assertion, so a failure cannot leave behind a
    // directory the next run's scratch() is unable to remove.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
        .expect("restoring the destination");

    assert!(
        !accepted,
        "control failed: the directory accepted a new file, so this test proves nothing"
    );

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    assert!(text.contains("could not be created"), "{text}");
    assert!(!text.contains("partial file left"), "{text}");
    assert!(text.contains("artifacts not written"), "{text}");
    assert!(text.contains(BIG_DIGEST_HEX), "{text}");
    assert!(written_files(&dir).is_empty());
}

/// `ADR-0016` section 11, conditions 2 and 4. Three files inside a deleted
/// subtree are recovered, at depths one and two, and each digest is the
/// digest of the content the generator wrote, so a wrong run is caught by a
/// value computed outside this tool.
///
/// The dot entries are listed and never followed: a path never re-enters
/// the directory it came from, and nothing is refused.
#[test]
fn the_files_inside_a_deleted_subtree_are_recovered() {
    let output = run(&[&fixture("fat32-deleted-subtree.img"), "--recover"]);

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    for digest in [ALPHA_DIGEST_HEX, BETA_DIGEST_HEX, GAMMA_DIGEST_HEX] {
        assert!(text.contains(digest), "{digest} missing from: {text}");
    }
    assert!(text.contains("deleted directory /?ONE"), "{text}");
    assert!(text.contains("deleted directory /?ONE/?EEP"), "{text}");
    assert!(!text.contains("directory /."), "{text}");
    assert!(!text.contains("/?ONE/."), "{text}");
    assert!(!text.contains("NOT READ"), "{text}");
    assert!(text.contains("coverage     complete"), "{text}");
}

/// `ADR-0016` Decision F and section 11 condition 7. Two entries at the
/// same slot of different directories, naming the same first cluster, are
/// written as two files.
///
/// In `fat32-slot-collision.img` both are slot 2, one in cluster 3 and one
/// in cluster 4, and both name cluster 5. Under `ADR-0015` Decision D both
/// were `slot-2-cluster-5.bin`, so the second was reported as a file that
/// already existed. The entry's own cluster in the name is what separates
/// them, and both hold the same content because both name the same run.
#[test]
fn two_entries_at_the_same_slot_of_different_directories_are_two_files() {
    let dir = scratch("collision");
    let output = run(&[
        &fixture("fat32-slot-collision.img"),
        "--recover",
        "--output",
        dir.to_str().expect("a utf-8 scratch path"),
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let text = stdout(&output);
    assert!(!text.contains("exists"), "{text}");

    let written = written_files(&dir);
    assert_eq!(written.len(), 2, "wrote {written:?}");
    for name in ["c3-s2-first-5.bin", "c4-s2-first-5.bin"] {
        let path = dir.join(name);
        assert!(path.exists(), "{name} missing from {written:?}");
        assert_eq!(digest_of_file(&path).to_string(), COLLISION_DIGEST_HEX);
    }
}
