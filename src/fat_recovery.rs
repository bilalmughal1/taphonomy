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
//! ADR-0010 Decision A held that nothing in this module writes a file.
//! ADR-0015 supersedes it for M10: [`extract`] writes the artifact when the
//! caller supplies a [`Destination`], streaming it in the same pass that
//! hashes it, and reads it back to state whether what landed matches what
//! was read. Without a destination the behaviour is unchanged.
//!
//! Eligibility and the implied run are computed from the entry and the
//! volume's geometry alone, reading nothing, so they can be tested against
//! entries built by hand. [`assess`] reads the active FAT to check the run
//! against it. [`extract`] reads the run's data clusters, streaming one
//! cluster at a time, and returns a digest of the file's bytes without the
//! bytes themselves.
//!
//! # Which failures stop a run
//!
//! ADR-0015 Decision G. A read failure in the evidence voids the
//! extraction: the hasher did not see every byte, so there is no digest and
//! the error is returned. A failure on the destination does not. The run
//! read the evidence and hashed it, and a full disk says nothing about the
//! evidence, so the digest is reported together with a statement that
//! nothing was delivered. Everything in [`Output`] is a statement about the
//! destination.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::evidence::EvidenceReader;
use crate::fat::FIRST_DATA_CLUSTER;
use crate::fat_directory::{
    DeletedKind, DirectoryError, Entry, EntryKind, cluster_offset, read_fat_entry,
};
use crate::fat32::Fat32BootSector;
use crate::filesystem::VolumeExtent;
use crate::hash::{Sha256Digest, Sha256Hasher};

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

/// How the directory an entry was read from was reached.
///
/// ADR-0017 Decision C. In a listing the walk reached, only a deleted
/// entry's content is lost. In an orphaned listing, which nothing the run
/// read names, a live file entry's content is lost too: EXP-0007 measured
/// that a quick format leaves those entries unmarked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Listing {
    /// Reached by the walk from the root.
    Walked,

    /// Found by the orphan search, named by nothing the run read.
    Orphaned,
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
/// ADR-0017 Decision C makes one exception: a live short-name file in an
/// orphaned listing. Its directory is named by nothing, so its content is
/// lost as a deleted file's is, and it is asked about under the same
/// conditions. A live directory there is not: Decision B reports it.
///
/// Reads no evidence. The answer is a function of the entry and the volume's
/// geometry, so a wrong answer here is a wrong answer about arithmetic and
/// not about what is on the disk.
fn eligibility(entry: &Entry, listing: Listing, boot: &Fat32BootSector) -> Option<Eligibility> {
    let refuse = |reason| Some(Eligibility::Refused(reason));

    let (directory, first_cluster, file_size) = match (&entry.kind, listing) {
        (EntryKind::Deleted { was }, _) => match was {
            DeletedKind::LongName { .. } => return refuse(Ineligible::LongNameComponent),
            DeletedKind::VolumeLabel { .. } => return refuse(Ineligible::VolumeLabel),
            DeletedKind::Invalid { attr } => {
                return refuse(Ineligible::InvalidEntry { attr: *attr });
            }
            DeletedKind::ShortName {
                directory,
                first_cluster,
                file_size,
                ..
            } => (*directory, *first_cluster, *file_size),
        },
        (
            EntryKind::ShortName {
                directory: false,
                first_cluster,
                file_size,
                ..
            },
            Listing::Orphaned,
        ) => (false, *first_cluster, *file_size),
        _ => return None,
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

    /// A read of a data cluster failed.
    ///
    /// Distinct from the variant above, which reaches this module through
    /// the shared helpers. This one is a read this module made itself, and
    /// keeping them apart records where the failure happened.
    Evidence(Error),

    /// The evidence failed part way through an artifact, and the partial
    /// file could not be removed.
    ///
    /// ADR-0015 section 9 requires both failures to be reported. The
    /// evidence failure is the cause and voids the digest, so it is kept
    /// and is this error's source; the removal failure is a statement
    /// about the destination, carried beside it rather than in its place.
    PartialLeft {
        /// The evidence failure.
        cause: Box<RecoveryError>,
        /// The partial file that remains in the destination.
        path: PathBuf,
        /// Why it could not be removed.
        removal: String,
    },
}

impl From<DirectoryError> for RecoveryError {
    fn from(e: DirectoryError) -> Self {
        RecoveryError::Directory(e)
    }
}

impl From<Error> for RecoveryError {
    fn from(e: Error) -> Self {
        RecoveryError::Evidence(e)
    }
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryError::Directory(e) => write!(f, "{e}"),
            RecoveryError::Evidence(e) => write!(f, "{e}"),
            RecoveryError::PartialLeft {
                cause,
                path,
                removal,
            } => write!(
                f,
                "{cause}; partial file {} left: {removal}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RecoveryError::Directory(e) => Some(e),
            RecoveryError::Evidence(e) => Some(e),
            RecoveryError::PartialLeft { cause, .. } => Some(&**cause),
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
    assess_listed(entry, Listing::Walked, boot, extent, reader)
}

/// [`assess`], for an entry read from a listing reached as `listing` says.
///
/// ADR-0017 Decision C. The run is inferred and checked exactly as it is
/// for a deleted entry; only which entries are asked about differs.
pub fn assess_listed<R: EvidenceReader>(
    entry: &Entry,
    listing: Listing,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    reader: &mut R,
) -> Result<Option<Assessment>, RecoveryError> {
    let run = match eligibility(entry, listing, boot) {
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

/// Where an extracted artifact is to be written.
///
/// The directory is the caller's, and ADR-0015 Decision B as its Appendix
/// A.3 corrects it requires the caller to have established, before the
/// evidence was opened, that the directory is not the one holding the
/// evidence. Sharing a filesystem with an image file is permitted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Destination<'a> {
    /// Directory the artifact is created in.
    pub directory: &'a Path,

    /// Directory cluster the entry being recovered was read from.
    pub cluster: u32,

    /// Slot of that entry within its directory cluster, counting from zero.
    pub slot: usize,
}

impl Destination<'_> {
    /// The path this artifact is written to.
    ///
    /// ADR-0015 Decision D, with the location ADR-0016 Decision F adds.
    /// Composed from integers the run established, so no byte of evidence
    /// reaches the path. The entry's cluster and slot locate it uniquely on
    /// the volume, where a slot alone repeats in every directory cluster.
    /// `SECURITY.md` section 7
    /// requires that a recovered filename never allow a write outside the
    /// destination; a name that cannot contain a separator, a `..` or a
    /// leading `/` has no such failure to get wrong.
    ///
    /// The extension states that the content was not identified. No
    /// validator exists, and ADR-0003 section 3.1 makes a level a property
    /// of an artifact a validator has seen.
    pub fn path(&self, first_cluster: u32) -> PathBuf {
        self.directory.join(format!(
            "c{}-s{}-first-{first_cluster}.bin",
            self.cluster, self.slot
        ))
    }
}

/// What became of an artifact the caller asked to be written.
///
/// Every variant is a statement about the destination. A read failure in
/// the evidence never reaches here; it is returned as an error.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Output {
    /// Written, then read back and hashed.
    ///
    /// `readback` is the digest of the file on disk, not of what was sent
    /// to the kernel. ADR-0015 Decision H leaves the comparison against
    /// [`Extraction::digest`] to the caller to report: a difference is a
    /// finding about the destination, not an error here.
    Written {
        /// Path written.
        path: PathBuf,
        /// Digest of the bytes read back from that path.
        readback: Sha256Digest,
    },

    /// Written and flushed, and the file could not be read back.
    ///
    /// The file is left in place. Every write and the flush succeeded, so
    /// it may be sound, and removing a possibly-recovered artifact because
    /// the destination could not be re-read would destroy more than it
    /// protects. A failed flush is not this case; it is [`Output::Failed`].
    /// ADR-0015 section 10 does not cover this case and is owed an appendix
    /// recording it.
    Unverified {
        /// Path written.
        path: PathBuf,
        /// Why the read back failed.
        message: String,
    },

    /// A file of that name existed already and was not touched.
    ///
    /// ADR-0015 Decision C, which `SAFETY.md` section 15 requires: the
    /// default behaviour preserves existing output.
    Exists {
        /// Path that was left alone.
        path: PathBuf,
    },

    /// The file could not be created, so nothing was written.
    ///
    /// Distinct from [`Output::Failed`] because nothing reached the
    /// destination and nothing is left to remove. `CLAUDE.md` section 14
    /// requires a permission failure to be told apart from an I/O failure,
    /// and an unwritable directory is the ordinary cause of this one.
    NotCreated {
        /// Path that was attempted.
        path: PathBuf,
        /// Why it could not be created.
        message: String,
    },

    /// The file was created, and a write or the flush failed.
    ///
    /// ADR-0015 Decision G: the partial file is removed, because a
    /// truncated file on disk cannot be told apart from a short file that
    /// was recovered whole, and `SAFETY.md` section 12 forbids a failure
    /// becoming a silent partial success. A failed flush counts: after a
    /// writeback error the kernel may already have discarded the pages, so
    /// the file cannot be taken to hold what was written.
    Failed {
        /// Path that was attempted.
        path: PathBuf,
        /// Why it failed.
        message: String,
        /// Whether the partial file was successfully removed.
        ///
        /// Reported rather than assumed. A removal can fail too, and the
        /// tool states what it knows rather than claiming a cleanliness it
        /// did not achieve.
        removed: bool,
    },
}

/// The result of reading a run's content.
///
/// The content is not here. Extraction hashes what it read and reports;
/// where a [`Destination`] was supplied the bytes also went to a file, and
/// `output` says what became of it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Extraction {
    /// Digest of the file's bytes, and of nothing else.
    pub digest: Sha256Digest,

    /// Number of bytes hashed, counted by the hasher rather than assumed.
    ///
    /// Equal to the size the entry declared. It is reported so that the
    /// digest is never separated from a statement of what it covers.
    pub bytes_hashed: u64,

    /// Bytes of the final cluster that were read and not hashed.
    ///
    /// File slack. It belongs to whatever held the cluster before this file
    /// and is evidence in its own right, so its size is reported rather
    /// than silently dropped. Recovering it is a separate capability and is
    /// not this milestone's.
    pub slack_bytes: u32,

    /// What became of the written file, where one was asked for.
    ///
    /// `None` when the caller supplied no [`Destination`], which is the
    /// default invocation and the only behaviour before M10.
    pub output: Option<Output>,
}

/// Where the bytes are going while a run is streamed.
///
/// Private. It exists so the loop has one thing to write to whether or not
/// a file was opened, and so a destination failure part way through stops
/// writing without stopping the hashing.
enum Sink {
    /// No destination was asked for.
    Absent,
    /// Open and being written.
    Open(fs::File),
    /// The path was taken; nothing was opened.
    Taken,
    /// Creating the file failed, with the reason. Nothing is on disk.
    Unopened(String),
    /// The file was created and a write failed, with the reason. A partial
    /// file is on disk and must be removed.
    Broken(String),
}

/// Reads a run's content and returns a digest of it.
///
/// Requires an [`UnallocatedRun`], which only [`assess`] produces and only
/// after every cluster in the run has read as free. A run the FAT reports as
/// in use cannot reach this function.
///
/// A digest is not a verdict. It states what these bytes are, not that these
/// bytes are the file's. Nothing here establishes that the run held this
/// file's content, and ADR-0003 section 3.1 governs what may be done with a
/// result no validator has seen.
///
/// Streams. One cluster-sized buffer is allocated and reused, so memory does
/// not scale with the declared size, which is untrusted evidence under
/// `SECURITY.md` section 16.
///
/// The final cluster is read whole, because a cluster is the unit the volume
/// addresses, and its slack is then excluded from the digest.
///
/// With a [`Destination`] the same bytes the hasher takes are written, in
/// the same pass, so no artifact is held in memory. ADR-0015 Decisions E
/// and F: exactly `file_size` bytes reach the file, and slack does not,
/// which is what makes the written file comparable to the digest.
pub fn extract<R: EvidenceReader>(
    found: &UnallocatedRun,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    reader: &mut R,
    destination: Option<Destination<'_>>,
) -> Result<Extraction, RecoveryError> {
    let run = found.run();
    let cluster_bytes = boot.geometry.cluster_bytes() as usize;

    let path = destination.map(|d| d.path(run.first_cluster));
    let mut sink = open_sink(path.as_deref());

    // One buffer for the whole run. `file_size` is never allocated.
    let mut buffer = vec![0u8; cluster_bytes];
    let mut hasher = Sha256Hasher::new();

    let read = stream(
        found,
        boot,
        extent,
        reader,
        &mut buffer,
        &mut hasher,
        &mut sink,
    );

    if let Err(e) = read {
        // ADR-0015 Decision G. The evidence failed, so there is no digest
        // to report and nothing may be left behind that looks like one.
        // Where something is left behind anyway, section 9 requires both
        // failures to be reported.
        return Err(match (discard(sink, path.as_deref()), path) {
            (Some(removal), Some(path)) => RecoveryError::PartialLeft {
                cause: Box::new(e),
                path,
                removal,
            },
            _ => e,
        });
    }

    let hashed = hasher.finish();

    Ok(Extraction {
        digest: hashed.digest,
        bytes_hashed: hashed.bytes_read,
        slack_bytes: run.slack_bytes,
        output: settle(sink, path, &mut buffer),
    })
}

/// Opens the destination, if one was asked for.
///
/// `create_new` makes the existence check and the creation one operation,
/// so nothing can appear between them. ADR-0015 Decision C.
fn open_sink(path: Option<&Path>) -> Sink {
    let Some(path) = path else {
        return Sink::Absent;
    };

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => Sink::Open(file),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Sink::Taken,
        Err(e) => Sink::Unopened(e.to_string()),
    }
}

/// Reads the run, hashing every byte and writing those the file declares.
///
/// Split out so that a failure returns here and the caller can remove a
/// partial file before propagating it.
fn stream<R: EvidenceReader>(
    found: &UnallocatedRun,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    reader: &mut R,
    buffer: &mut [u8],
    hasher: &mut Sha256Hasher,
    sink: &mut Sink,
) -> Result<(), RecoveryError> {
    let run = found.run();
    let cluster_bytes = buffer.len();
    let mut remaining = run.file_size as u64;

    for cluster in run.first_cluster..=run.last_cluster() {
        let offset = cluster_offset(boot, extent, cluster)?;
        reader.read_exact_at(offset, buffer)?;

        // The last cluster contributes only the bytes the file declares.
        // Every earlier one contributes all of them, because the run length
        // was computed from the same size.
        let take = remaining.min(cluster_bytes as u64) as usize;
        hasher.update(&buffer[..take]);
        remaining -= take as u64;

        // The same slice, in the same pass. A write failure stops the
        // writing and not the hashing: the digest is a fact about the
        // evidence and the destination has no say in it.
        if let Sink::Open(file) = sink
            && let Err(e) = file.write_all(&buffer[..take])
        {
            *sink = Sink::Broken(e.to_string());
        }
    }

    Ok(())
}

/// Removes a partial file after the evidence failed, and says why not
/// where it could not.
///
/// Both sinks that created a file are removed. A write that failed before
/// the read did leaves a truncated file just as surely as an open one, and
/// ADR-0015 Decision G removes the file on any error after it is created.
/// Section 9 also requires that a failed removal be reported beside the
/// evidence failure, so its reason is returned for the caller to carry.
fn discard(sink: Sink, path: Option<&Path>) -> Option<String> {
    let path = path?;

    match sink {
        Sink::Open(file) => {
            // Closed before removal, so the file is not held open on
            // platforms that care.
            drop(file);
            remove_partial(path)
        }
        Sink::Broken(_) => remove_partial(path),
        Sink::Absent | Sink::Taken | Sink::Unopened(_) => None,
    }
}

/// Removes a file this run created, returning why it remains if it does.
///
/// A file already gone is not a failure: nothing is left behind, which is
/// what the removal was for.
fn remove_partial(path: &Path) -> Option<String> {
    match fs::remove_file(path) {
        Ok(()) => None,
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => Some(e.to_string()),
    }
}

/// Closes the file and states what became of it.
fn settle(sink: Sink, path: Option<PathBuf>, buffer: &mut [u8]) -> Option<Output> {
    let path = path?;

    match sink {
        Sink::Absent => None,
        Sink::Taken => Some(Output::Exists { path }),
        Sink::Unopened(message) => Some(Output::NotCreated { path, message }),
        Sink::Broken(message) => {
            let removed = fs::remove_file(&path).is_ok();
            Some(Output::Failed {
                path,
                message,
                removed,
            })
        }
        Sink::Open(file) => {
            // Flushed before it is read back, because dropping a file
            // ignores the errors closing it can report, and `sync_all` is
            // where they surface. Closed before it is read back, so what is
            // hashed is what the filesystem holds rather than what a buffer
            // still owes it.
            let flushed = file.sync_all();
            drop(file);

            // A failed flush is a failed write. ADR-0015 Decision G.
            if let Err(e) = flushed {
                let removed = fs::remove_file(&path).is_ok();
                return Some(Output::Failed {
                    path,
                    message: e.to_string(),
                    removed,
                });
            }

            // ADR-0015 Decision H. Hashing during the write proves what was
            // handed to the kernel; this proves what landed.
            match read_back(&path, buffer) {
                Ok(readback) => Some(Output::Written { path, readback }),
                Err(e) => Some(Output::Unverified {
                    path,
                    message: e.to_string(),
                }),
            }
        }
    }
}

/// Hashes a written file, reusing the run's buffer.
fn read_back(path: &Path, buffer: &mut [u8]) -> io::Result<Sha256Digest> {
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256Hasher::new();

    loop {
        let read = file.read(buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finish().digest)
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

    /// Byte offset of cluster 2, the first data cluster.
    const CLUSTER2_BASE: u64 = 2_081_792;

    /// Bytes in one cluster of the synthetic volume.
    const CLUSTER_BYTES: usize = 512;

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
        match eligibility(entry, Listing::Walked, &boot()) {
            Some(Eligibility::Run(run)) => run,
            other => panic!("expected a run, got {other:?}"),
        }
    }

    fn refusal_of(entry: &Entry) -> Ineligible {
        match eligibility(entry, Listing::Walked, &boot()) {
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
        assert_eq!(
            FAT1_BASE + boot.geometry.fat_size as u64 * 512,
            CLUSTER2_BASE
        );
        assert_eq!(boot.geometry.cluster_bytes() as usize, CLUSTER_BYTES);
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

        assert_eq!(eligibility(&live, Listing::Walked, &boot()), None);
    }

    /// ADR-0017 Decision C. In an orphaned listing a live file is asked
    /// about under a deleted file's conditions, including their refusals,
    /// and a live directory is still not asked about. The same live file in
    /// a walked listing is not asked about at all.
    #[test]
    fn an_orphaned_listing_offers_its_live_files() {
        let live = |directory, file_size| Entry {
            cluster: 4,
            slot: 2,
            kind: EntryKind::ShortName {
                name: Some("IMG_0001.JPG".to_string()),
                raw_name: *b"IMG_0001JPG",
                directory,
                first_cluster: 5,
                file_size,
                nt_res: 0,
            },
        };

        match eligibility(&live(false, 13), Listing::Orphaned, &boot()) {
            Some(Eligibility::Run(run)) => {
                assert_eq!(run.first_cluster, 5);
                assert_eq!(run.cluster_count, 1);
                assert_eq!(run.file_size, 13);
            }
            other => panic!("expected a run, got {other:?}"),
        }
        assert_eq!(
            eligibility(&live(false, 0), Listing::Orphaned, &boot()),
            Some(Eligibility::Refused(Ineligible::EmptyFile))
        );
        assert_eq!(
            eligibility(&live(true, 0), Listing::Orphaned, &boot()),
            None
        );
        assert_eq!(
            eligibility(&live(false, 13), Listing::Walked, &boot()),
            None
        );
    }

    /// ADR-0017 Decision C. A deleted entry is asked about the same way
    /// whichever listing it was read from.
    #[test]
    fn a_deleted_file_is_asked_about_in_either_listing() {
        let entry = deleted_file(4, 30);
        assert_eq!(
            eligibility(&entry, Listing::Orphaned, &boot()),
            eligibility(&entry, Listing::Walked, &boot())
        );
    }

    #[test]
    fn a_terminator_is_not_asked_about() {
        let entry = Entry {
            cluster: 2,
            slot: 0,
            kind: EntryKind::Terminator,
        };

        assert_eq!(eligibility(&entry, Listing::Walked, &boot()), None);
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

    /// Writes raw bytes at the start of a data cluster.
    fn write_cluster_bytes(image: &mut MemoryImage, cluster: u32, bytes: &[u8]) {
        image.write(
            CLUSTER2_BASE + (cluster as u64 - 2) * CLUSTER_BYTES as u64,
            bytes,
        );
    }

    /// The digest of exactly these bytes.
    ///
    /// Computed through `hash_reader` over an independently built slice, so
    /// what the assertion tests is which bytes `extract` hashed. That the
    /// digest is SHA-256 is established by `tests/nist_vectors.rs`.
    fn digest_of(bytes: &[u8]) -> Sha256Digest {
        let mut input = bytes;
        crate::hash::hash_reader(&mut input)
            .expect("hashing a slice cannot fail")
            .digest
    }

    /// An `UnallocatedRun` obtained the only way there is: by assessing an
    /// entry against a FAT that reports every cluster free.
    fn recoverable(
        entry: &Entry,
        boot: &Fat32BootSector,
        image: &mut MemoryImage,
    ) -> UnallocatedRun {
        match assessed(entry, boot, image) {
            Assessment::Recoverable(found) => found,
            other => panic!("expected Recoverable, got {other:?}"),
        }
    }

    #[test]
    fn a_file_shorter_than_a_cluster_hashes_only_its_own_bytes() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);
        let content = b"taphonomy no long name fixture";
        write_cluster_bytes(&mut image, 4, content);

        let found = recoverable(&deleted_file(4, content.len() as u32), &boot, &mut image);
        let extracted =
            extract(&found, &boot, extent(), &mut image, None).expect("the run is readable");

        assert_eq!(extracted.digest, digest_of(content));
        assert_eq!(extracted.bytes_hashed, content.len() as u64);
    }

    /// The bytes past the file's end are the previous occupant's. Reading
    /// the cluster whole is correct; hashing it whole is not.
    #[test]
    fn slack_is_read_and_not_hashed() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);

        let mut cluster = vec![0xAAu8; CLUSTER_BYTES];
        let content = b"thirty bytes of file content..";
        cluster[..content.len()].copy_from_slice(content);
        write_cluster_bytes(&mut image, 4, &cluster);

        let found = recoverable(&deleted_file(4, content.len() as u32), &boot, &mut image);
        let extracted =
            extract(&found, &boot, extent(), &mut image, None).expect("the run is readable");

        assert_eq!(extracted.digest, digest_of(content));
        assert_ne!(
            extracted.digest,
            digest_of(&cluster),
            "the whole cluster was hashed, so slack was included"
        );
        assert_eq!(
            extracted.slack_bytes,
            (CLUSTER_BYTES - content.len()) as u32
        );
    }

    #[test]
    fn a_run_of_several_clusters_is_assembled_in_order() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);

        let first = vec![b'A'; CLUSTER_BYTES];
        let second = vec![b'B'; CLUSTER_BYTES];
        let third = vec![b'C'; 100];
        write_cluster_bytes(&mut image, 4, &first);
        write_cluster_bytes(&mut image, 5, &second);
        write_cluster_bytes(&mut image, 6, &third);

        let size = CLUSTER_BYTES * 2 + third.len();
        let found = recoverable(&deleted_file(4, size as u32), &boot, &mut image);
        let extracted =
            extract(&found, &boot, extent(), &mut image, None).expect("the run is readable");

        let mut expected = Vec::new();
        expected.extend_from_slice(&first);
        expected.extend_from_slice(&second);
        expected.extend_from_slice(&third);

        assert_eq!(extracted.digest, digest_of(&expected));
        assert_eq!(extracted.bytes_hashed, size as u64);
        assert_eq!(extracted.slack_bytes, (CLUSTER_BYTES - third.len()) as u32);
    }

    /// Order is part of the content. A run read back to front would hash to
    /// something else, and this fails if the loop ever stops caring.
    #[test]
    fn the_order_of_the_clusters_changes_the_digest() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);

        let first = vec![b'A'; CLUSTER_BYTES];
        let second = vec![b'B'; CLUSTER_BYTES];
        write_cluster_bytes(&mut image, 4, &first);
        write_cluster_bytes(&mut image, 5, &second);

        let size = CLUSTER_BYTES * 2;
        let found = recoverable(&deleted_file(4, size as u32), &boot, &mut image);
        let extracted =
            extract(&found, &boot, extent(), &mut image, None).expect("the run is readable");

        let mut reversed = Vec::new();
        reversed.extend_from_slice(&second);
        reversed.extend_from_slice(&first);

        assert_ne!(extracted.digest, digest_of(&reversed));
    }

    /// A cluster never written reads as zero, which is what a freshly
    /// formatted volume holds. That is content, not an error.
    #[test]
    fn an_unwritten_cluster_hashes_as_zeroes() {
        let boot = boot();
        let mut image = MemoryImage::new(IMAGE_BYTES);

        let found = recoverable(&deleted_file(4, 100), &boot, &mut image);
        let extracted =
            extract(&found, &boot, extent(), &mut image, None).expect("the run is readable");

        assert_eq!(extracted.digest, digest_of(&[0u8; 100]));
    }

    #[test]
    fn a_read_past_the_end_of_evidence_fails_the_extraction() {
        let boot = boot();
        let mut full = MemoryImage::new(IMAGE_BYTES);
        let found = recoverable(&deleted_file(4, 30), &boot, &mut full);

        let mut truncated = MemoryImage::new(CLUSTER2_BASE);
        let result = extract(&found, &boot, extent(), &mut truncated, None);

        assert!(
            matches!(result, Err(RecoveryError::Evidence(_))),
            "expected an evidence read failure, got {result:?}"
        );
    }

    /// An empty directory of this test's own under the system temporary
    /// directory, removed first so a previous run cannot decide this one.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("taphonomy-unit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("creating the scratch directory");
        dir
    }

    /// ADR-0015 Decision G. The evidence fails on the second cluster, after
    /// the first has been written, and no truncated file may remain.
    ///
    /// The control runs the same entry against the whole image, so a pass
    /// cannot come from a destination that was never written at all.
    #[test]
    fn a_read_failure_part_way_through_leaves_no_partial_file() {
        let boot = boot();
        let mut full = MemoryImage::new(IMAGE_BYTES);
        let size = (CLUSTER_BYTES * 2) as u32;
        let found = recoverable(&deleted_file(4, size), &boot, &mut full);

        let control = scratch("read-failure-control");
        let destination = Destination {
            directory: &control,
            cluster: 2,
            slot: 1,
        };
        extract(&found, &boot, extent(), &mut full, Some(destination))
            .expect("the whole image is readable");
        assert!(
            destination.path(4).exists(),
            "control failed: nothing was written, so this test proves nothing"
        );

        // Cluster 4 ends exactly here, so it reads and cluster 5 does not.
        let mut truncated = MemoryImage::new(CLUSTER2_BASE + 3 * CLUSTER_BYTES as u64);
        let dir = scratch("read-failure");
        let destination = Destination {
            directory: &dir,
            cluster: 2,
            slot: 1,
        };
        let result = extract(&found, &boot, extent(), &mut truncated, Some(destination));

        assert!(
            matches!(result, Err(RecoveryError::Evidence(_))),
            "expected an evidence read failure, got {result:?}"
        );
        assert!(
            !destination.path(4).exists(),
            "a partial file was left after the evidence failed"
        );
    }

    /// ADR-0015 Decision G. A write that failed earlier left a file behind,
    /// and a later evidence failure must remove it too.
    #[test]
    fn a_file_whose_write_had_failed_is_removed_when_the_evidence_then_fails() {
        let dir = scratch("broken-then-read");
        let path = dir.join("c2-s1-first-4.bin");
        fs::write(&path, b"truncated").expect("planting a partial file");

        let left = discard(
            Sink::Broken("no space left".to_string()),
            Some(path.as_path()),
        );

        assert!(!path.exists(), "the partial file survived");
        assert!(
            left.is_none(),
            "a removal that succeeded was reported: {left:?}"
        );
    }

    /// A file of that name that this run did not create is not this run's
    /// to remove, whichever way the run failed.
    ///
    /// ADR-0015 Decision C and `SAFETY.md` section 15.
    #[test]
    fn a_file_this_run_did_not_create_is_never_removed() {
        let dir = scratch("not-ours");
        let path = dir.join("c2-s1-first-4.bin");
        fs::write(&path, b"not this tool's").expect("planting a file");

        let taken = discard(Sink::Taken, Some(path.as_path()));
        let unopened = discard(Sink::Unopened("denied".to_string()), Some(path.as_path()));
        assert!(
            taken.is_none() && unopened.is_none(),
            "{taken:?} {unopened:?}"
        );
        let output = settle(
            Sink::Unopened("denied".to_string()),
            Some(path.clone()),
            &mut [0u8; CLUSTER_BYTES],
        );

        assert!(
            matches!(output, Some(Output::NotCreated { .. })),
            "expected NotCreated, got {output:?}"
        );
        assert_eq!(
            fs::read(&path).expect("reading the planted file"),
            b"not this tool's"
        );
    }

    /// A file that could not be created is reported as such, and not as a
    /// failure that may have left something behind.
    ///
    /// A missing directory is used rather than permission bits, so the
    /// result does not depend on whether the tests run as root.
    #[test]
    fn a_file_that_could_not_be_created_is_reported_not_created() {
        let dir = scratch("uncreatable");
        let path = dir.join("missing").join("c2-s1-first-4.bin");

        let sink = open_sink(Some(&path));
        assert!(
            matches!(sink, Sink::Unopened(_)),
            "expected the open to fail"
        );

        let output = settle(sink, Some(path.clone()), &mut [0u8; CLUSTER_BYTES]);

        assert!(
            matches!(output, Some(Output::NotCreated { .. })),
            "expected NotCreated, got {output:?}"
        );
        assert!(!path.exists());
    }

    /// ADR-0015 Decision G. A write failure removes what was written and
    /// says whether the removal succeeded.
    #[test]
    fn a_write_failure_removes_the_partial_file_and_says_so() {
        let dir = scratch("write-failure");
        let path = dir.join("c2-s1-first-4.bin");
        fs::write(&path, b"truncated").expect("planting a partial file");

        let output = settle(
            Sink::Broken("no space left".to_string()),
            Some(path.clone()),
            &mut [0u8; CLUSTER_BYTES],
        );

        assert!(
            matches!(output, Some(Output::Failed { removed: true, .. })),
            "expected Failed with the file removed, got {output:?}"
        );
        assert!(!path.exists(), "the partial file survived");
    }

    /// ADR-0015 section 9. A partial file the run could not remove is
    /// reported, not assumed gone.
    ///
    /// Unix-specific because it relies on permission bits, as
    /// `tests/read_only.rs` does. Removing a file needs the same write
    /// permission on its directory as creating one, so the control proves
    /// the directory refuses both; running as root would defeat it, and the
    /// control makes that a failure rather than a vacuous pass.
    #[cfg(unix)]
    #[test]
    fn a_partial_file_that_cannot_be_removed_is_reported() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("unremovable");
        let path = dir.join("c2-s1-first-4.bin");
        fs::write(&path, b"truncated").expect("planting a partial file");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555))
            .expect("making the destination read-only");

        let accepted = fs::File::create(dir.join("probe")).is_ok();
        let left = discard(
            Sink::Broken("no space left".to_string()),
            Some(path.as_path()),
        );

        // Restored before any assertion, so a failure cannot leave behind a
        // directory the next run's scratch() is unable to remove.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
            .expect("restoring the destination");

        assert!(
            !accepted,
            "control failed: the directory accepted a new file, so this test proves nothing"
        );
        assert!(left.is_some(), "a failed removal was not reported");
        assert!(path.exists(), "the file was removed after all");
    }

    /// Both failures reach the operator: the message carries the evidence
    /// failure, the file left behind and why, and the source stays the
    /// evidence failure, which is what voided the digest.
    #[test]
    fn a_partial_file_left_is_reported_beside_the_evidence_failure() {
        let boot = boot();
        let mut full = MemoryImage::new(IMAGE_BYTES);
        let found = recoverable(&deleted_file(4, 30), &boot, &mut full);

        let mut truncated = MemoryImage::new(CLUSTER2_BASE);
        let cause = extract(&found, &boot, extent(), &mut truncated, None)
            .expect_err("the evidence ends before cluster 4");
        let cause_text = cause.to_string();

        let error = RecoveryError::PartialLeft {
            cause: Box::new(cause),
            path: PathBuf::from("/destination/c2-s1-first-4.bin"),
            removal: "Permission denied".to_string(),
        };

        let text = error.to_string();
        assert!(text.starts_with(&cause_text), "{text}");
        assert!(text.contains("/destination/c2-s1-first-4.bin"), "{text}");
        assert!(text.contains("Permission denied"), "{text}");

        let source = std::error::Error::source(&error).expect("a source");
        assert_eq!(source.to_string(), cause_text);
    }
}
