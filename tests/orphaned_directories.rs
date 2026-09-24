//! Directories no read entry names, found by the orphan search.
//!
//! `ADR-0017` Decisions A to D. The search runs inside the binary, after the
//! walk, so it is asserted against the binary. Every expected digest is of
//! content the generator wrote, recorded in EXP-0007 or computed here from
//! the bytes written, never taken from the tool's own output.
//!
//! The tests use the generated fixtures, which are not committed. Run
//! `./scripts/generate-fixtures.sh` first.

use std::path::Path;
use std::process::{Command, Output};

/// `IMG_0001.JPG`, `IMG_0002.JPG` and `IMG_0003.JPG`, as EXP-0007 recorded
/// them, with the size of each.
const FORMATTED_FILES: [(&str, u32); 3] = [
    (
        "86a773f90bcf220bea99ad333fdc1476de79daddba2ea7e017de7f302e394417",
        13,
    ),
    (
        "6aaed5ab92714c55bf157801cb012d61cfcdec8b5abcff7059722582da8c951f",
        13,
    ),
    (
        "01910a698db4916cbeb1ae6311f0d4e9e38e9e26b753db0747e0e41c002530ee",
        1300,
    ),
];

/// `OMEGA.TXT\n`, the content the generator gave the split fixture's file.
const OMEGA_DIGEST_HEX: &str = "8b42e68f21914c9afd688d68aebc725fd55c6c5754a274b0ea4814a2cfd86c08";

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

/// `ADR-0017` section 10, condition 2. A quick format leaves the tree whole
/// and names none of it, and the search finds both directories and every
/// file they list.
#[test]
fn a_formatted_tree_is_found_and_its_files_recovered() {
    let output = run(&[&fixture("fat32-formatted-tree.img"), "--recover"]);

    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(
        text.contains("orphaned directory c3, .. names c0"),
        "{text}"
    );
    assert!(
        text.contains("orphaned directory c4, .. names c3"),
        "{text}"
    );
    assert!(text.contains("orphaned content"), "{text}");
    assert!(!text.contains("deleted content"), "{text}");

    for (digest, bytes) in FORMATTED_FILES {
        let line = format!("recovered sha256 {digest} over {bytes} bytes");
        assert!(text.contains(&line), "missing {line} in: {text}");
    }

    assert!(text.contains("coverage     complete"), "{text}");
}

/// `ADR-0017` section 10, condition 3. Once the tree is recreated on the
/// same clusters, nothing names the old files and nothing is orphaned: the
/// walk reads clusters 3 and 4, and the search skips what it read.
#[test]
fn a_reused_volume_has_no_orphaned_directory() {
    let output = run(&[&fixture("fat32-formatted-reused.img"), "--recover"]);

    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(!text.contains("orphaned directory"), "{text}");
    assert!(!text.contains("recovered sha256"), "{text}");
    assert!(text.contains("coverage     complete"), "{text}");
}

/// `ADR-0017` section 10, condition 4. `TAIL` was reachable only through
/// `BIG`'s second cluster, which nothing locates, but its own first cluster
/// carries its signature. Cluster 4, `PAYLOAD.BIN`'s data, and cluster 20,
/// `BIG`'s continuation, are still never read as directories.
#[test]
fn a_directory_behind_a_lost_cluster_is_found_by_the_search() {
    let output = run(&[&fixture("fat32-deleted-split-directory.img"), "--recover"]);

    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(
        text.contains("orphaned directory c19, .. names c3"),
        "{text}"
    );

    let line = format!("recovered sha256 {OMEGA_DIGEST_HEX} over 10 bytes");
    assert!(text.contains(&line), "missing {line} in: {text}");

    for unreached in ["c4 s", "c20 s"] {
        assert!(!text.contains(unreached), "read {unreached} from: {text}");
    }
    assert!(text.contains("listings that may continue 1"), "{text}");
}

/// `ADR-0017` section 10, condition 5. The residue fixture's poke made
/// `/gone`'s root slot the terminator, so the walk never names it, and its
/// cluster, freed by `mrd` and never reused, still carries its signature.
/// It held nothing, so nothing is offered.
#[test]
fn a_directory_hidden_by_a_terminator_is_found_empty() {
    let output = run(&[&fixture("fat32-deleted-residue.img"), "--recover"]);

    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(
        text.contains("orphaned directory c5, .. names c0"),
        "{text}"
    );
    assert_eq!(text.matches("orphaned directory").count(), 1, "{text}");
    assert!(!text.contains("orphaned content"), "{text}");
}
