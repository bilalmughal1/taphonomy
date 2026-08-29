//! Partition parsing against generated fixtures.
//!
//! Fixtures are not committed. Run `./scripts/generate-fixtures.sh` before
//! these tests. Their expected digests are recorded in
//! `fixtures/partition/MANIFEST.sha256`.
//!
//! Five of the seven fixtures exist to make the parser fail. A parser tested
//! only against valid input has been demonstrated, not tested.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::partition::{Anomaly, ParseError, PartitionTable, SECTOR_SIZE, parse_mbr};

fn fixture(name: &str) -> PathBuf {
    let path = Path::new("fixtures/partition").join(name);
    assert!(
        path.exists(),
        "fixture missing: {}\nrun ./scripts/generate-fixtures.sh first",
        path.display()
    );
    path
}

/// Reads the first sector and the evidence size in sectors.
fn first_sector(name: &str) -> ([u8; SECTOR_SIZE], u64) {
    let mut evidence = EvidenceFile::open(fixture(name)).expect("opening fixture");
    let sectors = evidence.reported_size() / SECTOR_SIZE as u64;

    let mut sector = [0u8; SECTOR_SIZE];
    evidence
        .read_exact_at(0, &mut sector)
        .expect("reading first sector");

    (sector, sectors)
}

#[test]
fn single_fat32_partition_is_parsed() {
    let (sector, sectors) = first_sector("mbr-single-fat32.img");
    let outcome = parse_mbr(&sector, sectors).expect("valid MBR");

    let PartitionTable::Mbr {
        disk_signature,
        partitions,
    } = outcome.table
    else {
        panic!("expected an MBR table");
    };

    assert_eq!(disk_signature, 0x1a2b_3c4d);
    assert_eq!(partitions.len(), 1);

    let p = partitions[0];
    assert_eq!(p.index, 1);
    assert!(p.is_bootable());
    assert_eq!(p.partition_type, 0x0c, "W95 FAT32 (LBA)");
    assert_eq!(p.start_lba, 2048);
    assert_eq!(p.sector_count, 129_024);
    assert_eq!(p.start_byte(), 1_048_576);
}

#[test]
fn four_partitions_are_all_parsed() {
    let (sector, sectors) = first_sector("mbr-four-partitions.img");
    let outcome = parse_mbr(&sector, sectors).expect("valid MBR");

    let PartitionTable::Mbr { partitions, .. } = outcome.table else {
        panic!("expected an MBR table");
    };

    assert_eq!(partitions.len(), 4);
    assert_eq!(partitions[0].partition_type, 0x0c);
    assert_eq!(partitions[1].partition_type, 0x0c);
    assert_eq!(partitions[2].partition_type, 0x83);
    assert_eq!(partitions[3].partition_type, 0x83);

    for pair in partitions.windows(2) {
        assert!(
            pair[0].start_lba < pair[1].start_lba,
            "fixture partitions should be in ascending order"
        );
    }

    assert!(
        !outcome
            .anomalies
            .iter()
            .any(|a| matches!(a, Anomaly::OverlappingPartitions { .. })),
        "sfdisk-generated partitions must not overlap"
    );
}

#[test]
fn empty_table_is_valid_and_empty() {
    let (sector, sectors) = first_sector("mbr-empty.img");
    let outcome = parse_mbr(&sector, sectors).expect("an empty table is valid");

    let PartitionTable::Mbr { partitions, .. } = outcome.table else {
        panic!("expected an MBR table");
    };
    assert!(partitions.is_empty());
}

#[test]
fn absent_signature_is_rejected() {
    let (sector, sectors) = first_sector("no-signature.img");
    let err = parse_mbr(&sector, sectors).unwrap_err();
    assert_eq!(
        err,
        ParseError::MissingSignature {
            found: [0x00, 0x00]
        }
    );
}

/// The dangerous case. A valid partition entry with a corrupted signature
/// must be rejected rather than parsed optimistically.
#[test]
fn corrupted_signature_is_rejected_despite_valid_entry() {
    let (sector, sectors) = first_sector("bad-signature.img");

    // The entry itself is intact.
    assert_eq!(sector[0x1BE + 4], 0x0c, "fixture should have a valid entry");

    let err = parse_mbr(&sector, sectors).unwrap_err();
    assert!(matches!(err, ParseError::MissingSignature { .. }));
}

/// A partition declaring more sectors than the evidence contains must be
/// rejected. DEVELOPMENT_ENVIRONMENT.md section 18.
#[test]
fn partition_beyond_end_of_evidence_is_rejected() {
    let (sector, sectors) = first_sector("partition-beyond-end.img");
    assert_eq!(sectors, 131_072);

    let err = parse_mbr(&sector, sectors).unwrap_err();
    match err {
        ParseError::PartitionBeyondEnd {
            index,
            start_lba,
            sector_count,
            image_sectors,
        } => {
            assert_eq!(index, 1);
            assert_eq!(start_lba, 2048);
            assert_eq!(sector_count, u32::MAX);
            assert_eq!(image_sectors, 131_072);
        }
        other => panic!("expected PartitionBeyondEnd, got {other:?}"),
    }
}

/// A GPT disk carries a valid protective MBR. Reporting its 0xEE entry as a
/// real whole-disk partition would be a false positive on every GPT disk.
/// ADR-0005 section 4.
#[test]
fn gpt_protective_mbr_is_detected_not_misreported() {
    let (sector, sectors) = first_sector("gpt-protective.img");

    // The protective entry is present and spans the disk.
    assert_eq!(sector[0x1BE + 4], 0xEE);

    let outcome = parse_mbr(&sector, sectors).expect("a protective MBR parses");
    assert_eq!(
        outcome.table,
        PartitionTable::GptProtective,
        "a GPT protective MBR must not be reported as a real partition"
    );
}

/// Parsing must not modify the evidence.
#[test]
fn parsing_does_not_modify_evidence() {
    let path = fixture("mbr-single-fat32.img");

    let before = std::fs::metadata(&path).expect("metadata before");
    let before_mtime = before.modified().expect("mtime before");

    let mut evidence = EvidenceFile::open(&path).expect("opening evidence");
    let mut sector = [0u8; SECTOR_SIZE];
    evidence.read_exact_at(0, &mut sector).expect("reading");
    let _ = parse_mbr(&sector, evidence.reported_size() / SECTOR_SIZE as u64);

    let after = std::fs::metadata(&path).expect("metadata after");
    assert_eq!(before.len(), after.len(), "size changed");
    assert_eq!(
        before_mtime,
        after.modified().expect("mtime after"),
        "modification time changed"
    );
}

/// Parsing must be repeatable. PROJECT.md section 6.4.
#[test]
fn parsing_is_deterministic() {
    let (sector, sectors) = first_sector("mbr-four-partitions.img");
    let first = parse_mbr(&sector, sectors).expect("first parse");
    let second = parse_mbr(&sector, sectors).expect("second parse");
    assert_eq!(first, second);
}
