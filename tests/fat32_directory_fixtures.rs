//! Root directory enumeration against generated fixtures.
//!
//! Fixtures are not committed. Run `./scripts/generate-fixtures.sh` first.
//! Their expected digests are recorded in
//! `fixtures/partition/MANIFEST.sha256`.
//!
//! The unit tests in `src/fat_directory.rs` build directories in memory and
//! address them with offsets written out by hand. That proves the walking
//! logic and deliberately does not prove the addressing: a test that locates
//! a cluster with the same function it is testing agrees with that function
//! whether or not either is right.
//!
//! These tests read bytes `mkfs.vfat` and `mtools` wrote, at offsets
//! `cluster_offset` and `fat_entry_offset` compute. They are what establishes
//! that the addressing is correct.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::fat_directory::{
    DeletedKind, DirectoryObservation, EntryKind, enumerate_root, recover_first_byte,
};
use taphonomy::fat32::{Fat32BootSector, parse_boot_sector};
use taphonomy::filesystem::{VBR_SIZE, VolumeExtent};
use taphonomy::partition::{PartitionTable, SECTOR_SIZE, parse_mbr};

/// Cluster size of every fixture used here, in directory entries.
///
/// All are formatted at one 512-byte sector per cluster, so a cluster holds
/// sixteen 32-byte entries. Measured with
/// `od -An -tu1 -j 1048589 -N 1 fixtures/partition/fat32-root-entries.img`.
const SLOTS_PER_CLUSTER: usize = 16;

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
///
/// The sector count comes from `reported_size`, which is filesystem metadata
/// rather than a count of bytes actually read. That is acceptable here only
/// because every fixture is a complete regular file, where the two cannot
/// differ. Production code derives the bound from `HashResult::bytes_read`,
/// as `src/main.rs` does.
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

/// The five entry shapes `fat32-root-entries.img` was built to carry, plus
/// the lowercase directory name it carries incidentally.
///
/// Slot order is fixed by the order `mcopy` and `mmd` are invoked in
/// `scripts/generate-fixtures.sh`, which is why these can be asserted
/// positionally.
#[test]
fn every_entry_shape_in_the_fixture_is_classified() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert_eq!(
        root.entries.len(),
        7,
        "eight slots are used, and the terminator is not an entry: {:?}",
        root.entries
    );
    assert_eq!(
        root.short_entry_count(),
        4,
        "three files and the LOGS subdirectory"
    );
    assert_eq!(root.long_name_count(), 2);

    match &root.entries[0].kind {
        EntryKind::VolumeLabel { name, .. } => {
            assert_eq!(name.as_deref(), Some("TAPHFIX"));
        }
        other => panic!("slot 0 must be the volume label, got {other:?}"),
    }

    match &root.entries[6].kind {
        EntryKind::ShortName {
            name,
            directory,
            first_cluster,
            file_size,
            nt_res,
            ..
        } => {
            assert_eq!(name.as_deref(), Some("LOGS"));
            assert!(directory, "mmd created a subdirectory");
            assert_eq!(*first_cluster, 6);
            assert_eq!(*file_size, 0, "a directory records no size");
            assert_eq!(*nt_res, 0x08, "created as ::/logs, lowercase stem");
        }
        other => panic!("slot 6 must be the subdirectory, got {other:?}"),
    }

    assert!(
        root.observations.is_empty(),
        "a conformant directory has nothing to report, got {:?}",
        root.observations
    );
}

/// The volume label carries attribute `0x08`, which is contained in the
/// long-name attribute `0x0F`. A bitmask test for the volume-id bit would
/// classify both long-name entries in this fixture as volume labels.
#[test]
fn the_volume_label_is_not_confused_with_a_long_name() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    let labels = root
        .entries
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::VolumeLabel { .. }))
        .count();

    assert_eq!(labels, 1, "exactly one entry carries 0x08 alone");
}

/// ADR-0007 section 5.4: long-name entries are retained in on-disk order and
/// counted, with their position relative to the following short entry
/// preserved. They precede it, and the last component appears first.
#[test]
fn the_long_name_set_precedes_its_short_entry_in_order() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    match (&root.entries[3].kind, &root.entries[4].kind) {
        (
            EntryKind::LongName {
                ordinal: first_ordinal,
                last,
                checksum: first_checksum,
            },
            EntryKind::LongName {
                ordinal: second_ordinal,
                checksum: second_checksum,
                ..
            },
        ) => {
            assert_eq!(*first_ordinal, 2);
            assert!(last, "the last component is stored first");
            assert_eq!(*second_ordinal, 1);
            assert_eq!(
                first_checksum, second_checksum,
                "both components checksum the same short name"
            );
        }
        other => panic!("slots 3 and 4 must be a long-name set, got {other:?}"),
    }

    match &root.entries[5].kind {
        EntryKind::ShortName { name, .. } => {
            assert_eq!(
                name.as_deref(),
                Some("ANNUAL~1.TXT"),
                "the set names the short entry that follows it"
            );
        }
        other => panic!("slot 5 must be the short entry, got {other:?}"),
    }
}

/// `DIR_NTRes` is recorded and not applied. The specification instructs
/// readers not to look at it; `mtools` writes case flags into it. Recording
/// the byte without acting on it preserves both facts.
#[test]
fn a_lowercase_name_is_recorded_through_nt_res_and_not_applied() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    match &root.entries[2].kind {
        EntryKind::ShortName {
            name,
            nt_res,
            first_cluster,
            file_size,
            ..
        } => {
            assert_eq!(
                *nt_res, 0x18,
                "copied as ::/readme.md, both parts lowercase"
            );
            assert_eq!(
                name.as_deref(),
                Some("README.MD"),
                "the name is rendered as stored, not as the flags suggest"
            );
            assert_eq!(*first_cluster, 4);
            assert_eq!(*file_size, 25);
        }
        other => panic!("slot 2 must be README.MD, got {other:?}"),
    }
}

/// The stem and extension are space-padded separately. Trimming the eleven
/// bytes as one field would produce `HELLO   TXT`.
#[test]
fn a_short_name_is_rendered_with_its_implied_dot() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    match &root.entries[1].kind {
        EntryKind::ShortName {
            name,
            first_cluster,
            file_size,
            nt_res,
            ..
        } => {
            assert_eq!(name.as_deref(), Some("HELLO.TXT"));
            assert_eq!(*first_cluster, 3);
            assert_eq!(*file_size, 24);
            assert_eq!(*nt_res, 0x00, "copied as ::/HELLO.TXT, already uppercase");
        }
        other => panic!("slot 1 must be HELLO.TXT, got {other:?}"),
    }
}

/// This fixture's root directory occupies one cluster, and `mkfs.vfat`
/// terminates it with `0x0FFFFFF8` rather than `0x0FFFFFFF`. An end-of-chain
/// test written as equality against `0x0FFFFFFF` would read that as cluster
/// 268,435,448 and walk off the volume here.
#[test]
fn a_single_cluster_root_ends_at_its_first_cluster() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert_eq!(root.clusters, vec![boot.root_cluster]);
    assert_eq!(boot.root_cluster, 2);
}

/// The fixture that exists because no other one exercises chain walking.
/// ADR-0008 section 8.1.
#[test]
fn a_multicluster_root_follows_its_chain() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-multicluster.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert_eq!(
        root.clusters,
        vec![2, 19],
        "the root directory grows into cluster 19, not the adjacent cluster"
    );
    assert_eq!(root.entries.len(), 21, "volume label plus twenty files");
    assert_eq!(root.short_entry_count(), 20);
    assert_eq!(
        root.long_name_count(),
        0,
        "pure 8.3 names produce no long-name entries"
    );
    assert!(root.observations.is_empty(), "got {:?}", root.observations);
}

/// The first cluster is completely full, so it contains no terminator at
/// all. An enumeration that stopped at the end of a cluster rather than
/// following the chain would report sixteen entries and never encounter a
/// terminator to justify stopping.
#[test]
fn a_full_cluster_is_not_treated_as_terminated() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-multicluster.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    let in_first = root.entries.iter().filter(|e| e.cluster == 2).count();

    assert_eq!(
        in_first, SLOTS_PER_CLUSTER,
        "every slot of the first cluster is used"
    );
    assert_eq!(
        root.entries[SLOTS_PER_CLUSTER].cluster, 19,
        "the seventeenth entry comes from the second cluster"
    );
    assert_eq!(
        root.entries[SLOTS_PER_CLUSTER].slot, 0,
        "and from its first slot"
    );
}

/// Cluster 19 sits between file data: cluster 18 holds FILE16's content and
/// cluster 20 holds FILE17's. A walk that read the adjacent cluster instead
/// of following the FAT would read file text as directory entries.
#[test]
fn the_second_cluster_is_not_adjacent_to_the_first() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-multicluster.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    let second = root.clusters[1];
    assert_ne!(
        second,
        root.clusters[0] + 1,
        "the fixture would not discriminate if the chain were contiguous"
    );

    match &root.entries[SLOTS_PER_CLUSTER].kind {
        EntryKind::ShortName { name, .. } => {
            assert_eq!(
                name.as_deref(),
                Some("FILE16.TXT"),
                "reading cluster {} yielded directory entries, not file content",
                second
            );
        }
        other => panic!("expected a short name, got {other:?}"),
    }
}

/// Enumeration must not modify the evidence. `docs/SAFETY.md` section 4.
#[test]
fn enumeration_does_not_modify_evidence() {
    let path = fixture("fat32-root-multicluster.img");

    let before = std::fs::metadata(&path).expect("metadata before");
    let before_mtime = before.modified().expect("mtime before");

    let (mut evidence, boot, extent) = open_volume("fat32-root-multicluster.img");
    let _ = enumerate_root(&mut evidence, &boot, extent);

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
fn enumeration_is_deterministic() {
    let (mut evidence, boot, extent) = open_volume("fat32-root-entries.img");

    let first = enumerate_root(&mut evidence, &boot, extent).expect("first enumeration");
    let second = enumerate_root(&mut evidence, &boot, extent).expect("second enumeration");

    assert_eq!(first, second);
}

/// Every deleted shape `fat32-deleted-entries.img` was built to carry.
///
/// Slot order is fixed by the order `mcopy`, `mdel` and `mrd` are invoked in
/// `scripts/generate-fixtures.sh`, which is why these are asserted
/// positionally.
#[test]
fn every_deleted_shape_in_the_fixture_is_classified() {
    let (mut evidence, boot, extent) = open_volume("fat32-deleted-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert_eq!(
        root.entries.len(),
        11,
        "eleven slots are used, and the terminator is not an entry: {:?}",
        root.entries
    );

    let deleted = root
        .entries
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::Deleted { .. }))
        .count();

    assert_eq!(
        deleted, 7,
        "three long-name components and four short entries were deleted"
    );

    let EntryKind::Deleted {
        was:
            DeletedKind::ShortName {
                surviving_name,
                directory,
                first_cluster,
                file_size,
                ..
            },
    } = &root.entries[4].kind
    else {
        panic!(
            "slot 4 must be a deleted file, got {:?}",
            root.entries[4].kind
        );
    };

    assert_eq!(
        surviving_name, b"ARTIA~1TXT",
        "ten bytes survive, not eleven"
    );
    assert!(!*directory);
    assert_eq!(*first_cluster, 4);
    assert_eq!(*file_size, 30);

    let EntryKind::Deleted {
        was:
            DeletedKind::ShortName {
                surviving_name,
                directory,
                first_cluster,
                nt_res,
                ..
            },
    } = &root.entries[5].kind
    else {
        panic!(
            "slot 5 must be a deleted directory, got {:?}",
            root.entries[5].kind
        );
    };

    assert_eq!(
        surviving_name, b"ONE       ",
        "the directory was named gone"
    );
    assert!(*directory, "mrd removed a subdirectory, not a file");
    assert_eq!(*first_cluster, 5);
    assert_eq!(*nt_res, 0x08, "created as ::/gone, lowercase stem");
}

/// The destroyed byte is determined by the checksum, not narrowed by it.
///
/// Both sets recover the byte `mtools` wrote before `mdel` overwrote it:
/// `PARTIA~1.TXT` and `COMPLE~1.TXT`. ADR-0009 section 6.1.
#[test]
fn a_deleted_first_byte_is_recovered_from_its_long_name_set() {
    let (mut evidence, boot, extent) = open_volume("fat32-deleted-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    for (component, short, expected) in [(3, 4, b'P'), (7, 8, b'C')] {
        let EntryKind::Deleted {
            was: DeletedKind::LongName { checksum },
        } = &root.entries[component].kind
        else {
            panic!("slot {component} must be a deleted long-name component");
        };

        let EntryKind::Deleted {
            was: DeletedKind::ShortName { surviving_name, .. },
        } = &root.entries[short].kind
        else {
            panic!("slot {short} must be a deleted short entry");
        };

        assert_eq!(
            recover_first_byte(surviving_name, *checksum),
            Some(expected),
            "recovering slot {short} from the checksum in slot {component}"
        );
    }
}

/// Slot reuse consumed one component of the first set and neither of the
/// second. Nothing in the evidence says so: the ordinals that counted the
/// components are destroyed, so only position distinguishes them.
/// ADR-0009 section 5.3.
#[test]
fn a_partial_long_name_set_looks_like_a_complete_one() {
    let (mut evidence, boot, extent) = open_volume("fat32-deleted-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    let components = |mut slot: usize| {
        let mut n = 0;
        while slot > 0 {
            slot -= 1;
            if !matches!(
                root.entries[slot].kind,
                EntryKind::Deleted {
                    was: DeletedKind::LongName { .. }
                }
            ) {
                break;
            }
            n += 1;
        }
        n
    };

    assert_eq!(components(4), 1, "one component survived reuse");
    assert_eq!(components(8), 2, "both components survived");

    assert!(
        matches!(root.entries[2].kind, EntryKind::ShortName { .. }),
        "a live entry occupies the slot the missing component held"
    );
}

/// Slot 10's first byte cannot be recovered by any method in this milestone.
/// The entry before it is live, so no long-name set belongs to it and no
/// checksum constrains the destroyed byte. It is reported as deleted and no
/// name is offered. ADR-0009 section 6.4.
#[test]
fn a_deleted_entry_without_a_long_name_set_offers_no_first_byte() {
    let (mut evidence, boot, extent) = open_volume("fat32-deleted-entries.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert!(
        matches!(root.entries[9].kind, EntryKind::ShortName { .. }),
        "the entry before slot 10 is live, so slot 10 has no set"
    );

    let EntryKind::Deleted {
        was: DeletedKind::ShortName { surviving_name, .. },
    } = &root.entries[10].kind
    else {
        panic!("slot 10 must be a deleted short entry");
    };

    assert_eq!(surviving_name, b"LAIN   TXT");
}

/// The residue fixture stops at a terminator that a poked byte created, and
/// reports the entries beyond it rather than discarding them.
///
/// Those entries are not yet listed. Making them addressable is ADR-0009
/// Decision B, which is not implemented here.
#[test]
fn the_residue_fixture_ends_early_and_reports_what_follows() {
    let (mut evidence, boot, extent) = open_volume("fat32-deleted-residue.img");
    let root = enumerate_root(&mut evidence, &boot, extent).expect("enumerating");

    assert_eq!(
        root.entries.len(),
        5,
        "the listing ends at the poked terminator in slot 5"
    );

    // Slot 5 is that terminator, and its own remaining bytes survive: it held
    // a deleted directory entry before one byte was set to zero. They are
    // neither listed nor counted, because the terminator branch consumes the
    // slot before the residue branch sees it.
    assert_eq!(
        root.observations,
        vec![DirectoryObservation::ContentAfterTerminator {
            cluster: 2,
            first_slot: 6,
            slots: 5,
        }],
        "five slots past the terminator still hold entries"
    );

    assert_eq!(root.residue.len(), 5, "and all five are now classified");
    assert_eq!(root.residue[0].slot, 6);
    assert_eq!(root.residue[4].slot, 10);

    // The complete long-name set and the short entry it names both lie past
    // the terminator, so the recovery works on residue exactly as it works
    // on the listing.
    let EntryKind::Deleted {
        was: DeletedKind::LongName { checksum },
    } = &root.residue[1].kind
    else {
        panic!("residue slot 7 must be a deleted long-name component");
    };

    let EntryKind::Deleted {
        was: DeletedKind::ShortName { surviving_name, .. },
    } = &root.residue[2].kind
    else {
        panic!("residue slot 8 must be a deleted short entry");
    };

    assert_eq!(recover_first_byte(surviving_name, *checksum), Some(b'C'));

    assert!(
        matches!(root.residue[3].kind, EntryKind::ShortName { .. }),
        "a live entry sits past the terminator too, which is why residue is \
         not simply a list of deleted entries"
    );
}
