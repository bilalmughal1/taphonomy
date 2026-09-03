# ADR-0008: M5 Implementation Decisions

* **Status:** Accepted
* **Date:** 2026-09-03
* **Decision owners:** Taphonomy project
* **Scope:** The receiver of the evidence-access trait; which file allocation
  table the cluster chain walk reads; treatment of the `0x00` directory
  terminator; error architecture for directory parsing; the module that holds
  M5; fixture coverage required before M5 may claim a tested chain walk
* **Amends:** ADR-0007 §2D and §6 (module name only)
* **Completes:** ADR-0007 §2A (receiver), §4.2 (cycle detection), §5.4
* **Related:** ADR-0002 §8 (M5, M6); ADR-0003 §3.1; ADR-0006 §5.1;
  `CLAUDE.md` §9, §12, §15, §34, §38, §41; `SECURITY.md` §15, §16;
  `PROJECT.md` §5; `DEVELOPMENT_ENVIRONMENT.md` §12

---

## 1. Context

ADR-0007 is the M5 design. It decided that evidence reaches parsers through a
project-defined trait with one positional method, bounded directory
enumeration at 65,536 entries, retained long-filename entries without
decoding them, and placed M5 in a new module.

It left six things unstated or open. Two were named in its own text as
deliberately deferred to implementation. Four became visible only when the
codebase was audited against the design.

This ADR records all six, together with the evidence that decided them. It
does not revisit ADR-0007's four decisions, which stand.

### 1.1 Basis

Every claim below was measured against the working tree at `7e00155`, clean,
on 2026-09-03.

```text
git log --oneline -1        7e00155
git status --short          (no output)
cargo test --workspace      59 + 9 + 7 + 5 + 9 + 6 = 95 passing
./scripts/verify-fixtures.sh
                            Deterministic: 13/13 fixtures byte-identical
grep -rn "trait" src/       (no output)
```

Documentation was verified against the repository before being quoted:

```text
find . -name '*.md' -not -path './.git/*' -exec sha256sum {} + | ...
                            20 of 20 digests matched
```

The fixture `fat32-root-entries.img` was decoded rather than assumed, with
its geometry read from the BIOS parameter block rather than from the script
that generated it:

```text
BPS=512 SPC=1 RSV=32 NF=2 FATSz32=993 RootClus=2
RootDirOffset=2081792  FATOffset=1064960
```

Cross-check: 129,024 − 32 − (2 × 993) = 127,006 clusters, which is
`FIXTURE_CLUSTER_COUNT` in `tests/fat32_fixtures.rs`.

Microsoft's *FAT32 File System Specification*, version 1.03 (6 December
2000), was read in full for §8. Prior sessions had relied on secondary
accounts of it.

---

## 2. Decisions

**A. The trait method takes `&mut self`.** `src/evidence.rs` is not modified.

**B. The chain walk reads `boot.active_fat().unwrap_or(0)`** — FAT 0 when
mirroring is enabled, the declared active FAT when it is not.

**C. Enumeration scans the whole cluster.** The allocated-entry listing ends
at the first `0x00` first byte; non-zero content beyond it is reported as an
observation and parsing succeeds.

**D. Entry classification is total and infallible; only the chain walk can
fail.** `src/error.rs` is not modified.

**E. M5 lives in `src/fat_directory.rs`,** not `src/directory.rs`.

**F. A multi-cluster root directory fixture is added before the parser is
written.**

---

## 3. Decision A: the trait method takes `&mut self`

The signature is `read_exact_at(&mut self, offset: u64, buf: &mut [u8]) ->
Result<(), Error>`, identical to the inherent method `EvidenceFile` already
has, which becomes the implementation body unchanged.

ADR-0007 §2A requires only that evidence access cross a project-defined trait
with one positional method mirroring `EvidenceFile::read_exact_at`. It does
not prescribe the receiver.

### 3.1 The alternative

`&self`, backed by `std::os::unix::fs::FileExt::read_exact_at`, which is
`pread64`: the offset is independent of the current cursor, the cursor is not
affected, and it is one syscall rather than a seek followed by a read. It
also handles `ErrorKind::Interrupted` internally and returns
`UnexpectedEof` on a short file, which `Error::from_io` already classifies
identically.

This is a better primitive. It was recommended during this session and then
withdrawn on the evidence below. See §11.

### 3.2 It would make the library platform-specific for the first time

Measured:

```text
grep -rn "os::unix\|os::windows\|FileExt\|cfg(unix\|cfg(target" src/ tests/
  tests/read_only.rs:11:#![cfg(unix)]
  tests/read_only.rs:15:use std::os::unix::fs::PermissionsExt;
```

Two hits, both in tests, none in `src/`. `tests/read_only.rs` is not a
precedent for platform-specific library code. Its own header reserves the
question:

> These tests are Unix-specific because they rely on file permission bits.
> Windows behaviour must be established separately before any Windows
> support is claimed.

The platforms are not symmetric either. `std::os::windows::fs::FileExt`
offers `seek_read`, which does move the cursor and is a short-read API with
no `read_exact_at` equivalent. "Positional and cursor-free" is a Unix
property, not a portable one, so adopting it would silently narrow what
`CLAUDE.md` §38 requires be documented and tested separately.

### 3.3 It would widen the write surface in the module that must not have one

Importing `FileExt` brings `write_at` and `write_all_at` into scope on the
inner `File`. Both would fail at the kernel, because the handle is opened
without write access, but no test establishes that.

`tests/read_only.rs` proves that a write **handle** is refused at mode
`0o444` and that `EvidenceFile::open` still succeeds. Its control assertion
confirms the fixture genuinely rejects a write handle, so the pass cannot
come from a permission change that did nothing. Nothing in it asserts that a
write **method** on the inner handle fails.

`EvidenceFile`'s doc comment states there is no method on this type that
writes. That remains true of the public type while becoming weaker inside the
module, with no test detecting the difference. `SAFETY.md` §4 and §5 reduce
at this layer to one property, and widening it for an unmeasured performance
gain inverts the project's priority order.

### 3.4 The objection that motivated the change was misapplied

ADR-0007 §3.1 rejects `Read + Seek` because it is stateful, so a caller could
leave the cursor anywhere between calls.

That is an objection to a **trait's public contract**. A one-method
positional trait exposes no cursor: a caller cannot observe one, move one, or
leave one anywhere. Whether the implementation seeks internally is invisible
across the boundary. §3.1's argument therefore does not reach the
implementation, and citing it against seek-then-read was an error of reading,
not a difference of judgement.

### 3.5 The performance argument is unavailable

One syscall rather than two is the remaining case for `pread`. `CLAUDE.md`
§12 places correctness before optimisation, and §41 requires that performance
be measured rather than guessed. Nothing was measured. The argument may be
made later, with numbers.

### 3.6 What `&mut self` costs

Exactly one thing: two readers over one evidence file at the same time. No
milestone in ADR-0002 §8 requires it.

---

## 4. Decision B: the chain walk reads the active FAT

### 4.1 The codebase already decided this

The alternative considered was reading FAT 0 unconditionally and recording
the limitation. It is prohibited by a statement already in `src/fat32.rs`.
The doc comment on `Fat32Observation::FatMirroringDisabled` reads:

> Only one FAT is maintained; the others are stale.
>
> Any later read of allocation data must use the active FAT. Reading
> FAT 0 by default would return unmaintained data.

That is a normative instruction to M5, written during M4. It was found only
after the recommendation to ignore it had been made. See §11.

### 4.2 The machinery is already present and already validated

At `HEAD`, inside `parse_boot_sector`:

```rust
let ext_flags = le_u16(sector, OFF_EXT_FLAGS);
if ext_flags & EXT_FLAGS_MIRRORING_DISABLED != 0 {
    let active = (ext_flags & EXT_FLAGS_ACTIVE_FAT) as u8;
    if active >= geometry.fat_count {
        return Err(Fat32Error::InvalidField {
            field: "active_fat",
            value: active as u64,
        });
    }
    observations.push(Fat32Observation::FatMirroringDisabled { active_fat: active });
}
```

Two properties follow, and the walk depends on both.

An index at or beyond the declared FAT count is refused before a
`Fat32BootSector` exists. A FAT base offset computed from the index therefore
cannot land outside the reserved-plus-FAT region.

`active_fat()` returns `None` when mirroring is enabled, so the low nibble is
never consulted on a volume where the specification says it carries no
meaning.

### 4.3 The constants are right

`EXT_FLAGS_MIRRORING_DISABLED` is `0x0080` and `EXT_FLAGS_ACTIVE_FAT` is
`0x000F`. The specification defines `BPB_ExtFlags` bits 0–3 as the zero-based
active FAT number, valid only when mirroring is disabled, bits 4–6 reserved,
bit 7 as the mirroring flag, and bits 8–15 reserved.

Microsoft separately documents the same flag as bit 8 of the in-memory Drive
Parameter Block, a different structure. Conflating the two is a known source
of error. This code does not make it.

### 4.4 A counter-example worth recording

The `defrag` project computes its FAT start by multiplying the low nibble of
`BPB_ExtFlags` by the FAT size **unconditionally**, without first testing
bit 7. On a conformant mirrored volume carrying a non-zero low nibble that
reads the wrong table. The structure of `active_fat()` makes the error
unrepresentable here, which is worth stating because the protection is
structural rather than a check that could be forgotten.

### 4.5 Effect on `KNOWN_ISSUES.md`

The entry "No fixture exercises FAT mirroring" states that what is untested
is any code that acts on the index. M5 is the first code that acts on it.

No fixture can reach that path: `mkfs.vfat` offers no option to set the
field, and a discriminating fixture would have to poke `ExtFlags` **and** make
the FATs differ. The in-memory implementation of the trait can reach it. A
synthetic volume carrying `ExtFlags = 0x0081` with a divergent chain in FAT 1
gives unit coverage of a path no fixture can produce.

The known issue narrows from "no code acts on the index" to "no fixture
exercises it; unit coverage exists." It does not close.

### 4.6 Not in scope, recorded for later

The forensically stronger behaviour is to compare the FAT copies and report
divergence rather than silently reading one. An independent Rust forensic FAT
reader, `fat-core` (Apache-2.0), does this, emitting a FAT-mirror-mismatch
anomaly naming the entry and both values, phrased as consistent with a
post-hoc edit rather than as a verdict.

That is the same observation-not-verdict discipline `ARCHITECTURE.md` §36 and
`PROJECT.md` §5 require, reached independently. Consulted as a research
reference under `CLAUDE.md` §8. No code was read for reuse and none was
copied.

---

## 5. Decision C: `0x00` ends the listing, the cluster is scanned in full

### 5.1 The fixture cannot decide this, and that is a finding

ADR-0007 left open whether enumeration stops at the first `0x00` first-byte
marker or scans the whole cluster, to be settled against
`fat32-root-entries.img` rather than assumed.

Decoded at offset 2,081,792, the root directory is:

```text
0  TAPHFIX        attr 0x08   volume label
1  HELLO   TXT    attr 0x20   pure 8.3,        FstClus 3, size 24
2  README  MD     attr 0x20   NTRes 0x18,      FstClus 4, size 25
3  ord 0x42       attr 0x0F   long name, last, cksum 0xD8
4  ord 0x01       attr 0x0F   long name,       cksum 0xD8
5  ANNUAL~1TXT    attr 0x20   short entry,     FstClus 5, size 28
6  LOGS           attr 0x10   NTRes 0x08,      FstClus 6, size 0
7  0x00           terminator
8-15             zero
```

Six entry shapes, not the five ADR-0007 §6 of the day-5 handover lists:
`LOGS` carries `NTRes = 0x08`, so a lowercased **directory** name is covered
as well as a lowercased file name.

Entry 7 is a terminator followed by 448 zero bytes. That is what a conformant
volume looks like. It cannot distinguish the two candidate behaviours,
because they differ only on volumes that are not conformant. Measuring the
fixture was still correct — it produced §5.4 and §8 — but it could not have
answered the question it was expected to answer.

### 5.2 The specification permits stopping

A first byte of `0xE5` marks a free entry. A first byte of `0x00` marks a
free entry and additionally states that no allocated entries follow, every
subsequent first byte being zero as well, so a driver need not examine the
rest of the directory.

### 5.3 Practice disagrees, and there is documented hardware

A 2002 report to the Linux kernel list describes Psion EPOC devices that
write a `0x00` marker without clearing what follows, so a reader scanning
past it recovers old or random entries — in one case from a sector previously
holding file data, producing implausible filenames. The Linux FAT
maintainer's position in the same discussion is that stopping at a zero first
byte is an optimisation rather than a semantic requirement. Linux added the
stop only in 2018, specifically to avoid displaying garbage.

### 5.4 Why the forensic answer differs from the driver answer

For a filesystem driver, showing nothing past the terminator is right. For a
forensic instrument it is backwards. Residue past a terminator is evidence,
there is documented hardware that leaves it, and M6 will need exactly that
region.

The chosen behaviour is the pattern the codebase already applies four times:
parse succeeds, the finding is reported. It is the same shape as
`Fat32Observation::HiddenSectorsDisagree` and as the partition anomalies. It
satisfies `PROJECT.md` §5 without discarding evidence, and it does not
present a partial listing as complete, which ADR-0007 §4.1 forbids.

Directory observations get their own enum in `fat_directory.rs` rather than
extending `Fat32Observation`, for the same reason `filesystem.rs` and
`fat32.rs` each have one: the finding belongs to the structure being parsed.

### 5.5 A fixture is required and does not exist

A valid volume carrying a `0x00` entry followed by non-zero bytes. That is
deliberate corruption of an image `mkfs.vfat` produced, which ADR-0006 §5.1
permits as its narrow exception: the surrounding structure still comes from
an independent implementation, so the fixture cannot encode this project's
reading of the specification.

---

## 6. Decision D: classification is total; only the walk can fail

### 6.1 The shape

```text
classify(entry: &[u8; 32]) -> EntryKind        pure, infallible, PartialEq
enumerate_root(...) -> Result<RootDirectory, DirectoryError>
```

Classification is total because every possible 32 bytes is some kind of
entry: terminator, deleted, long-name, volume label, short name, or the
invalid combination §8.2 describes. There is nothing for it to reject, so it
has no error type.

### 6.2 The fixed-size array is a safety requirement

Measured. `le_u16` and `le_u32` in `src/filesystem.rs` are `pub(crate)`,
carry no doc comment of their own, and index the slice directly:

```rust
pub(crate) fn le_u16(sector: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([sector[offset], sector[offset + 1]])
}
```

They panic on a short slice. There is no bounds check inside them and no
default return. Every existing caller reads from a 512-byte sector whose
length has already been checked, in `identify`, at `if sector.len() <
VBR_SIZE`. The obligation is discharged by the caller, and it is discharged
once for all callers.

A directory parser reads at sixteen offsets from a cluster buffer.
`CLAUDE.md` §15 requires that normal malformed evidence not cause
uncontrolled crashes, and `SECURITY.md` §15 requires that invalid ranges
produce controlled errors rather than undefined behaviour.

With a `&[u8; 32]` parameter and offsets that are compile-time constants
below 32, the compiler discharges the obligation and no panic is
representable. This is preferable to sixteen explicit checks, which would be
correct only until someone adds a seventeenth field.

### 6.3 The error split follows a boundary the codebase already draws

Measured. `src/error.rs` derives `Debug` only, because `Error::Io` holds an
`io::Error`. Every other public type in `src/` derives `PartialEq, Eq` —
sixteen of them across `fat.rs`, `fat32.rs`, `filesystem.rs`, `partition.rs`
and `hash.rs`.

Pure parsing is comparable. I/O is not. That is not a new rule; it is the
existing one, stated.

The boundary is load-bearing for the integration tests, which assert on
errors by value:

```rust
assert_eq!(
    parse_fixture("fat32-oversized-volume.img"),
    Err(Fat32Error::VolumeExceedsExtent { .. }),
);
```

A walk error that can wrap `crate::Error` cannot support that. Unit tests in
`src/fat32.rs` already use the other idiom — `assert!(matches!(...,
Err(...)))` — so both exist in the codebase and each will be used where it
fits: `assert_eq!` against `EntryKind`, `matches!` against `DirectoryError`.

---

## 7. Decision E: the module is `src/fat_directory.rs`

**This amends ADR-0007 §2D and §6, which named `src/directory.rs`. The name
changes; nothing else does.**

§6's reasoning stands in full. `src/fat32.rs` is 727 lines and the largest
file in `src/`, `CLAUDE.md` §34 lists giant files under what to avoid, and
directory entries are a third concern beside shared FAT structure and
FAT32-only boot sector fields.

§6 justifies the generic name by observing that the 32-byte entry format is
shared across FAT12, FAT16 and FAT32. That is true of the entry format and
false of the rest of the module. Locating a FAT12 or FAT16 root directory
does not involve a cluster chain at all — it is a fixed-size region computed
from `BPB_RootEntCnt`, which the specification requires to be zero on FAT32.
Chain walking is not shared even within the FAT family.

The convention the codebase already follows, confirmed by the module
dependency graph: generic names for filesystem-agnostic modules (`evidence`,
`hash`, `partition`, `filesystem`), FAT-prefixed names for FAT-specific ones
(`fat`, `fat32`). Dependencies run `filesystem` ↔ `fat`, with `fat32`
depending on both, and `partition` standalone.

Day 5 already paid for a name that did not match its contents: a module named
`fat32.rs` containing FAT12 and FAT16 logic, later renamed `fat.rs`. This is
the same mistake in the opposite direction, caught before it is made.

The module doc must state the boundary explicitly: which parts are shared
across FAT variants and which are FAT32-only.

Tests in the new module build on `crate::fat::tests::fat32_sector`, the
`pub(crate)` builder already shared into `fat32.rs`, rather than introducing
a third boot-sector builder.

---

## 8. Decision F, and the constraints the specification imposes

### 8.1 The fixture set contains no chain-walk coverage

The existing fixture's root directory occupies one cluster: 512 bytes per
cluster at one sector per cluster, sixteen 32-byte slots, eight used and the
ninth a terminator. The first eight FAT entries read:

```text
FAT[0] 0x0FFFFFF8   media descriptor 0xF8 in the low byte
FAT[1] 0x0FFFFFFF
FAT[2] 0x0FFFFFF8   root directory, end of chain on the first link
FAT[3] 0x0FFFFFFF   HELLO.TXT
FAT[4] 0x0FFFFFFF   readme.md
FAT[5] 0x0FFFFFFF   ANNUAL~1.TXT
FAT[6] 0x0FFFFFFF   LOGS
FAT[7] 0x00000000   free
```

The entire justification for ADR-0007 §2A is that M5 reads at offsets not
known until the previous read completes. Against this fixture that code path
never executes. M5 would ship its central mechanism with zero fixture
coverage: the fixture illustrates chain walking without proving it.

`mkfs.vfat` also terminates the root directory with `0x0FFFFFF8` and every
file with `0x0FFFFFFF`, so both end-of-chain encodings are present in one
image and a single test proves the comparison is a range test rather than an
equality test.

### 8.2 What the specification requires of the parser

Read from Microsoft's FAT32 File System Specification version 1.03 rather
than from secondary accounts. Prior sessions relied on the latter.

**Long-name detection.** The long-name attribute is the combination of
read-only, hidden, system and volume-id, which is `0x0F`. The mask for
testing it additionally includes directory and archive, giving `0x3F`. The
specification's stated algorithm requires **both** that the attribute masked
with `0x3F` equals `0x0F` **and** that the first byte is not `0xE5`. A
bitmask test against the volume-id bit alone matches every long-name entry,
because `0x0F` contains `0x08`. The fixture contains both a volume label and
a long-name set, so the trap is live on the first test.

**Short-entry classification.** Mask the attribute with directory and
volume-id together: zero is a file, the directory bit alone is a directory,
the volume-id bit alone is a volume label, and both set is explicitly an
**invalid** directory entry. That is a fourth classification, not an edge
case to be folded into one of the other three.

**`0x05` is not a deleted entry.** A first byte of `0x05` means the real
first character is `0xE5`, encoded that way because `0xE5` is a valid lead
byte in the Japanese character set. This does not affect M5, which does not
interpret deletion, and it is a trap for M6, recorded here so it is not
discovered there.

**FAT entry semantics.** A FAT32 entry is 28 bits; the high four are
reserved and must be masked off on read with `0x0FFFFFFF`. End of chain is
any value at or above `0x0FFFFFF8`, not equality with `0x0FFFFFFF`. Bad
cluster is `0x0FFFFFF7`. `FAT[0]` holds the media descriptor in its low eight
bits with the remaining bits set, which is why the fixture reads `f8ff ff0f`.

**Validate the cluster before computing an offset from it.** The
specification warns that `BPB_FATSz32` may be larger than required, that the
FAT ends at the entry for the cluster count plus one rather than at the end
of the last FAT sector, and that nothing may be assumed about the bytes
beyond it. A cluster number must therefore be range-checked against the
cluster count **before** a FAT offset is computed from it, not after the read.

**Reserved fields are not defects.** The specification instructs utilities
not to treat non-zero reserved fields as bad and not to zero them. This
confirms the existing treatment of `BPB_Reserved` in `parse_boot_sector`.

**The root directory has no dot entries** on any FAT type, and it is the only
directory in which an entry carrying the volume-id attribute alone is valid.

### 8.3 The fixture

`fat32-root-multicluster.img`, a new generator function in
`scripts/generate-fixtures.sh`, built by the same procedure as
`fat32-root-entries.img`.

Twenty files with pure uppercase 8.3 names produce exactly one entry each and
no long-name entries, so the entry count is predictable rather than emergent:
one volume label plus twenty short entries is 21, against sixteen slots per
cluster.

**Each file must carry content.** An empty file records a first cluster of
zero and consumes no cluster, so the root directory's second cluster would be
allocated adjacently and a defective walk that ignored the FAT entirely,
simply reading the next cluster, would pass. With content, files consume
clusters before the root needs to grow, and the root's chain is
non-contiguous. That difference is the entire value of the fixture.

The change is confined. `verify-fixtures.sh` names no fixture and globs
`"$WORK/run1"/*.img`, so it requires no edit; measured by reading it rather
than assumed. The manifest is regenerated wholesale by `sha256sum ./*.img`,
so the correct verification is that exactly one line is added and the other
thirteen digests are unchanged. `verify-fixtures.sh` must then report 14 of
14, and `EXPERIMENTS.md` EXP-0001 gains a fourth re-measurement under
`DEVELOPMENT_ENVIRONMENT.md` §12.

### 8.4 Arithmetic

The cluster-to-offset conversion is the first code in `src/` to perform it;
measured, nothing existing reads a FAT entry, follows a chain, or maps a
cluster number to an address. It follows the house idiom, which is: widen
disk-derived fields with `as u64` before combining them, use `checked_add`
and `checked_sub` into a domain error where untrusted values could overflow
or underflow, and use `saturating_*` only for intermediates that a subsequent
`checked_*` bounds. `SECURITY.md` §15 requires overflow-safe operations and
controlled errors on invalid ranges.

---

## 9. Consequences

### Positive

* The evidence layer is unchanged, so M5 adds no risk to the project's
  central safety property
* The chain walk honours a constraint the codebase had already recorded
* Residue past a terminator becomes visible rather than being discarded,
  and M6 inherits the region it needs
* Entry classification cannot panic, by construction rather than by
  discipline
* The module name states what the module contains
* The chain walk will have fixture coverage on the day it is written
* Four specification constraints are recorded before the code that must
  satisfy them exists

### Negative

* `&mut self` forecloses concurrent readers over one evidence file, and
  forgoes a syscall reduction that may prove real when measured
* Scanning the full cluster reads bytes a conformant volume does not need
  read, and adds an observation category that will almost always be empty
* Two fixtures are required before M5 is complete, one of which does not
  yet exist even in outline
* `KNOWN_ISSUES.md` item 1 narrows but does not close
* The invalid-attribute classification has no fixture and will be covered
  only by unit tests

---

## 10. Review trigger

Revisit if:

* a milestone requires concurrent reads over one evidence file, or
  profiling actually performed under `CLAUDE.md` §41 shows syscall count
  dominating
* a fixture exercising FAT mirroring becomes constructible, which would
  allow FAT-copy comparison to be implemented and tested
* long-name decoding is implemented, which changes ADR-0007 §5's treatment
  and brings the patent question in §11.4 into scope
* a second filesystem needs directory enumeration, at which point the
  split between shared entry format and FAT-specific chain walking is
  tested for the first time

---

## 11. Errors in the preparation of this decision

Recorded because the working method requires that a reversal be stated
rather than quietly absorbed, and because the failures share one cause.

### 11.1 Two recommendations were made and withdrawn

Both before any code was written, and both withdrawn on evidence that the
audit should have preceded them.

**The `&self`/`pread` change (§3).** Recommended after reading
`src/evidence.rs` but before measuring platform-specific code in `src/`,
before reading `tests/read_only.rs`, and while misapplying ADR-0007 §3.1's
objection to the implementation rather than to the interface. Three
independent reasons against it, all discoverable beforehand.

**Reading FAT 0 unconditionally (§4).** Recommended after reading the public
API surface of `src/fat32.rs` but before reading its doc comments, one of
which prohibits exactly that. The recommendation was made from a partial read
of the file that contained the answer.

### 11.2 Two audit briefs contained false premises

An audit brief asserted that "the 59 unit tests" build boot sectors by hand
in `src/fat32.rs`. Measured: the 59 are spread across five files, of which
`fat32.rs` holds 18. ADR-0007 §3.4's reference to the other 59 unit tests is
accurate; the paraphrase of it was not.

A second brief asked for the two most recent `CHANGELOG.md` entries.
`CHANGELOG.md` has one `## Unreleased` heading with `### Added` and
`### Changed` beneath it.

Both were corrected by the agent executing the brief rather than answered
around, which is the behaviour the brief format asks for.

### 11.3 A verification step was proposed that could not be performed

An instruction was given to append a drafted fragment directly from the
Windows download directory into the target document, accompanied by a SHA-256
digest of the fragment. The digest could never be checked, because the
fragment would never exist in the Linux filesystem as a file. The
verification was decorative.

The correct sequence — land the artefact, verify it in isolation, use it,
verify the result, read the diff, commit, then push or hold — is not recorded
anywhere in the project's documentation. It should be.

### 11.4 One decision was drafted against the wrong instrument

These six decisions were first drafted as an appendix to ADR-0007. Measured
afterwards: the project has two appendices, at `ADR-0002:255` and
`ADR-0007:295`, both titled "Correction to §X" and both correcting a factual
error. Amending a decision is done with a new ADR carrying a header field, as
ADR-0002 does against ADR-0001 and ADR-0003 does against `SAFETY.md` and
`PROJECT.md`.

An appendix corrects a fact. A new ADR makes or amends a decision. The draft
mixed six decisions, one amendment and two corrections into an appendix. This
ADR is the corrected form; the two factual corrections went to ADR-0007
Appendix B, where they belong.

### 11.5 Cause

Every failure in §11.1 has the same cause as the three corrected briefs
recorded in the day-5 handover: writing from a partial reading of a file
rather than from the file. §11.2 and §11.4 have a second cause: asserting a
property of the project's own records without measuring it first.

ADR-0002 Appendix A §A.5 requires that an ADR state the command used to
obtain any environmental claim. ADR-0007 Appendix A adds that a measurement
is valid at an instant and must be re-run rather than quoted. Neither
requirement covers a claim about what a file *says*, as opposed to what the
environment reports. The working method's requirement to quote project
documents verbatim rather than paraphrase from recollection covers it, and it
was not followed.

---

## 12. Open after this decision

1. A fixture carrying non-zero content past a `0x00` terminator (§5.5).
2. `mdel` determinism, unmeasured, required before M6 introduces
   deleted-entry fixtures. `EXPERIMENTS.md` EXP-0002 next action 4.
3. VFAT long-filename patent status. ADR-0007 §5.5 and `KNOWN_ISSUES.md`
   record this as unresearched. The specification read for §8 is
   distributed under a Microsoft agreement containing a limited copyright
   licence and a covenant not to sue over necessary patent claims, with the
   grant restricted to stated purposes. That agreement is a concrete
   document bearing on the question. No view is offered here on whether it
   reaches this project; that is a legal question and requires a
   qualified answer.
4. A fixture exercising FAT mirroring, which would require poking
   `BPB_ExtFlags` and additionally making the FATs differ
   (`KNOWN_ISSUES.md`).
5. Recording the artefact handling sequence described in §11.3.
