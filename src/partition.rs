//! MBR partition table parsing.
//!
//! Scope is MBR only, per ADR-0005. GPT is detected and reported as
//! unsupported rather than misparsed.
//!
//! # Parsing discipline
//!
//! `DEVELOPMENT_ENVIRONMENT.md` section 18 requires that code parsing
//! external data assume the input may be truncated, corrupted, internally
//! contradictory, or deliberately hostile. Every declared offset and length
//! here is validated against the actual size of the evidence. No declared
//! value is trusted.
//!
//! # Invalid versus anomalous
//!
//! The parser distinguishes two things:
//!
//! * **Invalid** — the structure cannot be interpreted. Parsing fails.
//! * **Anomalous** — the structure is interpretable but unusual. Parsing
//!   succeeds and the observation is reported.
//!
//! Damaged evidence is frequently anomalous. Refusing to parse it would make
//! the tool useless for its purpose. Silently normalising it would hide
//! findings. Both are recorded, and the caller decides.

use std::fmt;

/// Bytes per sector. Fixed at 512 for now; 4Kn media requires a decision
/// that has not been made.
pub const SECTOR_SIZE: usize = 512;

/// Offset of the first partition entry within the first sector.
pub const TABLE_OFFSET: usize = 0x1BE;

/// Offset of the two-byte boot signature.
pub const SIGNATURE_OFFSET: usize = 0x1FE;

/// Bytes per partition entry.
pub const ENTRY_SIZE: usize = 16;

/// Number of primary partition entries.
pub const ENTRY_COUNT: usize = 4;

/// Required boot signature value.
pub const SIGNATURE: [u8; 2] = [0x55, 0xAA];

/// Partition type marking a GPT protective MBR.
pub const TYPE_GPT_PROTECTIVE: u8 = 0xEE;

/// Partition type marking an unused entry.
pub const TYPE_UNUSED: u8 = 0x00;

/// Offset of the disk signature within the first sector.
const DISK_SIGNATURE_OFFSET: usize = 0x1B8;

/// A single primary partition entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MbrPartition {
    /// Entry position, 1 to 4.
    pub index: u8,
    /// Boot indicator byte, as read.
    pub boot_indicator: u8,
    /// Partition type byte.
    pub partition_type: u8,
    /// First sector of the partition, LBA.
    pub start_lba: u32,
    /// Length of the partition in sectors.
    pub sector_count: u32,
}

impl MbrPartition {
    /// Whether the boot indicator marks this partition active.
    pub const fn is_bootable(&self) -> bool {
        self.boot_indicator == 0x80
    }

    /// Byte offset of the partition's first sector.
    pub const fn start_byte(&self) -> u64 {
        self.start_lba as u64 * SECTOR_SIZE as u64
    }

    /// Length of the partition in bytes.
    pub const fn length_bytes(&self) -> u64 {
        self.sector_count as u64 * SECTOR_SIZE as u64
    }

    /// One past the partition's last sector, LBA.
    ///
    /// Returns `None` on arithmetic overflow, which itself indicates a
    /// malformed entry.
    pub const fn end_lba(&self) -> Option<u64> {
        (self.start_lba as u64).checked_add(self.sector_count as u64)
    }
}

/// What the first sector was found to contain.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PartitionTable {
    /// A conventional MBR partition table.
    Mbr {
        /// Disk signature at offset 0x1B8.
        disk_signature: u32,
        /// Entries with a non-zero partition type, in table order.
        partitions: Vec<MbrPartition>,
    },
    /// A GPT protective MBR.
    ///
    /// The disk uses GPT. Its real partition table is at LBA 1 and is not
    /// parsed. Reporting the protective entry as a real partition would be a
    /// false positive; see ADR-0005 section 4.
    GptProtective,
}

/// An interpretable but unusual observation.
///
/// Media-specific conformance checks (for example, SD Association layout
/// requirements) are deferred until Taphonomy can identify the device an
/// image came from. See ADR-0005 section 3.3.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Anomaly {
    /// Boot indicator is neither 0x00 nor 0x80.
    InvalidBootIndicator { index: u8, value: u8 },
    /// More than one partition is marked bootable.
    MultipleBootablePartitions { count: usize },
    /// Two partitions occupy overlapping sectors.
    OverlappingPartitions { first: u8, second: u8 },
    /// An entry is marked unused but is not zero-filled.
    NonZeroUnusedEntry { index: u8 },
    /// Entries are not in ascending start order.
    EntriesOutOfOrder,
}

/// A reason the first sector could not be interpreted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// Fewer bytes were supplied than a sector contains.
    ShortSector { supplied: usize },
    /// The boot signature is absent or wrong.
    MissingSignature { found: [u8; 2] },
    /// A partition claims sectors beyond the end of the evidence.
    PartitionBeyondEnd {
        index: u8,
        start_lba: u32,
        sector_count: u32,
        image_sectors: u64,
    },
    /// A partition's start and length overflow when added.
    LengthOverflow {
        index: u8,
        start_lba: u32,
        sector_count: u32,
    },
    /// A partition has a type but occupies no sectors.
    ZeroLengthPartition { index: u8, partition_type: u8 },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::ShortSector { supplied } => write!(
                f,
                "first sector truncated: {supplied} bytes supplied, {SECTOR_SIZE} required"
            ),
            ParseError::MissingSignature { found } => write!(
                f,
                "no MBR signature at offset 0x1FE: found {:02x}{:02x}, expected 55aa",
                found[0], found[1]
            ),
            ParseError::PartitionBeyondEnd {
                index,
                start_lba,
                sector_count,
                image_sectors,
            } => write!(
                f,
                "partition {index} claims sectors {start_lba}..{} but evidence has {image_sectors} sectors",
                *start_lba as u64 + *sector_count as u64
            ),
            ParseError::LengthOverflow {
                index,
                start_lba,
                sector_count,
            } => write!(
                f,
                "partition {index} length overflows: start {start_lba} plus {sector_count} sectors"
            ),
            ParseError::ZeroLengthPartition {
                index,
                partition_type,
            } => write!(
                f,
                "partition {index} has type {partition_type:#04x} but zero sectors"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// The outcome of parsing a first sector.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParseOutcome {
    /// What the sector was found to contain.
    pub table: PartitionTable,
    /// Interpretable but unusual observations.
    pub anomalies: Vec<Anomaly>,
}

/// Parses an MBR from the first sector of evidence.
///
/// `image_sectors` is the true size of the evidence in sectors. Every
/// declared partition extent is checked against it. Passing a value not
/// derived from the evidence defeats the bounds checking.
///
/// This function performs no I/O. It is a pure function of its inputs, which
/// makes every failure path testable without a filesystem.
pub fn parse_mbr(sector: &[u8], image_sectors: u64) -> Result<ParseOutcome, ParseError> {
    if sector.len() < SECTOR_SIZE {
        return Err(ParseError::ShortSector {
            supplied: sector.len(),
        });
    }

    let signature = [sector[SIGNATURE_OFFSET], sector[SIGNATURE_OFFSET + 1]];
    if signature != SIGNATURE {
        return Err(ParseError::MissingSignature { found: signature });
    }

    let disk_signature = u32::from_le_bytes([
        sector[DISK_SIGNATURE_OFFSET],
        sector[DISK_SIGNATURE_OFFSET + 1],
        sector[DISK_SIGNATURE_OFFSET + 2],
        sector[DISK_SIGNATURE_OFFSET + 3],
    ]);

    let mut anomalies = Vec::new();
    let mut partitions = Vec::new();

    for slot in 0..ENTRY_COUNT {
        let base = TABLE_OFFSET + slot * ENTRY_SIZE;
        let entry = &sector[base..base + ENTRY_SIZE];
        let index = (slot + 1) as u8;

        let boot_indicator = entry[0];
        let partition_type = entry[4];
        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
        let sector_count = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);

        // A GPT protective entry means the real table is elsewhere. Stop
        // rather than report the protective partition as a real one.
        if partition_type == TYPE_GPT_PROTECTIVE {
            return Ok(ParseOutcome {
                table: PartitionTable::GptProtective,
                anomalies,
            });
        }

        if partition_type == TYPE_UNUSED {
            if entry.iter().any(|&b| b != 0) {
                anomalies.push(Anomaly::NonZeroUnusedEntry { index });
            }
            continue;
        }

        if sector_count == 0 {
            return Err(ParseError::ZeroLengthPartition {
                index,
                partition_type,
            });
        }

        let end = (start_lba as u64).checked_add(sector_count as u64).ok_or(
            ParseError::LengthOverflow {
                index,
                start_lba,
                sector_count,
            },
        )?;

        if end > image_sectors {
            return Err(ParseError::PartitionBeyondEnd {
                index,
                start_lba,
                sector_count,
                image_sectors,
            });
        }

        if boot_indicator != 0x00 && boot_indicator != 0x80 {
            anomalies.push(Anomaly::InvalidBootIndicator {
                index,
                value: boot_indicator,
            });
        }

        partitions.push(MbrPartition {
            index,
            boot_indicator,
            partition_type,
            start_lba,
            sector_count,
        });
    }

    let bootable = partitions.iter().filter(|p| p.is_bootable()).count();
    if bootable > 1 {
        anomalies.push(Anomaly::MultipleBootablePartitions { count: bootable });
    }

    for i in 0..partitions.len() {
        for j in (i + 1)..partitions.len() {
            let a = &partitions[i];
            let b = &partitions[j];
            let (Some(a_end), Some(b_end)) = (a.end_lba(), b.end_lba()) else {
                continue;
            };
            if (a.start_lba as u64) < b_end && (b.start_lba as u64) < a_end {
                anomalies.push(Anomaly::OverlappingPartitions {
                    first: a.index,
                    second: b.index,
                });
            }
        }
    }

    if partitions
        .windows(2)
        .any(|w| w[0].start_lba > w[1].start_lba)
    {
        anomalies.push(Anomaly::EntriesOutOfOrder);
    }

    Ok(ParseOutcome {
        table: PartitionTable::Mbr {
            disk_signature,
            partitions,
        },
        anomalies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a sector with a valid signature and no entries.
    fn blank_sector() -> Vec<u8> {
        let mut s = vec![0u8; SECTOR_SIZE];
        s[SIGNATURE_OFFSET] = SIGNATURE[0];
        s[SIGNATURE_OFFSET + 1] = SIGNATURE[1];
        s
    }

    fn write_entry(sector: &mut [u8], slot: usize, boot: u8, ptype: u8, start: u32, count: u32) {
        let base = TABLE_OFFSET + slot * ENTRY_SIZE;
        sector[base] = boot;
        sector[base + 4] = ptype;
        sector[base + 8..base + 12].copy_from_slice(&start.to_le_bytes());
        sector[base + 12..base + 16].copy_from_slice(&count.to_le_bytes());
    }

    #[test]
    fn short_sector_is_rejected() {
        let err = parse_mbr(&[0u8; 511], 1000).unwrap_err();
        assert_eq!(err, ParseError::ShortSector { supplied: 511 });
    }

    #[test]
    fn missing_signature_is_rejected() {
        let sector = vec![0u8; SECTOR_SIZE];
        let err = parse_mbr(&sector, 1000).unwrap_err();
        assert_eq!(err, ParseError::MissingSignature { found: [0, 0] });
    }

    #[test]
    fn empty_table_is_valid() {
        let outcome = parse_mbr(&blank_sector(), 1000).expect("empty table is valid");
        match outcome.table {
            PartitionTable::Mbr { partitions, .. } => assert!(partitions.is_empty()),
            other => panic!("expected Mbr, got {other:?}"),
        }
    }

    #[test]
    fn single_partition_is_parsed() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x80, 0x0c, 2048, 129_024);

        let outcome = parse_mbr(&s, 131_072).expect("valid single partition");
        let PartitionTable::Mbr { partitions, .. } = outcome.table else {
            panic!("expected Mbr");
        };

        assert_eq!(partitions.len(), 1);
        let p = partitions[0];
        assert_eq!(p.index, 1);
        assert!(p.is_bootable());
        assert_eq!(p.partition_type, 0x0c);
        assert_eq!(p.start_lba, 2048);
        assert_eq!(p.sector_count, 129_024);
        assert_eq!(p.start_byte(), 1_048_576);
    }

    #[test]
    fn partition_beyond_end_is_rejected() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 2048, u32::MAX);

        let err = parse_mbr(&s, 131_072).unwrap_err();
        assert!(matches!(
            err,
            ParseError::PartitionBeyondEnd { index: 1, .. }
        ));
    }

    #[test]
    fn exact_fit_is_accepted() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 0, 1000);
        parse_mbr(&s, 1000).expect("a partition filling the evidence exactly is valid");
    }

    #[test]
    fn one_sector_over_is_rejected() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 0, 1001);
        let err = parse_mbr(&s, 1000).unwrap_err();
        assert!(matches!(err, ParseError::PartitionBeyondEnd { .. }));
    }

    #[test]
    fn zero_length_partition_is_rejected() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 2048, 0);
        let err = parse_mbr(&s, 131_072).unwrap_err();
        assert!(matches!(
            err,
            ParseError::ZeroLengthPartition { index: 1, .. }
        ));
    }

    #[test]
    fn gpt_protective_is_detected_not_parsed() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, TYPE_GPT_PROTECTIVE, 1, 131_071);

        let outcome = parse_mbr(&s, 131_072).expect("protective MBR parses");
        assert_eq!(outcome.table, PartitionTable::GptProtective);
    }

    #[test]
    fn gpt_protective_wins_over_later_entries() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, TYPE_GPT_PROTECTIVE, 1, 131_071);
        write_entry(&mut s, 1, 0x00, 0x0c, 2048, 1000);

        let outcome = parse_mbr(&s, 131_072).expect("protective MBR parses");
        assert_eq!(outcome.table, PartitionTable::GptProtective);
    }

    #[test]
    fn overlapping_partitions_are_flagged() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 2048, 10_000);
        write_entry(&mut s, 1, 0x00, 0x0c, 5000, 10_000);

        let outcome = parse_mbr(&s, 131_072).expect("overlapping partitions still parse");
        assert!(outcome.anomalies.contains(&Anomaly::OverlappingPartitions {
            first: 1,
            second: 2
        }));
    }

    #[test]
    fn adjacent_partitions_do_not_overlap() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x00, 0x0c, 2048, 1000);
        write_entry(&mut s, 1, 0x00, 0x0c, 3048, 1000);

        let outcome = parse_mbr(&s, 131_072).expect("adjacent partitions parse");
        assert!(
            !outcome
                .anomalies
                .iter()
                .any(|a| matches!(a, Anomaly::OverlappingPartitions { .. })),
            "adjacent partitions must not be reported as overlapping"
        );
    }

    #[test]
    fn invalid_boot_indicator_is_flagged_not_rejected() {
        let mut s = blank_sector();
        write_entry(&mut s, 0, 0x42, 0x0c, 2048, 1000);

        let outcome = parse_mbr(&s, 131_072).expect("odd boot indicator still parses");
        assert!(outcome.anomalies.contains(&Anomaly::InvalidBootIndicator {
            index: 1,
            value: 0x42
        }));
    }

    #[test]
    fn unused_entry_with_junk_is_flagged() {
        let mut s = blank_sector();
        let base = TABLE_OFFSET + ENTRY_SIZE;
        s[base + 1] = 0xff;

        let outcome = parse_mbr(&s, 131_072).expect("junk in unused entry still parses");
        assert!(
            outcome
                .anomalies
                .contains(&Anomaly::NonZeroUnusedEntry { index: 2 })
        );
    }

    #[test]
    fn disk_signature_is_read() {
        let mut s = blank_sector();
        s[DISK_SIGNATURE_OFFSET..DISK_SIGNATURE_OFFSET + 4]
            .copy_from_slice(&0x1a2b_3c4du32.to_le_bytes());

        let outcome = parse_mbr(&s, 1000).expect("valid");
        let PartitionTable::Mbr { disk_signature, .. } = outcome.table else {
            panic!("expected Mbr");
        };
        assert_eq!(disk_signature, 0x1a2b_3c4d);
    }

    #[test]
    fn a_longer_buffer_is_accepted() {
        let mut s = blank_sector();
        s.extend_from_slice(&[0u8; 512]);
        parse_mbr(&s, 1000).expect("extra bytes beyond the sector are ignored");
    }
}
