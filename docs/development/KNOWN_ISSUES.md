# Taphonomy Known Issues

This document tracks known limitations, defects, and unresolved failure
modes in Taphonomy that have not yet been fixed, so that users and
contributors can distinguish an untested gap from a confirmed, documented
issue.

---

## No fixture exercises FAT mirroring

Every fixture writes `BPB_ExtFlags` as `0x0000`, meaning mirroring is
enabled and all FATs are maintained. Measured 2026-08-30:

```text
xxd -s $((1048576 + 0x28)) -l 2 -p fixtures/partition/mbr-single-fat32.img
  0000
```

`mkfs.vfat` offers no option to set the field. A fixture exercising the
mirroring-disabled path must poke `ExtFlags` and additionally make the
FATs differ, because `mkfs.vfat` writes them identically and a parser
reading FAT 0 would otherwise produce the same result as one reading
FAT 1.

`parse_boot_sector` validates the active FAT index against the declared
FAT count, and that check has unit coverage. Code that acts on the index
now exists: `enumerate_root` reads the FAT named by `BPB_ExtFlags` when
mirroring is disabled, and FAT 0 otherwise. It is covered by
`the_active_fat_is_read_when_mirroring_is_disabled`, which builds a volume
in memory with `ExtFlags` set to `0x0081`, a stale chain in FAT 0 and the
maintained chain in FAT 1.

What remains untested is the path against evidence a formatting tool
produced. No fixture reaches it, and none can until the fixture described
above exists.

## Long filename patent status unverified

Microsoft held patents on the VFAT long-filename mechanism and litigated
them. Their current status has not been researched. ADR-0007 section 5.5
records this as required before long-name decoding is implemented. It
does not affect enumeration, which counts long-name entries without
interpreting them.

## No fixture can exercise a zeroed first-cluster high word

Deleting a file leaves the directory entry's `DIR_FstClusHI` intact under
`mtools`. Measured 2026-09-04, EXP-0003:

```text
HIGHFILE.TXT before deletion   hi=1 lo=2055  cluster 67591
HIGHFILE.TXT after mdel        hi=1 lo=2055  cluster 67591
```

Microsoft implementations are widely reported to zero that word, so a
deleted file beginning above cluster 65,535 loses its start location
entirely on evidence from a Windows host. The FAT32 specification does
not require the behaviour either way.

No fixture can produce the case, because the tool that builds the
fixtures does not produce it. Reaching it requires poking an image under
ADR-0006 section 5.1. Until that fixture exists, any code that handles a
zeroed high word is untested against evidence a formatting tool wrote.

`mdel`, `mrd` and `mdeltree` determinism is measured and is no longer an
open issue; EXP-0003 records it.

---

## FAT-level helpers live in the directory module

`cluster_offset` computes a data cluster's byte offset and `read_fat_entry`
reads one entry from the file allocation table. Neither is about
directories. Both are declared in `src/fat_directory.rs`, where M5 put them
because it was the only caller.

M7 added `src/fat_recovery.rs` as a second caller, so both widened to
`pub(crate)`. ADR-0010 section 9 decided against moving them: the move
would edit a path M5's and M6's tests cover, for an organisational gain
rather than a measured one.

The cost shows in the error type. `RecoveryError::Directory` carries a
`DirectoryError`, and the variants reaching the recovery module through
that path are `OffsetOverflow`, `ClusterOutOfRange` and `BadCluster`. None
of those is a directory failure, so a reader of a recovery error is told
the wrong thing about where it happened. ADR-0010 Appendix B6 records the
same trade.

The fix is to move both into `src/fat.rs`, which is BIOS parameter block
parsing and variant determination and today holds no FAT-table access at
all. The trigger is a third caller, or any change that touches
`fat_directory.rs`'s FAT handling for another reason.

---

## Only one fragmentation arrangement is measured

Deletion zeroes the cluster chain in every FAT, measured in EXP-0003, so
nothing in the evidence says a deleted file occupied its clusters
contiguously. The recovery path computes the run the entry implies,
refuses it where any cluster is in use, and extracts where every cluster
is free.

The failure that follows is this tool's characteristic false positive: a
deleted file that was fragmented, whose clusters have since been freed,
produces a run that passes the allocation check and a digest that is
plausible and wrong.

That case is now measured. `fat32-fragmented-deleted.img` holds a deleted
file whose three clusters were not adjacent, with the clusters between its
fragments freed as well. It is built with `mkfs.vfat`, `mcopy` and `mdel`
alone, with no poked field, by the technique EXP-0004 records. On it, three
of the five deleted entries recover content that is not their own file's,
and the tool reports all five identically: every cluster free, no refusal,
a digest for each.

What remains open is the rest of the space. NIST's CFTT deleted file
recovery suite defines seventeen test cases, and this fixture is the
equivalent of one of them. Unmeasured here: an active file lying between
the fragments rather than a deleted one, fragments in non-sequential
order, files of more than two fragments, overwritten files, and large
files. Until more of those are built, item 5 of the README's development
sequence stays open.

Validation against a reference does not close the remainder. It detects a
wrong digest only where the operator already holds the right one, which is
not the case the requirement is about.

---

## Two test suites are dominated by whole-image reads

`tests/cli_arguments.rs` runs the binary rather than calling into the
crate, because argument handling and exit status are decisions `main`
makes about `argv` and are not reachable from the library. Three of its
eight tests open a fixture, and the binary hashes the whole 64 MB image
before it reports anything.

`tests/fat32_recovery_fixtures.rs` is now comparable, and for a different
reason. It calls into the crate and spawns no process, so the cost is not
the binary: it is opening 64 MB fixtures. Measured 2026-09-15:

```text
tests/cli_arguments.rs           8 tests   9.34s
tests/fat32_recovery_fixtures.rs 22 tests  8.26s
```

Every other suite in the workspace finishes in under a tenth of a second.

A fixture small enough for tests that only need an argument decision would
address the first and not the second. Reaching those decisions without a
whole-image read would address the first only. Neither is needed yet, but
the entry should not be read as naming a single suite.
