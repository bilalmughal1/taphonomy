//! What a run reports about its own coverage.
//!
//! `ADR-0014` Appendix B.5 derives a coverage status from what the run did
//! not analyse, and Appendix B.10 exits non-zero where it analysed nothing.
//! Neither is observable from the library: both are decisions the binary
//! makes once, over a whole run, so they are asserted against the binary.
//!
//! `tests/cli_arguments.rs` covers the three states through the exit status.
//! This suite covers what the block says, the kinds of gap that reach it,
//! and the one gap no fixture holds.
//!
//! The tests use the generated fixtures, which are not committed. Run
//! `./scripts/generate-fixtures.sh` first.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// `BIG.TXT`'s content digest, as recorded in `ADR-0013` section 16.1.
const BIG_DIGEST_HEX: &str = "5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a91";

/// The SHA-256 of no bytes, which an empty evidence file hashes to.
const EMPTY_SHA256_HEX: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_taphonomy"));
    command.args(args);
    command.output().expect("running the binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A generated fixture by name, so a missing one fails as a missing fixture
/// rather than as an evidence error deep in a later assertion.
fn fixture(name: &str) -> String {
    let path = format!("fixtures/partition/{name}");
    assert!(
        Path::new(&path).exists(),
        "fixture missing: {path}\nrun ./scripts/generate-fixtures.sh first"
    );
    path
}

/// The summary block, from the coverage line to the end of the run.
///
/// Asserting on the whole of stdout would pass on a line printed anywhere,
/// including among the entries, which is what Decision B forbids.
fn summary(output: &Output) -> String {
    let stdout = stdout(output);
    let start = stdout
        .find("\ncoverage     ")
        .expect("the run printed no coverage line");

    stdout[start + 1..].to_string()
}

#[test]
fn a_run_that_analysed_nothing_says_so_and_exits_three() {
    // Three different ways to reach it: a GPT disk, a volume whose boot
    // sector was refused, and four partitions holding nothing identifiable.
    // Each states its own kind, because `CLAUDE.md` section 14 requires an
    // unsupported format, unreadable metadata and an absence to be told
    // apart rather than counted together.
    let cases = [
        ("gpt-protective.img", "GPT not analysed           1"),
        ("fat32-bad-root-cluster.img", "boot sectors rejected      1"),
        ("mbr-four-partitions.img", "filesystems not identified 4"),
    ];

    for (name, gap) in cases {
        let output = run(&[&fixture(name)]);

        assert_eq!(output.status.code(), Some(3), "{name} did not exit 3");

        let summary = summary(&output);
        assert!(
            summary.contains("coverage     none"),
            "{name} reported: {summary}"
        );
        assert!(summary.contains(gap), "{name} reported: {summary}");
        assert!(summary.contains("volumes analysed           0"));
    }
}

/// `ADR-0014` Appendix B.3. Where sector 0 cannot be read the table was
/// never read, so it was never parsed. No fixture holds this and none needs
/// to: an empty file reaches it, and the run before M9 hashed that file,
/// printed nothing on stdout about the failure, and exited zero.
#[test]
fn an_unreadable_first_sector_is_reported_without_a_fixture() {
    let name = format!("taphonomy-empty-{}.img", std::process::id());
    let path: PathBuf = std::env::temp_dir().join(name);
    fs::write(&path, b"").expect("writing an empty file");

    let output = run(&[&path.to_string_lossy()]);
    fs::remove_file(&path).expect("removing the empty file");

    let summary = summary(&output);

    assert_eq!(output.status.code(), Some(3));
    assert!(summary.contains("coverage     none"), "reported: {summary}");
    assert!(
        summary.contains("partition table not read"),
        "reported: {summary}"
    );

    // The digest still stands on its own: what succeeded is still reported.
    let digest = format!("sha256       {EMPTY_SHA256_HEX}");
    assert!(stdout(&output).contains(&digest), "reported: {digest}");
}

/// `ADR-0016` Decision A. A live directory is entered, and once its
/// contents are read it is no gap. `fat32-root-entries.img` holds `/logs`,
/// which is empty; before subdirectories were read this run reported it as
/// the volume's one gap.
#[test]
fn a_live_directory_is_entered_and_leaves_no_gap() {
    let output = run(&[&fixture("fat32-root-entries.img")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("\n    directory /"));

    let summary = summary(&output);
    assert!(
        summary.contains("coverage     complete"),
        "reported: {summary}"
    );
    assert!(
        !summary.contains("directories not read"),
        "reported: {summary}"
    );
}

/// `ADR-0013` Appendix C.4. An operator asking whether a file is present
/// needs to know which questions went unanswered, and a refusal and an
/// unread run are different answers. `fat32-recover-collision.img` holds
/// one of each: `BIG.TXT`'s run reaches a cluster `LIVE1.TXT` holds, and
/// `SMALL.TXT`'s run is free and was not asked for.
#[test]
fn a_refusal_and_an_unread_run_are_counted_apart() {
    let output = run(&[&fixture("fat32-recover-collision.img")]);

    let summary = summary(&output);
    assert!(
        summary.contains("unrecovered  1 refused, 1 not read"),
        "reported: {summary}"
    );

    // Nothing was read, so nothing was produced to report a level for.
    assert!(!summary.contains("artifacts"), "reported: {summary}");
}

/// The same volume with `--recover`. The unread run becomes an artifact and
/// leaves the line; the refusal stays, because reading was never the thing
/// that stopped it.
#[test]
fn reading_content_leaves_only_what_refused_to_be_read() {
    let output = run(&[&fixture("fat32-recover-collision.img"), "--recover"]);

    // The whole line, so that a run which also left something unread would
    // fail here. A bare substring would pass on "1 refused, 1 not read",
    // and the gap line above it contains "not read" in any case.
    let summary = summary(&output);
    assert!(
        summary.contains("artifacts    1 RECONSTRUCTED"),
        "reported: {summary}"
    );
    assert!(
        summary.contains("unrecovered  1 refused\n"),
        "reported: {summary}"
    );
}

/// `ADR-0014` Decision B and Appendix A.10. The level belongs to a
/// reconstructed artifact, so it appears once, on the line that counts
/// them, and never against an entry that produced none.
#[test]
fn the_confidence_level_appears_once_and_only_where_artifacts_are_counted() {
    let path = fixture("fat32-fragmented-deleted.img");

    let recovered = stdout(&run(&[&path, "--recover"]));
    assert_eq!(
        recovered.matches("RECONSTRUCTED").count(),
        1,
        "the level was printed more than once: {recovered}"
    );

    // Without `--recover` nothing is reconstructed, so the word does not
    // appear at all rather than appearing with a count of zero.
    let reported = stdout(&run(&[&path]));
    assert_eq!(reported.matches("RECONSTRUCTED").count(), 0);
}

/// Coverage is about reach, not about correctness.
///
/// EXP-0004 measured this image recovering, for three of its five deleted
/// entries, content that is not that entry's file: the runs are contiguous
/// and the files were fragmented. The run analysed everything it could
/// reach, so its coverage is complete, and saying so is correct. A status
/// word implying the recoveries are right is what `ADR-0014` Appendix B.6
/// rejected `SUCCESS` for.
#[test]
fn complete_coverage_does_not_claim_the_recoveries_are_right() {
    let output = run(&[&fixture("fat32-fragmented-deleted.img"), "--recover"]);

    assert_eq!(output.status.code(), Some(0));

    let summary = summary(&output);
    assert!(
        summary.contains("coverage     complete"),
        "reported: {summary}"
    );
    assert!(summary.contains("artifacts    5 RECONSTRUCTED"));

    // The caveat that this run cannot be read as a finding is still
    // attached: `ADR-0003` section 4.2 and `ADR-0010` Decision C.
    assert!(summary.contains("A free run means nothing has claimed those"));
}

/// `ADR-0013` Decision I. One reference is compared against every
/// extraction the run performs, so the line counts both answers rather than
/// reporting the run as matching or not.
#[test]
fn a_reference_is_counted_against_every_extraction() {
    let output = run(&[
        &fixture("fat32-recover-run.img"),
        "--recover",
        "--reference-digest",
        BIG_DIGEST_HEX,
    ]);

    assert_eq!(output.status.code(), Some(0));

    let summary = summary(&output);
    assert!(
        summary.contains("compared     1 matched, 1 differed"),
        "reported: {summary}"
    );
}
