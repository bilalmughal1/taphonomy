//! FAT32 boot sector parsing against generated fixtures.
//!
//! Fixtures are not committed. Run `./scripts/generate-fixtures.sh` first.
//! Their expected digests are recorded in
//! `fixtures/partition/MANIFEST.sha256`.
//!
//! The unit tests in `src/fat32.rs` build boot sectors by hand. These read
//! bytes a real formatting tool wrote, then corrupted one field at a time,
//! and check the parser against the extent the partition table reports
//! rather than against an extent the test invented.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::fat32::{Fat32Error, Fat32Observation, parse_boot_sector};
use taphonomy::filesystem::{VBR_SIZE, VolumeExtent};
use taphonomy::partition::{MbrPartition, PartitionTable, SECTOR_SIZE, parse_mbr};

/// The clean fixture's geometry, measured from
/// `xxd -s 1048576 -l 96 fixtures/partition/mbr-single-fat32.img`.
const FIXTURE_TOTAL_SECTORS: u32 = 129_024;
const FIXTURE_CLUSTER_COUNT: u32 = 127_006;

fn fixture(name: &str) -> PathBuf {
    let path = Path::new("fixtures/partition").join(name);
    assert!(
        path.exists(),
        "fixture missing: {}\nrun ./scripts/generate-fixtures.sh first",
        path.display()
    );
    path
}

/// Opens a fixture and returns its evidence handle with the first
/// partition.
///
/// The sector count comes from `reported_size`, which is filesystem
/// metadata rather than a count of bytes actually read. That is acceptable
/// here only because every fixture is a complete regular file, where the
/// two cannot differ. Production code must derive the bound from
/// `HashResult::bytes_read`, as `src/main.rs` does, because on a block
/// device or on media with unreadable sectors the metadata size overstates
/// what can be read.
fn open_first_partition(name: &str) -> (EvidenceFile, MbrPartition) {
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

    let first = *partitions.first().expect("at least one partition");
    (evidence, first)
}

/// The extent a partition occupies, as the partition table reports it.
fn extent_of(p: &MbrPartition) -> VolumeExtent {
    VolumeExtent {
        start_lba: p.start_lba,
        sector_count: p.sector_count,
    }
}

/// Reads a partition's volume boot record and parses it as FAT32.
fn parse_fixture(name: &str) -> Result<taphonomy::fat32::Fat32BootSector, Fat32Error> {
    let (mut evidence, p) = open_first_partition(name);

    let mut vbr = [0u8; VBR_SIZE];
    evidence
        .read_exact_at(p.start_byte(), &mut vbr)
        .expect("reading volume boot record");

    parse_boot_sector(&vbr, extent_of(&p))
}

/// The expected-success case, against bytes `mkfs.vfat` wrote.
#[test]
fn valid_fat32_volume_parses() {
    let boot = parse_fixture("mbr-single-fat32.img").expect("the fixture volume is valid FAT32");

    assert_eq!(boot.geometry.total_sectors, FIXTURE_TOTAL_SECTORS);
    assert_eq!(boot.geometry.cluster_count, FIXTURE_CLUSTER_COUNT);
    assert_eq!(boot.root_cluster, 2);
    assert_eq!(boot.fs_info_sector, 1);
    assert_eq!(boot.backup_boot_sector, 6);
    assert_eq!(boot.volume_id, Some(0x1234_abcd));
    assert_eq!(boot.volume_label.as_deref(), Some("TAPHFIX"));

    assert!(boot.mirroring_enabled());
    assert_eq!(boot.active_fat(), None);

    // mkfs.vfat leaves BPB_HiddSec at zero when formatting at an offset.
    // That is an unrecorded field, not a disagreement.
    assert_eq!(boot.hidden_sectors, 0);

    assert!(
        boot.observations.is_empty(),
        "a conformant volume matching its extent has nothing to report, got {:?}",
        boot.observations
    );
}

/// The check identification structurally cannot make. `identify` sees only
/// one sector and cannot know how much room the volume has.
///
/// This fixture fails two checks: its inflated cluster count also needs a
/// larger FAT than the volume declares. The extent check is reached first,
/// and this asserts that ordering.
#[test]
fn volume_larger_than_its_partition_is_refused() {
    assert_eq!(
        parse_fixture("fat32-oversized-volume.img"),
        Err(Fat32Error::VolumeExceedsExtent {
            declared_sectors: 200_000,
            extent_sectors: FIXTURE_TOTAL_SECTORS,
        }),
        "the extent check must be reached before the FAT sizing check"
    );
}

/// Invisible to identification. The volume records a start it does not
/// have, which is evidence it was moved, carved, or had its partition
/// table rebuilt.
#[test]
fn hidden_sector_disagreement_is_reported() {
    let boot =
        parse_fixture("fat32-hidden-mismatch.img").expect("a wrong start is reported, not refused");

    assert_eq!(boot.hidden_sectors, 9999);
    assert!(
        boot.observations
            .contains(&Fat32Observation::HiddenSectorsDisagree {
                declared: 9999,
                actual: 2048,
            }),
        "got {:?}",
        boot.observations
    );
}

#[test]
fn root_cluster_outside_the_data_region_is_refused() {
    assert_eq!(
        parse_fixture("fat32-bad-root-cluster.img"),
        Err(Fat32Error::InvalidRootCluster {
            cluster: 1,
            cluster_count: FIXTURE_CLUSTER_COUNT,
        })
    );
}

/// A FAT of four sectors holds 512 entries. The volume declares 128,984
/// clusters.
#[test]
fn fat_too_small_for_its_clusters_is_refused() {
    match parse_fixture("fat32-undersized-fat.img") {
        Err(Fat32Error::FatTooSmall {
            fat_bytes,
            required_bytes,
        }) => {
            assert_eq!(fat_bytes, 4 * 512);
            assert!(
                required_bytes > fat_bytes,
                "the error must state a requirement the volume fails"
            );
        }
        other => panic!("expected FatTooSmall, got {other:?}"),
    }
}

/// The partition entry declares 0x07, promising NTFS or exFAT. Parsing
/// follows the volume's own structure and is unaffected by the claim.
#[test]
fn declared_partition_type_does_not_affect_parsing() {
    let (_, p) = open_first_partition("mbr-type-mismatch.img");
    assert_eq!(
        p.partition_type, 0x07,
        "fixture must declare NTFS/exFAT for this test to mean anything"
    );

    let boot = parse_fixture("mbr-type-mismatch.img")
        .expect("the volume is FAT32 regardless of what the entry claims");

    assert_eq!(boot.geometry.total_sectors, FIXTURE_TOTAL_SECTORS);
    assert_eq!(boot.root_cluster, 2);
}

/// An unformatted partition must be refused, never parsed optimistically.
#[test]
fn unformatted_partition_is_not_fat32() {
    assert_eq!(
        parse_fixture("mbr-four-partitions.img"),
        Err(Fat32Error::NotFat32 { identified: None })
    );
}

/// Parsing must not modify the evidence. `docs/SAFETY.md` section 4.
#[test]
fn parsing_does_not_modify_evidence() {
    let path = fixture("fat32-hidden-mismatch.img");

    let before = std::fs::metadata(&path).expect("metadata before");
    let before_mtime = before.modified().expect("mtime before");

    let _ = parse_fixture("fat32-hidden-mismatch.img");

    let after = std::fs::metadata(&path).expect("metadata after");
    assert_eq!(before.len(), after.len(), "size changed");
    assert_eq!(
        before_mtime,
        after.modified().expect("mtime after"),
        "modification time changed"
    );
}

/// `docs/PROJECT.md` section 6.4 requires deterministic behaviour.
#[test]
fn parsing_is_deterministic() {
    let first = parse_fixture("mbr-single-fat32.img");
    let second = parse_fixture("mbr-single-fat32.img");
    assert_eq!(first, second);
}
