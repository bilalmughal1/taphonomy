//! Taphonomy command-line interface.
//!
//! The CLI is an interface to the library. It parses arguments, calls into
//! the library, and formats output. It contains no analysis logic.
//!
//! See `docs/development/RESEARCH_LOG.md` conclusion 6.

use std::process::ExitCode;

use taphonomy::EvidenceFile;
use taphonomy::Sha256Digest;
use taphonomy::fat_directory::{
    DeletedKind, Entry, EntryKind, FirstByte, associate, enumerate_root, recovered_name,
};
use taphonomy::fat_recovery::{Assessment, assess, extract};
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
    let mut caveats = Caveats::none();
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
                                caveats =
                                    caveats.merge(report_partition(&mut evidence, p, options));
                            }
                        }
                        PartitionTable::GptProtective => {
                            println!("GPT detected: not supported");
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
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Err(e) => eprintln!("error: {e}"),
    }

    print_caveats(caveats, options);

    Ok(())
}

/// Reports the filesystem found in one partition.
///
/// A read or identification failure for one partition is reported and does
/// not stop the others, nor change the process exit code: hashing already
/// succeeded and that result stands on its own.
fn report_partition(evidence: &mut EvidenceFile, p: &MbrPartition, options: Options) -> Caveats {
    let mut vbr = [0u8; VBR_SIZE];
    if let Err(e) = evidence.read_exact_at(p.start_byte(), &mut vbr) {
        eprintln!("error: partition {}: {e}", p.index);
        return Caveats::none();
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

    if id.filesystem() != Some(Filesystem::Fat32) {
        return Caveats::none();
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

            report_root_directory(evidence, &boot, extent, options)
        }
        Err(e) => {
            println!("    BOOT SECTOR REJECTED: {e}");
            Caveats::none()
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
) -> Caveats {
    let root = match enumerate_root(evidence, boot, extent) {
        Ok(root) => root,
        Err(e) => {
            println!("    ROOT DIRECTORY NOT ENUMERATED: {e}");
            return Caveats::none();
        }
    };

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

    let from_entries = report_recovery(evidence, boot, extent, &root.entries, options);
    let from_residue = report_recovery(evidence, boot, extent, &root.residue, options);

    from_entries.merge(from_residue)
}

/// Which statements a set of reported entries obliges the tool to make.
///
/// `report_recovery` runs once for the directory's entries and once for its
/// residue, so it returns what it owes rather than printing it. Printing
/// from both calls said the same thing twice: on
/// `fat32-deleted-residue.img`, which holds a recoverable deleted entry
/// past the terminator, the caveat block appeared twice in one run.
#[derive(Clone, Copy)]
struct Caveats {
    /// At least one entry's implied run was free in the FAT.
    any_free: bool,

    /// At least one extraction matched the operator's reference.
    any_match: bool,

    /// At least one extraction differed from it.
    any_differs: bool,
}

impl Caveats {
    /// Nothing owed. The value a path that reported no deleted content
    /// returns, so that the early exits do not each repeat a literal.
    const fn none() -> Self {
        Self {
            any_free: false,
            any_match: false,
            any_differs: false,
        }
    }

    /// What two sets of entries oblige between them.
    fn merge(self, other: Self) -> Self {
        Self {
            any_free: self.any_free || other.any_free,
            any_match: self.any_match || other.any_match,
            any_differs: self.any_differs || other.any_differs,
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
) -> Caveats {
    let mut heading = false;
    let mut any_free = false;
    let mut any_match = false;
    let mut any_differs = false;

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
                continue;
            }
        };

        head(&mut heading);

        match assessment {
            Assessment::Ineligible(reason) => {
                println!("        {position:<9} no content: {reason}");
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
            }
            Assessment::Recoverable(found) => {
                any_free = true;
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
                                    any_match = true;
                                    println!(
                                        "        {:<9} reference sha256 {reference} MATCHES",
                                        ""
                                    );
                                }
                                (Outcome::Differs, Some(reference)) => {
                                    any_differs = true;
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
                        }
                    }
                }
            }
        }
    }

    Caveats {
        any_free,
        any_match,
        any_differs,
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
fn print_caveats(caveats: Caveats, options: Options) {
    if !caveats.any_free {
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
    if caveats.any_match {
        println!();
        println!("A match establishes that these bytes are the");
        println!("reference's, byte for byte. Where content is not");
        println!("distinctive, other clusters could hold the same");
        println!("bytes.");
    }

    // ADR-0013 Decision E. The causes are listed and none is chosen,
    // because the evidence does not distinguish between them.
    if caveats.any_differs {
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
