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
- Recovery of the data of an unfragmented deleted file in the FAT32 root
  directory, extracted to memory and reported as a digest with the number
  of bytes it covers (ADR-0002 M7, ADR-0010)
- Refusal of a recovery whose implied cluster run reaches a cluster the
  active FAT reports as in use, naming the cluster that caused it
  (ADR-0010 §5)
- Eligibility reported as named refusals rather than silence, covering a
  deleted directory, a volume label, a long-name component, the invalid
  attribute pair, a zero size, a reserved first cluster, and a run
  extending past the last data cluster (ADR-0010 §6)
- Extraction streamed one cluster at a time, so memory does not scale with
  a size read from evidence; file slack is read, excluded from the digest,
  and its size reported (ADR-0010 §7)
- A project-owned incremental SHA-256 hasher, with `hash_reader` written
  over it so one implementation does the hashing (ADR-0010 §8)
- `--recover`, placing the reading of a deleted file's content behind an
  explicit request; without it the implied run and its allocation status
  are reported and no content is read (ADR-0010 §4)
- Fixtures carrying a four-cluster deleted file with its run intact, a
  single-cluster deleted file, a deleted directory, and a poked size whose
  implied run reaches a cluster a live file holds (ADR-0006 §5.1,
  ADR-0010 §10)
- Validation of recovered content against a digest the operator supplies,
  reported as a match or a difference alongside the number of bytes
  compared (ADR-0002 M8, ADR-0013)
- `--reference-digest`, accepting 64 hexadecimal characters in either case
  and rejecting everything else, including a pasted `sha256sum` line whose
  trailing filename would otherwise be discarded in silence (ADR-0013 §6.1)
- A reference that cannot be read stopping the run before any evidence is
  opened, which is the only condition M8 fails closed on (ADR-0013 §3.4)
- A reference supplied without `--recover` reported as an argument error
  rather than implying a flag that reads content (ADR-0013 Decision H)
- A differing digest reported as a finding about the evidence rather than a
  failed operation, so the run continues and the exit status is zero
  (ADR-0013 §3)
- The four conditions a differing digest is consistent with, enumerated
  without one of them being chosen (ADR-0013 Decision E)
- The limit of a match stated alongside it: byte equality with what was
  supplied, which establishes less where the content is not distinctive
  (ADR-0013 §8.2)
- `Sha256Digest::from_hex`, which reads a recorded digest and computes none
  (ADR-0013 §6.2)
- A `validation` module that compares digests, reads no evidence and names
  no filesystem structure, keeping validation separate from extraction
  (`docs/ARCHITECTURE.md` §5.6, ADR-0013 §11)
- Command-line argument handling covered by tests that run the binary,
  including that two of its checks precede opening the evidence
  (ADR-0013 §16)
- A fixture holding a deleted file whose three clusters were not adjacent,
  with the clusters between its fragments freed as well, built with
  `mtools` alone and containing no poked field (EXP-0004)
- Measurement of incorrect recovery on that fixture: three of its five
  deleted entries recover content that is not their own file's, and the
  tool reports all five identically (`CLAUDE.md` §26)
- A confidence level carried by a reconstructed artifact and reported once
  per run, the only level `ADR-0003`'s model leaves reachable for this tool
  (ADR-0002 M9, ADR-0014 Decision A)
- A coverage statement reporting what a run did not analyse, as a count per
  kind of gap and a status of complete, incomplete or none (ADR-0014
  Appendix B.5)
- A directory that was listed and not read counted as a gap, live or
  deleted, which four fixtures hold and no run previously reported
  (ADR-0014 Appendix B.2)
- Counts of the deleted entries that produced no artifact, by what stopped
  each one, so that an operator knows which questions went unanswered
  (ADR-0013 Appendix C.4)
- Artifact and comparison counts reported for the run rather than per
  entry, which `ADR-0003` §4.7 requires of a session (ADR-0014 Appendix B.8)
- A second fragmentation fixture, the same construction with the clusters
  between the fragments still allocated, on which the run the deleted entry
  implies reaches a live cluster and is refused rather than extracted. The
  refusal is reached with no poked field, where the existing collision
  fixture needs one (EXP-0004 Appendix A.5)

### Changed

- FAT BIOS parameter block parsing and variant determination moved from
  `filesystem.rs` into a dedicated `fat` module; `filesystem.rs` retains
  generic identification only
- `EntryKind::Deleted` carries what the surviving bytes establish the entry
  to have been; it previously carried nothing (ADR-0009 §3)
- `RootDirectory` carries the classified slots beyond the terminator; the
  observation reporting them is retained unchanged (ADR-0009 §4.4)
- `cluster_offset` and `read_fat_entry` widened from private to
  `pub(crate)`; neither moved (ADR-0010 §9)
- The in-memory `EvidenceReader` test double moved from `fat_directory.rs`
  into `evidence.rs`, where the trait it implements is declared, so both
  test modules share one rather than duplicating it (ADR-0010 Appendix C)
- The recovery digest line is labelled `recovered` and the comparison line
  `reference`, so the recovered-file hash and the validation hash are
  distinguishable wherever both appear (`docs/SAFETY.md` §10)
- Recovery caveats print once per volume rather than once per set of
  entries. On a volume holding a recoverable deleted entry past the
  directory terminator they previously appeared twice in one run
- The reporting functions take an options struct rather than a widening
  list of flags (ADR-0013 §13)
- A run that analysed nothing past the evidence digest exits 3. It
  previously exited 0 with nothing on standard output recording that
  nothing had been analysed (ADR-0014 Appendix B.10)
- Recovery caveats are computed from the run's counts rather than tracked
  separately alongside them, so the two cannot disagree (ADR-0014
  Appendix B.9)
