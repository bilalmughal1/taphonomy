//! Filesystem identification from a volume boot record.
//!
//! # Identification is not declaration
//!
//! An MBR partition type byte records what someone intended a partition to
//! contain. It is not evidence of what it does contain. The byte is
//! frequently wrong on damaged media and trivially falsified on hostile
//! media.
//!
//! Identification here is derived from the volume's own bytes. Where the
//! declared type and the observed filesystem disagree, that is reported as a
//! finding rather than resolved in favour of either.
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
//!
//! # Refusing to answer
//!
//! [`Identification::Unknown`] is a correct result. A boot sector with a
//! plausible jump instruction and inconsistent structure must not be
//! reported as a filesystem. `PROJECT.md` section 5 forbids presenting
//! guesses as identification.

use std::fmt;

/// Bytes in a volume boot record.
pub const VBR_SIZE: usize = 512;

const OFF_JUMP: usize = 0x00;
const OFF_OEM: usize = 0x03;
const OFF_BYTES_PER_SECTOR: usize = 0x0B;
const OFF_SECTORS_PER_CLUSTER: usize = 0x0D;
const OFF_RESERVED_SECTORS: usize = 0x0E;
const OFF_FAT_COUNT: usize = 0x10;
const OFF_ROOT_ENTRY_COUNT: usize = 0x11;
const OFF_TOTAL_SECTORS_16: usize = 0x13;
const OFF_MEDIA_DESCRIPTOR: usize = 0x15;
const OFF_FAT_SIZE_16: usize = 0x16;
const OFF_TOTAL_SECTORS_32: usize = 0x20;
const OFF_FAT_SIZE_32: usize = 0x24;
const OFF_FS_TYPE_32: usize = 0x52;
const OFF_SIGNATURE: usize = 0x1FE;

const SIGNATURE: [u8; 2] = [0x55, 0xAA];

/// FAT12/FAT16 boundary, from the FAT specification.
const FAT12_MAX_CLUSTERS: u32 = 4085;
/// FAT16/FAT32 boundary, from the FAT specification.
const FAT16_MAX_CLUSTERS: u32 = 65525;

/// Maximum cluster size in bytes permitted by the FAT specification.
const MAX_CLUSTER_BYTES: u32 = 32_768;

/// A recognised filesystem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filesystem {
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ntfs,
}

impl fmt::Display for Filesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Filesystem::Fat12 => "FAT12",
            Filesystem::Fat16 => "FAT16",
            Filesystem::Fat32 => "FAT32",
            Filesystem::ExFat => "exFAT",
            Filesystem::Ntfs => "NTFS",
        })
    }
}

/// Why a volume could not be identified.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UnknownReason {
    /// Fewer bytes were supplied than a volume boot record contains.
    ShortSector { supplied: usize },
    /// No 0x55AA signature at offset 0x1FE.
    NoSignature { found: [u8; 2] },
    /// First byte is not a recognised x86 jump instruction.
    NoJumpInstruction { found: u8 },
    /// A BIOS parameter block field holds a value the specification
    /// disallows.
    InvalidBpbField { field: &'static str, value: u64 },
    /// Cluster count could not be computed from the declared geometry.
    UndeterminableGeometry { detail: &'static str },
    /// The sector is structurally sound but matches no known filesystem.
    NoMatchingSignature,
}

impl fmt::Display for UnknownReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnknownReason::ShortSector { supplied } => write!(
                f,
                "volume boot record truncated: {supplied} bytes, {VBR_SIZE} required"
            ),
            UnknownReason::NoSignature { found } => write!(
                f,
                "no boot signature: found {:02x}{:02x}, expected 55aa",
                found[0], found[1]
            ),
            UnknownReason::NoJumpInstruction { found } => {
                write!(f, "no jump instruction: first byte is {found:#04x}")
            }
            UnknownReason::InvalidBpbField { field, value } => {
                write!(f, "invalid BPB field {field}: {value}")
            }
            UnknownReason::UndeterminableGeometry { detail } => {
                write!(f, "geometry undeterminable: {detail}")
            }
            UnknownReason::NoMatchingSignature => {
                f.write_str("structurally plausible but matches no known filesystem")
            }
        }
    }
}

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

/// An observation that does not prevent identification.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Observation {
    /// The FAT32 type string disagrees with the computed variant.
    ///
    /// The computed variant is authoritative. The FAT specification states
    /// the string must not be used to determine type.
    TypeStringDisagrees {
        declared: String,
        computed: Filesystem,
    },
    /// A FAT volume declares a FAT count other than 2.
    UnusualFatCount { count: u8 },
    /// Sector size is valid but not 512 bytes.
    NonStandardSectorSize { bytes: u16 },
    /// The OEM name field is not printable ASCII.
    UnprintableOemName,
}

/// The result of examining a volume boot record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Identification {
    /// A filesystem was identified from the volume's own structure.
    Identified {
        filesystem: Filesystem,
        /// Present for FAT volumes only.
        geometry: Option<FatGeometry>,
        /// OEM name field, if printable.
        oem_name: Option<String>,
        observations: Vec<Observation>,
    },
    /// No filesystem could be identified.
    Unknown { reason: UnknownReason },
}

impl Identification {
    /// The identified filesystem, if any.
    pub const fn filesystem(&self) -> Option<Filesystem> {
        match self {
            Identification::Identified { filesystem, .. } => Some(*filesystem),
            Identification::Unknown { .. } => None,
        }
    }
}

fn le_u16(sector: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([sector[offset], sector[offset + 1]])
}

fn le_u32(sector: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        sector[offset],
        sector[offset + 1],
        sector[offset + 2],
        sector[offset + 3],
    ])
}

fn printable_ascii(bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        Some(String::from_utf8_lossy(bytes).trim_end().to_string())
    } else {
        None
    }
}

/// Identifies the filesystem in a volume boot record.
///
/// Performs no I/O. `sector` is the first sector of the volume, not of the
/// disk.
pub fn identify(sector: &[u8]) -> Identification {
    if sector.len() < VBR_SIZE {
        return Identification::Unknown {
            reason: UnknownReason::ShortSector {
                supplied: sector.len(),
            },
        };
    }

    let signature = [sector[OFF_SIGNATURE], sector[OFF_SIGNATURE + 1]];
    if signature != SIGNATURE {
        return Identification::Unknown {
            reason: UnknownReason::NoSignature { found: signature },
        };
    }

    // 0xEB is a short jump, 0xE9 a near jump. Both appear in the wild.
    let jump = sector[OFF_JUMP];
    if jump != 0xEB && jump != 0xE9 {
        return Identification::Unknown {
            reason: UnknownReason::NoJumpInstruction { found: jump },
        };
    }

    let oem = &sector[OFF_OEM..OFF_OEM + 8];
    let oem_name = printable_ascii(oem);

    let mut observations = Vec::new();
    if oem_name.is_none() {
        observations.push(Observation::UnprintableOemName);
    }

    // exFAT and NTFS place their identity in the OEM field. Both are
    // recognised but not parsed; see ADR-0002.
    if oem == b"EXFAT   " {
        return Identification::Identified {
            filesystem: Filesystem::ExFat,
            geometry: None,
            oem_name,
            observations,
        };
    }

    if oem == b"NTFS    " {
        return Identification::Identified {
            filesystem: Filesystem::Ntfs,
            geometry: None,
            oem_name,
            observations,
        };
    }

    identify_fat(sector, oem_name, observations)
}

fn identify_fat(
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

/// Whether an MBR partition type byte is consistent with an identified
/// filesystem.
///
/// Returns `None` when the declared type carries no expectation, so absence
/// of a mismatch is not evidence of agreement.
pub fn declared_type_matches(partition_type: u8, filesystem: Filesystem) -> Option<bool> {
    let expected: &[Filesystem] = match partition_type {
        0x01 => &[Filesystem::Fat12],
        0x04 | 0x06 | 0x0E => &[Filesystem::Fat16],
        0x0B | 0x0C => &[Filesystem::Fat32],
        0x07 => &[Filesystem::Ntfs, Filesystem::ExFat],
        _ => return None,
    };
    Some(expected.contains(&filesystem))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal structurally valid FAT32 boot sector.
    fn fat32_sector() -> Vec<u8> {
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
    fn short_sector_is_unknown() {
        let id = identify(&[0u8; 100]);
        assert_eq!(
            id,
            Identification::Unknown {
                reason: UnknownReason::ShortSector { supplied: 100 }
            }
        );
    }

    #[test]
    fn missing_signature_is_unknown() {
        let mut s = fat32_sector();
        s[OFF_SIGNATURE] = 0;
        s[OFF_SIGNATURE + 1] = 0;
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::NoSignature { .. }
            }
        ));
    }

    #[test]
    fn missing_jump_is_unknown() {
        let mut s = fat32_sector();
        s[OFF_JUMP] = 0x00;
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::NoJumpInstruction { found: 0x00 }
            }
        ));
    }

    #[test]
    fn near_jump_is_accepted() {
        let mut s = fat32_sector();
        s[OFF_JUMP] = 0xE9;
        assert_eq!(identify(&s).filesystem(), Some(Filesystem::Fat32));
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

    #[test]
    fn exfat_is_identified_by_oem_field() {
        let mut s = fat32_sector();
        s[OFF_OEM..OFF_OEM + 8].copy_from_slice(b"EXFAT   ");
        assert_eq!(identify(&s).filesystem(), Some(Filesystem::ExFat));
    }

    #[test]
    fn ntfs_is_identified_by_oem_field() {
        let mut s = fat32_sector();
        s[OFF_OEM..OFF_OEM + 8].copy_from_slice(b"NTFS    ");
        assert_eq!(identify(&s).filesystem(), Some(Filesystem::Ntfs));
    }

    #[test]
    fn zero_bytes_per_sector_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_BYTES_PER_SECTOR..OFF_BYTES_PER_SECTOR + 2].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: "bytes_per_sector",
                    ..
                }
            }
        ));
    }

    #[test]
    fn non_power_of_two_cluster_size_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_SECTORS_PER_CLUSTER] = 3;
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: "sectors_per_cluster",
                    ..
                }
            }
        ));
    }

    #[test]
    fn oversized_cluster_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_SECTORS_PER_CLUSTER] = 128;
        s[OFF_BYTES_PER_SECTOR..OFF_BYTES_PER_SECTOR + 2].copy_from_slice(&4096u16.to_le_bytes());
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: "cluster_bytes",
                    ..
                }
            }
        ));
    }

    #[test]
    fn zero_reserved_sectors_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_RESERVED_SECTORS..OFF_RESERVED_SECTORS + 2].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: "reserved_sectors",
                    ..
                }
            }
        ));
    }

    #[test]
    fn invalid_media_descriptor_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_MEDIA_DESCRIPTOR] = 0x42;
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::InvalidBpbField {
                    field: "media_descriptor",
                    ..
                }
            }
        ));
    }

    /// Metadata larger than the whole volume is contradictory. It must not
    /// underflow into a huge cluster count.
    #[test]
    fn metadata_exceeding_volume_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_FAT_SIZE_32..OFF_FAT_SIZE_32 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::UndeterminableGeometry { .. }
            }
        ));
    }

    #[test]
    fn zero_total_sectors_is_rejected() {
        let mut s = fat32_sector();
        s[OFF_TOTAL_SECTORS_32..OFF_TOTAL_SECTORS_32 + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            identify(&s),
            Identification::Unknown {
                reason: UnknownReason::UndeterminableGeometry { .. }
            }
        ));
    }

    #[test]
    fn unusual_fat_count_is_observed_not_rejected() {
        let mut s = fat32_sector();
        s[OFF_FAT_COUNT] = 1;
        let Identification::Identified { observations, .. } = identify(&s) else {
            panic!("one FAT is unusual, not invalid");
        };
        assert!(observations.contains(&Observation::UnusualFatCount { count: 1 }));
    }

    #[test]
    fn random_bytes_are_not_identified() {
        let mut s = vec![0u8; VBR_SIZE];
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        s[OFF_SIGNATURE] = SIGNATURE[0];
        s[OFF_SIGNATURE + 1] = SIGNATURE[1];

        assert_eq!(
            identify(&s).filesystem(),
            None,
            "pseudo-random content must not be identified as a filesystem"
        );
    }

    #[test]
    fn declared_type_agreement() {
        assert_eq!(declared_type_matches(0x0c, Filesystem::Fat32), Some(true));
        assert_eq!(declared_type_matches(0x0c, Filesystem::Fat16), Some(false));
        assert_eq!(declared_type_matches(0x07, Filesystem::Ntfs), Some(true));
        assert_eq!(declared_type_matches(0x07, Filesystem::ExFat), Some(true));
        assert_eq!(declared_type_matches(0x83, Filesystem::Fat32), None);
    }
}
