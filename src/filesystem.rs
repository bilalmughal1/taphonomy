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
//! # Refusing to answer
//!
//! [`Identification::Unknown`] is a correct result. A boot sector with a
//! plausible jump instruction and inconsistent structure must not be
//! reported as a filesystem. `PROJECT.md` section 5 forbids presenting
//! guesses as identification.

use std::fmt;

use crate::fat::{self, FatGeometry};

/// Bytes in a volume boot record.
pub const VBR_SIZE: usize = 512;

pub(crate) const OFF_JUMP: usize = 0x00;
pub(crate) const OFF_OEM: usize = 0x03;
pub(crate) const OFF_SIGNATURE: usize = 0x1FE;

pub(crate) const SIGNATURE: [u8; 2] = [0x55, 0xAA];

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

/// The `declared` field of `TypeStringDisagrees` originates in evidence.
/// `printable_ascii` restricts it to bytes 0x20..=0x7E before it can reach
/// this type, so it cannot carry control characters, newlines or terminal
/// escape sequences. Widening `printable_ascii` would invalidate that.
impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Observation::TypeStringDisagrees { declared, computed } => write!(
                f,
                "type string declares {declared}, computed variant is {computed}"
            ),
            Observation::UnusualFatCount { count } => {
                write!(f, "volume declares {count} FATs, 2 is conventional")
            }
            Observation::NonStandardSectorSize { bytes } => {
                write!(f, "sector size is {bytes} bytes, not 512")
            }
            Observation::UnprintableOemName => f.write_str("OEM name field is not printable ASCII"),
        }
    }
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

pub(crate) fn le_u16(sector: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([sector[offset], sector[offset + 1]])
}

pub(crate) fn le_u32(sector: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        sector[offset],
        sector[offset + 1],
        sector[offset + 2],
        sector[offset + 3],
    ])
}

pub(crate) fn printable_ascii(bytes: &[u8]) -> Option<String> {
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

    fat::identify_fat(sector, oem_name, observations)
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
    use crate::fat::tests::fat32_sector;
    use crate::fat::{
        OFF_BYTES_PER_SECTOR, OFF_FAT_COUNT, OFF_FAT_SIZE_32, OFF_MEDIA_DESCRIPTOR,
        OFF_RESERVED_SECTORS, OFF_SECTORS_PER_CLUSTER, OFF_TOTAL_SECTORS_32,
    };

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

    #[test]
    fn observation_display_is_human_readable() {
        let observations = [
            Observation::TypeStringDisagrees {
                declared: "FAT32".to_string(),
                computed: Filesystem::Fat32,
            },
            Observation::UnusualFatCount { count: 1 },
            Observation::NonStandardSectorSize { bytes: 4096 },
            Observation::UnprintableOemName,
        ];

        for o in &observations {
            let rendered = o.to_string();
            assert!(!rendered.is_empty());
            assert!(
                !rendered.contains('{') && !rendered.contains('}'),
                "Debug-style struct syntax leaked into Display output: {rendered:?}"
            );
        }

        assert!(observations[0].to_string().contains("FAT32"));
    }
}
