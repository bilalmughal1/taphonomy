# Taphonomy Experiments

This document records recovery experiments performed during development,
each stating its question, hypothesis, input, method, expected result,
actual result, conclusion, and next action, so that recovery claims remain
traceable to the controlled evidence that produced them.

---

## EXP-0001: Determinism of synthetic partition fixture generation

**Date:** 2026-08-29
**Milestone:** M2 (ADR-0002 §8)
**Related:** ADR-0002 Appendix A §A.3; `PROJECT.md` §6.4;
`DEVELOPMENT_ENVIRONMENT.md` §11

---

### Question

Does `scripts/generate-fixtures.sh` produce byte-identical output on repeated
runs?

### Why it matters

`PROJECT.md` §6.4 requires deterministic behaviour where technically
possible. `DEVELOPMENT_ENVIRONMENT.md` §11 requires fixtures that are
deterministic and reproducible.

If fixture generation is not deterministic, a fixture-based test cannot
distinguish a regression in the parser from a change in the fixture, and
`DEVELOPMENT_ENVIRONMENT.md` §12's golden-result model does not hold.

ADR-0002 Appendix A §A.3 recorded FAT32 fixture determinism as an
**unverified** claim and required that it be measured before being cited.
This experiment discharges that obligation.

### Hypothesis

`mkfs.vfat --invariant` fixes values that would otherwise derive from the
system clock or a random source, and `sfdisk` with an explicit `label-id`
writes no time-derived fields. Given explicit GUIDs, `sgdisk` should
likewise be reproducible.

Predicted outcome: all seven fixtures byte-identical.

Confidence before running: moderate. `sgdisk` was considered the most likely
to fail, since GPT headers carry a disk GUID and the tool may ignore an
explicitly supplied one.

### Input

`scripts/generate-fixtures.sh`, producing seven images:

```text
mbr-single-fat32.img
mbr-four-partitions.img
mbr-empty.img
no-signature.img
bad-signature.img
partition-beyond-end.img
gpt-protective.img
```

Each 64 MiB (131,072 sectors of 512 bytes).

### Environment

```text
Host              Ubuntu 24.04 (noble) under WSL2 on Windows
mkfs.fat          4.2 (2021-01-31), dosfstools 4.2-1.1build1
sfdisk            util-linux 2.39.3
sgdisk            GPT fdisk 1.0.10
```

### Method

`scripts/verify-fixtures.sh` generates the full set twice into separate
temporary directories and compares each image with `cmp`.

No root privileges, no loop devices, no mounted filesystems. All targets are
regular files.

### Expected result

Seven of seven byte-identical.

### Actual result

**Seven of seven byte-identical.**

```text
IDENTICAL   bad-signature.img
IDENTICAL   gpt-protective.img
IDENTICAL   mbr-empty.img
IDENTICAL   mbr-four-partitions.img
IDENTICAL   mbr-single-fat32.img
IDENTICAL   no-signature.img
IDENTICAL   partition-beyond-end.img

Deterministic: 7/7 fixtures byte-identical across runs.
```

Recorded digests are in `fixtures/partition/MANIFEST.sha256`.

### Structural verification

Determinism alone does not establish that a fixture contains what it claims.
Each was inspected directly.

`mbr-single-fat32.img`, entry 1 at offset `0x1BE`:

```text
80 20 21 00 0c 28 20 08 00 08 00 00 00 f8 01 00
```

`0x80` bootable, type `0x0c` (W95 FAT32 LBA), start LBA 2048
(`00 08 00 00` little-endian), 129,024 sectors. Signature `55aa` at `0x1FE`.

FAT32 volume boot record at byte offset 1,048,576 (LBA 2048):

```text
eb 58 90 6d 6b 66 73 2e 66 61 74   jump, OEM "mkfs.fat"
... 54 41 50 48 46 49 58           label "TAPHFIX"
... 46 41 54 33 32                 type "FAT32"
```

Confirms `mkfs.vfat --offset=2048` wrote the filesystem inside the partition
rather than at sector 0.

`gpt-protective.img`, entry 1:

```text
00 00 02 00 ee 28 20 08 01 00 00 00 ff ff 01 00
```

Type `0xEE`, start LBA 1, 131,071 sectors. This is the protective MBR
ADR-0005 §4 requires the parser to detect.

`partition-beyond-end.img`, entry 1:

```text
00 20 21 00 0c 28 20 08 00 08 00 00 ff ff ff ff
```

Sector count `0xFFFFFFFF` against a 131,072-sector image.

`bad-signature.img`: signature bytes at `0x1FE` are `0000`.

### Conclusion

Fixture generation is deterministic on this environment and toolchain.
Fixture-based tests may be treated as reproducible, and
`DEVELOPMENT_ENVIRONMENT.md` §12's golden-result model applies.

The determinism claim in ADR-0002 Appendix A §A.3 is now **measured** for
FAT32.

### Limitations

1. Measured on one host with one toolchain version. Determinism across
   `dosfstools`, `util-linux` or `gdisk` versions is **not** established. A
   version change may alter output; `MANIFEST.sha256` is the detection
   mechanism.
2. Determinism across architectures or non-Linux hosts is not established.
3. `mkntfs` determinism, the comparison ADR-0002 §3.2 originally asserted,
   remains **unmeasured**. `ntfs-3g` is not installed and NTFS is not the
   current target. This does not affect the decision, whose primary reason is
   specification availability (ADR-0002 §3.1).

### Next action

1. Commit `MANIFEST.sha256` as a golden result.
2. Add a regression check comparing regenerated fixtures against the manifest
   when the toolchain changes.
3. Proceed to the MBR parser, using these fixtures as its test inputs.

### Re-measurement 2026-08-30

The fixture set grew to eight with the addition of `mbr-type-mismatch.img`.
`verify-fixtures.sh` was re-run and reported 8 of 8 byte-identical.

```text
Deterministic: 8/8 fixtures byte-identical across runs.
```

The conclusion is unchanged.

### Re-measurement 2026-08-30 (second)

The fixture set grew to twelve with the addition of four FAT32 boot
sector corruption fixtures: `fat32-oversized-volume.img`,
`fat32-hidden-mismatch.img`, `fat32-bad-root-cluster.img` and
`fat32-undersized-fat.img`. Each is generated by the same procedure as
`mbr-single-fat32.img`, then corrupted in a single BPB field by a new
`poke_le32` helper.

`verify-fixtures.sh` was re-run and reported 12 of 12 byte-identical.

```text
Deterministic: 12/12 fixtures byte-identical across runs.
```

The conclusion is unchanged. Determinism now covers byte-level
corruption applied after `mkfs.vfat`, which the original measurement did
not exercise.

### Re-measurement 2026-08-30 (third)

`fat32-hidden-mismatch.img` was regenerated. The original poked zero into
`BPB_HiddSec`, which measurement showed was already the value `mkfs.vfat`
writes for a volume formatted with `--offset`:

```text
xxd -s 1048576 -l 96 fixtures/partition/mbr-single-fat32.img
  00100010: 0200 0000 00f8 0000 2000 0800 0000 0000
```

Bytes `0x1C` through `0x1F` are zero. The fixture therefore differed from
`mbr-single-fat32.img` only in its label-id and tested nothing. It now
pokes 9999, a start the volume could not have had.

`verify-fixtures.sh` was re-run and reported 12 of 12 byte-identical.

```text
Deterministic: 12/12 fixtures byte-identical across runs.
```

One manifest digest changed, `fat32-hidden-mismatch.img` from
`1dd69836...` to `c14c1ddd...`. The other eleven are unchanged, which is
the property the manifest exists to establish.

The conclusion is unchanged.

### Re-measurement 2026-09-03

The fixture set grew to fourteen with the addition of
`fat32-root-multicluster.img`, a FAT32 volume whose root directory occupies
more than one cluster. ADR-0008 §8.1 records why it was needed: every
earlier fixture has a single-cluster root directory, so no fixture exercised
cluster chain walking.

`verify-fixtures.sh` was re-run and reported 14 of 14 byte-identical.

```text
Deterministic: 14/14 fixtures byte-identical across runs.
```

The manifest gained one line and the other thirteen digests are unchanged,
which is the property the manifest exists to establish.

**This series moves from twelve to fourteen.** The thirteenth fixture,
`fat32-root-entries.img`, was confirmed byte-identical when it was added on
2026-08-31 — EXP-0002 Next action item 3 records that as done, and EXP-0002
Limitations item 3 relies on it — but the figure was never entered here. The
gap is in this record, not in the measurement.

#### Structural verification of the new fixture

Determinism establishes that the fixture is reproducible. It does not
establish that the fixture exercises what it was built to exercise. The
file allocation table was read directly:

```text
xxd -s 1064960 -l 16 fixtures/partition/fat32-root-multicluster.img
  00104000: f8ff ff0f ffff ff0f 1300 0000 ffff ff0f

FAT[2] = 0x00000013, the root directory continues at cluster 19
```

The root directory chain is 2 → 19 → end of chain. The first cluster holds
the volume label and `FILE01` through `FILE15`: sixteen entries, completely
full, with no terminator anywhere in it. The second holds `FILE16` through
`FILE20` followed by a `0x00` terminator.

Cluster 19 sits between file data on both sides. Cluster 18 holds
`FILE16`'s content and cluster 20 holds `FILE17`'s, because the root
directory grew only when its seventeenth entry would not fit. A chain walk
that read the adjacent cluster instead of following the FAT would therefore
read file text rather than directory entries, and would fail visibly rather
than producing plausible output.

The fixture consequently proves three properties a single-cluster root
directory cannot: that the walk follows the FAT, that it does not treat a
full cluster as terminated, and that a terminator in a later cluster ends
the listing correctly.

One gap remains. `FAT[2]` is `0x00000013` with its high nibble zero, so no
fixture sets the reserved bits of a FAT32 entry. The 28-bit mask required
by ADR-0008 §8.2 is not exercised by any image and must be covered by a
unit test against an in-memory reader.

The conclusion is unchanged.

---

## EXP-0002: Determinism of mtools directory entry timestamps

**Date:** 2026-08-30
**Milestone:** M5 (ADR-0002 §8), prerequisite
**Related:** ADR-0006 §4, §6; `DEVELOPMENT_ENVIRONMENT.md` §11, §12;
`PROJECT.md` §6.4

---

### Question

Does `mcopy` produce byte-identical output across runs, and if not, can its
output be pinned?

### Why it matters

Every fixture before this one is an empty filesystem. M5 requires files
inside a volume, and `mkfs.vfat` cannot create them.

EXP-0001 established that fixture generation is deterministic and that
`MANIFEST.sha256` may therefore be treated as a golden result under
`DEVELOPMENT_ENVIRONMENT.md` §12. That finding was measured against tooling
that writes no timestamps. FAT directory entries carry creation, write and
access times, so a tool that populates them may break the property EXP-0001
established.

ADR-0002 Appendix A records a determinism claim written from assumption and
later found unsupported. This experiment was run before ADR-0006 was drafted
so that the same failure was not repeated.

### Hypothesis

`mcopy` writes the current time into `DIR_CrtTime` and `DIR_WrtTime`, making
its output non-deterministic across runs separated by more than the field's
two-second resolution.

Predicted outcome: two runs in quick succession identical, two runs seconds
apart differing in the directory entry.

Confidence before running: high on the mechanism, low on whether a
mitigation existed.

### Input

Four 64 MiB images built by identical procedure:

```text
dd if=/dev/zero of=IMAGE bs=512 count=131072
sfdisk --quiet --no-tell-kernel IMAGE   (label-id 0x1a2b3c4d, one type=c
                                         partition at LBA 2048)
mkfs.vfat --invariant --mbr=n -F 32 -n TAPHFIX --offset=2048 IMAGE 64512
MTOOLS_SKIP_CHECK=1 mcopy -i IMAGE@@1M payload.txt ::/payload.txt
```

`payload.txt` is 26 bytes, its mtime pinned with
`touch -d '2020-01-01 00:00:00 UTC'`.

All work was done in `/tmp`. No fixture in the repository was touched.

### Environment

```text
Host       Ubuntu 24.04 (noble) under WSL2 on Windows
mtools     4.0.43-1build1
TZ         Asia/Dubai (UTC+4) unless stated otherwise
```

### Method

Four comparisons:

1. `a.img` and `b.img`, built consecutively with no delay.
2. `a.img` and `c.img`, built five seconds apart.
3. `d.img` and `e.img`, three seconds apart, with
   `SOURCE_DATE_EPOCH=1577836800` exported.
4. `d.img` and `f.img`, both with `SOURCE_DATE_EPOCH` set, `f.img`
   additionally with `TZ=UTC`.

Each comparison used `cmp`, and the resulting directory entry was decoded
with `xxd` rather than relying on `cmp` alone. Two runs inside one clock
tick compare equal without being deterministic, and `cmp` cannot distinguish
that case.

### Expected result

Comparison 1 identical, comparison 2 differing.

Comparisons 3 and 4 had no prediction. Whether `mtools` honours
`SOURCE_DATE_EPOCH` was unknown before running.

### Actual result

**1. Same second: identical.**

**2. Five seconds apart: differing.**

```text
cmp a.img c.img
  a.img c.img differ: byte 2081839, line 5
   2081839 263 320
   2081847 263 320
```

Both bytes lie in the payload's root directory entry, at entry offsets
`0x0E` (`DIR_CrtTime`) and `0x16` (`DIR_WrtTime`). Values moved from
`0x98B3` to `0x98D0`, decoding as 19:05:38 to 19:06:32.

Only the time fields moved because both runs fell on the same day. Across
midnight the date fields would move as well.

The payload's pinned mtime did not propagate: the recorded date was the
current date, not 2020-01-01. `mcopy` does not preserve source modification
time by default.

**3. With `SOURCE_DATE_EPOCH=1577836800`: identical, and pinned.**

```text
001fc420: 5041 594c 4f41 4420 5458 5420 1800 0020  PAYLOAD TXT
001fc430: 2150 2150 0000 0020 2150 0300 1a00 0000
```

`DIR_CrtDate` and `DIR_WrtDate` read `0x5021`: year field 40, so 1980+40 =
2020, month 1, day 1. The identity is genuine rather than a clock-tick
coincidence.

**4. Adding `TZ=UTC`: different bytes for the same instant.**

```text
001fc420: 5041 594c 4f41 4420 5458 5420 1800 0000  PAYLOAD TXT
001fc430: 2150 2150 0000 0000 2150 0300 1a00 0000

cmp d.img f.img
  d.img f.img differ: byte 2081840, line 5
```

`DIR_CrtTime` is `0x0000` (00:00:00) under `TZ=UTC` and `0x2000` (04:00:00)
under UTC+4. `SOURCE_DATE_EPOCH=1577836800` is 2020-01-01 00:00:00 UTC in
both cases.

### Conclusion

`mtools` is **not** deterministic as invoked. It becomes deterministic when
both `SOURCE_DATE_EPOCH` and `TZ` are set. Either alone is insufficient:
without the first, output depends on when the script runs; without the
second, on where.

FAT directory entries store local time with no timezone field, so the
timezone dependence is a property of the filesystem format rather than of
`mtools`, and no tool choice avoids it.

`scripts/generate-fixtures.sh` exports both. EXP-0001's conclusion continues
to hold for the fixture set, now including file-bearing fixtures, on the
condition that those exports remain.

### Limitations

1. **Fixture bytes depend on the host timezone.** This is a new limitation.
   EXP-0001 records that determinism across architectures and non-Linux
   hosts is unmeasured; a timezone difference between two developers is far
   more likely than either. The exports in the script are the mitigation,
   and `verify-fixtures.sh` is the detection mechanism if they are removed.
2. Measured on one host with `mtools` 4.0.43-1build1. Whether other versions
   honour `SOURCE_DATE_EPOCH` identically is not established.
3. Only `mcopy` was measured. `mmd`, used for the subdirectory in
   `fat32-root-entries.img`, was not measured separately. It is covered
   transitively: `verify-fixtures.sh` reports that fixture byte-identical
   across runs, and it contains an `mmd`-created directory.
4. Deleting a file with `mdel`, which M6 will require, was not measured.

### Next action

1. Record the decision and its licence reasoning in ADR-0006. **Done.**
2. Export both variables in `scripts/generate-fixtures.sh`. **Done.**
3. Add `fat32-root-entries.img` and confirm 13 of 13 byte-identical.
   **Done.**
4. Measure `mdel` determinism before M6 introduces deleted-entry fixtures.
   **Done.** EXP-0003.

---

## EXP-0003: What mtools deletion destroys, and whether it is deterministic

**Date:** 2026-09-04
**Milestone:** M6 (ADR-0002 §8), prerequisite
**Related:** ADR-0003 §3; ADR-0006 §4, §5.1; ADR-0007 §5.3; ADR-0008 §12.2;
EXP-0002 Limitations 4 and Next action 4; `KNOWN_ISSUES.md`

---

### Question

Two questions, folded into one experiment because they share a volume.

1. Is `mdel` deterministic under `SOURCE_DATE_EPOCH` and `TZ=UTC`?
2. What does `mdel` actually destroy, and what survives?

A third was added before the experiment ran. `mdel` deletes files. It does
not delete directories, and M6 requires a fixture containing a deleted
subdirectory. `mrd` and `mdeltree` are therefore measured alongside it, and
their determinism and destruction behaviour were as unmeasured as `mdel`'s.

### Why it matters

EXP-0002 measured `mcopy` and, transitively, `mmd`. Deletion was not
measured, and `KNOWN_ISSUES.md` records it as required before M6 introduces
deleted-entry fixtures.

Question 2 is the one that decides what an M6 fixture can prove. Deletion
behaviour is implementation-specific rather than specified: the FAT32
specification states that a first byte of `0xE5` marks a free entry, and
says nothing about what else an implementation may do at the same time.
Microsoft implementations are widely reported to zero the high word of the
first cluster, which the specification does not require. If `mtools` does
not, a fixture built with it will not exercise that case, and the tool will
appear more capable against fixtures than against evidence from a Windows
host.

That is the "what a fixture proves versus what it illustrates" problem
recorded in ADR-0008 §8.1, in its second instance.

### Hypothesis

Recorded before the experiment was run.

1. `mdel` is deterministic, and would be even without the exports, because
   deletion writes no timestamp. The exports remain necessary for the
   `mcopy` steps that build the volume. Confidence: high on the mechanism,
   moderate on whether `mtools` touches anything else.
2. `mdel` writes `0xE5` into byte 0 of the short entry and of every
   preceding long-name entry. High.
3. `mdel` zeroes the file's cluster chain in the FAT. High.
4. `mdel` does **not** zero `DIR_FstClusHI`, and does not zero
   `DIR_FstClusLO`. Moderate.
5. Name bytes 1 to 10, `DIR_Attr`, `DIR_NTRes`, all timestamps,
   `DIR_FileSize` and the long-name checksum byte at `LDIR_Chksum` survive
   untouched. High.

No prediction was made about `mrd` or `mdeltree`, or about the FSInfo
sector.

### Input

One 64 MiB image, built by the procedure below and then copied before each
deletion, so that every deletion is measured against the same baseline.

```text
dd if=/dev/zero of=a.img bs=512 count=131072 status=none
mkfs.vfat --invariant --mbr=n -F 32 -n TAPHFIX a.img
mcopy -i a.img low.txt ::/LOWFILE.TXT
mcopy -i a.img long.txt "::/a long deleted name.txt"
mmd   -i a.img ::/SUBDIR
mcopy -i a.img low.txt ::/SUBDIR/INNER.TXT
mcopy -i a.img bigfill.bin ::/BIGFILL.BIN
mcopy -i a.img high.txt ::/HIGHFILE.TXT
```

No partition table. The image is a bare FAT32 volume, because the question
concerns entry and FAT bytes and a partition table would only displace every
offset by a constant.

Five deletion targets, each chosen for a property:

```text
LOWFILE.TXT               pure 8.3 name, one entry, low cluster
a long deleted name.txt   two long-name entries preceding a short entry
SUBDIR                    a directory, removed by mrd and by mdeltree
SUBDIR/INNER.TXT          a file inside a directory that is then removed
HIGHFILE.TXT              first cluster above 65,535
```

`BIGFILL.BIN` is 33 MiB of zeroes and is never deleted. It exists only to
consume clusters so that `HIGHFILE.TXT`, allocated after it, begins above
cluster 65,535 and can test the high word of the first cluster. Measured, it
does: `HIGHFILE.TXT` begins at cluster 67,591, with `DIR_FstClusHI` = 1 and
`DIR_FstClusLO` = 2055.

All work was done in `/tmp/exp0003`. No fixture in the repository was
touched, and no repository file was read or written by the experiment.

### Environment

```text
Host       Ubuntu 24.04 (noble) under WSL2 on Windows
mtools     4.0.43-1build1
mkfs.fat   4.2 (2021-01-31)
Exports    SOURCE_DATE_EPOCH=1577836800  TZ=UTC  MTOOLS_SKIP_CHECK=1
```

Volume geometry, read from the BIOS parameter block rather than from the
command line that produced it:

```text
BPS=512 SPC=1 RSV=32 FATs=2 FATSz32=1009 RootClus=2
FAT 0 at 16384   FAT 1 at 532992   data and root at 1049600
129,022 clusters
```

### Method

Deletions were performed on copies of one baseline image, and each result
compared against the baseline byte by byte. Every differing byte was located
to a named structure and decoded, rather than counted.

`cmp` alone is insufficient for the determinism question, for the reason
EXP-0002 records: two runs inside one clock tick compare equal without being
deterministic. Determinism was therefore tested by deleting from the same
baseline twice, four seconds apart, and the decoded result inspected
separately.

```text
1. dump the baseline root directory, decoding every entry
2. cp a.img del.img; mdel three files
3. sleep 4; cp a.img del2.img; mdel the same three files
4. cmp del.img del2.img
5. diff a.img against del.img, locating every changed byte
6. cp a.img rd.img; mdel INNER.TXT; mrd SUBDIR; diff against a.img
7. cp a.img dt.img; mdeltree SUBDIR;          diff against a.img
```

### Expected result

Comparison at step 4 identical. Step 5 to show `0xE5` in the first byte of
five root entries, zeroed FAT entries for the three deleted files, and
nothing else.

### Actual result

**1. `mdel` is deterministic.** Two deletions from the same baseline four
seconds apart produced byte-identical images.

```text
cmp del.img del2.img
  (no output)
```

**2. `mdel` changed exactly 30 bytes in a 64 MiB image.**

The root directory, before and after:

```text
before                                                    after
1  4c4f5746494c4520545854  clus 3      size 17            byte 0 -> 0xE5
2  LFN ord 0x42 LAST  cksum 0x32                          byte 0 -> 0xE5
3  LFN ord 0x01       cksum 0x32                          byte 0 -> 0xE5
4  414c4f4e47447e31545854  clus 4      size 24            byte 0 -> 0xE5
7  4849474846494c45545854  clus 67591  size 20            byte 0 -> 0xE5
                           hi 1  lo 2055
```

Of the 30 bytes: five are the first byte of a directory entry, twenty-four
are three FAT entries in **each of the two FATs**, and one is in the FSInfo
sector.

**Destroyed.** The first byte of every entry in the set, short and long
alike. For a long-name entry that byte is `LDIR_Ord`, so the ordinal and the
last-entry flag are destroyed for every component. The cluster chain of each
deleted file, zeroed in FAT 0 and in FAT 1 identically.

**Not destroyed.** `DIR_FstClusHI` and `DIR_FstClusLO`. `HIGHFILE.TXT`
retained `hi = 1, lo = 2055` after deletion, so its first cluster of 67,591
is still readable in full. Name bytes 1 to 10, `DIR_Attr`, `DIR_NTRes`, all
timestamps, `DIR_FileSize`, and the long-name checksum byte, which read
`0x32` before and after on both long-name entries.

**No data cluster was touched.** Not one of the 30 differing bytes lies in
the data region outside the root directory. File content survives deletion
in full.

**3. The first byte of a short name is recoverable, uniquely.**

The long-name checksum is computed over the eleven-byte short name. Ten
bytes survive. The map from the first byte to the checksum is a composition
of bijections — each round is a rotation followed by an addition modulo 256,
both bijective, and the first byte enters as the initial value — so exactly
one of the 256 candidates reproduces any given checksum.

Measured against the deleted entry:

```text
surviving name bytes: b'LONGD~1TXT'
target checksum:      0x32
candidates:           ['0x41']  ->  'A'
```

One candidate, and it is the correct one. The original name was
`ALONGD~1.TXT`.

Two candidate values can never be correct, which makes the recovery
self-checking: `0x00` is the terminator and `0xE5` is never stored
literally, being escaped to `0x05`. A recovery producing either proves the
long-name set does not belong to that short entry. A recovery producing
`0x05` means the real first character is `0xE5`.

**4. A deleted long-name entry reads as a valid last entry.**

`0xE5` is `1110 0101`, and bit 6 is the last-entry flag. After deletion,
`LDIR_Ord & 0x40` is therefore non-zero on **every** component of the set,
and `LDIR_Ord & ~0x40` is `0xA5`, or 165. Both values are artefacts of
deletion and neither is evidence of anything. The order of a multi-entry
long name must be reconstructed from position, never read from the ordinal.

**5. The first character of the long name survives in the entry adjacent to
the short entry, not in the one flagged last.**

Long-name entries are stored in reverse. The entry carrying
`LAST_LONG_ENTRY` holds the final fragment and sits at the lowest offset;
the ordinal-1 entry holds characters 1 to 13 and sits immediately before the
short entry. Measured on `a long deleted name.txt`:

```text
slot 2  ord 0x42 LAST  name1 = 640020006e0061006d00   "d nam"
slot 3  ord 0x01       name1 = 610020006c006f006e00   "a lon"
slot 4  ALONGD~1TXT
```

Deletion overwrites byte 0 only, so `LDIR_Name1` at bytes 1 to 10 is intact
in both. The first character of the name is in slot 3, the entry the
specification numbers 1 and does not flag as last.

**6. `mrd` and `mdeltree` are byte-identical in effect.**

Both changed the same 19 bytes: the first byte of the directory's entry in
the root, the cluster chains of the directory and of the file it contained
in both FATs, one byte in the FSInfo sector, and the first byte of the
contained file's entry inside the directory's own cluster.

The directory's cluster is otherwise untouched. Bytes 0 to 63 of cluster 5,
which hold the `.` and `..` entries, are unchanged. A deleted subdirectory
is therefore identifiable from two independent places: its entry in the
parent, and its own cluster's dot entries.

**7. Deletion updates the FSInfo sector.**

Not predicted. `BPB_FSInfo` names sector 1, and `FSI_Free_Count` at offset
488 within it moved by exactly the number of clusters freed:

```text
                       FSI_Free_Count   FSI_Nxt_Free
baseline                       61,432         67,591
after mdel of three files      61,435         67,591
after removing SUBDIR          61,434         67,591
```

`FSI_Nxt_Free` did not move in either case.

This is a volume-level record that deletion occurred, independent of the
directory entries, and it is not mentioned in any of the project's prior
research. It is a count of free clusters and not a count of deletions, so it
establishes that clusters were freed rather than how or when.

### Conclusion

`mdel`, `mrd` and `mdeltree` are deterministic under the exports
`scripts/generate-fixtures.sh` already sets. Deleted-entry fixtures may
therefore be added to the fixture set without weakening the property
EXP-0001 established, and `MANIFEST.sha256` remains a golden result.

Deletion by `mtools` destroys the first byte of each entry and the cluster
chain, and nothing else. Every field M6 needs in order to identify a deleted
entry survives: the attribute that distinguishes a long-name component from
a short entry, the checksum that re-associates a set with its short entry,
the ten name bytes that make the destroyed byte recoverable, and both words
of the first cluster.

The high word of the first cluster is **not** zeroed. This is the finding
that constrains the fixture set. A fixture built with `mtools` cannot
exercise the case where a deleted file above cluster 65,535 has lost its
start location, because `mtools` does not produce that case. Reaching it
requires poking an image under ADR-0006 §5.1, and until that fixture exists
the case is untested.

Any first character recovered by the checksum method is `RECONSTRUCTED`
under ADR-0003 §3 and never `VERIFIED`. The arithmetic is exact, but it
rests on two assumptions the evidence does not establish: that the
long-name set belongs to the short entry that follows it, and that the ten
surviving name bytes are the original ones. The uniqueness of the result
must not be reported as certainty about the name.

### Limitations

1. Measured on one host with `mtools` 4.0.43-1build1 and `mkfs.fat` 4.2.
   Whether other versions delete identically is not established. EXP-0002
   Limitation 2 applies unchanged.
2. **What `mtools` destroys is not what every implementation destroys.**
   This experiment measures the tool that builds the fixtures. It does not
   measure Windows, and it cannot: the high-word question is a question
   about Microsoft implementations, and no `mtools` measurement can answer
   it. What is established is what a fixture built here can and cannot
   prove.
3. One cluster size only, 512 bytes at one sector per cluster. A larger
   cluster changes how many entries a directory cluster holds and therefore
   where residue can appear, but not what deletion writes.
4. Deletion from the root directory and from one subdirectory. Deletion
   from a deeply nested directory was not measured.
5. `SUBDIR` was emptied before `mrd`. Whether `mrd` refuses a non-empty
   directory was not tested, because `mdeltree` covers that case and
   produced an identical result.

### Next action

1. Close the `mdel` determinism entry in `KNOWN_ISSUES.md` and mark EXP-0002
   Next action 4 done.
2. Record as a known issue that no fixture can exercise a deleted file whose
   first-cluster high word has been zeroed, and that the case is therefore
   untested against evidence a formatting tool produced.
3. Decide the M6 design against these measurements and record it.
4. Build the M6 fixtures, and confirm the fixture count byte-identical
   across runs under `DEVELOPMENT_ENVIRONMENT.md` §12.
