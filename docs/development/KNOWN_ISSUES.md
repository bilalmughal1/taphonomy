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

## Most fragmentation arrangements are unmeasured

Deletion zeroes the cluster chain in every FAT, measured in EXP-0003, so
nothing in the evidence says a deleted file occupied its clusters
contiguously. The recovery path computes the run the entry implies,
refuses it where any cluster is in use, and extracts where every cluster
is free.

The failure that follows is this tool's characteristic false positive: a
deleted file that was fragmented, whose clusters have since been freed,
produces a run that passes the allocation check and a digest that is
plausible and wrong.

Two arrangements are measured. Both are built with `mkfs.vfat`, `mcopy` and
`mdel` alone, with no poked field, by the technique EXP-0004 records, and
both share the generator's `build_fragmented_volume`.

`fat32-fragmented-deleted.img` holds a deleted file whose three clusters
were not adjacent, with the clusters between its fragments freed as well.
On it, three of the five deleted entries recover content that is not their
own file's, and the tool reports all five identically: every cluster free,
no refusal, a digest for each.

`fat32-fragmented-live-gap.img` is the same construction with the clusters
between the fragments still allocated to live files. The run the fragmented
entry implies reaches cluster 5, which the live `S2.BIN` holds, so the
entry is refused before any content is read. This is the arrangement the
tool handles correctly, and the refusal is reached with no byte written by
this project, where `fat32-recover-collision.img` needs a poked size to
produce one.

The run reports both images as covered completely, which is correct and is
a statement about reach alone: everything the tool could reach was
analysed. Three of the first image's five recoveries are still wrong.
Coverage is not an accuracy claim and must not be read as one.

Three arrangements this document previously listed as unmeasured are in
fact measured. The record is corrected here rather than carried forward:

* **More than two fragments.** `FRAG.BIN` occupies clusters 4, 6 and 8 in
  both images, which is three fragments and not two. EXP-0004 records the
  `mshowfat` output, and the generator asserts that arrangement on every
  run rather than assuming it.
* **An active file lying between the fragments.** That is
  `fat32-fragmented-live-gap.img`. EXP-0004 Appendix A.5 measured the
  construction and its determinism before it became a fixture.
* **Overwritten files.** `S3.BIN` and `S5.BIN` are deleted entries whose
  single cluster was reallocated to `FRAG.BIN` and never freed again, so
  both recover `FRAG.BIN`'s content rather than their own. EXP-0004
  Appendix A.2 records the digests and notes that this falsifies ADR-0010
  section 10.2.

What remains unmeasured is large files, a file wrapping from the end of the
volume to its beginning, and fragments in non-sequential order.

The last of those is blocked rather than merely unbuilt. `mtools` starts
each free-cluster search from the FAT32 FSINFO next-free hint and, once
that search has wrapped, takes the free clusters it finds in ascending
order, so a file whose logical fragment order runs backwards has no
allocation path. Both routes to one are closed: ADR-0006 section 5.2
rejects mounting, and a poked FAT chain or `mdoctorfat` falls under
section 5.1's objection to hand-built structure and outside the CFTT
specification's own testing scope, which excludes file system metadata that
has been corrupted, modified or otherwise manipulated.

NIST's CFTT deleted file recovery suite defines seventeen test cases, and
the tree now holds the equivalent of two of them, so no score across the
suite can be computed. Until more are built, item 5 of the README's
development sequence stays open.

Validation against a reference does not close the remainder. It detects a
wrong digest only where the operator already holds the right one, which is
not the case the requirement is about.

---

## Three test suites are dominated by whole-image reads

`tests/cli_arguments.rs` runs the binary rather than calling into the
crate, because argument handling and exit status are decisions `main`
makes about `argv` and are not reachable from the library. Six of its
eleven tests open a fixture, and the binary hashes the whole 64 MB image
before it reports anything.

`tests/coverage_reporting.rs` runs the binary for the same reason: coverage
and the exit status derived from it are decided once per run. Its eight
tests spawn it eleven times, and every one of those hashes an image.

`tests/fat32_recovery_fixtures.rs` is comparable, and for a different
reason. It calls into the crate and spawns no process, so the cost is not
the binary: it is opening 64 MB fixtures. Measured 2026-09-19:

```text
tests/coverage_reporting.rs       8 tests  24.11s
tests/cli_arguments.rs           11 tests  15.90s
tests/fat32_recovery_fixtures.rs 22 tests   8.37s
```

Every other suite in the workspace finishes in under a tenth of a second,
and `cargo test --workspace` now takes about 48 seconds, of which those
three are 48.

A fixture small enough for tests that only need an argument decision would
address the first and not the second. Reaching those decisions without a
whole-image read would address the first only. Neither is needed yet, but
the entry should not be read as naming a single suite.
