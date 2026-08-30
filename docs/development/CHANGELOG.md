# Taphonomy Changelog

This document records notable changes to Taphonomy across releases, including
new capabilities, fixes, and breaking changes, so that the history of the
project's behavior can be reviewed without reconstructing it from commit
history.

## Unreleased

### Added

- Read-only evidence file access with SHA-256 hashing (ADR-0002 M1)
- CLI reporting evidence path, reported size, bytes read, and digest
- SHA-256 verification against NIST FIPS 180-4 vectors (ADR-0004 §5.5)
- Evidence immutability tests covering permissions, mtime, and contents
- MBR partition table parser with bounds validation against evidence size
  (ADR-0002 M2, ADR-0005)
- GPT protective MBR detection, reported as unsupported rather than parsed
  (ADR-0005 §4)
- Deterministic synthetic fixture generation for partition parsing
- Positional evidence reads via `EvidenceFile::read_exact_at`
- Filesystem identification from volume boot record structure (ADR-0002 M3)
- FAT variant determined by data-region cluster count per the FAT
  specification, not by the advisory type string at offset 0x52
- Detection of disagreement between declared MBR partition type and observed
  filesystem
- exFAT and NTFS recognised and reported as unsupported
- Partition anomalies and filesystem observations reported as readable
  text rather than debug-formatted structures

### Changed

- FAT BIOS parameter block parsing and variant determination moved from
  `filesystem.rs` into a dedicated `fat` module; `filesystem.rs` retains
  generic identification only
