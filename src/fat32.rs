//! FAT32 boot sector parsing and validation against the volume's extent.
//!
//! # What this adds over identification
//!
//! [`crate::filesystem::identify`] receives one sector. It can establish that
//! a BIOS parameter block is internally consistent, but it cannot establish
//! that the volume fits the space it occupies, because it does not know where
//! that space begins or how large it is.
//!
//! This module receives the extent as well. A volume declaring 200,000
//! sectors inside a 129,024-sector partition is contradictory in a way a
//! single sector cannot reveal.
//!
//! # FAT32-only fields
//!
//! `fat.rs` reads the fields common to FAT12, FAT16 and FAT32, because those
//! are what variant determination needs. The fields read here exist only on
//! FAT32 and are meaningless on the other two.

use std::fmt;

use crate::fat::{FatGeometry, OFF_FAT_SIZE_16};
use crate::filesystem::{
    Filesystem, Identification, VBR_SIZE, VolumeExtent, identify, le_u16, le_u32, printable_ascii,
};

pub(crate) const OFF_HIDDEN_SECTORS: usize = 0x1C;
pub(crate) const OFF_EXT_FLAGS: usize = 0x28;
pub(crate) const OFF_FS_VERSION: usize = 0x2A;
pub(crate) const OFF_ROOT_CLUSTER: usize = 0x2C;
pub(crate) const OFF_FS_INFO_SECTOR: usize = 0x30;
pub(crate) const OFF_BACKUP_BOOT_SECTOR: usize = 0x32;
pub(crate) const OFF_BOOT_SIGNATURE: usize = 0x42;
pub(crate) const OFF_VOLUME_ID: usize = 0x43;
pub(crate) const OFF_VOLUME_LABEL: usize = 0x47;

/// Value of `BS_BootSig` indicating the volume ID and label fields are
/// present. Any other value means those fields carry no meaning.
const BOOT_SIGNATURE_PRESENT: u8 = 0x29;

/// Bit 7 of `BPB_ExtFlags`. Set means one FAT is active and the others are
/// not maintained. Clear means all FATs are mirrored.
const EXT_FLAGS_MIRRORING_DISABLED: u16 = 0x0080;

/// Bits 0-3 of `BPB_ExtFlags`, the zero-based index of the active FAT.
const EXT_FLAGS_ACTIVE_FAT: u16 = 0x000F;

/// Cluster numbers 0 and 1 are reserved. The first addressable cluster is 2.
const FIRST_DATA_CLUSTER: u32 = 2;

/// Bytes in one FAT32 file allocation table entry.
const FAT32_ENTRY_BYTES: u64 = 4;

/// A reason a volume boot record is not a usable FAT32 boot sector.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fat32Error {
    /// The sector does not describe a FAT32 volume.
    NotFat32 { identified: Option<Filesystem> },
    /// Sector size is valid per the specification but not supported here.
    ///
    /// Partition offsets are computed in 512-byte sectors. Mixing those with
    /// intra-volume offsets in another unit would produce offsets that are
    /// wrong without being detectably wrong.
    UnsupportedSectorSize { bytes: u16 },
    /// A field the specification requires to hold a particular value does
    /// not hold it.
    InvalidField { field: &'static str, value: u64 },
    /// The volume declares more sectors than its extent contains.
    VolumeExceedsExtent {
        declared_sectors: u32,
        extent_sectors: u32,
    },
    /// The root directory cluster is outside the data region.
    InvalidRootCluster { cluster: u32, cluster_count: u32 },
    /// The file allocation table is too small to hold an entry for every
    /// cluster the volume declares.
    FatTooSmall { fat_bytes: u64, required_bytes: u64 },
}

impl fmt::Display for Fat32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fat32Error::NotFat32 {
                identified: Some(fs),
            } => write!(f, "volume is {fs}, not FAT32"),
            Fat32Error::NotFat32 { identified: None } => {
                f.write_str("volume could not be identified as any filesystem")
            }
            Fat32Error::UnsupportedSectorSize { bytes } => {
                write!(f, "sector size {bytes} is not supported, only 512 bytes is")
            }
            Fat32Error::InvalidField { field, value } => {
                write!(f, "invalid FAT32 field {field}: {value}")
            }
            Fat32Error::VolumeExceedsExtent {
                declared_sectors,
                extent_sectors,
            } => write!(
                f,
                "volume declares {declared_sectors} sectors but its extent holds {extent_sectors}"
            ),
            Fat32Error::InvalidRootCluster {
                cluster,
                cluster_count,
            } => write!(
                f,
                "root directory cluster {cluster} is outside the {cluster_count} data clusters"
            ),
            Fat32Error::FatTooSmall {
                fat_bytes,
                required_bytes,
            } => write!(
                f,
                "file allocation table is {fat_bytes} bytes, {required_bytes} required"
            ),
        }
    }
}

/// Something worth reporting that does not prevent the boot sector being
/// used.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fat32Observation {
    /// `BPB_HiddSec` disagrees with where the volume actually begins.
    ///
    /// The volume records its own starting sector. Disagreement is evidence
    /// the image was carved, the partition table was rebuilt, or the volume
    /// was copied from a different layout. A declared zero is exempt: it
    /// means the field was never recorded, which is what mkfs.vfat writes
    /// for a volume formatted at an offset.
    HiddenSectorsDisagree { declared: u32, actual: u32 },
    /// The volume occupies less than its extent provides.
    ///
    /// Normal after a partition is enlarged or a smaller volume is written
    /// into an existing partition. The trailing sectors are outside the
    /// filesystem.
    VolumeSmallerThanExtent {
        declared_sectors: u32,
        extent_sectors: u32,
    },
    /// Only one FAT is maintained; the others are stale.
    ///
    /// Any later read of allocation data must use the active FAT. Reading
    /// FAT 0 by default would return unmaintained data.
    FatMirroringDisabled { active_fat: u8 },
}

impl fmt::Display for Fat32Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fat32Observation::HiddenSectorsDisagree { declared, actual } => write!(
                f,
                "volume records its start as sector {declared}, but it begins at {actual}"
            ),
            Fat32Observation::VolumeSmallerThanExtent {
                declared_sectors,
                extent_sectors,
            } => write!(
                f,
                "volume occupies {declared_sectors} of the {extent_sectors} sectors available"
            ),
            Fat32Observation::FatMirroringDisabled { active_fat } => write!(
                f,
                "FAT mirroring is disabled, only FAT {active_fat} is maintained"
            ),
        }
    }
}

/// A validated FAT32 boot sector.
///
/// Every field is as declared in the volume, and has been checked against
/// the specification and against the volume's extent. Reaching this type
/// does not establish that the data the fields point at is intact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fat32BootSector {
    /// Geometry shared with the other FAT variants.
    pub geometry: FatGeometry,
    /// Sectors preceding the volume, as the volume records them.
    pub hidden_sectors: u32,
    /// Raw `BPB_ExtFlags`. Interpreted by [`Fat32BootSector::active_fat`].
    pub ext_flags: u16,
    /// First cluster of the root directory.
    pub root_cluster: u32,
    /// Sector of the FSInfo structure, relative to the volume start.
    ///
    /// Located and bounds-checked. Its contents are not read: the free
    /// cluster count it caches is advisory and frequently stale.
    pub fs_info_sector: u16,
    /// Sector of the backup boot record, relative to the volume start.
    ///
    /// Located and bounds-checked. Not read, and not compared against this
    /// sector. Preferring a backup over a damaged primary is a recovery
    /// decision, not a parsing one.
    pub backup_boot_sector: u16,
    /// Volume serial number, if `BS_BootSig` marks it present.
    pub volume_id: Option<u32>,
    /// Volume label, if present and printable.
    pub volume_label: Option<String>,
    /// Findings that did not prevent parsing.
    pub observations: Vec<Fat32Observation>,
}

impl Fat32BootSector {
    /// Whether every FAT is kept in step with the others.
    pub const fn mirroring_enabled(&self) -> bool {
        self.ext_flags & EXT_FLAGS_MIRRORING_DISABLED == 0
    }

    /// The FAT that carries current allocation data.
    ///
    /// `None` when mirroring is enabled, because then every FAT is current
    /// and the active-FAT bits carry no meaning.
    pub const fn active_fat(&self) -> Option<u8> {
        if self.mirroring_enabled() {
            None
        } else {
            Some((self.ext_flags & EXT_FLAGS_ACTIVE_FAT) as u8)
        }
    }

    /// First sector of the data region, relative to the evidence rather than
    /// to the volume.
    pub const fn first_data_sector_absolute(&self, extent: VolumeExtent) -> u64 {
        extent.start_lba as u64 + self.geometry.first_data_sector() as u64
    }
}

/// Parses and validates a FAT32 boot sector against the extent it occupies.
///
/// Performs no I/O. `sector` is the first sector of the volume; `extent` is
/// where that volume sits in the evidence.
///
/// Identification is delegated to [`crate::filesystem::identify`] so that
/// there is never a second parser reading these bytes to a different
/// conclusion.
pub fn parse_boot_sector(
    sector: &[u8],
    extent: VolumeExtent,
) -> Result<Fat32BootSector, Fat32Error> {
    let id = identify(sector);

    let Identification::Identified {
        filesystem: Filesystem::Fat32,
        geometry: Some(geometry),
        ..
    } = id
    else {
        return Err(Fat32Error::NotFat32 {
            identified: id.filesystem(),
        });
    };

    // identify accepts every sector size the specification permits. Beyond
    // this point offsets are computed, and partition offsets are already
    // fixed at 512 bytes. Refuse rather than mix units.
    if geometry.bytes_per_sector != 512 {
        return Err(Fat32Error::UnsupportedSectorSize {
            bytes: geometry.bytes_per_sector,
        });
    }

    // A FAT32 volume has no fixed-size root directory and no 16-bit FAT
    // size. Both fields must be zero. identify tolerates them because it
    // serves all three variants.
    if geometry.root_entry_count != 0 {
        return Err(Fat32Error::InvalidField {
            field: "root_entry_count",
            value: geometry.root_entry_count as u64,
        });
    }

    let fat_size_16 = le_u16(sector, OFF_FAT_SIZE_16);
    if fat_size_16 != 0 {
        return Err(Fat32Error::InvalidField {
            field: "fat_size_16",
            value: fat_size_16 as u64,
        });
    }

    let fs_version = le_u16(sector, OFF_FS_VERSION);
    if fs_version != 0 {
        return Err(Fat32Error::InvalidField {
            field: "fs_version",
            value: fs_version as u64,
        });
    }

    // BPB_Reserved at 0x34, twelve bytes, is not checked. The
    // specification asks formatters to zero it but places no requirement
    // on readers, so a non-zero value describes a volume that is still
    // entirely readable. Refusing it would fail closed on recoverable
    // evidence, and reporting it would be an observation with no action
    // attached to it.

    // Both structures live in the reserved region, before the first FAT.
    // A sector number outside it points at data the volume is using for
    // something else.
    let reserved_sectors = geometry.reserved_sectors;

    let fs_info_sector = le_u16(sector, OFF_FS_INFO_SECTOR);
    if fs_info_sector == 0 || fs_info_sector >= reserved_sectors {
        return Err(Fat32Error::InvalidField {
            field: "fs_info_sector",
            value: fs_info_sector as u64,
        });
    }

    let backup_boot_sector = le_u16(sector, OFF_BACKUP_BOOT_SECTOR);
    if backup_boot_sector == 0 || backup_boot_sector >= reserved_sectors {
        return Err(Fat32Error::InvalidField {
            field: "backup_boot_sector",
            value: backup_boot_sector as u64,
        });
    }

    // The check identify structurally cannot make.
    if geometry.total_sectors > extent.sector_count {
        return Err(Fat32Error::VolumeExceedsExtent {
            declared_sectors: geometry.total_sectors,
            extent_sectors: extent.sector_count,
        });
    }

    let root_cluster = le_u32(sector, OFF_ROOT_CLUSTER);
    let last_cluster = geometry.cluster_count as u64 + FIRST_DATA_CLUSTER as u64;
    if (root_cluster as u64) < FIRST_DATA_CLUSTER as u64 || root_cluster as u64 >= last_cluster {
        return Err(Fat32Error::InvalidRootCluster {
            cluster: root_cluster,
            cluster_count: geometry.cluster_count,
        });
    }

    // Every cluster needs an entry, and entries 0 and 1 are reserved. A FAT
    // too small to hold them cannot describe the volume it belongs to.
    let fat_bytes = geometry.fat_size as u64 * geometry.bytes_per_sector as u64;
    let required_bytes =
        (geometry.cluster_count as u64 + FIRST_DATA_CLUSTER as u64) * FAT32_ENTRY_BYTES;
    if fat_bytes < required_bytes {
        return Err(Fat32Error::FatTooSmall {
            fat_bytes,
            required_bytes,
        });
    }

    let mut observations = Vec::new();

    if geometry.total_sectors < extent.sector_count {
        observations.push(Fat32Observation::VolumeSmallerThanExtent {
            declared_sectors: geometry.total_sectors,
            extent_sectors: extent.sector_count,
        });
    }

    // A declared zero means the formatting tool did not record the field.
    // mkfs.vfat writes zero for any volume it formats at an offset, so
    // treating that as disagreement would fire on conformant evidence and
    // teach the reader to ignore the category. A non-zero value that does
    // not match is a genuine finding: the volume records a start it does
    // not have.
    let hidden_sectors = le_u32(sector, OFF_HIDDEN_SECTORS);
    if hidden_sectors != 0 && hidden_sectors != extent.start_lba {
        observations.push(Fat32Observation::HiddenSectorsDisagree {
            declared: hidden_sectors,
            actual: extent.start_lba,
        });
    }

    let ext_flags = le_u16(sector, OFF_EXT_FLAGS);
    if ext_flags & EXT_FLAGS_MIRRORING_DISABLED != 0 {
        // Bits 0-3 index the active FAT. An index at or beyond the
        // declared FAT count names a table that does not exist, and any
        // offset computed from it would land in the data region and be
        // read as allocation entries.
        let active = (ext_flags & EXT_FLAGS_ACTIVE_FAT) as u8;
        if active >= geometry.fat_count {
            return Err(Fat32Error::InvalidField {
                field: "active_fat",
                value: active as u64,
            });
        }
        observations.push(Fat32Observation::FatMirroringDisabled { active_fat: active });
    }

    // The label fields carry meaning only when the boot signature says so.
    let (volume_id, volume_label) = if sector[OFF_BOOT_SIGNATURE] == BOOT_SIGNATURE_PRESENT {
        (
            Some(le_u32(sector, OFF_VOLUME_ID)),
            printable_ascii(&sector[OFF_VOLUME_LABEL..OFF_VOLUME_LABEL + 11]),
        )
    } else {
        (None, None)
    };

    debug_assert!(sector.len() >= VBR_SIZE, "identify guarantees this");

    Ok(Fat32BootSector {
        geometry,
        hidden_sectors,
        ext_flags,
        root_cluster,
        fs_info_sector,
        backup_boot_sector,
        volume_id,
        volume_label,
        observations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fat::tests::fat32_sector;
    use crate::fat::{OFF_FAT_SIZE_32, OFF_ROOT_ENTRY_COUNT, OFF_TOTAL_SECTORS_32};

    /// The extent of the fixture volume: one partition at LBA 2048 spanning
    /// 129,024 sectors, matching `mbr-single-fat32.img`.
    const FIXTURE_EXTENT: VolumeExtent = VolumeExtent {
        start_lba: 2048,
        sector_count: 129_024,
    };

    /// A FAT32 boot sector with the FAT32-only fields populated.
    ///
    /// `fat::tests::fat32_sector` sets only the fields identification needs.
    fn fat32_boot_sector() -> Vec<u8> {
        let mut s = fat32_sector();
        s[OFF_HIDDEN_SECTORS..OFF_HIDDEN_SECTORS + 4].copy_from_slice(&2048u32.to_le_bytes());
        s[OFF_EXT_FLAGS..OFF_EXT_FLAGS + 2].copy_from_slice(&0u16.to_le_bytes());
        s[OFF_FS_VERSION..OFF_FS_VERSION + 2].copy_from_slice(&0u16.to_le_bytes());
        s[OFF_ROOT_CLUSTER..OFF_ROOT_CLUSTER + 4].copy_from_slice(&2u32.to_le_bytes());
        s[OFF_FS_INFO_SECTOR..OFF_FS_INFO_SECTOR + 2].copy_from_slice(&1u16.to_le_bytes());
        s[OFF_BACKUP_BOOT_SECTOR..OFF_BACKUP_BOOT_SECTOR + 2].copy_from_slice(&6u16.to_le_bytes());
        s[OFF_BOOT_SIGNATURE] = BOOT_SIGNATURE_PRESENT;
        s[OFF_VOLUME_ID..OFF_VOLUME_ID + 4].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        s[OFF_VOLUME_LABEL..OFF_VOLUME_LABEL + 11].copy_from_slice(b"TAPHFIX    ");
        s
    }

    #[test]
    fn valid_boot_sector_parses() {
        let boot = parse_boot_sector(&fat32_boot_sector(), FIXTURE_EXTENT)
            .expect("the fixture sector is valid FAT32");

        assert_eq!(boot.root_cluster, 2);
        assert_eq!(boot.hidden_sectors, 2048);
        assert_eq!(boot.fs_info_sector, 1);
        assert_eq!(boot.backup_boot_sector, 6);
        assert_eq!(boot.volume_id, Some(0x1234_5678));
        assert_eq!(boot.volume_label.as_deref(), Some("TAPHFIX"));
        assert!(boot.mirroring_enabled());
        assert_eq!(boot.active_fat(), None);
        assert!(
            boot.observations.is_empty(),
            "a volume matching its extent has nothing to report, got {:?}",
            boot.observations
        );
    }

    #[test]
    fn non_fat32_volume_is_refused() {
        let mut s = fat32_boot_sector();
        // Shrink the volume into the FAT16 cluster range.
        s[OFF_TOTAL_SECTORS_32..OFF_TOTAL_SECTORS_32 + 4].copy_from_slice(&30_000u32.to_le_bytes());

        assert_eq!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::NotFat32 {
                identified: Some(Filesystem::Fat16)
            })
        );
    }

    #[test]
    fn unidentifiable_sector_is_refused() {
        assert_eq!(
            parse_boot_sector(&[0u8; 100], FIXTURE_EXTENT),
            Err(Fat32Error::NotFat32 { identified: None })
        );
    }

    /// The check identification structurally cannot make.
    #[test]
    fn volume_larger_than_its_extent_is_refused() {
        let s = fat32_boot_sector();
        let cramped = VolumeExtent {
            start_lba: 2048,
            sector_count: 1000,
        };

        assert_eq!(
            parse_boot_sector(&s, cramped),
            Err(Fat32Error::VolumeExceedsExtent {
                declared_sectors: 129_024,
                extent_sectors: 1000,
            })
        );
    }

    #[test]
    fn volume_smaller_than_its_extent_is_observed_not_refused() {
        let s = fat32_boot_sector();
        let roomy = VolumeExtent {
            start_lba: 2048,
            sector_count: 200_000,
        };

        let boot = parse_boot_sector(&s, roomy).expect("a short volume is valid");
        assert!(
            boot.observations
                .contains(&Fat32Observation::VolumeSmallerThanExtent {
                    declared_sectors: 129_024,
                    extent_sectors: 200_000,
                })
        );
    }

    /// Invisible to identification, which does not know where the volume
    /// begins.
    #[test]
    fn hidden_sector_disagreement_is_observed() {
        let mut s = fat32_boot_sector();
        s[OFF_HIDDEN_SECTORS..OFF_HIDDEN_SECTORS + 4].copy_from_slice(&9999u32.to_le_bytes());

        let boot = parse_boot_sector(&s, FIXTURE_EXTENT).expect("still a usable boot sector");
        assert!(
            boot.observations
                .contains(&Fat32Observation::HiddenSectorsDisagree {
                    declared: 9999,
                    actual: 2048,
                })
        );
    }

    /// Zero means the field was never recorded. Reporting it as
    /// disagreement would fire on every volume mkfs.vfat produces.
    #[test]
    fn unrecorded_hidden_sectors_are_not_observed() {
        let mut s = fat32_boot_sector();
        s[OFF_HIDDEN_SECTORS..OFF_HIDDEN_SECTORS + 4].copy_from_slice(&0u32.to_le_bytes());

        let boot = parse_boot_sector(&s, FIXTURE_EXTENT).expect("valid");
        assert!(
            boot.observations.is_empty(),
            "a zero hidden-sector count is not a disagreement, got {:?}",
            boot.observations
        );
    }

    #[test]
    fn root_cluster_below_first_data_cluster_is_refused() {
        let mut s = fat32_boot_sector();
        s[OFF_ROOT_CLUSTER..OFF_ROOT_CLUSTER + 4].copy_from_slice(&1u32.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidRootCluster { cluster: 1, .. })
        ));
    }

    #[test]
    fn root_cluster_beyond_data_region_is_refused() {
        let mut s = fat32_boot_sector();
        s[OFF_ROOT_CLUSTER..OFF_ROOT_CLUSTER + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidRootCluster { .. })
        ));
    }

    #[test]
    fn fat_too_small_for_its_clusters_is_refused() {
        let mut s = fat32_boot_sector();
        s[OFF_FAT_SIZE_32..OFF_FAT_SIZE_32 + 4].copy_from_slice(&4u32.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::FatTooSmall { .. })
        ));
    }

    #[test]
    fn nonzero_root_entry_count_is_refused() {
        let mut s = fat32_boot_sector();
        s[OFF_ROOT_ENTRY_COUNT..OFF_ROOT_ENTRY_COUNT + 2].copy_from_slice(&512u16.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidField {
                field: "root_entry_count",
                ..
            })
        ));
    }

    #[test]
    fn fs_info_outside_reserved_region_is_refused() {
        let mut s = fat32_boot_sector();
        // The fixture reserves 32 sectors.
        s[OFF_FS_INFO_SECTOR..OFF_FS_INFO_SECTOR + 2].copy_from_slice(&100u16.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidField {
                field: "fs_info_sector",
                ..
            })
        ));
    }

    #[test]
    fn backup_boot_sector_outside_reserved_region_is_refused() {
        let mut s = fat32_boot_sector();
        s[OFF_BACKUP_BOOT_SECTOR..OFF_BACKUP_BOOT_SECTOR + 2]
            .copy_from_slice(&100u16.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidField {
                field: "backup_boot_sector",
                ..
            })
        ));
    }

    #[test]
    fn disabled_mirroring_reports_the_active_fat() {
        let mut s = fat32_boot_sector();
        // Bit 7 set, active FAT 1.
        s[OFF_EXT_FLAGS..OFF_EXT_FLAGS + 2].copy_from_slice(&0x0081u16.to_le_bytes());

        let boot = parse_boot_sector(&s, FIXTURE_EXTENT).expect("valid, and worth reporting");
        assert!(!boot.mirroring_enabled());
        assert_eq!(boot.active_fat(), Some(1));
        assert!(
            boot.observations
                .contains(&Fat32Observation::FatMirroringDisabled { active_fat: 1 })
        );
    }

    /// Bits 0-3 name a FAT that must exist. The fixture declares two.
    #[test]
    fn active_fat_beyond_the_fat_count_is_refused() {
        let mut s = fat32_boot_sector();
        // Mirroring disabled, FAT 2 active, but only FATs 0 and 1 exist.
        s[OFF_EXT_FLAGS..OFF_EXT_FLAGS + 2].copy_from_slice(&0x0082u16.to_le_bytes());

        assert!(matches!(
            parse_boot_sector(&s, FIXTURE_EXTENT),
            Err(Fat32Error::InvalidField {
                field: "active_fat",
                ..
            })
        ));
    }

    #[test]
    fn absent_boot_signature_yields_no_label() {
        let mut s = fat32_boot_sector();
        s[OFF_BOOT_SIGNATURE] = 0x00;

        let boot = parse_boot_sector(&s, FIXTURE_EXTENT).expect("the label is optional");
        assert_eq!(boot.volume_id, None);
        assert_eq!(boot.volume_label, None);
    }

    #[test]
    fn error_display_is_human_readable() {
        let errors = [
            Fat32Error::NotFat32 {
                identified: Some(Filesystem::Ntfs),
            },
            Fat32Error::NotFat32 { identified: None },
            Fat32Error::UnsupportedSectorSize { bytes: 4096 },
            Fat32Error::InvalidField {
                field: "fs_version",
                value: 1,
            },
            Fat32Error::VolumeExceedsExtent {
                declared_sectors: 200_000,
                extent_sectors: 129_024,
            },
            Fat32Error::InvalidRootCluster {
                cluster: 1,
                cluster_count: 127_006,
            },
            Fat32Error::FatTooSmall {
                fat_bytes: 2048,
                required_bytes: 508_032,
            },
        ];

        for e in &errors {
            let rendered = e.to_string();
            assert!(!rendered.is_empty());
            assert!(
                !rendered.contains('{') && !rendered.contains('}'),
                "Debug-style struct syntax leaked into Display output: {rendered:?}"
            );
        }
    }

    #[test]
    fn observation_display_is_human_readable() {
        let observations = [
            Fat32Observation::HiddenSectorsDisagree {
                declared: 0,
                actual: 2048,
            },
            Fat32Observation::VolumeSmallerThanExtent {
                declared_sectors: 129_024,
                extent_sectors: 200_000,
            },
            Fat32Observation::FatMirroringDisabled { active_fat: 1 },
        ];

        for o in &observations {
            let rendered = o.to_string();
            assert!(!rendered.is_empty());
            assert!(
                !rendered.contains('{') && !rendered.contains('}'),
                "Debug-style struct syntax leaked into Display output: {rendered:?}"
            );
        }
    }
}
