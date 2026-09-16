//! Taphonomy: local-first digital evidence recovery and analysis.
//!
//! # Status
//!
//! Milestones M1 to M8 of ADR-0002 section 8. Taphonomy can open a RAW
//! evidence image read-only and hash it, parse an MBR partition table,
//! identify the filesystem in a partition from its own structure, validate
//! a FAT32 boot sector against the extent it occupies, enumerate the root
//! directory by walking its cluster chain, identify the deleted entries in
//! it, including those beyond the terminator, recover the data of an
//! unfragmented deleted file, and validate that data against a reference
//! digest the operator supplies.
//!
//! Recovery extracts to memory and reports a digest. No file is written:
//! ADR-0010 Decision A. Deletion zeroes the cluster chain, so nothing in
//! the evidence establishes that a deleted file was unfragmented. The run
//! its entry implies is checked against the active FAT and refused where
//! any cluster in it is in use. A run of free clusters is the absence of
//! contrary evidence and is not treated as more than that.
//!
//! A digest match against a reference is the only evidence available that
//! the run read was the file's clusters, and it is evidence about that one
//! recovery rather than about the assumption: ADR-0013 section 8. A
//! reference is supplied by the operator and never discovered. Where none
//! is supplied nothing is validated, which is reported rather than left as
//! a silence.
//!
//! A deleted entry's long name is not decoded.
//!
//! # Safety model
//!
//! Source evidence is immutable. This crate contains no code that opens a
//! source path for writing. See `docs/SAFETY.md`.

pub mod confidence;
pub mod error;
pub mod evidence;
pub mod fat;
pub mod fat32;
pub mod fat_directory;
pub mod fat_recovery;
pub mod filesystem;
pub mod hash;
pub mod partition;
pub mod validation;

pub use error::Error;
pub use evidence::EvidenceFile;
pub use hash::{DIGEST_LEN, HashResult, Sha256Digest};
