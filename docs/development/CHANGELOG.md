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
- FAT32 boot sector parsing validated against the partition extent
  (ADR-0002 M4)
- Detection of a volume declaring more sectors than its partition holds,
  a root directory cluster outside the data region, and a file allocation
  table too small for the clusters declared
- Reporting of a hidden sector count that records a start the volume does
  not have
- FAT32 boot sector detail reported by the CLI, including root cluster,
  backup boot sector location, FAT mirroring state, and volume label
- Rejection of an active FAT index at or beyond the declared FAT count
- FAT32 root directory enumeration by cluster chain walk (ADR-0002 M5,
  ADR-0008)
- Directory entry classification covering the volume label, 8.3 names,
  long-name components, and the attribute combination the specification
  names invalid
- Long-name entries retained in on-disk order and counted, not decoded
  (ADR-0007 §5)
- Cluster chain links read from the active FAT rather than from FAT 0
  unconditionally (ADR-0008 §4)
- Reporting of non-zero content found after a directory terminator, which
  a conformant volume does not contain (ADR-0008 §5)
- Refusal of a cyclic cluster chain, a chain link outside the data region,
  and a directory exceeding the specification's maximum size
- Root directory contents reported by the CLI, including the cluster chain
  and the position of every entry
- Deleted directory entry identification (ADR-0002 M6, ADR-0009)
- Classification of what a deleted entry was, with the fields deletion
  destroys omitted rather than computed, so that a destroyed long-name
  ordinal cannot be read as a valid one (ADR-0009 §3, EXP-0003 §4)
- Recovery of a short name's destroyed first byte from the checksum a
  surviving long-name entry carries, which determines it rather than
  narrowing it (ADR-0007 Appendix C, ADR-0009 §6.1)
- Rejection of a long-name association whose recovered byte is one a live
  entry never holds, which proves the set belongs to another entry
  (ADR-0009 §6.2)
- Entries beyond a directory terminator classified and reported separately
  from the allocated listing, rather than counted (ADR-0008 §5,
  ADR-0009 §4)
- Deleted entry count reported by the CLI alongside the short and long-name
  counts
- Fixtures carrying deleted 8.3 entries, a complete long-name set, a
  partial one left by slot reuse, a removed subdirectory, and entries past
  a terminator (ADR-0006 §5.1, ADR-0009 §8)
- Measurement of what mtools deletion destroys and leaves, and of what a
  fixture built with it can and cannot prove (EXP-0003)

### Changed

- FAT BIOS parameter block parsing and variant determination moved from
  `filesystem.rs` into a dedicated `fat` module; `filesystem.rs` retains
  generic identification only
- `EntryKind::Deleted` carries what the surviving bytes establish the entry
  to have been; it previously carried nothing (ADR-0009 §3)
- `RootDirectory` carries the classified slots beyond the terminator; the
  observation reporting them is retained unchanged (ADR-0009 §4.4)
