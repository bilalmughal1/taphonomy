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
