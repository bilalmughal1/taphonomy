//! Taphonomy: local-first digital evidence recovery and analysis.
//!
//! # Status
//!
//! Milestone M1 of ADR-0002 section 8. Taphonomy can open a RAW evidence
//! image read-only, hash it, and report its size. No recovery capability is
//! implemented.
//!
//! # Safety model
//!
//! Source evidence is immutable. This crate contains no code that opens a
//! source path for writing. See `docs/SAFETY.md`.

pub mod error;
pub mod evidence;
pub mod fat;
pub mod fat32;
pub mod filesystem;
pub mod hash;
pub mod partition;

pub use error::Error;
pub use evidence::EvidenceFile;
pub use hash::{DIGEST_LEN, HashResult, Sha256Digest};
