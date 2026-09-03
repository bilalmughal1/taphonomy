//! Shared FAT BIOS parameter block structure and variant determination.
//!
//! # FAT variant determination
//!
//! Microsoft's FAT specification states that the filesystem type string at
//! offset 0x52 is advisory and must not be used to determine FAT type. The
//! authoritative method is to compute the number of clusters in the data
//! region and apply two thresholds:
//!
//! ```text
//! clusters <  4085   FAT12
//! clusters < 65525   FAT16
//! otherwise          FAT32
//! ```
//!
//! This module implements that method. The type string is read only to
//! report disagreement with it.

use crate::filesystem::{
    Filesystem, Identification, Observation, UnknownReason, le_u16, le_u32, printable_ascii,
};

pub(crate) const OFF_BYTES_PER_SECTOR: usize = 0x0B;
pub(crate) const OFF_SECTORS_PER_CLUSTER: usize = 0x0D;
pub(crate) const OFF_RESERVED_SECTORS: usize = 0x0E;
pub(crate) const OFF_FAT_COUNT: usize = 0x10;
pub(crate) const OFF_ROOT_ENTRY_COUNT: usize = 0x11;
pub(crate) const OFF_TOTAL_SECTORS_16: usize = 0x13;
pub(crate) const OFF_MEDIA_DESCRIPTOR: usize = 0x15;
pub(crate) const OFF_FAT_SIZE_16: usize = 0x16;
pub(crate) const OFF_TOTAL_SECTORS_32: usize = 0x20;
pub(crate) const OFF_FAT_SIZE_32: usize = 0x24;
pub(crate) const OFF_FS_TYPE_32: usize = 0x52;

/// FAT12/FAT16 boundary, from the FAT specification.
const FAT12_MAX_CLUSTERS: u32 = 4085;
/// FAT16/FAT32 boundary, from the FAT specification.
const FAT16_MAX_CLUSTERS: u32 = 65525;

/// Maximum cluster size in bytes permitted by the FAT specification.
const MAX_CLUSTER_BYTES: u32 = 32_768;

/// Cluster numbers 0 and 1 are reserved. The first addressable cluster is 2.
///
/// True of FAT12, FAT16 and FAT32 alike. The specification states it in its
/// general description of the FAT data structure, not in a variant-specific
/// section, which is why it lives here rather than in `fat32.rs`.
pub(crate) const FIRST_DATA_CLUSTER: u32 = 2;

/// Geometry read from a FAT BIOS parameter block.
///
/// Every field is as declared in the volume. Values are validated against the
/// FAT specification but are not otherwise trusted; M4 must re-check them
/// against the actual partition size before using them to address data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FatGeometry {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub root_entry_count: u16,
    pub media_descriptor: u8,
    pub total_sectors: u32,
    pub fat_size: u32,
    pub root_dir_sectors: u32,
    pub data_sectors: u32,
    pub cluster_count: u32,
}

impl FatGeometry {
    /// Byte size of one cluster.
    pub const fn cluster_bytes(&self) -> u32 {
        self.bytes_per_sector as u32 * self.sectors_per_cluster as u32
    }

    /// First sector of the data region, relative to the volume start.
    pub const fn first_data_sector(&self) -> u32 {
        self.reserved_sectors as u32
            + (self.fat_count as u32 * self.fat_size)
            + self.root_dir_sectors
    }
}

pub(crate) fn identify_fat(
    sector: &[u8],
    oem_name: Option<String>,
    mut observations: Vec<Observation>,
) -> Identification {
    macro_rules! reject {
        ($field:expr, $value:expr) => {
            return Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: $field,
                    value: $value as u64,
                },
            }
        };
    }

    let bytes_per_sector = le_u16(sector, OFF_BYTES_PER_SECTOR);
    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
        reject!("bytes_per_sector", bytes_per_sector);
    }
    if bytes_per_sector != 512 {
        observations.push(Observation::NonStandardSectorSize {
            bytes: bytes_per_sector,
        });
    }

    let sectors_per_cluster = sector[OFF_SECTORS_PER_CLUSTER];
    if !sectors_per_cluster.is_power_of_two() {
        reject!("sectors_per_cluster", sectors_per_cluster);
    }
    if bytes_per_sector as u32 * sectors_per_cluster as u32 > MAX_CLUSTER_BYTES {
        reject!(
            "cluster_bytes",
            bytes_per_sector as u32 * sectors_per_cluster as u32
        );
    }

    let reserved_sectors = le_u16(sector, OFF_RESERVED_SECTORS);
    if reserved_sectors == 0 {
        reject!("reserved_sectors", 0);
    }

    let fat_count = sector[OFF_FAT_COUNT];
    if fat_count == 0 {
        reject!("fat_count", 0);
    }
    if fat_count != 2 {
        observations.push(Observation::UnusualFatCount { count: fat_count });
    }

    let media_descriptor = sector[OFF_MEDIA_DESCRIPTOR];
    if media_descriptor != 0xF0 && media_descriptor < 0xF8 {
        reject!("media_descriptor", media_descriptor);
    }

    let root_entry_count = le_u16(sector, OFF_ROOT_ENTRY_COUNT);

    let total_sectors_16 = le_u16(sector, OFF_TOTAL_SECTORS_16);
    let total_sectors_32 = le_u32(sector, OFF_TOTAL_SECTORS_32);
    let total_sectors = if total_sectors_16 != 0 {
        total_sectors_16 as u32
    } else {
        total_sectors_32
    };
    if total_sectors == 0 {
        return Identification::Unknown {
            reason: UnknownReason::UndeterminableGeometry {
                detail: "both 16-bit and 32-bit total sector counts are zero",
            },
        };
    }

    let fat_size_16 = le_u16(sector, OFF_FAT_SIZE_16);
    let fat_size_32 = le_u32(sector, OFF_FAT_SIZE_32);
    let fat_size = if fat_size_16 != 0 {
        fat_size_16 as u32
    } else {
        fat_size_32
    };
    if fat_size == 0 {
        return Identification::Unknown {
            reason: UnknownReason::UndeterminableGeometry {
                detail: "both 16-bit and 32-bit FAT sizes are zero",
            },
        };
    }

    // RootDirSectors = ((RootEntCnt * 32) + (BytsPerSec - 1)) / BytsPerSec
    let root_dir_bytes = (root_entry_count as u32).saturating_mul(32);
    let root_dir_sectors =
        root_dir_bytes.saturating_add(bytes_per_sector as u32 - 1) / bytes_per_sector as u32;

    // DataSec = TotSec - (RsvdSecCnt + (NumFATs * FATSz) + RootDirSectors)
    let metadata_sectors = (reserved_sectors as u32)
        .checked_add((fat_count as u32).saturating_mul(fat_size))
        .and_then(|s| s.checked_add(root_dir_sectors));

    let Some(metadata_sectors) = metadata_sectors else {
        return Identification::Unknown {
            reason: UnknownReason::UndeterminableGeometry {
                detail: "reserved, FAT and root directory sectors overflow",
            },
        };
    };

    let Some(data_sectors) = total_sectors.checked_sub(metadata_sectors) else {
        return Identification::Unknown {
            reason: UnknownReason::UndeterminableGeometry {
                detail: "declared metadata exceeds declared total sectors",
            },
        };
    };

    if data_sectors == 0 {
        return Identification::Unknown {
            reason: UnknownReason::UndeterminableGeometry {
                detail: "data region is empty",
            },
        };
    }

    let cluster_count = data_sectors / sectors_per_cluster as u32;

    let filesystem = if cluster_count < FAT12_MAX_CLUSTERS {
        Filesystem::Fat12
    } else if cluster_count < FAT16_MAX_CLUSTERS {
        Filesystem::Fat16
    } else {
        Filesystem::Fat32
    };

    // The type string is advisory. Read it only to report disagreement.
    if filesystem == Filesystem::Fat32
        && let Some(declared) = printable_ascii(&sector[OFF_FS_TYPE_32..OFF_FS_TYPE_32 + 8])
        && declared != "FAT32"
    {
        observations.push(Observation::TypeStringDisagrees {
            declared,
            computed: filesystem,
        });
    }

    Identification::Identified {
        filesystem,
        geometry: Some(FatGeometry {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            fat_count,
            root_entry_count,
            media_descriptor,
            total_sectors,
            fat_size,
            root_dir_sectors,
            data_sectors,
            cluster_count,
        }),
        oem_name,
        observations,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::filesystem::identify;
    use crate::filesystem::{OFF_JUMP, OFF_OEM, OFF_SIGNATURE, SIGNATURE, VBR_SIZE};

    /// A minimal structurally valid FAT32 boot sector.
    pub(crate) fn fat32_sector() -> Vec<u8> {
        let mut s = vec![0u8; VBR_SIZE];
        s[OFF_JUMP] = 0xEB;
        s[1] = 0x58;
        s[2] = 0x90;
        s[OFF_OEM..OFF_OEM + 8].copy_from_slice(b"mkfs.fat");
        s[OFF_BYTES_PER_SECTOR..OFF_BYTES_PER_SECTOR + 2].copy_from_slice(&512u16.to_le_bytes());
        s[OFF_SECTORS_PER_CLUSTER] = 1;
        s[OFF_RESERVED_SECTORS..OFF_RESERVED_SECTORS + 2].copy_from_slice(&32u16.to_le_bytes());
        s[OFF_FAT_COUNT] = 2;
        s[OFF_MEDIA_DESCRIPTOR] = 0xF8;
        s[OFF_TOTAL_SECTORS_32..OFF_TOTAL_SECTORS_32 + 4]
            .copy_from_slice(&129_024u32.to_le_bytes());
        s[OFF_FAT_SIZE_32..OFF_FAT_SIZE_32 + 4].copy_from_slice(&993u32.to_le_bytes());
        s[OFF_FS_TYPE_32..OFF_FS_TYPE_32 + 8].copy_from_slice(b"FAT32   ");
        s[OFF_SIGNATURE] = SIGNATURE[0];
        s[OFF_SIGNATURE + 1] = SIGNATURE[1];
        s
    }

    #[test]
    fn fat32_is_identified_by_cluster_count() {
        let id = identify(&fat32_sector());
        let Identification::Identified {
            filesystem,
            geometry,
            ..
        } = id
        else {
            panic!("expected identification");
        };

        assert_eq!(filesystem, Filesystem::Fat32);
        let g = geometry.expect("FAT geometry");
        assert!(
            g.cluster_count >= FAT16_MAX_CLUSTERS,
            "fixture should exceed the FAT16 threshold, got {}",
            g.cluster_count
        );
        assert_eq!(g.bytes_per_sector, 512);
        assert_eq!(g.fat_count, 2);
    }

    /// The FAT specification forbids using the type string to determine
    /// type. A volume labelled FAT32 whose geometry says FAT16 must be
    /// reported as FAT16.
    #[test]
    fn type_string_does_not_override_cluster_count() {
        let mut s = fat32_sector();
        // Shrink the volume so the cluster count falls in the FAT16 range,
        // while leaving the "FAT32   " string in place.
        s[OFF_TOTAL_SECTORS_32..OFF_TOTAL_SECTORS_32 + 4].copy_from_slice(&30_000u32.to_le_bytes());

        let id = identify(&s);
        assert_eq!(
            id.filesystem(),
            Some(Filesystem::Fat16),
            "cluster count must override the declared type string"
        );
    }

    #[test]
    fn tiny_volume_is_fat12() {
        let mut s = fat32_sector();
        s[OFF_TOTAL_SECTORS_32..OFF_TOTAL_SECTORS_32 + 4].copy_from_slice(&4000u32.to_le_bytes());
        s[OFF_FAT_SIZE_32..OFF_FAT_SIZE_32 + 4].copy_from_slice(&12u32.to_le_bytes());
        assert_eq!(identify(&s).filesystem(), Some(Filesystem::Fat12));
    }
}
