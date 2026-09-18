//! Taphonomy command-line interface.
//!
//! The CLI is an interface to the library. It parses arguments, calls into
//! the library, formats output, and counts what the run covered so that it
//! can state a coverage status. It contains no recovery logic.
//!
//! See `docs/development/RESEARCH_LOG.md` conclusion 6 and `CLAUDE.md`
//! section 11. `ADR-0014` Appendix B.9 records why the counts live here
//! rather than in the library: a count of what was covered is reporting
//! rather than recovery, and no consumer other than this binary exists.

use std::fmt;
use std::process::ExitCode;

use taphonomy::EvidenceFile;
use taphonomy::Sha256Digest;
use taphonomy::confidence::Confidence;
use taphonomy::fat_directory::{
    DeletedKind, Entry, EntryKind, FirstByte, associate, enumerate_root, recovered_name,
};
use taphonomy::fat_recovery::{Assessment, Ineligible, assess, extract};
use taphonomy::fat32::{Fat32BootSector, parse_boot_sector};
use taphonomy::filesystem::{
    Filesystem, Identification, VBR_SIZE, VolumeExtent, declared_type_matches, identify,
};
use taphonomy::partition::{MbrPartition, PartitionTable, SECTOR_SIZE, parse_mbr};
use taphonomy::validation::{Outcome, validate};

/// One line of usage, printed on any argument error.
const USAGE: &str = "usage: taphonomy <evidence-image> [--recover] [--reference-digest <hex>]";

/// What the operator asked for.
///
/// The reporting functions take this rather than a widening list of flags.
/// `ADR-0013` section 13.
#[derive(Clone, Copy)]
struct Options {
    /// Read and hash the content of a deleted file whose run is free.
    ///
    /// `ADR-0010` Decision B: opt-in, because the default invocation
    /// reports what the volume states without reading any content.
    recover: bool,

    /// Digest of the file the operator is looking for, where they gave one.
    ///
    /// `ADR-0013` Decision A: the tool never discovers a reference, and
    /// this is the only way one enters. Decision H: it is an argument
    /// error without `recover`, because comparing needs content read.
    reference: Option<Sha256Digest>,
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);

    let Some(path) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    // Two flags, matched exactly. ADR-0010 Decision B puts reading a deleted
    // file's content behind an explicit request, and ADR-0013 section 13 sets
    // the ceiling this reasoning stops at: a third flag, or a flag taking
    // more than one value, is where an argument parser is reconsidered.
    let mut options = Options {
        recover: false,
        reference: None,
    };
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--recover") => options.recover = true,
            Some("--reference-digest") => {
                let Some(value) = args.next() else {
                    eprintln!("error: --reference-digest requires a digest");
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                };

                // ADR-0013 section 6.1. A digest that cannot be read stops
                // the run before any evidence is opened, which is the one
                // thing M8 fails closed on.
                match value.to_str().map(Sha256Digest::from_hex) {
                    Some(Ok(digest)) => options.reference = Some(digest),
                    Some(Err(e)) => {
                        eprintln!("error: --reference-digest: {e}");
                        eprintln!("{USAGE}");
                        return ExitCode::from(2);
                    }
                    None => {
                        eprintln!("error: --reference-digest is not valid utf-8");
                        eprintln!("{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
            _ => {
                eprintln!("error: unexpected argument");
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    // ADR-0013 Decision H. A reference is useless without content to
    // compare, and implying --recover would enable reading a deleted file's
    // content without the operator asking for it.
    if options.reference.is_some() && !options.recover {
        eprintln!("error: --reference-digest needs --recover, which reads content");
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    match inspect(&path, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(path: &std::ffi::OsStr, options: Options) -> Result<(), taphonomy::Error> {
    let mut evidence = EvidenceFile::open(path)?;
    let reported = evidence.reported_size();
    let result = evidence.digest()?;

    println!("path         {}", evidence.path().display());
    println!("size         {reported} bytes");
    println!("read         {} bytes", result.bytes_read);
    println!("sha256       {}", result.digest);

    if result.bytes_read != reported {
        println!();
        println!("WARNING: bytes read does not match reported size.");
        println!("The digest covers {} bytes only.", result.bytes_read);
    }

    println!();
    let mut counts = RunCounts::default();
    let mut sector = [0u8; SECTOR_SIZE];
    match evidence.read_exact_at(0, &mut sector) {
        Ok(()) => {
            let image_sectors = result.bytes_read / SECTOR_SIZE as u64;
            match parse_mbr(&sector, image_sectors) {
                Ok(outcome) => {
                    match outcome.table {
                        PartitionTable::Mbr {
                            disk_signature,
                            partitions,
                        } => {
                            println!("disk signature  {disk_signature:#010x}");
                            for p in &partitions {
                                println!(
                                    "partition {}   type {:#04x}   start {}   sectors {}   bootable {}",
                                    p.index,
                                    p.partition_type,
                                    p.start_lba,
                                    p.sector_count,
                                    p.is_bootable()
                                );
                            }

                            println!();
                            for p in &partitions {
                                report_partition(&mut evidence, p, options, &mut counts);
                            }
                        }
                        PartitionTable::GptProtective => {
                            println!("GPT detected: not supported");
                            counts.gpt += 1;
                        }
                    }

                    if !outcome.anomalies.is_empty() {
                        println!();
                        println!("anomalies");
                        for a in &outcome.anomalies {
                            println!("  {a}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    counts.table_rejected += 1;
                }
            }
        }
        // The table was never read, so it was never parsed: `ADR-0014`
        // Appendix B.3 measured this on an empty file, where nothing past
        // the digest is analysed and nothing on stdout says so.
        Err(e) => {
            eprintln!("error: {e}");
            counts.table_unread += 1;
        }
    }

    print_summary(&counts, options);
    print_caveats(&counts, options);

    Ok(())
}

/// Reports the filesystem found in one partition.
///
/// A read or identification failure for one partition is reported and does
/// not stop the others, nor change the process exit code: hashing already
/// succeeded and that result stands on its own. Each such failure is
/// counted, so that the coverage line states what the run did not analyse.
fn report_partition(
    evidence: &mut EvidenceFile,
    p: &MbrPartition,
    options: Options,
    counts: &mut RunCounts,
) {
    let mut vbr = [0u8; VBR_SIZE];
    if let Err(e) = evidence.read_exact_at(p.start_byte(), &mut vbr) {
        eprintln!("error: partition {}: {e}", p.index);
        counts.partition_unread += 1;
        return;
    }

    let id = identify(&vbr);

    match &id {
        Identification::Identified { filesystem, .. } => {
            println!("partition {}   filesystem {filesystem}", p.index);
        }
        Identification::Unknown { reason } => {
            println!("partition {}   filesystem unknown ({reason})", p.index);
        }
    }

    if let Identification::Identified {
        geometry: Some(g), ..
    } = &id
    {
        println!("    bytes per sector     {}", g.bytes_per_sector);
        println!("    sectors per cluster  {}", g.sectors_per_cluster);
        println!("    cluster count        {}", g.cluster_count);
        println!("    first data sector    {}", g.first_data_sector());
    }

    if let Some(filesystem) = id.filesystem()
        && declared_type_matches(p.partition_type, filesystem) == Some(false)
    {
        println!(
            "    MISMATCH: declared type {:#04x} disagrees with observed filesystem {filesystem}",
            p.partition_type
        );
    }

    if let Identification::Identified { observations, .. } = &id
        && !observations.is_empty()
    {
        println!("    observations");
        for o in observations {
            println!("      {o}");
        }
    }

    // Two different gaps, separated because `CLAUDE.md` section 14 requires
    // an unsupported format and unreadable metadata to be told apart: a
    // volume whose filesystem was identified and not analysed is not the
    // same finding as one nothing could identify.
    match id.filesystem() {
        Some(Filesystem::Fat32) => {}
        Some(_) => {
            counts.unsupported += 1;
            return;
        }
        None => {
            counts.unidentified += 1;
            return;
        }
    }

    let extent = VolumeExtent {
        start_lba: p.start_lba,
        sector_count: p.sector_count,
    };

    match parse_boot_sector(&vbr, extent) {
        Ok(boot) => {
            println!("    root cluster         {}", boot.root_cluster);
            println!("    backup boot sector   {}", boot.backup_boot_sector);

            match boot.active_fat() {
                Some(n) => println!("    FAT mirroring        disabled, FAT {n} active"),
                None => println!("    FAT mirroring        enabled"),
            }

            if let Some(id) = boot.volume_id {
                println!("    volume id            {id:#010x}");
            }

            if let Some(label) = &boot.volume_label {
                println!("    volume label         {label}");
            }

            if !boot.observations.is_empty() {
                println!("    boot sector observations");
                for o in &boot.observations {
                    println!("      {o}");
                }
            }

            report_root_directory(evidence, &boot, extent, options, counts);
        }
        Err(e) => {
            println!("    BOOT SECTOR REJECTED: {e}");
            counts.boot_rejected += 1;
        }
    }
}

/// Reports the root directory of a FAT32 volume.
///
/// An enumeration failure is reported and does not stop the other
/// partitions, nor change the process exit code, for the same reason a read
/// failure does not: hashing already succeeded and that result stands on
/// its own.
///
/// Every entry is listed. A directory may hold 65,536 of them, so this can
/// be long, but truncating it would present a partial listing as a complete
/// one, which `docs/PROJECT.md` section 5 forbids.
///
/// Entries past the terminator are listed separately and under their own
/// heading. They are evidence of what the directory previously held, not a
/// claim about what it holds now.
fn report_root_directory(
    evidence: &mut EvidenceFile,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    options: Options,
    counts: &mut RunCounts,
) {
    let root = match enumerate_root(evidence, boot, extent) {
        Ok(root) => root,
        Err(e) => {
            println!("    ROOT DIRECTORY NOT ENUMERATED: {e}");
            counts.root_unread += 1;
            return;
        }
    };

    counts.volumes_analysed += 1;

    let chain: Vec<String> = root.clusters.iter().map(|c| c.to_string()).collect();

    println!("    root directory");
    println!("      cluster chain      {}", chain.join(" -> "));
    println!("      entries            {}", root.entries.len());
    println!("      short entries      {}", root.short_entry_count());
    println!("      long name entries  {}", root.long_name_count());
    println!("      deleted entries    {}", root.deleted_count());

    for (index, entry) in root.entries.iter().enumerate() {
        print_entry(entry, &root.entries, index);
    }

    if !root.residue.is_empty() {
        println!("      past the terminator");
        for (index, entry) in root.residue.iter().enumerate() {
            print_entry(entry, &root.residue, index);
        }
    }

    if !root.observations.is_empty() {
        println!("      directory observations");
        for o in &root.observations {
            println!("        {o}");
        }
    }

    // Every directory this run listed and did not read. `ADR-0014` Appendix
    // B.2: what a listed directory holds was not analysed, and from the
    // evidence the tool cannot know whether anything was there, so it is
    // counted whether it is live or deleted and whatever it holds.
    let listed = root.entries.iter().chain(root.residue.iter());
    counts.directories_unread += listed.filter(|entry| is_directory(&entry.kind)).count();

    report_recovery(evidence, boot, extent, &root.entries, options, counts);
    report_recovery(evidence, boot, extent, &root.residue, options, counts);
}

/// Whether an entry describes a directory, live or deleted.
///
/// A deleted directory is counted here and not among the ineligible
/// assessments, so that one entry is one gap rather than two findings.
fn is_directory(kind: &EntryKind) -> bool {
    match kind {
        EntryKind::ShortName { directory, .. } => *directory,
        EntryKind::Deleted {
            was: DeletedKind::ShortName { directory, .. },
        } => *directory,
        _ => false,
    }
}

/// What one run covered, produced, and left unanalysed.
///
/// One value accumulates for the whole run, because `ADR-0003` section 4.7
/// requires a statement about a session rather than about a volume. It is
/// passed down as `&mut` rather than returned and merged: a merge of these
/// fields is a second place the arithmetic can fall out of step with the
/// struct, which is the disagreement `ADR-0014` Appendix A.8 avoids by
/// deriving a status rather than tracking one.
///
/// Only independently observed facts are stored. Everything that follows
/// from them, including the caveats this file used to carry as three
/// booleans, is derived below. `ADR-0014` Appendix B.8.
#[derive(Default, Debug)]
struct RunCounts {
    /// Volumes whose root directory was enumerated.
    volumes_analysed: usize,

    /// Sector 0 could not be read, so no table was seen at all.
    table_unread: usize,

    /// The partition table was read and refused.
    table_rejected: usize,

    /// A GPT disk, which this tool does not analyse.
    gpt: usize,

    /// A partition whose first sector could not be read.
    partition_unread: usize,

    /// A partition holding no filesystem this tool could identify.
    unidentified: usize,

    /// A partition whose filesystem was identified and is not FAT32.
    unsupported: usize,

    /// A FAT32 volume whose boot sector was refused.
    boot_rejected: usize,

    /// A FAT32 volume whose root directory could not be enumerated.
    root_unread: usize,

    /// A directory that was listed and whose contents were not read.
    directories_unread: usize,

    /// A deleted entry whose assessment failed.
    not_assessed: usize,

    /// A deleted entry whose run was free and whose extraction failed.
    not_extracted: usize,

    /// A deleted entry whose implied run was free in the FAT.
    free_runs: usize,

    /// A deleted entry whose run holds a cluster still in use.
    refused: usize,

    /// An extraction whose digest equalled the operator's reference.
    matched: usize,

    /// A deleted entry stating a size of zero.
    empty: usize,

    /// A deleted entry naming cluster 0 or 1.
    reserved_cluster: usize,

    /// A deleted entry whose run would pass the last data cluster.
    run_out_of_range: usize,
}

impl RunCounts {
    /// Everything the run did not analyse, counted once per occurrence.
    ///
    /// The eleven kinds `ADR-0014` Appendix B.4 enumerates. Three of them
    /// no fixture reaches and one no fixture can, which is why the
    /// derivation below is unit tested rather than measured alone.
    const fn gaps(&self) -> usize {
        self.table_unread
            + self.table_rejected
            + self.gpt
            + self.partition_unread
            + self.unidentified
            + self.unsupported
            + self.boot_rejected
            + self.root_unread
            + self.not_assessed
            + self.not_extracted
            + self.directories_unread
    }

    /// How much of the evidence the run covered.
    ///
    /// `ADR-0014` Appendix B.5. A run that analysed nothing is kept apart
    /// from one that analysed part of the evidence, because `SAFETY.md`
    /// section 12 forbids a failure being converted silently into a partial
    /// success. An image declaring no partition reaches `Complete` with no
    /// volume analysed, which is the control Appendix A.6 names.
    const fn coverage(&self) -> Coverage {
        if self.gaps() == 0 {
            Coverage::Complete
        } else if self.volumes_analysed == 0 {
            Coverage::NothingAnalysed
        } else {
            Coverage::Incomplete
        }
    }

    /// Artifacts produced, which only `--recover` can produce.
    ///
    /// Every free run is either extracted or counted as not extracted, so
    /// this follows from the two and is not tracked beside them. Saturating
    /// because a count that disagrees with that invariant must not panic in
    /// the middle of reporting evidence.
    const fn artifacts(&self, options: Options) -> usize {
        if options.recover {
            self.free_runs.saturating_sub(self.not_extracted)
        } else {
            0
        }
    }

    /// Free runs left unread because the operator did not ask to read them.
    const fn not_read(&self, options: Options) -> usize {
        if options.recover {
            return 0;
        }

        self.free_runs
    }

    /// Extractions compared against a reference that did not equal it.
    const fn differed(&self, options: Options) -> usize {
        if options.reference.is_some() {
            self.artifacts(options).saturating_sub(self.matched)
        } else {
            0
        }
    }
}

/// How much of the evidence a run analysed.
///
/// Three values rather than two: `ADR-0014` Appendix B.5 and B.6. The word
/// names what it measures, because a run can cover everything and still
/// recover content that is not the file's, which EXP-0004 measured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Coverage {
    /// Nothing went unanalysed.
    Complete,

    /// Part of the evidence was analysed and part was not.
    Incomplete,

    /// Nothing past the evidence digest was analysed.
    ///
    /// Named for what it says rather than `None`, which would shadow
    /// `Option::None` wherever this is matched.
    NothingAnalysed,
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => f.write_str("complete"),
            Self::Incomplete => f.write_str("incomplete"),
            Self::NothingAnalysed => f.write_str("none"),
        }
    }
}

/// Reports what the volume says about each deleted entry's content.
///
/// `ADR-0010` Decision B. Without `--recover` this reads the FAT and reports
/// the run each deleted entry implies; with it, the runs that pass are read
/// and hashed. The first is what the volume states, the second is derived
/// from an assumption the evidence cannot confirm.
///
/// An assessment failure is reported and does not stop the others, for the
/// same reason a read failure does not: what already succeeded stands.
fn report_recovery(
    evidence: &mut EvidenceFile,
    boot: &Fat32BootSector,
    extent: VolumeExtent,
    entries: &[Entry],
    options: Options,
    counts: &mut RunCounts,
) {
    let mut heading = false;

    let head = |heading: &mut bool| {
        if !*heading {
            println!("      deleted content");
            *heading = true;
        }
    };

    for entry in entries {
        let position = format!("c{} s{}", entry.cluster, entry.slot);

        let assessment = match assess(entry, boot, extent, evidence) {
            Ok(None) => continue,
            Ok(Some(assessment)) => assessment,
            Err(e) => {
                head(&mut heading);
                println!("        {position:<9} NOT ASSESSED: {e}");
                counts.not_assessed += 1;
                continue;
            }
        };

        head(&mut heading);

        match assessment {
            Assessment::Ineligible(reason) => {
                println!("        {position:<9} no content: {reason}");

                match reason {
                    // Entries that describe a file which yielded nothing.
                    Ineligible::EmptyFile => counts.empty += 1,
                    Ineligible::ReservedFirstCluster { .. } => counts.reserved_cluster += 1,
                    Ineligible::RunOutOfRange { .. } => counts.run_out_of_range += 1,
                    // A deleted directory is a coverage gap, counted where
                    // the directory was listed. The rest describe no file:
                    // a long-name component, a volume label and an entry
                    // with impossible attributes locate no content, so
                    // counting them as files that went unrecovered would
                    // answer `ADR-0013` Appendix C.4's question wrongly.
                    Ineligible::Directory
                    | Ineligible::VolumeLabel
                    | Ineligible::LongNameComponent
                    | Ineligible::InvalidEntry { .. } => {}
                }
            }
            Assessment::RunBroken {
                run,
                first_allocated,
            } => {
                let detail = format!(
                    "run {}-{}, {} bytes, REFUSED: cluster {first_allocated} is in use",
                    run.first_cluster,
                    run.last_cluster(),
                    run.file_size
                );
                println!("        {position:<9} {detail}");
                counts.refused += 1;
            }
            Assessment::Recoverable(found) => {
                counts.free_runs += 1;
                let run = found.run();
                let detail = format!(
                    "run {}-{}, {} bytes, {} slack, every cluster free",
                    run.first_cluster,
                    run.last_cluster(),
                    run.file_size,
                    run.slack_bytes
                );
                println!("        {position:<9} {detail}");

                if options.recover {
                    match extract(&found, boot, extent, evidence) {
                        Ok(extracted) => {
                            // `docs/SAFETY.md` section 10 requires the
                            // recovered-file hash and the validation hash to
                            // be distinguishable where both are reported.
                            println!(
                                "        {:<9} recovered sha256 {} over {} bytes",
                                "", extracted.digest, extracted.bytes_hashed
                            );

                            let validation = validate(
                                extracted.digest,
                                extracted.bytes_hashed,
                                options.reference,
                            );

                            match (validation.outcome, validation.reference) {
                                (Outcome::Match, Some(reference)) => {
                                    counts.matched += 1;
                                    println!(
                                        "        {:<9} reference sha256 {reference} MATCHES",
                                        ""
                                    );
                                }
                                (Outcome::Differs, Some(reference)) => {
                                    println!(
                                        "        {:<9} reference sha256 {reference} DIFFERS",
                                        ""
                                    );
                                }
                                // NotAttempted carries no reference, and a
                                // reference is always carried where a
                                // comparison happened.
                                _ => {}
                            }
                        }
                        Err(e) => {
                            println!("        {:<9} NOT EXTRACTED: {e}", "");
                            counts.not_extracted += 1;
                        }
                    }
                }
            }
        }
    }
}

/// Column the gap counts print in, set by the longest label below.
const GAP_LABEL_WIDTH: usize = 26;

/// Prints what the run covered and what it produced, once per run.
///
/// `ADR-0003` section 4.7 requires a session to report counts per level and
/// forbids a single combined success figure. Under `ADR-0014` Decision A one
/// level is reachable, so the level is one line here and the count on it is
/// what varies: Appendix A.10 keeps the word off the per-entry lines, and
/// Appendix B.8 keeps the count on this one.
///
/// The coverage line comes first because it scopes every count below it, and
/// each line beneath states what was not analysed rather than what was
/// there, which Appendix A.6 requires: the tool cannot know what a directory
/// it did not read held, or what a partition it could not identify carried.
///
/// Printed for every run, including one whose partition table failed.
/// Appendix A.4 measured that such a run reports its error on stderr and
/// says nothing on stdout, so a reader capturing stdout alone sees a digest
/// and no indication that nothing else was analysed.
fn print_summary(counts: &RunCounts, options: Options) {
    println!();
    println!("coverage     {}", counts.coverage());

    print_count("volumes analysed", counts.volumes_analysed);
    print_gap("partition table not read", counts.table_unread);
    print_gap("partition table not parsed", counts.table_rejected);
    print_gap("GPT not analysed", counts.gpt);
    print_gap("partitions not read", counts.partition_unread);
    print_gap("filesystems not identified", counts.unidentified);
    print_gap("filesystems not analysed", counts.unsupported);
    print_gap("boot sectors rejected", counts.boot_rejected);
    print_gap("root directories not read", counts.root_unread);
    print_gap("entries not assessed", counts.not_assessed);
    print_gap("entries not extracted", counts.not_extracted);
    print_gap("directories not read", counts.directories_unread);

    // Only `--recover` produces an artifact, so without it the line would
    // report a zero that the invocation already determined.
    if options.recover {
        let artifacts = counts.artifacts(options);
        if artifacts == 0 {
            println!("artifacts    0");
        } else {
            println!("artifacts    {artifacts} {}", Confidence::of_inferred_run());
        }
    }

    // Only where a reference was supplied. Where none was, the caveat block
    // below already states that nothing was validated, and saying it twice
    // would present one fact about the invocation as two findings.
    if options.reference.is_some() {
        let matched = counts.matched;
        let differed = counts.differed(options);
        println!("compared     {matched} matched, {differed} differed");
    }

    // Deleted entries that produced no artifact, by what stopped them.
    // `ADR-0013` Appendix C.4: an operator asking whether a file is present
    // needs to know the question went unanswered for part of the volume.
    let mut unrecovered = Vec::new();
    push_part(&mut unrecovered, counts.refused, "refused");
    push_part(&mut unrecovered, counts.not_read(options), "not read");
    push_part(&mut unrecovered, counts.not_assessed, "not assessed");
    push_part(&mut unrecovered, counts.not_extracted, "not extracted");
    push_part(&mut unrecovered, counts.empty, "size zero");
    push_part(
        &mut unrecovered,
        counts.reserved_cluster,
        "reserved first cluster",
    );
    push_part(
        &mut unrecovered,
        counts.run_out_of_range,
        "run out of range",
    );

    if !unrecovered.is_empty() {
        println!("unrecovered  {}", unrecovered.join(", "));
    }
}

/// One line of the coverage block.
fn print_count(label: &str, count: usize) {
    println!("  {label:<width$} {count}", width = GAP_LABEL_WIDTH);
}

/// One line of the coverage block, printed only where there is something to
/// report. A zero would state that a kind of gap did not occur, which reads
/// as a finding about the evidence rather than about the run.
fn print_gap(label: &str, count: usize) {
    if count > 0 {
        print_count(label, count);
    }
}

/// One part of the unrecovered line, left out where its count is zero.
fn push_part(parts: &mut Vec<String>, count: usize, label: &str) {
    if count > 0 {
        parts.push(format!("{count} {label}"));
    }
}

/// Prints what the reported entries oblige, once per volume.
///
/// `ADR-0003` section 4.2 and `ADR-0010` Decision C for the first
/// paragraph, `ADR-0013` section 8.2 and Decision E for the others. None of
/// these is a summary of findings, which `ADR-0013` Decision I reserves for
/// M9: each states what a finding does not establish.
///
/// Printed once per run. `ADR-0013` Appendix C.6 made this once per volume
/// when `report_recovery` printed it directly; `ADR-0003` section 4.7
/// requires a statement about a session rather than a volume, so the value
/// now travels to `inspect` and prints there.
///
/// Indented to zero for the same reason. At volume indentation it would
/// read as a statement about the last volume printed rather than about the
/// run it now covers.
fn print_caveats(counts: &RunCounts, options: Options) {
    if counts.free_runs == 0 {
        return;
    }

    // ADR-0003 section 4.2 and ADR-0010 Decision C. Saying only that the run
    // is free would let a reader take it for a finding. It is the absence of
    // contrary evidence, which is not the same thing and never becomes it.
    println!();
    println!("A free run means nothing has claimed those");
    println!("clusters since deletion. It is not evidence");
    println!("that the content there is this file's.");

    // ADR-0013 section 8.2. A match is byte equality with what the operator
    // supplied, and where the content is not distinctive that is weaker
    // evidence than it reads as.
    if counts.matched > 0 {
        println!();
        println!("A match establishes that these bytes are the");
        println!("reference's, byte for byte. Where content is not");
        println!("distinctive, other clusters could hold the same");
        println!("bytes.");
    }

    // ADR-0013 Decision E. The causes are listed and none is chosen,
    // because the evidence does not distinguish between them.
    if counts.differed(options) > 0 {
        println!();
        println!("A differing digest does not say which of these");
        println!("happened: the file was fragmented, clusters of");
        println!("the run were reused, the recorded size is wrong,");
        println!("or the reference is another file.");
    }

    // Attached to the paragraph above rather than set apart: each is one
    // line about the invocation, not a caveat of its own.
    if !options.recover {
        println!("No content read. Pass --recover to read it.");
    } else if options.reference.is_none() {
        println!("No reference supplied. Nothing was validated.");
    }
}

/// A name the parser could render, or a marker that it could not.
///
/// A name field is eleven bytes of untrusted evidence. When the parser
/// refuses it, the bytes appear in an observation as hexadecimal rather than
/// reaching the terminal as characters.
fn rendered(name: Option<&str>) -> &str {
    name.unwrap_or("<not printable ascii>")
}

/// Prints one entry, from the listing or from the residue.
///
/// Both are printed the same way and in the same columns, because both are
/// directory entries read from the same bytes. Which vector an entry came
/// from is shown by the heading above it, not by its formatting.
///
/// `entries` and `index` locate the entry among its neighbours, which a
/// deleted short entry needs: the checksum that recovers its first byte is
/// held by the entries before it.
fn print_entry(entry: &Entry, entries: &[Entry], index: usize) {
    let position = format!("c{} s{}", entry.cluster, entry.slot);
    let (label, detail) = describe(&entry.kind, entries, index);

    println!("      {position:<9} {label:<13} {detail}");
}

/// A label and a one-line description for one classified entry.
fn describe(kind: &EntryKind, entries: &[Entry], index: usize) -> (&'static str, String) {
    match kind {
        EntryKind::VolumeLabel { name, .. } => {
            ("volume label", rendered(name.as_deref()).to_string())
        }
        EntryKind::ShortName {
            name,
            directory,
            first_cluster,
            file_size,
            ..
        } => {
            let name = rendered(name.as_deref());
            if *directory {
                ("directory", format!("{name:<13} cluster {first_cluster}"))
            } else {
                (
                    "file",
                    format!("{name:<13} {file_size} bytes, cluster {first_cluster}"),
                )
            }
        }
        EntryKind::LongName { ordinal, last, .. } => {
            let detail = if *last {
                format!("ordinal {ordinal}, last")
            } else {
                format!("ordinal {ordinal}")
            };
            ("long name", detail)
        }
        EntryKind::Deleted { was } => ("deleted", deleted_detail(was, associate(entries, index))),
        EntryKind::Invalid { attr } => ("invalid", format!("attribute {attr:#04x}")),
        // The listing never contains a terminator, but the residue can: a
        // slot whose first byte is zero and whose remaining bytes are not.
        EntryKind::Terminator => ("terminator", "first byte zero, rest not".to_string()),
    }
}

/// What can be said about a deleted entry without inventing a name.
///
/// A name appears only where the checksum of a surviving long-name component
/// determined the destroyed first byte. Where nothing determined it, that is
/// stated rather than filled in: ADR-0009 section 6.4 forbids substituting a
/// guess, and an omitted field would read as an absence of interest rather
/// than an absence of evidence.
fn deleted_detail(was: &DeletedKind, first: Option<FirstByte>) -> String {
    match was {
        DeletedKind::LongName { checksum } => {
            format!("long name component, checksum {checksum:#04x}")
        }
        DeletedKind::VolumeLabel { .. } => "volume label".to_string(),
        DeletedKind::ShortName {
            surviving_name,
            directory,
            first_cluster,
            file_size,
            ..
        } => {
            let what = if *directory {
                format!("directory, cluster {first_cluster}")
            } else {
                format!("file, {file_size} bytes, cluster {first_cluster}")
            };

            format!("{what}, {}", first_byte_detail(first, surviving_name))
        }
        DeletedKind::Invalid { attr } => format!("invalid, attribute {attr:#04x}"),
    }
}

/// How the destroyed first byte of a short name turned out.
fn first_byte_detail(first: Option<FirstByte>, surviving: &[u8; 10]) -> String {
    match first {
        Some(FirstByte::Recovered(byte)) => match recovered_name(byte, surviving) {
            Some(name) => format!("name {name} recovered"),
            None => format!("first byte {byte:#04x} recovered, name not printable ascii"),
        },
        Some(FirstByte::Destroyed) => "first byte destroyed, no long name survives".to_string(),
        Some(FirstByte::NotAssociated) => {
            "first byte destroyed, the long name before it names another entry".to_string()
        }
        // Unreachable: this arm is only reached for a deleted short entry,
        // which is exactly when `associate` answers.
        None => "first byte destroyed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference digest, for the invocations that supply one. Its value
    /// never reaches a comparison here: these tests exercise the counts,
    /// and `validation` owns what a comparison decides.
    fn reference() -> Sha256Digest {
        Sha256Digest::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
            .expect("64 hexadecimal characters")
    }

    fn recovering(reference: Option<Sha256Digest>) -> Options {
        Options {
            recover: true,
            reference,
        }
    }

    fn reporting() -> Options {
        Options {
            recover: false,
            reference: None,
        }
    }

    #[test]
    fn an_image_declaring_no_partition_is_covered_completely() {
        // `mbr-empty.img`: nothing was analysed because the table declares
        // nothing to analyse, which `ADR-0014` Appendix A.6 names as the
        // control a coverage statement must not report as a gap.
        let counts = RunCounts::default();

        assert_eq!(counts.coverage(), Coverage::Complete);
    }

    #[test]
    fn a_volume_analysed_with_nothing_missed_is_covered_completely() {
        let counts = RunCounts {
            volumes_analysed: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.coverage(), Coverage::Complete);
    }

    #[test]
    fn a_rejected_partition_table_leaves_nothing_analysed() {
        // `bad-signature.img`. Appendix A.8's rule reported this as a
        // success, because it counted uncovered partitions and this run
        // parsed none.
        let counts = RunCounts {
            table_rejected: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.coverage(), Coverage::NothingAnalysed);
    }

    #[test]
    fn a_listed_directory_leaves_a_volume_incompletely_covered() {
        // `fat32-recover-run.img`, whose root holds a deleted `/gone`.
        let counts = RunCounts {
            volumes_analysed: 1,
            directories_unread: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.coverage(), Coverage::Incomplete);
    }

    #[test]
    fn a_volume_lost_beside_one_analysed_is_incomplete() {
        // No fixture holds two volumes of which one is analysed and one is
        // not, which `ADR-0014` Appendix B.5 records. This is that case.
        let counts = RunCounts {
            volumes_analysed: 1,
            boot_rejected: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.coverage(), Coverage::Incomplete);
    }

    #[test]
    fn every_kind_of_gap_is_counted_as_one() {
        // The eleven kinds of Appendix B.4. Four are unreachable from any
        // fixture, so this is the only place they are exercised.
        let kinds: [fn(&mut RunCounts); 11] = [
            |counts| counts.table_unread += 1,
            |counts| counts.table_rejected += 1,
            |counts| counts.gpt += 1,
            |counts| counts.partition_unread += 1,
            |counts| counts.unidentified += 1,
            |counts| counts.unsupported += 1,
            |counts| counts.boot_rejected += 1,
            |counts| counts.root_unread += 1,
            |counts| counts.not_assessed += 1,
            |counts| counts.not_extracted += 1,
            |counts| counts.directories_unread += 1,
        ];

        for set in kinds {
            let mut counts = RunCounts::default();
            set(&mut counts);

            assert_eq!(counts.gaps(), 1);
            assert_eq!(counts.coverage(), Coverage::NothingAnalysed);

            counts.volumes_analysed = 1;

            assert_eq!(counts.coverage(), Coverage::Incomplete);
        }
    }

    #[test]
    fn an_artifact_is_a_free_run_that_was_read() {
        let counts = RunCounts {
            free_runs: 3,
            not_extracted: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.artifacts(recovering(None)), 2);
        assert_eq!(counts.not_read(recovering(None)), 0);
    }

    #[test]
    fn without_recover_every_free_run_is_unread_and_no_artifact_exists() {
        // The default invocation. `ADR-0010` Decision B: the run reports
        // what the volume states and reads no content.
        let counts = RunCounts {
            free_runs: 3,
            ..RunCounts::default()
        };

        assert_eq!(counts.artifacts(reporting()), 0);
        assert_eq!(counts.not_read(reporting()), 3);
    }

    #[test]
    fn a_difference_is_an_artifact_the_reference_did_not_equal() {
        let counts = RunCounts {
            free_runs: 3,
            matched: 1,
            ..RunCounts::default()
        };

        assert_eq!(counts.differed(recovering(Some(reference()))), 2);
    }

    #[test]
    fn nothing_differs_where_no_reference_was_supplied() {
        // `validate` returns `NotAttempted` for every extraction of such a
        // run, so no artifact is a difference rather than every one.
        let counts = RunCounts {
            free_runs: 3,
            ..RunCounts::default()
        };

        assert_eq!(counts.differed(recovering(None)), 0);
    }
}
