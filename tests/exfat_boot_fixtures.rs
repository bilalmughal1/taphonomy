//! The exFAT boot region against the two images EXP-0009 measured.
//!
//! The images are committed compressed and expanded by
//! `./scripts/generate-fixtures.sh`; they are the first test inputs not
//! generated from ordinary filesystem tools (ADR-0019 Appendix A). Windows
//! formatted them, so these tests read a boot region this project did not
//! write.
//!
//! The expected values come from the EXP-0009 analysis script, which parses
//! the same bytes independently, and from The Sleuth Kit's `fsstat`
//! (4.12.1), not from the code under test.

use std::path::{Path, PathBuf};

use taphonomy::EvidenceFile;
use taphonomy::exfat_boot::{
    BootRegion, ExFatBootFailure, ExFatError, boot_checksum, parse_boot_region, validate_region,
};
use taphonomy::filesystem::VolumeExtent;

/// Where EXP-0009's partition begins and how long it is, in 512-byte sectors.
const EXTENT: VolumeExtent = VolumeExtent {
    start_lba: 128,
    sector_count: 30_592,
};

/// The boot checksum both images carry, computed by the analysis script.
const CHECKSUM: u32 = 0x02C5_82C6;

fn fixture(name: &str) -> PathBuf {
    let path = Path::new("fixtures/partition").join(name);
    assert!(
        path.exists(),
        "fixture missing: {}\nrun ./scripts/generate-fixtures.sh first",
        path.display()
    );
    path
}

fn open(name: &str) -> EvidenceFile {
    EvidenceFile::open(fixture(name)).expect("opening fixture")
}

#[test]
fn both_images_boot_from_the_main_region_with_the_geometry_the_script_reports() {
    for name in ["exp9-A.vhd", "exp9-B.vhd"] {
        let mut evidence = open(name);
        let boot = parse_boot_region(&mut evidence, EXTENT).expect("a valid boot region");

        assert_eq!(boot.region, BootRegion::Main, "{name}");
        assert_eq!(boot.bytes_per_sector(), 512, "{name}");
        assert_eq!(boot.cluster_bytes(), 4096, "{name}");
        assert_eq!(boot.volume_length, 30_592, "{name}");
        assert_eq!(boot.fat_offset, 128, "{name}");
        assert_eq!(boot.fat_length, 32, "{name}");
        assert_eq!(boot.cluster_heap_offset, 256, "{name}");
        assert_eq!(boot.cluster_count, 3792, "{name}");
        assert_eq!(boot.root_cluster, 5, "{name}");
        assert_eq!(boot.revision_minor, 0, "{name}");
        assert_eq!(boot.volume_flags, Some(0), "{name}");
        assert!(boot.observations.is_empty(), "{name}");
    }
}

/// Windows changed PercentInUse between the two images, and a boot checksum
/// that included it would have changed too.
#[test]
fn the_percent_in_use_differs_between_the_images_and_the_checksum_does_not() {
    let percent = |name: &str| {
        let mut evidence = open(name);
        parse_boot_region(&mut evidence, EXTENT)
            .expect("a valid boot region")
            .percent_in_use
    };

    assert_eq!(percent("exp9-A.vhd"), Some(52));
    assert_eq!(percent("exp9-B.vhd"), Some(49));
}

#[test]
fn the_checksum_of_both_regions_of_both_images_is_the_one_the_script_computed() {
    for name in ["exp9-A.vhd", "exp9-B.vhd"] {
        let mut evidence = open(name);
        let start = EXTENT.start_lba as u64 * 512;

        let mut region = vec![0u8; 24 * 512];
        evidence
            .read_exact_at(start, &mut region)
            .expect("reading both boot regions");

        assert_eq!(
            boot_checksum(&region[..12 * 512], 9),
            CHECKSUM,
            "{name} main"
        );
        assert_eq!(
            boot_checksum(&region[12 * 512..], 9),
            CHECKSUM,
            "{name} backup"
        );
    }
}

/// Both regions are valid on both images. The main region is damaged in
/// memory, and the two regions are then validated separately: the damaged one
/// is refused and the real backup validates as a backup. This does not
/// exercise `parse_boot_region`'s fallback, which the unit tests cover. The
/// image on disk is not touched.
#[test]
fn the_real_backup_region_validates_as_a_backup_when_the_main_is_damaged() {
    let mut evidence = open("exp9-A.vhd");
    let start = EXTENT.start_lba as u64 * 512;
    let mut region = vec![0u8; 12 * 512];
    evidence
        .read_exact_at(start, &mut region)
        .expect("reading the main region");

    region[100] ^= 0xFF;
    assert!(matches!(
        validate_region(&region, BootRegion::Main, EXTENT),
        Err(ExFatError::ChecksumMismatch { .. })
    ));

    let mut backup = vec![0u8; 12 * 512];
    evidence
        .read_exact_at(start + 12 * 512, &mut backup)
        .expect("reading the backup region");
    let boot = validate_region(&backup, BootRegion::Backup, EXTENT).expect("the backup is valid");
    assert_eq!(boot.region, BootRegion::Backup);
    assert_eq!(boot.cluster_count, 3792);
    assert_eq!(boot.volume_flags, None);
}

/// The same image read as a partition one sector longer than the volume
/// observes the difference, and one sector shorter refuses it.
#[test]
fn the_partition_length_is_compared_with_the_volumes_in_bytes() {
    let mut evidence = open("exp9-A.vhd");

    let longer = VolumeExtent {
        sector_count: EXTENT.sector_count + 1,
        ..EXTENT
    };
    let boot = parse_boot_region(&mut evidence, longer).expect("a shorter volume is analysed");
    assert_eq!(boot.observations.len(), 1);

    let shorter = VolumeExtent {
        sector_count: EXTENT.sector_count - 1,
        ..EXTENT
    };
    match parse_boot_region(&mut evidence, shorter) {
        Err(ExFatBootFailure::Rejected { main, backup }) => {
            assert!(matches!(main, ExFatError::VolumeLongerThanPartition { .. }));
            assert!(matches!(
                backup,
                ExFatError::VolumeLongerThanPartition { .. }
            ));
        }
        other => panic!("expected the volume refused, got {other:?}"),
    }
}
