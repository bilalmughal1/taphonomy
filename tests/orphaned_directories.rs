//! Directories no read entry names, found by the orphan search.
//!
//! `ADR-0017` Decisions A to D. The search is `taphonomy::analysis::run`,
//! called directly: `ADR-0018` moved it into the library, so these tests
//! open a fixture and call the library rather than spawn the binary. One
//! test still runs the binary, to keep two headings and one line of text
//! the CLI's `Sink` renders under a binary-level test, per `ADR-0018`
//! section 5. Every expected digest is of content the generator wrote,
//! recorded in EXP-0007 or computed here from the bytes written, never
//! taken from the tool's own output.
//!
//! The tests use the generated fixtures, which are not committed. Run
//! `./scripts/generate-fixtures.sh` first.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Output};

use taphonomy::EvidenceFile;
use taphonomy::Sha256Digest;
use taphonomy::analysis::{
    self, Coverage, Event, NotReadReason, Options, Reached, RunCounts, Sink,
};
use taphonomy::fat_recovery::Listing;

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

/// Parses a digest the same way the operator's `--reference-digest` would.
fn digest(hex: &str) -> Sha256Digest {
    Sha256Digest::from_hex(hex).expect("64 hexadecimal characters")
}

/// Every finding a test below needs, recorded as owned values. Events
/// borrow from the run and cannot outlive a `record` call, so each match
/// arm copies out only the fields that test assertions need.
#[derive(Default)]
struct RecordingSink {
    /// `(cluster, parent)` of every directory the orphan search found.
    orphaned: Vec<(u32, Option<u32>)>,

    /// Every cluster read as a directory: the root, walked, or orphaned.
    directory_clusters: HashSet<u32>,

    /// Whether a finding was reported under a walked listing's heading.
    saw_deleted_content: bool,

    /// Whether a finding was reported under an orphaned listing's heading.
    saw_orphaned_content: bool,

    /// `(digest, bytes hashed)` of every free run that was read.
    extracted: Vec<(Sha256Digest, u64)>,

    /// Whether a directory was refused for being deeper than `MAX_DEPTH`.
    too_deep: bool,

    /// `first_cluster` of every orphaned listing the search never found.
    orphaned_listing_unread: Vec<u32>,
}

impl RecordingSink {
    /// Records which listing a content finding was reported under.
    /// `ADR-0018` follow-up: every content event carries its `Listing`
    /// directly now, so this is a plain match rather than a check for
    /// whichever finding happened to fire first under that listing.
    fn note_listing(&mut self, listing: Listing) {
        match listing {
            Listing::Walked => self.saw_deleted_content = true,
            Listing::Orphaned => self.saw_orphaned_content = true,
        }
    }
}

impl Sink for RecordingSink {
    fn record(&mut self, event: Event<'_>) {
        match event {
            Event::Directory { reached, directory } => {
                self.directory_clusters
                    .extend(directory.clusters.iter().copied());
                if let Reached::Orphaned { cluster, parent } = reached {
                    self.orphaned.push((cluster, parent));
                }
            }
            Event::DirectoryNotRead {
                reason: NotReadReason::TooDeep,
                ..
            } => self.too_deep = true,
            Event::OrphanedListingUnread { first_cluster, .. } => {
                self.orphaned_listing_unread.push(first_cluster);
            }
            Event::NotAssessed { listing, .. } => self.note_listing(listing),
            Event::Ineligible { listing, .. } => self.note_listing(listing),
            Event::RunBroken { listing, .. } => self.note_listing(listing),
            Event::Recoverable { listing, .. } => self.note_listing(listing),
            Event::Extracted { extraction, .. } => {
                self.extracted
                    .push((extraction.digest, extraction.bytes_hashed));
            }
            _ => {}
        }
    }
}

/// Opens `name` and runs the library's analysis of it directly.
///
/// `ADR-0018` Decision D: no digest is computed, so the whole-image read
/// these tests used to pay for by spawning the binary does not happen
/// here; the length `analysis::run` needs is `reported_size` alone.
fn analyse(name: &str, recover: bool) -> (RunCounts, RecordingSink) {
    let mut evidence = EvidenceFile::open(fixture(name)).expect("opening the fixture");
    let length = evidence.reported_size();
    let options = Options {
        recover,
        reference: None,
        output: None,
    };
    let mut sink = RecordingSink::default();
    let counts = analysis::run(&mut evidence, length, options, &mut sink);
    (counts, sink)
}

/// `ADR-0017` section 10, condition 2. A quick format leaves the tree whole
/// and names none of it, and the search finds both directories and every
/// file they list.
#[test]
fn a_formatted_tree_is_found_and_its_files_recovered() {
    let (counts, sink) = analyse("fat32-formatted-tree.img", true);

    assert_eq!(counts.coverage(), Coverage::Complete);
    assert!(sink.orphaned.contains(&(3, Some(0))), "{:?}", sink.orphaned);
    assert!(sink.orphaned.contains(&(4, Some(3))), "{:?}", sink.orphaned);
    assert!(sink.saw_orphaned_content);
    assert!(!sink.saw_deleted_content);

    for (hex, bytes) in FORMATTED_FILES {
        let expected = (digest(hex), bytes as u64);
        assert!(
            sink.extracted.contains(&expected),
            "missing {hex} over {bytes} bytes in {:?}",
            sink.extracted
        );
    }
}

/// `ADR-0017` section 10, condition 3. Once the tree is recreated on the
/// same clusters, nothing names the old files and nothing is orphaned: the
/// walk reads clusters 3 and 4, and the search skips what it read.
#[test]
fn a_reused_volume_has_no_orphaned_directory() {
    let (counts, sink) = analyse("fat32-formatted-reused.img", true);

    assert_eq!(counts.coverage(), Coverage::Complete);
    assert!(sink.orphaned.is_empty(), "{:?}", sink.orphaned);
    assert!(sink.extracted.is_empty(), "{:?}", sink.extracted);
}

/// `ADR-0017` section 10, condition 4. `TAIL` was reachable only through
/// `BIG`'s second cluster, which nothing locates, but its own first cluster
/// carries its signature. Cluster 4, `PAYLOAD.BIN`'s data, and cluster 20,
/// `BIG`'s continuation, are still never read as directories.
#[test]
fn a_directory_behind_a_lost_cluster_is_found_by_the_search() {
    let (counts, sink) = analyse("fat32-deleted-split-directory.img", true);

    assert_eq!(counts.coverage(), Coverage::Incomplete);
    assert!(
        sink.orphaned.contains(&(19, Some(3))),
        "{:?}",
        sink.orphaned
    );
    assert!(
        sink.extracted.contains(&(digest(OMEGA_DIGEST_HEX), 10)),
        "missing OMEGA.TXT's extraction in {:?}",
        sink.extracted
    );

    for unreached in [4, 20] {
        assert!(
            !sink.directory_clusters.contains(&unreached),
            "cluster {unreached} was read as a directory: {:?}",
            sink.directory_clusters
        );
    }
    assert_eq!(counts.listings_may_continue(), 1);
}

/// `ADR-0017` section 10, condition 5. The residue fixture's poke made
/// `/gone`'s root slot the terminator, so the walk never names it, and its
/// cluster, freed by `mrd` and never reused, still carries its signature.
/// It held nothing, so nothing is offered.
#[test]
fn a_directory_hidden_by_a_terminator_is_found_empty() {
    let (counts, sink) = analyse("fat32-deleted-residue.img", true);

    assert_eq!(counts.coverage(), Coverage::Complete);
    assert_eq!(sink.orphaned, vec![(5, Some(0))]);
    assert!(!sink.saw_orphaned_content);
}

/// `ADR-0017` section 10, condition 6. The walk declines level 129 of the
/// deleted tree at the depth bound, and the search does not read it though
/// its first cluster still identifies itself. Level 130, named only inside
/// 129, is found by the search. The level it lists, whose `.` entry was
/// poked, is a directory the search did not find, and so a gap.
#[test]
fn the_search_keeps_the_walks_refusals_and_reports_what_it_cannot_find() {
    let (counts, sink) = analyse("fat32-deleted-nested.img", false);

    assert_eq!(counts.coverage(), Coverage::Incomplete);
    assert!(sink.too_deep);
    assert_eq!(sink.orphaned, vec![(132, Some(131))]);
    assert!(
        sink.orphaned_listing_unread.contains(&133),
        "{:?}",
        sink.orphaned_listing_unread
    );
    assert_eq!(counts.directories_unread(), 2);
}

/// Runs the binary and returns what it printed.
fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_taphonomy"));
    command.args(args);
    command.output().expect("running the binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Renderings the tests above no longer exercise as text, now that they
/// call the library directly. `ADR-0018` section 5: kept as one binary
/// test, on the one fixture and invocation that produces all three
/// together (`ADR-0017` section 10, condition 6, the same as
/// `the_search_keeps_the_walks_refusals_and_reports_what_it_cannot_find`
/// above). No other binary test under `tests/` asserts any of the three:
/// the orphaned-directory heading, the orphaned-content heading, and an
/// unfound listing's line.
#[test]
fn the_orphaned_and_content_headings_and_an_unfound_listing_render() {
    let output = run(&[&fixture("fat32-deleted-nested.img")]);
    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(
        text.contains("orphaned directory c132, .. names c131"),
        "{text}"
    );
    assert!(text.contains("orphaned content"), "{text}");
    assert!(
        text.contains("NOT READ: cluster 133 was not found by the search"),
        "{text}"
    );
}
