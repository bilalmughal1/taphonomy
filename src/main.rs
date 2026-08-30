//! Taphonomy command-line interface.
//!
//! The CLI is an interface to the library. It parses arguments, calls into
//! the library, and formats output. It contains no analysis logic.
//!
//! See `docs/development/RESEARCH_LOG.md` conclusion 6.

use std::process::ExitCode;

use taphonomy::EvidenceFile;
use taphonomy::fat32::parse_boot_sector;
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
        }
        Err(e) => {
            println!("    BOOT SECTOR REJECTED: {e}");
        }
    }
}
