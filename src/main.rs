//! Taphonomy command-line interface.
//!
//! The CLI is an interface to the library. It parses arguments, calls into
//! the library, and formats output. It contains no analysis logic.
//!
//! See `docs/development/RESEARCH_LOG.md` conclusion 6.

use std::process::ExitCode;

use taphonomy::EvidenceFile;
use taphonomy::fat_directory::{
    DeletedKind, Entry, EntryKind, FirstByte, associate, enumerate_root, recovered_name,
};
use taphonomy::fat32::{Fat32BootSector, parse_boot_sector};
use taphonomy::filesystem::{
    Filesystem, Identification, VBR_SIZE, VolumeExtent, declared_type_matches, identify,
};
use taphonomy::partition::{MbrPartition, PartitionTable, SECTOR_SIZE, parse_mbr};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);

    let Some(path) = args.next() else {
        eprintln!("usage: taphonomy <evidence-image>");
        return ExitCode::from(2);
    };

    if args.next().is_some() {
        eprintln!("error: expected exactly one path");
        return ExitCode::from(2);
    }

    match inspect(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(path: &std::ffi::OsStr) -> Result<(), taphonomy::Error> {
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
                                report_partition(&mut evidence, p);
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

    Ok(())
}

/// Reports the filesystem found in one partition.
///
/// A read or identification failure for one partition is reported and does
/// not stop the others, nor change the process exit code: hashing already
/// succeeded and that result stands on its own.
fn report_partition(evidence: &mut EvidenceFile, p: &MbrPartition) {
    let mut vbr = [0u8; VBR_SIZE];
    if let Err(e) = evidence.read_exact_at(p.start_byte(), &mut vbr) {
        eprintln!("error: partition {}: {e}", p.index);
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

    if id.filesystem() != Some(Filesystem::Fat32) {
        return;
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

            report_root_directory(evidence, &boot, extent);
        }
        Err(e) => {
            println!("    BOOT SECTOR REJECTED: {e}");
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
) {
    let root = match enumerate_root(evidence, boot, extent) {
        Ok(root) => root,
        Err(e) => {
            println!("    ROOT DIRECTORY NOT ENUMERATED: {e}");
            return;
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
