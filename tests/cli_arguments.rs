//! Argument handling at the command line.
//!
//! `ADR-0013` section 16 requires that a reference supplied without
//! `--recover` and a reference that cannot be parsed each exit non-zero
//! before any evidence is opened. Neither is observable from the library:
//! both are decisions `main` makes about `argv`, so this is the first suite
//! in the tree that runs the binary rather than calling into the crate.
//!
//! Where a check is supposed to precede opening the evidence, the test
//! passes a path that cannot exist. An argument error rather than a file
//! error is then the proof of ordering, which asserting on the message
//! alone would not give.
//!
//! The tests that need content read use the generated fixtures, which are
//! not committed. Run `./scripts/generate-fixtures.sh` first.

use std::path::Path;
use std::process::{Command, Output};

/// A path no evidence can be at, for the tests that prove a check runs
/// before the evidence is opened.
const ABSENT: &str = "/taphonomy-no-such-evidence.img";

/// `BIG.TXT`'s content digest, as recorded in `ADR-0013` section 16.1.
const BIG_DIGEST_HEX: &str = "5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a91";

/// The same digest with its final character changed.
const WRONG_DIGEST_HEX: &str = "5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a92";

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
/// rather than as an evidence error deep in a later assertion.
fn fixture(name: &str) -> String {
    let path = format!("fixtures/partition/{name}");
    assert!(
        Path::new(&path).exists(),
        "fixture missing: {path}\nrun ./scripts/generate-fixtures.sh first"
    );
    path
}

fn recovery_fixture() -> &'static str {
    let path = "fixtures/partition/fat32-recover-run.img";
    assert!(
        Path::new(path).exists(),
        "fixture missing: {path}\nrun ./scripts/generate-fixtures.sh first"
    );
    path
}

#[test]
fn no_arguments_is_an_argument_error() {
    let output = run(&[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage: taphonomy"));
    assert!(stdout(&output).is_empty());
}

#[test]
fn an_unexpected_argument_is_an_argument_error() {
    let output = run(&[ABSENT, "--recover", "--wat"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unexpected argument"));
    assert!(stdout(&output).is_empty());
}

/// `ADR-0013` Decision H. A reference with no content to compare against is
/// an error rather than an implied `--recover`, because implying it would
/// read a deleted file's content without being asked to.
#[test]
fn a_reference_without_recover_is_refused_before_the_evidence_is_opened() {
    let output = run(&[ABSENT, "--reference-digest", BIG_DIGEST_HEX]);

    assert_eq!(output.status.code(), Some(2));

    let stderr = stderr(&output);
    assert!(
        stderr.contains("--reference-digest needs --recover"),
        "expected the Decision H error, instead got: {stderr}"
    );
    assert!(
        !stderr.contains(ABSENT),
        "the evidence was opened: {stderr}"
    );
    assert!(stdout(&output).is_empty());
}

#[test]
fn a_reference_digest_with_no_value_is_an_argument_error() {
    let output = run(&[ABSENT, "--recover", "--reference-digest"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("requires a digest"));
    assert!(stdout(&output).is_empty());
}

/// `ADR-0013` section 3.4. A reference that cannot be read is the one thing
/// M8 fails closed on, and it stops the run before evidence is touched.
#[test]
fn a_malformed_reference_is_refused_before_the_evidence_is_opened() {
    let output = run(&[ABSENT, "--recover", "--reference-digest", "5ecddc87"]);

    assert_eq!(output.status.code(), Some(2));

    let stderr = stderr(&output);
    assert!(
        stderr.contains("64 hexadecimal characters"),
        "expected the parse error to name the length, instead got: {stderr}"
    );
    assert!(
        !stderr.contains(ABSENT),
        "the evidence was opened: {stderr}"
    );
    assert!(stdout(&output).is_empty());
}

/// Flag order does not decide it. The check runs once the whole of `argv`
/// has been read, so a reference before `--recover` is accepted.
#[test]
fn a_reference_before_recover_is_accepted() {
    let output = run(&[
        recovery_fixture(),
        "--reference-digest",
        BIG_DIGEST_HEX,
        "--recover",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("MATCHES"));
}

/// `docs/SAFETY.md` section 10 requires the recovered-file hash and the
/// validation hash to be distinguishable wherever both are reported. That is
/// a property of the output, so it is asserted against the output.
#[test]
fn a_matching_reference_is_reported_as_a_match() {
    let output = run(&[
        recovery_fixture(),
        "--recover",
        "--reference-digest",
        BIG_DIGEST_HEX,
    ]);

    assert_eq!(output.status.code(), Some(0));

    let stdout = stdout(&output);
    assert!(stdout.contains(&format!(
        "recovered sha256 {BIG_DIGEST_HEX} over 1600 bytes"
    )));
    assert!(stdout.contains(&format!("reference sha256 {BIG_DIGEST_HEX} MATCHES")));
}

/// `ADR-0013` section 3. A differing digest is a finding about the evidence
/// and not a failed operation, so the run continues and the status is zero.
/// `SAFETY.md` section 12's fail-closed list concerns an operation meeting
/// uncertainty; the read here succeeded and its result is exact.
#[test]
fn a_differing_reference_is_a_finding_and_not_a_failure() {
    let output = run(&[
        recovery_fixture(),
        "--recover",
        "--reference-digest",
        WRONG_DIGEST_HEX,
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "a mismatch must not stop the run"
    );

    let stdout = stdout(&output);
    assert!(stdout.contains(&format!("reference sha256 {WRONG_DIGEST_HEX} DIFFERS")));
    assert!(stdout.contains(&format!(
        "recovered sha256 {BIG_DIGEST_HEX} over 1600 bytes"
    )));
    assert!(stdout.contains("A differing digest does not say which of these"));
}

/// `ADR-0014` Appendix B.10. A run that analysed nothing past the evidence
/// digest exits non-zero. Appendix A.4 measured this image reporting its
/// error on stderr, printing nothing on stdout that said so, and exiting
/// zero, which `SAFETY.md` section 12 forbids: a failure must never be
/// converted silently into a partial success.
#[test]
fn a_rejected_partition_table_analyses_nothing_and_exits_three() {
    let output = run(&[&fixture("bad-signature.img")]);

    assert_eq!(
        output.status.code(),
        Some(3),
        "a run that analysed nothing must not exit zero"
    );

    let stdout = stdout(&output);
    assert!(
        stdout.contains("coverage     none"),
        "expected the coverage line on stdout, instead got: {stdout}"
    );
    assert!(stdout.contains("partition table not parsed"));
}

/// The control of `ADR-0014` Appendix A.6. An image whose table declares no
/// partition analysed everything there was to analyse, so it is covered
/// completely and exits zero. Under Appendix A.8's rule this image and the
/// one above reported the same thing.
#[test]
fn an_image_declaring_no_partition_is_covered_completely_and_exits_zero() {
    let output = run(&[&fixture("mbr-empty.img")]);

    assert_eq!(output.status.code(), Some(0));

    let stdout = stdout(&output);
    assert!(
        stdout.contains("coverage     complete"),
        "expected complete coverage, instead got: {stdout}"
    );
}

/// `ADR-0014` Appendix B.10. Incomplete coverage exits zero. While
/// subdirectories are not read this is the ordinary state of any volume
/// holding one, so a non-zero code here would fire on almost every run and
/// be learned as noise. The gap is reported on stdout instead.
#[test]
fn a_directory_that_was_not_read_leaves_coverage_incomplete_and_exits_zero() {
    let output = run(&[recovery_fixture()]);

    assert_eq!(output.status.code(), Some(0));

    let stdout = stdout(&output);
    assert!(
        stdout.contains("coverage     incomplete"),
        "expected incomplete coverage, instead got: {stdout}"
    );
    assert!(stdout.contains("directories not read"));
}
