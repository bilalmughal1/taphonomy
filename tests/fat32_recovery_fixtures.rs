//! Recovery of a deleted file's data against generated fixtures.
//!
//! Fixtures are not committed. Run `./scripts/generate-fixtures.sh` first.
//! Their expected digests are recorded in
//! `fixtures/partition/MANIFEST.sha256`.
//!
//! The unit tests in `src/fat_recovery.rs` build FATs and data clusters in
//! memory at offsets written out by hand. That proves the run walk and the
//! extraction loop, and deliberately does not prove the addressing: a test
//! that locates a cluster with the same function it is testing agrees with
//! that function whether or not either is right.
//!
//! These tests read bytes `mkfs.vfat` and `mtools` wrote, at offsets
//! `cluster_offset` and `fat_entry_offset` compute, and compare a digest of
//! what was extracted against a digest of what the generator wrote.
//!
//! The validation tests compare against the reference digest recorded in
//! `ADR-0013` section 16.1, which is text in a document rather than a value
//! this crate computes.
//!
//! `fixture` and `open_volume` are duplicated from
//! `tests/fat32_directory_fixtures.rs`. Each file under `tests/` compiles as
//! its own crate and cannot see the other's items.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::fat_directory::{DeletedKind, Entry, EntryKind, enumerate_root};
use taphonomy::fat_recovery::{Assessment, Ineligible, assess, extract};
use taphonomy::fat32::{Fat32BootSector, parse_boot_sector};
use taphonomy::filesystem::{VBR_SIZE, VolumeExtent};
use taphonomy::hash::{Sha256Digest, hash_reader};
use taphonomy::partition::{PartitionTable, SECTOR_SIZE, parse_mbr};
use taphonomy::validation::{Outcome, validate};

/// Cluster size of both fixtures used here, in bytes.
///
/// Both are formatted at one 512-byte sector per cluster. Measured with
/// `od -An -tu1 -j 1048589 -N 1 fixtures/partition/fat32-recover-run.img`.
const CLUSTER_BYTES: usize = 512;

/// Size in bytes of `BIG.TXT`, as its deleted entry still declares it.
const BIG_SIZE: u32 = 1600;

/// Size the collision fixture's poke substitutes for `BIG_SIZE`.
///
/// Five clusters rather than four, so the implied run reaches cluster 7.
const POKED_SIZE: u32 = 2560;

/// `BIG.TXT`'s content digest, as recorded in `ADR-0013` section 16.1.
///
/// Written out rather than computed, so that a test compares the value the
/// record commits to against one derived from the generator's bytes. A test
/// that computed both sides would only agree with itself.
const BIG_DIGEST_HEX: &str = "5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a91";

/// Size in bytes of `FRAG.BIN`, as its deleted entry still declares it.
///
/// Three 512-byte clusters exactly, so the run carries no slack.
const FRAG_SIZE: u32 = 1536;

/// `FRAG.BIN`'s own content digest, as recorded in `EXP-0004`.
///
/// This is what a correct recovery of that entry would produce. The tool
/// does not produce it, which is the point of the fixture.
const FRAG_DIGEST_HEX: &str = "a27a7e9556749147a521e0f133a9a1a84375861885f883b404c125f4719f3bfd";

/// The digest the tool actually produces for `FRAG.BIN`'s entry, as
/// recorded in `EXP-0004`.
const COMMINGLED_DIGEST_HEX: &str =
    "7eef746ae1c9b211bf0384deec32abf8f91eb024f49c59f8478bc617db342914";

fn fixture(name: &str) -> PathBuf {
    let path = Path::new("fixtures/partition").join(name);
    assert!(
        path.exists(),
        "fixture missing: {}\nrun ./scripts/generate-fixtures.sh first",
        path.display()
    );
    path
}

/// Opens a fixture and parses its first partition's boot sector.
fn open_volume(name: &str) -> (EvidenceFile, Fat32BootSector, VolumeExtent) {
    let mut evidence = EvidenceFile::open(fixture(name)).expect("opening fixture");
    let sectors = evidence.reported_size() / SECTOR_SIZE as u64;

    let mut sector = [0u8; SECTOR_SIZE];
    evidence
        .read_exact_at(0, &mut sector)
        .expect("reading first sector");

    let outcome = parse_mbr(&sector, sectors).expect("parsing MBR");
    let PartitionTable::Mbr { partitions, .. } = outcome.table else {
        panic!("expected an MBR table in {name}");
    };
    let p = *partitions.first().expect("at least one partition");

    let extent = VolumeExtent {
        start_lba: p.start_lba,
        sector_count: p.sector_count,
    };

    let mut vbr = [0u8; VBR_SIZE];
    evidence
        .read_exact_at(p.start_byte(), &mut vbr)
        .expect("reading volume boot record");

    let boot = parse_boot_sector(&vbr, extent).expect("the fixture volume is valid FAT32");

    (evidence, boot, extent)
}

/// The bytes `build_recovery_volume` writes into `BIG.TXT`.
///
/// Rebuilt here rather than read back from the image, so that a digest
/// assertion compares what the tool extracted against what the generator
/// wrote, by two independent routes.
fn big_content() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 1..=40 {
        out.extend_from_slice(format!("taphonomy multicluster fixture line {i:03}\n").as_bytes());
    }
    out
}

/// The content the generator wrote to `S<n>.BIN`: twenty tagged lines cut
/// to one cluster.
///
/// Every line names the file it belongs to, so a cluster recovered into the
/// wrong object can be traced to its source by reading it. NIST's CFTT
/// requires exactly this of a recovery test image.
fn spacer_content(n: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for j in 0..20 {
        let line = format!("taphonomy spacer{n} block {j:06}\n");
        out.extend_from_slice(line.as_bytes());
    }
    out.truncate(CLUSTER_BYTES);
    out
}

/// The content the generator wrote to `FRAG.BIN`: fifty-six tagged lines cut
/// to three clusters.
fn frag_content() -> Vec<u8> {
    let mut out = Vec::new();
    for j in 0..56 {
        let line = format!("taphonomy fragment block {j:06}\n");
        out.extend_from_slice(line.as_bytes());
    }
    out.truncate(FRAG_SIZE as usize);
    out
}

/// What clusters 4, 5 and 6 hold, in that order.
///
/// The first third of `FRAG.BIN`, the whole of the deleted `S2.BIN`, and
/// the second third of `FRAG.BIN`. This is what the entry's implied run
/// reads, and it is not any one file's content.
fn commingled_content() -> Vec<u8> {
    let frag = frag_content();
    let mut out = Vec::new();
    out.extend_from_slice(&frag[..CLUSTER_BYTES]);
    out.extend_from_slice(&spacer_content(2));
    out.extend_from_slice(&frag[CLUSTER_BYTES..2 * CLUSTER_BYTES]);
    out
}

/// The first cluster and declared size of a deleted short entry.
///
/// `EntryKind` is `Clone` and not `Copy`, so this binds the two `u32` fields
/// and ignores the rest rather than moving out of the entry.
fn deleted_short(entry: &Entry) -> (u32, u32) {
    let EntryKind::Deleted {
        was:
            DeletedKind::ShortName {
                first_cluster,
                file_size,
                ..
            },
    } = entry.kind
    else {
        panic!("expected a deleted short entry: {entry:?}");
    };

    (first_cluster, file_size)
}

fn digest_of(bytes: &[u8]) -> Sha256Digest {
    let mut input = bytes;
    hash_reader(&mut input)
        .expect("hashing a slice cannot fail")
        .digest
}

/// The entries of a fixture's root directory, in slot order.
fn entries(name: &str) -> (EvidenceFile, Fat32BootSector, VolumeExtent, Vec<Entry>) {
    let (mut evidence, boot, extent) = open_volume(name);
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");
    (evidence, boot, extent, root.entries)
}

/// Slot order is fixed by the order `mcopy`, `mmd`, `mdel` and `mrd` are
/// invoked in `scripts/generate-fixtures.sh`, which is why the entries below
/// can be addressed positionally.
///
/// This test states the premise every other test in this file depends on. If
/// the generator changes, this fails first and says what moved, rather than
/// a recovery assertion failing for a reason that looks like a bug in the
/// recovery.
#[test]
fn the_fixture_has_the_layout_the_generator_built() {
    let (_evidence, boot, _extent, entries) = entries("fat32-recover-run.img");

    assert_eq!(boot.geometry.cluster_bytes() as usize, CLUSTER_BYTES);
    assert_eq!(
        entries.len(),
        6,
        "a volume label, two live files, two deleted files and a deleted directory: {entries:?}"
    );

    let EntryKind::Deleted {
        was:
            DeletedKind::ShortName {
                directory,
                first_cluster,
                file_size,
                ..
            },
    } = entries[1].kind
    else {
        panic!("slot 1 should be a deleted short entry: {:?}", entries[1]);
    };

    assert!(!directory, "BIG.TXT is a file");
    assert_eq!(first_cluster, 3, "BIG.TXT was written first");
    assert_eq!(file_size, BIG_SIZE);

    assert!(
        matches!(entries[2].kind, EntryKind::ShortName { .. }),
        "slot 2 is LIVE1.TXT and is not deleted: {:?}",
        entries[2]
    );
}

#[test]
fn a_four_cluster_deleted_file_is_recoverable() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let assessment = assess(&entries[1], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 1 is a deleted file");

    let Assessment::Recoverable(found) = assessment else {
        panic!("clusters 3 to 6 are free in the FAT: {assessment:?}");
    };

    assert_eq!(found.run().first_cluster, 3);
    assert_eq!(found.run().cluster_count, 4);
    assert_eq!(found.run().file_size, BIG_SIZE);
    assert_eq!(found.run().last_cluster(), 6);
    assert_eq!(
        found.run().slack_bytes,
        4 * CLUSTER_BYTES as u32 - BIG_SIZE,
        "448 bytes of the fourth cluster lie past the file's end"
    );
}

/// The assertion the milestone exists to make.
#[test]
fn extraction_reproduces_the_content_the_generator_wrote() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

    let content = big_content();
    assert_eq!(
        content.len(),
        BIG_SIZE as usize,
        "the fixture's own premise"
    );

    assert_eq!(extracted.digest, digest_of(&content));
    assert_eq!(extracted.bytes_hashed, u64::from(BIG_SIZE));
    assert_eq!(extracted.slack_bytes, 4 * CLUSTER_BYTES as u32 - BIG_SIZE);
}

/// Slack is read and not hashed. The run occupies 2048 bytes and the file is
/// 1600 of them, so hashing the run whole would produce a different digest.
#[test]
fn the_digest_covers_the_file_and_not_the_whole_run() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

    let mut whole_run = big_content();
    whole_run.resize(4 * CLUSTER_BYTES, 0);

    assert_ne!(
        extracted.digest,
        digest_of(&whole_run),
        "the trailing 448 bytes were hashed, so slack was included"
    );
}

#[test]
fn a_single_cluster_deleted_file_is_recoverable() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let assessment = assess(&entries[4], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 4 is a deleted file");

    let Assessment::Recoverable(found) = assessment else {
        panic!("cluster 9 is free in the FAT: {assessment:?}");
    };

    assert_eq!(found.run().first_cluster, 9);
    assert_eq!(found.run().cluster_count, 1);

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");
    assert_eq!(
        extracted.digest,
        digest_of(b"taphonomy single cluster fix\n")
    );
}

#[test]
fn a_deleted_directory_is_refused() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let assessment = assess(&entries[5], &boot, extent, &mut evidence)
        .expect("no FAT read is attempted")
        .expect("slot 5 is a deleted entry");

    assert_eq!(assessment, Assessment::Ineligible(Ineligible::Directory));
}

#[test]
fn a_live_entry_is_not_assessed() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    assert_eq!(
        assess(&entries[2], &boot, extent, &mut evidence).expect("no read is attempted"),
        None,
        "LIVE1.TXT is not deleted and the question does not arise"
    );
}

/// The poked fixture. `BIG.TXT`'s declared size reaches cluster 7, which
/// `LIVE1.TXT` holds, so the run is broken and no extraction is offered.
#[test]
fn a_run_reaching_a_live_cluster_is_refused() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-collision.img");

    let assessment = assess(&entries[1], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 1 is a deleted file");

    let Assessment::RunBroken {
        run,
        first_allocated,
    } = assessment
    else {
        panic!("cluster 7 is allocated to LIVE1.TXT: {assessment:?}");
    };

    assert_eq!(first_allocated, 7);
    assert_eq!(run.first_cluster, 3);
    assert_eq!(
        run.cluster_count, 5,
        "2560 bytes need five 512-byte clusters"
    );
    assert_eq!(run.file_size, POKED_SIZE);
}

/// The two fixtures come from one builder and differ by one poked field.
/// Everything the recovery reads is otherwise identical, which is what makes
/// the refusal above attributable to the size and to nothing else.
#[test]
fn the_two_fixtures_differ_only_in_the_declared_size() {
    let (_e1, _b1, _x1, run_entries) = entries("fat32-recover-run.img");
    let (_e2, _b2, _x2, collision_entries) = entries("fat32-recover-collision.img");

    assert_eq!(run_entries.len(), collision_entries.len());

    for (index, (a, b)) in run_entries.iter().zip(&collision_entries).enumerate() {
        if index == 1 {
            continue;
        }
        assert_eq!(a.kind, b.kind, "slot {index} should be untouched");
    }

    let (
        EntryKind::Deleted {
            was: DeletedKind::ShortName { file_size: a, .. },
        },
        EntryKind::Deleted {
            was: DeletedKind::ShortName { file_size: b, .. },
        },
    ) = (&run_entries[1].kind, &collision_entries[1].kind)
    else {
        panic!("slot 1 is a deleted short entry in both fixtures");
    };

    assert_eq!(*a, BIG_SIZE);
    assert_eq!(*b, POKED_SIZE);
}

/// `docs/SAFETY.md` sections 4 and 5. Recovery reads and never writes.
#[test]
fn recovery_does_not_modify_evidence() {
    let path = fixture("fat32-recover-run.img");
    let before = std::fs::metadata(&path).expect("metadata before");
    let before_mtime = before.modified().expect("mtime before");

    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");
    let digest_before = evidence.digest().expect("hashing before");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };
    extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

    let digest_after = evidence.digest().expect("hashing after");
    let after = std::fs::metadata(&path).expect("metadata after");

    assert_eq!(digest_before, digest_after, "evidence digest changed");
    assert_eq!(before.len(), after.len(), "size changed");
    assert_eq!(
        before_mtime,
        after.modified().expect("mtime after"),
        "modification time changed"
    );
}

/// `docs/PROJECT.md` section 6.4 requires deterministic behaviour.
#[test]
fn extraction_is_deterministic() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };

    let first = extract(&found, &boot, extent, &mut evidence, None).expect("first extraction");
    let second = extract(&found, &boot, extent, &mut evidence, None).expect("second extraction");

    assert_eq!(first, second);
}

/// `ADR-0013` section 16.1. The digest the record commits to is the digest
/// of the bytes the generator wrote, reached by two independent routes:
/// parsed from the recorded text, and computed from the content.
#[test]
fn the_recorded_reference_digest_is_the_generators_content() {
    let recorded = Sha256Digest::from_hex(BIG_DIGEST_HEX).expect("the recorded digest parses");

    assert_eq!(recorded, digest_of(&big_content()));
    assert_eq!(recorded.to_hex(), BIG_DIGEST_HEX);
}

/// The milestone's assertion. An extraction whose digest equals a reference
/// the operator supplied is byte for byte that file, which is the only
/// evidence available that the run read was the file's clusters.
#[test]
fn a_recovered_run_matches_the_reference_digest() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");
    let reference = Sha256Digest::from_hex(BIG_DIGEST_HEX).expect("the recorded digest parses");

    let validation = validate(extracted.digest, extracted.bytes_hashed, Some(reference));

    assert_eq!(validation.outcome, Outcome::Match);
    assert_eq!(validation.covers, u64::from(BIG_SIZE));
    assert_eq!(validation.reference, Some(reference));
}

/// One character is enough. `ADR-0013` Decision E: the outcome is that they
/// differ, and which of the four possible causes produced it is not decided
/// here or anywhere else in the tool.
#[test]
fn a_reference_differing_in_one_character_does_not_match() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-run.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[1], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 1 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

    let mut text = BIG_DIGEST_HEX.to_string();
    text.replace_range(63.., "2");
    let reference = Sha256Digest::from_hex(&text).expect("still 64 hex characters");

    let validation = validate(extracted.digest, extracted.bytes_hashed, Some(reference));

    assert_ne!(reference, extracted.digest, "the premise of this test");
    assert_eq!(validation.outcome, Outcome::Differs);
    assert_eq!(validation.covers, u64::from(BIG_SIZE));
}

/// A reference cannot induce an extraction the FAT forbids. The collision
/// fixture's run is refused before content is read, so there is no
/// extraction to validate: `ADR-0003` section 3.1 makes validation
/// something that happens to a Candidate, and a refused run never becomes
/// one.
#[test]
fn a_refused_run_offers_nothing_to_validate() {
    let (mut evidence, boot, extent, entries) = entries("fat32-recover-collision.img");

    let assessment = assess(&entries[1], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 1 is a deleted file");

    assert!(
        !matches!(assessment, Assessment::Recoverable(_)),
        "cluster 7 is in use, so no run is offered: {assessment:?}"
    );
}

/// Slot order is fixed by the order the generator invokes `mcopy` and
/// `mdel`. `FRAG.BIN` occupies the slot `S1.BIN` freed, which is why the
/// fragmented entry is slot 2 rather than the last.
///
/// This test states the premise the six below depend on. If the generator
/// changes, this fails first and says what moved.
#[test]
fn the_fragmented_fixture_has_the_layout_the_generator_built() {
    let (_evidence, boot, _extent, entries) = entries("fat32-fragmented-deleted.img");

    assert_eq!(boot.geometry.cluster_bytes() as usize, CLUSTER_BYTES);
    assert_eq!(
        entries.len(),
        9,
        "a volume label, two live files, five deleted files and the filler: {entries:?}"
    );

    let (first_cluster, file_size) = deleted_short(&entries[2]);
    assert_eq!(first_cluster, 4, "FRAG.BIN took the slot S1.BIN freed");
    assert_eq!(file_size, FRAG_SIZE);

    for (slot, cluster) in [(3usize, 5u32), (4, 6), (5, 7), (6, 8)] {
        let (first, size) = deleted_short(&entries[slot]);
        assert_eq!(first, cluster, "slot {slot}");
        assert_eq!(size, CLUSTER_BYTES as u32, "slot {slot}");
    }

    // A block referenced by two deleted files is what NIST's forced-overwrite
    // construction produces, and it is what makes this fixture worth having.
    // Clusters 5 and 6 lie inside the fragmented entry's implied run and are
    // also slots 3 and 4's own first clusters.
    let clusters_needed = file_size.div_ceil(CLUSTER_BYTES as u32);
    let last = first_cluster + clusters_needed - 1;
    assert_eq!(last, 6);

    for slot in [3usize, 4] {
        let (first, _size) = deleted_short(&entries[slot]);
        assert!(
            (first_cluster..=last).contains(&first),
            "slot {slot}'s cluster lies inside the fragmented entry's run"
        );
    }

    assert!(
        matches!(entries[1].kind, EntryKind::ShortName { .. }),
        "slot 1 is S0.BIN and is live: {:?}",
        entries[1]
    );
    assert!(
        matches!(entries[7].kind, EntryKind::ShortName { .. }),
        "slot 7 is S6.BIN and is live: {:?}",
        entries[7]
    );
}

/// `EXP-0004` commits to two digests. Both are reached here by two
/// independent routes: parsed from the recorded text, and computed from
/// content rebuilt in this file. A drift between the record and the
/// generator fails here rather than inside a recovery assertion.
#[test]
fn the_recorded_digests_are_the_reconstructed_content() {
    let truth = Sha256Digest::from_hex(FRAG_DIGEST_HEX).expect("the recorded digest parses");
    assert_eq!(truth, digest_of(&frag_content()));
    assert_eq!(truth.to_hex(), FRAG_DIGEST_HEX);

    let commingled =
        Sha256Digest::from_hex(COMMINGLED_DIGEST_HEX).expect("the recorded digest parses");
    assert_eq!(commingled, digest_of(&commingled_content()));
}

/// FAT32 deletion zeroes the cluster chain, so the entry states a first
/// cluster and a size and nothing about fragmentation. The run those imply
/// reads as entirely free and the extraction is offered.
#[test]
fn a_fragmented_deleted_file_is_assessed_as_though_contiguous() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-deleted.img");

    let assessment = assess(&entries[2], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 2 is a deleted file");

    let Assessment::Recoverable(found) = assessment else {
        panic!("clusters 4 to 6 all read as free: {assessment:?}");
    };

    assert_eq!(found.run().first_cluster, 4);
    assert_eq!(found.run().cluster_count, 3);
    assert_eq!(found.run().last_cluster(), 6);
    assert_eq!(found.run().file_size, FRAG_SIZE);
    assert_eq!(
        found.run().slack_bytes,
        0,
        "1536 bytes fill three 512-byte clusters exactly"
    );
}

/// The measurement `CLAUDE.md` section 26 requires, and the case no fixture
/// reached before `EXP-0004`.
///
/// Asserted by composition rather than against an opaque digest, so that a
/// failure says which bytes were recovered. Cluster 5 belonged to `S2.BIN`
/// and is recovered into this entry's object.
#[test]
fn the_recovery_contains_a_cluster_belonging_to_another_file() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-deleted.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[2], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 2 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

    assert_eq!(
        extracted.digest,
        digest_of(&commingled_content()),
        "clusters 4, 5 and 6 hold FRAG.BIN, S2.BIN and FRAG.BIN in that order"
    );
    assert_ne!(
        extracted.digest,
        digest_of(&frag_content()),
        "the recovered bytes are not the content of the file the entry names"
    );
    assert_eq!(extracted.bytes_hashed, u64::from(FRAG_SIZE));
}

/// The reference is the true content of a file that is present on this
/// volume, in three pieces. The comparison still differs, and `ADR-0013`
/// Decision E forbids the tool from saying which of the four causes applied.
#[test]
fn a_reference_to_the_fragmented_file_itself_differs() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-deleted.img");

    let Some(Assessment::Recoverable(found)) =
        assess(&entries[2], &boot, extent, &mut evidence).expect("reading the FAT")
    else {
        panic!("slot 2 should be recoverable");
    };

    let extracted = extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");
    let reference = Sha256Digest::from_hex(FRAG_DIGEST_HEX).expect("the recorded digest parses");

    let validation = validate(extracted.digest, extracted.bytes_hashed, Some(reference));

    assert_eq!(validation.outcome, Outcome::Differs);
    assert_eq!(validation.covers, u64::from(FRAG_SIZE));
    assert_eq!(validation.reference, Some(reference));
}

/// Nothing in the assessment separates a correct recovery from an incorrect
/// one. All five deleted entries are offered, and the FAT has no evidence
/// that would let any of them be refused.
#[test]
fn every_deleted_entry_on_the_volume_is_offered_for_recovery() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-deleted.img");

    for slot in [2usize, 3, 4, 5, 6] {
        let assessment = assess(&entries[slot], &boot, extent, &mut evidence)
            .expect("reading the FAT")
            .expect("slots 2 to 6 are deleted files");

        assert!(
            matches!(assessment, Assessment::Recoverable(_)),
            "slot {slot} is offered like every other: {assessment:?}"
        );
    }
}

/// The accuracy measurement. Each case pairs the content the entry's own
/// file held with the content its implied run actually reads.
///
/// Slots 3 and 5 name clusters nothing reused and recover correctly. Slots
/// 2, 4 and 6 name clusters `FRAG.BIN` was later written across. The tool
/// reports all five identically.
#[test]
fn three_of_the_five_recoveries_are_not_the_entrys_own_content() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-deleted.img");

    let frag = frag_content();
    let second = frag[CLUSTER_BYTES..2 * CLUSTER_BYTES].to_vec();
    let third = frag[2 * CLUSTER_BYTES..].to_vec();

    let cases: [(usize, Vec<u8>, Vec<u8>); 5] = [
        (2, frag_content(), commingled_content()),
        (3, spacer_content(2), spacer_content(2)),
        (4, spacer_content(3), second),
        (5, spacer_content(4), spacer_content(4)),
        (6, spacer_content(5), third),
    ];

    let mut incorrect = 0;

    for (slot, own, actual) in cases {
        let Some(Assessment::Recoverable(found)) =
            assess(&entries[slot], &boot, extent, &mut evidence).expect("reading the FAT")
        else {
            panic!("slot {slot} should be recoverable");
        };

        let extracted =
            extract(&found, &boot, extent, &mut evidence, None).expect("reading the run");

        assert_eq!(
            extracted.digest,
            digest_of(&actual),
            "slot {slot} recovers these bytes"
        );

        if extracted.digest != digest_of(&own) {
            incorrect += 1;
        }
    }

    assert_eq!(
        incorrect, 3,
        "three of five recoveries are not the content the entry's file held"
    );
}

/// Slot order on the live-gap volume, which differs from
/// `fat32-fragmented-deleted.img` only in what is still live.
///
/// `build_fragmented_volume` writes both, and this fixture omits the two
/// `mdel` calls that free the clusters between the fragments. So `S2.BIN`
/// and `S4.BIN` hold clusters 5 and 7, and slots 3 and 5 are live entries
/// where the other fixture has deleted ones.
///
/// This test states the premise the one below depends on. If the generator
/// changes, this fails first and says what moved.
#[test]
fn the_live_gap_fixture_has_the_layout_the_generator_built() {
    let (_evidence, boot, _extent, entries) = entries("fat32-fragmented-live-gap.img");

    assert_eq!(boot.geometry.cluster_bytes() as usize, CLUSTER_BYTES);
    assert_eq!(
        entries.len(),
        9,
        "a volume label, four live files, three deleted files and the filler: {entries:?}"
    );

    let (first_cluster, file_size) = deleted_short(&entries[2]);
    assert_eq!(first_cluster, 4, "FRAG.BIN took the slot S1.BIN freed");
    assert_eq!(file_size, FRAG_SIZE);

    for (slot, cluster) in [(4usize, 6u32), (6, 8)] {
        let (first, size) = deleted_short(&entries[slot]);
        assert_eq!(first, cluster, "slot {slot}");
        assert_eq!(size, CLUSTER_BYTES as u32, "slot {slot}");
    }

    for slot in [1usize, 3, 5, 7] {
        assert!(
            matches!(entries[slot].kind, EntryKind::ShortName { .. }),
            "slot {slot} is live: {:?}",
            entries[slot]
        );
    }
}

/// `ADR-0010` Decision C, reached from ordinary file operations.
///
/// `FRAG.BIN` occupied clusters 4, 6 and 8, so the run its entry implies is
/// 4 to 6 and cluster 5 is held by the live `S2.BIN`. Contiguity is refuted
/// by the FAT and the entry is refused before any content is read.
///
/// Two things make this worth asserting separately from
/// `a_refused_run_offers_nothing_to_validate`. That test reaches a refusal
/// through the collision fixture's poked `DIR_FileSize`; this one reaches it
/// with no byte written by this project, which is what `EXP-0004`
/// Appendix A.5 measured. And it is Case 2 of the canonical list in Meyer
/// and Roy, recorded in `EXP-0004`'s external practice section, where the
/// tools they tested recover such a file anyway because they do not refuse
/// on an allocated cluster. It is the one arrangement in that list this tool
/// handles correctly, and nothing in this tree measured it before.
#[test]
fn a_run_reaching_a_live_cluster_is_refused_without_a_poke() {
    let (mut evidence, boot, extent, entries) = entries("fat32-fragmented-live-gap.img");

    let assessment = assess(&entries[2], &boot, extent, &mut evidence)
        .expect("reading the FAT")
        .expect("slot 2 is a deleted file");

    let Assessment::RunBroken {
        run,
        first_allocated,
    } = assessment
    else {
        panic!("cluster 5 is held by the live S2.BIN: {assessment:?}");
    };

    assert_eq!(run.first_cluster, 4);
    assert_eq!(run.last_cluster(), 6);
    assert_eq!(run.file_size, FRAG_SIZE);
    assert_eq!(
        first_allocated, 5,
        "refused at the first cluster of the run a live file holds"
    );
}
