//! Filesystem identification against generated fixtures.
//!
//! Fixtures are not committed. Run `./scripts/generate-fixtures.sh` first.
//!
//! These exercise identification against bytes a real formatting tool wrote,
//! rather than against hand-built sectors. The unit tests in
//! `src/filesystem.rs` cover the edge cases; these cover reality.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::filesystem::{
    Filesystem, Identification, VBR_SIZE, declared_type_matches, identify,
};
use taphonomy::partition::{MbrPartition, PartitionTable, SECTOR_SIZE, parse_mbr};

fn fixture(name: &str) -> PathBuf {
    let path = Path::new("fixtures/partition").join(name);
    assert!(
        path.exists(),
        "fixture missing: {}\nrun ./scripts/generate-fixtures.sh first",
        path.display()
    );
    path
}

/// Opens a fixture, parses its MBR, and returns the evidence handle with the
/// partition list.
fn open_partitions(name: &str) -> (EvidenceFile, Vec<MbrPartition>) {
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

    (evidence, partitions)
}

/// Reads the volume boot record of a partition and identifies it.
fn identify_partition(evidence: &mut EvidenceFile, p: &MbrPartition) -> Identification {
    let mut vbr = [0u8; VBR_SIZE];
    evidence
        .read_exact_at(p.start_byte(), &mut vbr)
        .expect("reading volume boot record");
    identify(&vbr)
}

#[test]
fn fat32_is_identified_from_volume_structure() {
    let (mut evidence, partitions) = open_partitions("mbr-single-fat32.img");
    assert_eq!(partitions.len(), 1);

    let id = identify_partition(&mut evidence, &partitions[0]);
    let Identification::Identified {
        filesystem,
        geometry,
        oem_name,
        ..
    } = id
    else {
        panic!("expected FAT32 to be identified, got {id:?}");
    };

    assert_eq!(filesystem, Filesystem::Fat32);
    assert_eq!(oem_name.as_deref(), Some("mkfs.fat"));

    let g = geometry.expect("FAT geometry");
    assert_eq!(g.bytes_per_sector, 512);
    assert_eq!(g.fat_count, 2);
    assert_eq!(g.root_entry_count, 0, "FAT32 has no fixed root directory");
    assert_eq!(g.root_dir_sectors, 0);

    // Derived values must be internally consistent.
    assert_eq!(
        g.first_data_sector(),
        g.reserved_sectors as u32 + (g.fat_count as u32 * g.fat_size)
    );
    assert_eq!(
        g.data_sectors,
        g.total_sectors - g.first_data_sector(),
        "data sectors must equal total minus metadata"
    );
    assert_eq!(
        g.cluster_count,
        g.data_sectors / g.sectors_per_cluster as u32
    );

    // The FAT specification's threshold, not the type string.
    assert!(
        g.cluster_count >= 65_525,
        "cluster count {} must exceed the FAT16 threshold for this to be FAT32",
        g.cluster_count
    );
}

/// A declared type that matches the observed filesystem must be reported as
/// agreement.
#[test]
fn agreeing_declared_type_is_recognised() {
    let (mut evidence, partitions) = open_partitions("mbr-single-fat32.img");
    let p = partitions[0];
    assert_eq!(p.partition_type, 0x0c, "fixture declares FAT32 (LBA)");

    let filesystem = identify_partition(&mut evidence, &p)
        .filesystem()
        .expect("identified");

    assert_eq!(
        declared_type_matches(p.partition_type, filesystem),
        Some(true)
    );
}

/// The forensically interesting case.
///
/// The partition entry declares type 0x07, which promises NTFS or exFAT. The
/// volume is FAT32. Identification must follow the volume, and the
/// disagreement must be detected.
#[test]
fn disagreeing_declared_type_is_detected() {
    let (mut evidence, partitions) = open_partitions("mbr-type-mismatch.img");
    assert_eq!(partitions.len(), 1);

    let p = partitions[0];
    assert_eq!(
        p.partition_type, 0x07,
        "fixture must declare NTFS/exFAT for this test to mean anything"
    );

    let id = identify_partition(&mut evidence, &p);
    let filesystem = id.filesystem().expect("volume is identifiable");

    assert_eq!(
        filesystem,
        Filesystem::Fat32,
        "identification must follow the volume, not the declared type"
    );

    assert_eq!(
        declared_type_matches(p.partition_type, filesystem),
        Some(false),
        "declared type 0x07 disagrees with observed FAT32 and must be reported"
    );
}

/// A declared type carrying no expectation must not be reported as
/// disagreement. Absence of a mismatch is not evidence of agreement.
#[test]
fn uninformative_declared_type_yields_no_verdict() {
    // 0x83 declares Linux, which implies nothing about the filesystem.
    assert_eq!(declared_type_matches(0x83, Filesystem::Fat32), None);
    assert_eq!(declared_type_matches(0x00, Filesystem::Ntfs), None);
}

/// An unformatted partition must be reported unknown, never guessed at.
#[test]
fn unformatted_partitions_are_unknown() {
    let (mut evidence, partitions) = open_partitions("mbr-four-partitions.img");
    assert_eq!(partitions.len(), 4);

    for p in &partitions {
        let id = identify_partition(&mut evidence, p);
        assert_eq!(
            id.filesystem(),
            None,
            "partition {} is unformatted and must not be identified",
            p.index
        );
    }
}

#[test]
fn identification_does_not_modify_evidence() {
    let path = fixture("mbr-type-mismatch.img");

    let before = std::fs::metadata(&path).expect("metadata before");
    let before_mtime = before.modified().expect("mtime before");

    let (mut evidence, partitions) = open_partitions("mbr-type-mismatch.img");
    let _ = identify_partition(&mut evidence, &partitions[0]);

    let after = std::fs::metadata(&path).expect("metadata after");
    assert_eq!(before.len(), after.len(), "size changed");
    assert_eq!(
        before_mtime,
        after.modified().expect("mtime after"),
        "modification time changed"
    );
}

/// PROJECT.md section 6.4 requires deterministic behaviour.
#[test]
fn identification_is_repeatable() {
    let (mut evidence, partitions) = open_partitions("mbr-single-fat32.img");
    let first = identify_partition(&mut evidence, &partitions[0]);
    let second = identify_partition(&mut evidence, &partitions[0]);
    assert_eq!(first, second);
}
