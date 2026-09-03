//! Taphonomy: local-first digital evidence recovery and analysis.
//!
//! # Status
//!
//! Milestones M1 to M5 of ADR-0002 section 8. Taphonomy can open a RAW
//! evidence image read-only and hash it, parse an MBR partition table,
//! identify the filesystem in a partition from its own structure, validate
//! a FAT32 boot sector against the extent it occupies, and enumerate the
//! root directory by walking its cluster chain.
//!
//! No recovery capability is implemented. No file content is read, and no
//! deleted entry is interpreted.
//!
//! # Safety model
//!
//! Source evidence is immutable. This crate contains no code that opens a
//! source path for writing. See `docs/SAFETY.md`.

pub mod error;
pub mod evidence;
pub mod fat;
pub mod fat32;
pub mod fat_directory;
pub mod filesystem;
pub mod hash;
pub mod partition;

pub use error::Error;
pub use evidence::EvidenceFile;
pub use hash::{DIGEST_LEN, HashResult, Sha256Digest};
