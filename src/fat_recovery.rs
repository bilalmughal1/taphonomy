//! Recovery of a deleted file's data from a FAT32 volume.
//!
//! # What a deleted entry says, and what it does not
//!
//! Deletion overwrites the first byte of the name with `0xE5` and zeroes the
//! cluster chain in every FAT. It leaves `DIR_FstClusHI`, `DIR_FstClusLO`
//! and `DIR_FileSize` intact, so a start and a length survive. EXP-0003.
//!
//! A start and a length imply a run of consecutive clusters. Nothing in the
//! evidence says the file occupied that run, because the entry that would
//! have said so is the one deletion destroyed. This module computes the run
//! the entry implies and reports it as an implication, not as a finding.
//!
//! Neither field is verified. A first cluster below 65,536 read from a
//! deleted entry cannot be distinguished from one whose high word was
//! destroyed, and `KNOWN_ISSUES.md` records that no fixture can exercise the
//! difference. `DIR_FileSize` is untrusted input under `SECURITY.md`
//! section 16 and is bounded here before it is used.
//!
//! # Scope
//!
//! ADR-0010 Decision A: nothing in this module writes a file. Extraction is
//! to memory and the result is a digest.
//!
//! Eligibility and the implied run are computed from the entry and the
//! volume's geometry alone, reading nothing, so they can be tested against
//! entries built by hand. [`assess`] reads the active FAT to check the run
//! against it, and reads nothing else. No data cluster is read anywhere in
//! this file: extraction is not yet implemented.

use std::fmt;

use crate::evidence::EvidenceReader;
use crate::fat::FIRST_DATA_CLUSTER;
use crate::fat_directory::{DeletedKind, DirectoryError, Entry, EntryKind, read_fat_entry};
use crate::fat32::Fat32BootSector;
use crate::filesystem::VolumeExtent;

/// The run of clusters a deleted entry implies for its content.
///
/// Every field is as the entry states it or is derived from what the entry
/// states. None of it has been checked against the FAT, and a run being
/// computable says nothing about whether the bytes in it are the file's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClusterRun {
    /// First cluster, assembled from the entry's high and low words.
    pub first_cluster: u32,
    /// Number of clusters `file_size` requires at this volume's cluster
    /// size. Always at least one, because an empty file is refused.
    pub cluster_count: u32,
    /// Size in bytes, as the entry states it.
    pub file_size: u32,
    /// Bytes of the final cluster lying past the file's end.
    ///
    /// This is file slack. It belongs to whatever occupied the cluster
    /// before, is evidence in its own right, and is not part of the file.
    /// ADR-0010 Decision E excludes it from the content and the digest.
    pub slack_bytes: u32,
}

impl ClusterRun {
    /// The last cluster the run covers.
    ///
    /// Saturating, because the fields are public and a run this module did
    /// not produce could hold anything. A run this module produced has been
    /// bounds checked and cannot saturate.
    pub const fn last_cluster(&self) -> u32 {
        self.first_cluster
            .saturating_add(self.cluster_count)
            .saturating_sub(1)
    }
}

/// Why a deleted entry's content cannot be located.
///
/// Every reason is named. ADR-0010 Decision D: an omitted field reads as an
/// absence of interest rather than an absence of evidence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ineligible {
    /// A deleted subdirectory. M7 recovers the data of a file.
    Directory,

    /// A deleted volume label, which locates no content.
    VolumeLabel,

    /// One component of a deleted long-name set, which locates no content.
    LongNameComponent,

    /// Both the directory and volume-id attribute bits are set, so the
    /// entry describes nothing the specification defines.
    InvalidEntry {
        /// The attribute byte as stored.
        attr: u8,
    },

    /// The entry states a size of zero, so there is no content to locate.
    EmptyFile,

    /// The entry names cluster 0 or 1, which the specification reserves.
    ///
    /// Cluster 0 is also what a live entry holds for an empty file, and
    /// what remains when a first cluster has been zeroed.
    ReservedFirstCluster {
        /// The cluster the entry names.
        cluster: u32,
    },

    /// The implied run extends past the volume's last data cluster.
    ///
    /// The size is evidence, not a fact, and a size large enough to run off
    /// the end of the volume is refused before any offset is computed from
    /// it.
    RunOutOfRange {
        /// Last cluster the run would cover. Held as `u64` because a run
        /// computed from an unchecked size can exceed `u32`.
        last_cluster: u64,
        /// Number of data clusters the volume declares.
        data_clusters: u32,
    },
}

impl fmt::Display for Ineligible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ineligible::Directory => f.write_str("a deleted directory, not a file"),
            Ineligible::VolumeLabel => f.write_str("a deleted volume label"),
            Ineligible::LongNameComponent => f.write_str("a deleted long-name component"),
            Ineligible::InvalidEntry { attr } => {
                write!(f, "attribute {attr:#04x} describes no defined entry")
            }
            Ineligible::EmptyFile => f.write_str("size is zero, no content to locate"),
            Ineligible::ReservedFirstCluster { cluster } => {
                write!(f, "first cluster {cluster} is reserved")
            }
            Ineligible::RunOutOfRange {
                last_cluster,
                data_clusters,
            } => write!(
                f,
                "run would end at cluster {last_cluster}, past the {data_clusters} data clusters"
            ),
        }
    }
}

/// What a deleted entry's fields imply about its content.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Eligibility {
    /// The content cannot be located, for the reason given.
    Refused(Ineligible),

    /// The run the entry implies. Not yet checked against the FAT.
    Run(ClusterRun),
}

/// What the entry's own fields imply about where its content lay.
///
/// Returns `None` when the entry is not a deleted file, in which case the
/// question does not arise. A live entry's content is not lost, and
/// reporting it as unrecoverable would be false. This follows `associate`,
/// which answers the same way for the same reason.
///
/// Reads no evidence. The answer is a function of the entry and the volume's
/// geometry, so a wrong answer here is a wrong answer about arithmetic and
/// not about what is on the disk.
fn eligibility(entry: &Entry, boot: &Fat32BootSector) -> Option<Eligibility> {
    let EntryKind::Deleted { was } = &entry.kind else {
        return None;
    };

    let refuse = |reason| Some(Eligibility::Refused(reason));

    let (directory, first_cluster, file_size) = match was {
        DeletedKind::LongName { .. } => return refuse(Ineligible::LongNameComponent),
        DeletedKind::VolumeLabel { .. } => return refuse(Ineligible::VolumeLabel),
        DeletedKind::Invalid { attr } => return refuse(Ineligible::InvalidEntry { attr: *attr }),
        DeletedKind::ShortName {
            directory,
            first_cluster,
            file_size,
            ..
        } => (*directory, *first_cluster, *file_size),
    };

    if directory {
        return refuse(Ineligible::Directory);
    }

    if file_size == 0 {
        return refuse(Ineligible::EmptyFile);
    }

    if first_cluster < FIRST_DATA_CLUSTER {
        return refuse(Ineligible::ReservedFirstCluster {
            cluster: first_cluster,
        });
    }

    // In 64-bit arithmetic throughout. A size close to the 32-bit maximum
    // rounded up to a cluster boundary does not fit in 32 bits, and the
    // implied last cluster of an unchecked size need not either.
    let cluster_bytes = boot.geometry.cluster_bytes() as u64;
    let size = file_size as u64;

    let clusters_needed = size.div_ceil(cluster_bytes);
    let slack_bytes = (clusters_needed * cluster_bytes - size) as u32;

    // The bound is written as `enumerate_root` writes it, so that the two
    // cannot disagree about which clusters exist.
    let last_cluster = boot.geometry.cluster_count as u64 + FIRST_DATA_CLUSTER as u64;
    let last_in_run = first_cluster as u64 + clusters_needed - 1;

    if last_in_run >= last_cluster {
        return refuse(Ineligible::RunOutOfRange {
            last_cluster: last_in_run,
            data_clusters: boot.geometry.cluster_count,
        });
    }

    Some(Eligibility::Run(ClusterRun {
        first_cluster,
        cluster_count: clusters_needed as u32,
        file_size,
        slack_bytes,
    }))
}

/// A run every cluster of which the active FAT reports as unallocated.
///
/// Constructible only by [`assess`], and only after every cluster in the run
/// has been read and found free. Extraction takes one of these, so a call
/// that extracts a run the FAT says is in use cannot be written: the
/// argument cannot be obtained. ADR-0010 Decision C, enforced by the
/// compiler rather than by discipline.
///
/// A free run is a necessary condition for attempting an extraction and not
/// a sufficient one for believing the result. Clusters can be written and
/// freed again, leaving the FAT zero and the content foreign. ADR-0003
/// section 4.2 forbids raising a level on the absence of contrary evidence,
/// and nothing here raises anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnallocatedRun(ClusterRun);

impl UnallocatedRun {
    /// The run, for reporting.
    ///
    /// Read-only. A `ClusterRun` copied out of this cannot be used to
    /// extract, because extraction requires the wrapper and not its
    /// contents.
    pub const fn run(&self) -> &ClusterRun {
        &self.0
    }
}

/// What the volume says about a deleted entry's content.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Assessment {
    /// The content cannot be located, for the reason given.
    Ineligible(Ineligible),

    /// A cluster inside the implied run is allocated, so the run is broken.
    ///
    /// Either the file was fragmented, or its clusters have been reused
    /// since it was deleted. In both cases the bytes at those offsets are
    /// not this file's. Where the first cluster itself is allocated, the
    /// entry may instead be the remains of a move within the volume, with a
    /// live entry elsewhere describing the same clusters and a better size.
    RunBroken {
        /// The run the entry implies.
        run: ClusterRun,
        /// The first cluster of the run that the FAT reports as in use.
        first_allocated: u32,
    },

    /// Every cluster in the implied run reads as unallocated.
    Recoverable(UnallocatedRun),
}

/// A failure that stopped an assessment from being made.
///
/// Distinct from [`Ineligible`], which is a fact about the evidence and
/// travels in the `Ok` arm. This is a fault in reading the evidence, not
/// something the evidence says.
#[derive(Debug)]
pub enum RecoveryError {
    /// An offset computation or a FAT read failed.
    ///
    /// Named for the module the shared helpers live in rather than for the
    /// kind of failure: `read_fat_entry` and `cluster_offset` return
    /// `DirectoryError` and stay in `fat_directory`. ADR-0010 Appendix B6.
    Directory(DirectoryError),
}

impl From<DirectoryError> for RecoveryError {
    fn from(e: DirectoryError) -> Self {
        RecoveryError::Directory(e)
    }
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryError::Directory(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RecoveryError::Directory(e) => Some(e),
        }
    }
}

/// What the volume says about where a deleted entry's content lay.
///
/// Returns `None` when the entry is not a deleted file, in which case the
/// question does not arise.
///
/// Reads the active FAT and nothing else. No data cluster is touched, so
/// this costs one four-byte read per cluster of the implied run and reveals
/// nothing about the content.
///
/// The FAT can only refuse. Every cluster reading free does not establish
/// that the run holds this file's bytes; it establishes only that nothing
/// has claimed those clusters since the entry was deleted.
pub fn assess<R: EvidenceReader>(
    entry: &Entry,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    reader: &mut R,
) -> Result<Option<Assessment>, RecoveryError> {
    let run = match eligibility(entry, boot) {
        None => return Ok(None),
        Some(Eligibility::Refused(reason)) => return Ok(Some(Assessment::Ineligible(reason))),
        Some(Eligibility::Run(run)) => run,
    };

    // Derived here, never passed in. Where mirroring is disabled the other
    // FATs are stale, and an allocation answer read from a stale FAT would
    // be confidently wrong. `unwrap_or(0)` is not a fallback for a missing
    // answer: when mirroring is enabled every FAT is current.
    let fat_index = boot.active_fat().unwrap_or(0);

    for cluster in run.first_cluster..=run.last_cluster() {
        if read_fat_entry(reader, boot, extent, fat_index, cluster)? != 0 {
            return Ok(Some(Assessment::RunBroken {
                run,
                first_allocated: cluster,
            }));
        }
    }

    Ok(Some(Assessment::Recoverable(UnallocatedRun(run))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::tests::MemoryImage;
    use crate::fat_directory::NAME_LEN;

    const START_LBA: u32 = 2048;
    const TOTAL_SECTORS: u32 = 129_024;

    /// Byte offset of the first FAT, and of the second.
    const FAT0_BASE: u64 = 1_064_960;
    const FAT1_BASE: u64 = 1_573_376;

    /// Bytes in the whole evidence image.
    const IMAGE_BYTES: u64 = 67_108_864;

    /// Data clusters in the synthetic volume, from its declared geometry:
    /// 129,024 total less 32 reserved and two FATs of 993 sectors each, at
    /// one sector per cluster. Asserted in `the_synthetic_geometry_holds`
    /// so that every boundary test below rests on a checked number.
    const DATA_CLUSTERS: u32 = 127_006;

    fn extent() -> VolumeExtent {
        VolumeExtent {
            start_lba: START_LBA,
            sector_count: TOTAL_SECTORS,
        }
    }

    /// A parsed boot sector for the synthetic volume, with mirroring on.
    fn boot() -> Fat32BootSector {
        boot_flags(0)
    }

    /// A parsed boot sector for the synthetic volume with the given
    /// `BPB_ExtFlags`.
    fn boot_flags(ext_flags: u16) -> Fat32BootSector {
        use crate::fat32::{
            OFF_BACKUP_BOOT_SECTOR, OFF_EXT_FLAGS, OFF_FS_INFO_SECTOR, OFF_ROOT_CLUSTER,
            parse_boot_sector,
        };

        let mut s = crate::fat::tests::fat32_sector();
        s[OFF_ROOT_CLUSTER..OFF_ROOT_CLUSTER + 4].copy_from_slice(&2u32.to_le_bytes());
        s[OFF_FS_INFO_SECTOR..OFF_FS_INFO_SECTOR + 2].copy_from_slice(&1u16.to_le_bytes());
        s[OFF_BACKUP_BOOT_SECTOR..OFF_BACKUP_BOOT_SECTOR + 2].copy_from_slice(&6u16.to_le_bytes());
        s[OFF_EXT_FLAGS..OFF_EXT_FLAGS + 2].copy_from_slice(&ext_flags.to_le_bytes());

        parse_boot_sector(&s, extent()).expect("the synthetic boot sector is valid FAT32")
    }

    /// Writes one FAT entry into the FAT beginning at `fat_base`.
    fn write_fat(image: &mut MemoryImage, fat_base: u64, cluster: u32, value: u32) {
        image.write(fat_base + cluster as u64 * 4, &value.to_le_bytes());
    }

    fn deleted(was: DeletedKind) -> Entry {
        Entry {
            cluster: 2,
            slot: 0,
            kind: EntryKind::Deleted { was },
        }
    }

    fn deleted_file(first_cluster: u32, file_size: u32) -> Entry {
        deleted(DeletedKind::ShortName {
            surviving_name: [b'X'; NAME_LEN - 1],
            directory: false,
            first_cluster,
            file_size,
            nt_res: 0,
        })
    }

    fn run_of(entry: &Entry) -> ClusterRun {
        match eligibility(entry, &boot()) {
            Some(Eligibility::Run(run)) => run,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    fn refusal_of(entry: &Entry) -> Ineligible {
        match eligibility(entry, &boot()) {
            Some(Eligibility::Refused(reason)) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Every boundary test below is stated in terms of `DATA_CLUSTERS`. If
    /// the synthetic geometry ever changes, this fails first and says so,
    /// rather than the boundary tests failing for a reason that looks like
    /// an arithmetic bug.
    #[test]
    fn the_synthetic_geometry_holds() {
        let boot = boot();
        assert_eq!(boot.geometry.cluster_count, DATA_CLUSTERS);
        assert_eq!(boot.geometry.cluster_bytes(), 512);
        assert_eq!(
            (START_LBA as u64 + boot.geometry.reserved_sectors as u64) * 512,
            FAT0_BASE
        );
        assert_eq!(FAT0_BASE + boot.geometry.fat_size as u64 * 512, FAT1_BASE);
    }

    #[test]
    fn a_live_entry_is_not_asked_about() {
        let live = Entry {
            cluster: 2,
            slot: 0,
            kind: EntryKind::ShortName {
                name: Some("KEEP    TXT".to_string()),
                raw_name: [b'K'; NAME_LEN],
                directory: false,
                first_cluster: 7,
                file_size: 30,
                nt_res: 0,
            },
        };

        assert_eq!(eligibility(&live, &boot()), None);
    }

    #[test]
    fn a_terminator_is_not_asked_about() {
        let entry = Entry {
            cluster: 2,
            slot: 0,
            kind: EntryKind::Terminator,
        };

        assert_eq!(eligibility(&entry, &boot()), None);
    }

    #[test]
    fn a_deleted_long_name_component_is_refused() {
        let entry = deleted(DeletedKind::LongName { checksum: 0x5A });
        assert_eq!(refusal_of(&entry), Ineligible::LongNameComponent);
    }

    #[test]
    fn a_deleted_volume_label_is_refused() {
        let entry = deleted(DeletedKind::VolumeLabel {
            surviving_name: [b'X'; NAME_LEN - 1],
        });
        assert_eq!(refusal_of(&entry), Ineligible::VolumeLabel);
    }

    #[test]
    fn a_deleted_invalid_entry_is_refused_with_its_attribute() {
        let entry = deleted(DeletedKind::Invalid { attr: 0x18 });
        assert_eq!(
            refusal_of(&entry),
            Ineligible::InvalidEntry { attr: 0x18 },
            "the attribute must be carried, not summarised away"
        );
    }

    #[test]
    fn a_deleted_directory_is_refused() {
        let entry = deleted(DeletedKind::ShortName {
            surviving_name: [b'X'; NAME_LEN - 1],
            directory: true,
            first_cluster: 5,
            file_size: 0,
            nt_res: 0,
        });

        assert_eq!(
            refusal_of(&entry),
            Ineligible::Directory,
            "a directory is refused for being a directory, not for its zero size"
        );
    }

    #[test]
    fn a_deleted_empty_file_is_refused() {
        assert_eq!(refusal_of(&deleted_file(3, 0)), Ineligible::EmptyFile);
    }

    #[test]
    fn a_reserved_first_cluster_is_refused() {
        for cluster in [0, 1] {
            assert_eq!(
                refusal_of(&deleted_file(cluster, 30)),
                Ineligible::ReservedFirstCluster { cluster },
                "cluster {cluster} is reserved"
            );
        }
    }

    #[test]
    fn a_file_shorter_than_a_cluster_occupies_one() {
        let run = run_of(&deleted_file(4, 30));

        assert_eq!(run.first_cluster, 4);
        assert_eq!(run.cluster_count, 1);
        assert_eq!(run.file_size, 30);
        assert_eq!(run.slack_bytes, 482);
        assert_eq!(run.last_cluster(), 4);
    }

    #[test]
    fn a_file_that_fills_a_cluster_exactly_has_no_slack() {
        let run = run_of(&deleted_file(4, 512));

        assert_eq!(run.cluster_count, 1);
        assert_eq!(run.slack_bytes, 0);
        assert_eq!(run.last_cluster(), 4);
    }

    #[test]
    fn one_byte_past_a_cluster_needs_a_second() {
        let run = run_of(&deleted_file(4, 513));

        assert_eq!(run.cluster_count, 2);
        assert_eq!(run.slack_bytes, 511);
        assert_eq!(run.last_cluster(), 5);
    }

    /// The highest cluster the volume has is `DATA_CLUSTERS + 1`, because
    /// numbering starts at two. A one-cluster run there is inside the
    /// volume and must be accepted.
    #[test]
    fn a_run_ending_on_the_last_data_cluster_is_accepted() {
        let last = DATA_CLUSTERS + FIRST_DATA_CLUSTER - 1;
        let run = run_of(&deleted_file(last, 1));

        assert_eq!(run.last_cluster(), last);
    }

    /// One cluster further is outside it and must be refused. This and the
    /// test above are the pair that would catch an off-by-one; either alone
    /// would not.
    #[test]
    fn a_run_ending_one_past_the_last_data_cluster_is_refused() {
        let last = DATA_CLUSTERS + FIRST_DATA_CLUSTER - 1;

        assert_eq!(
            refusal_of(&deleted_file(last, 513)),
            Ineligible::RunOutOfRange {
                last_cluster: last as u64 + 1,
                data_clusters: DATA_CLUSTERS,
            }
        );
    }

    /// A size near the 32-bit maximum rounds up past `u32::MAX` and the
    /// implied last cluster does not fit in 32 bits. Both are computed in
    /// 64-bit arithmetic, so this refuses rather than wrapping into a run
    /// that looks valid.
    #[test]
    fn a_size_that_overflows_32_bit_arithmetic_is_refused() {
        let reason = refusal_of(&deleted_file(u32::MAX - 1, u32::MAX));

        match reason {
            Ineligible::RunOutOfRange { last_cluster, .. } => assert!(
                last_cluster > u32::MAX as u64,
                "the implied last cluster should exceed u32, got {last_cluster}"
            ),
            other => panic!("expected RunOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn every_refusal_renders_without_panicking() {
        let reasons = [
            Ineligible::Directory,
            Ineligible::VolumeLabel,
            Ineligible::LongNameComponent,
            Ineligible::InvalidEntry { attr: 0x18 },
            Ineligible::EmptyFile,
            Ineligible::ReservedFirstCluster { cluster: 1 },
            Ineligible::RunOutOfRange {
                last_cluster: 127_008,
                data_clusters: DATA_CLUSTERS,
            },
        ];

        for reason in reasons {
            assert!(!reason.to_string().is_empty(), "{reason:?} rendered empty");
        }
    }

    /// Reads an assessment that must exist and must not fail.
    fn assessed(entry: &Entry, boot: &Fat32BootSector, image: &mut MemoryImage) -> Assessment {
        assess(entry, boot, extent(), image)
            .expect("no read in these fixtures runs past the end")
            .expect("the entry is a deleted file")
    }

    #[test]
    fn a_free_run_is_recoverable() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);

        match assessed(&deleted_file(4, 513), &boot, &mut image) {
            Assessment::Recoverable(found) => {
                assert_eq!(found.run().first_cluster, 4);
                assert_eq!(found.run().cluster_count, 2);
                assert_eq!(found.run().last_cluster(), 5);
            }
            other => panic!("expected Recoverable, got {other:?}"),
        }
    }

    #[test]
    fn an_allocated_cluster_inside_the_run_breaks_it() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        write_fat(&mut image, FAT0_BASE, 5, 0x0FFF_FFFF);

        match assessed(&deleted_file(4, 1025), &boot, &mut image) {
            Assessment::RunBroken {
                run,
                first_allocated,
            } => {
                assert_eq!(first_allocated, 5);
                assert_eq!(run.cluster_count, 3, "the run is still reported in full");
            }
            other => panic!("expected RunBroken, got {other:?}"),
        }
    }

    /// The starting cluster is refused like any other. It is also the case
    /// where the entry may be the remains of a move rather than a deletion.
    #[test]
    fn an_allocated_first_cluster_breaks_the_run() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        write_fat(&mut image, FAT0_BASE, 4, 0x0FFF_FFFF);

        match assessed(&deleted_file(4, 30), &boot, &mut image) {
            Assessment::RunBroken {
                first_allocated, ..
            } => assert_eq!(first_allocated, 4),
            other => panic!("expected RunBroken, got {other:?}"),
        }
    }

    #[test]
    fn the_earliest_allocated_cluster_is_the_one_reported() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        write_fat(&mut image, FAT0_BASE, 6, 0x0FFF_FFFF);
        write_fat(&mut image, FAT0_BASE, 5, 0x0FFF_FFFF);

        match assessed(&deleted_file(4, 2000), &boot, &mut image) {
            Assessment::RunBroken {
                first_allocated, ..
            } => assert_eq!(first_allocated, 5, "5 is reached before 6"),
            other => panic!("expected RunBroken, got {other:?}"),
        }
    }

    #[test]
    fn an_ineligible_entry_is_reported_as_ineligible() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        let entry = deleted(DeletedKind::ShortName {
            surviving_name: [b'X'; NAME_LEN - 1],
            directory: true,
            first_cluster: 5,
            file_size: 0,
            nt_res: 0,
        });

        assert_eq!(
            assessed(&entry, &boot, &mut image),
            Assessment::Ineligible(Ineligible::Directory)
        );
    }

    #[test]
    fn a_live_entry_is_not_assessed() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        let entry = Entry {
            cluster: 2,
            slot: 0,
            kind: EntryKind::Terminator,
        };

        assert_eq!(
            assess(&entry, &boot, extent(), &mut image).expect("no read is attempted"),
            None
        );
    }

    /// ADR-0010 Appendix B7. Mirroring disabled and FAT 1 active: the answer
    /// must come from FAT 1, which says the cluster is in use, and not from
    /// FAT 0, which is stale and says it is free.
    #[test]
    fn the_active_fat_is_read_when_mirroring_is_disabled() {
        let boot = boot_flags(0x0081);
        assert_eq!(boot.active_fat(), Some(1), "fixture premise");

        let mut image = MemoryImage::new(IMAGE_BYTES);
        write_fat(&mut image, FAT1_BASE, 4, 0x0FFF_FFFF);

        match assessed(&deleted_file(4, 30), &boot, &mut image) {
            Assessment::RunBroken {
                first_allocated, ..
            } => assert_eq!(first_allocated, 4),
            other => panic!("FAT 1 is active and reports cluster 4 in use, got {other:?}"),
        }
    }

    /// The other half of the pair. With mirroring enabled FAT 0 is
    /// authoritative, and a value written only into FAT 1 must not be read.
    /// Either test alone would pass with the index chosen wrongly.
    #[test]
    fn fat_zero_is_read_when_mirroring_is_enabled() {
        let boot = boot();
        assert_eq!(boot.active_fat(), None, "fixture premise");

        let mut image = MemoryImage::new(IMAGE_BYTES);
        write_fat(&mut image, FAT1_BASE, 4, 0x0FFF_FFFF);

        match assessed(&deleted_file(4, 30), &boot, &mut image) {
            Assessment::Recoverable(_) => {}
            other => panic!("FAT 0 is authoritative and reports cluster 4 free, got {other:?}"),
        }
    }

    /// A fault in reading is an error. It is not an `Ineligible`, which is
    /// something the evidence says rather than something that went wrong.
    #[test]
    fn a_read_past_the_end_of_evidence_is_an_error_not_a_refusal() {
        let boot = boot();
        let mut image = MemoryImage::new(FAT0_BASE);

        let result = assess(&deleted_file(4, 30), &boot, extent(), &mut image);

        assert!(
            matches!(result, Err(RecoveryError::Directory(_))),
            "expected a read failure, got {result:?}"
        );
    }

    #[test]
    fn a_recovery_error_renders_and_carries_its_source() {
        let boot = boot();
        let mut image = MemoryImage::new(FAT0_BASE);
        let error = assess(&deleted_file(4, 30), &boot, extent(), &mut image)
            .expect_err("the read runs past the end");

        assert!(!error.to_string().is_empty());
        assert!(std::error::Error::source(&error).is_some());
    }
}
