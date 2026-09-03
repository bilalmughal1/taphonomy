# ADR-0007: Evidence Access for Multi-Sector Parsing

* **Status:** Accepted
* **Date:** 2026-08-30
* **Decision owners:** Taphonomy project
* **Scope:** How filesystem parsers reach evidence beyond a single sector;
  first trait in the codebase; bounds on directory enumeration; treatment of
  long-filename entries
* **Related:** ADR-0002 §8 (M5, M6); ADR-0003 §3.1; `ARCHITECTURE.md` §16;
  `SECURITY.md` §15, §16; `PROJECT.md` §5; `CLAUDE.md` §9, §34

---

## 1. Context

Every parser written so far is a pure function over one 512-byte buffer.
`parse_mbr`, `identify` and `parse_boot_sector` all take `&[u8]` and perform
no I/O. The caller reads the sector; the parser interprets it.

Milestone M5, "Enumerate the root directory", breaks that shape. The root
directory begins at the cluster `BPB_RootClus` names, and its extent is a
chain in the file allocation table. Following that chain means reading at
offsets that are not known until the previous read completes.

The audit confirming the current state, run against `HEAD` before this
decision:

* No `trait` is defined anywhere in `src/`.
* `EvidenceFile::digest` is the only operation in `src/` that reads more
  than one sector in a single call.
* All seven call sites of `read_exact_at` read exactly one fixed-size buffer.
* `src/hash.rs` declares `hash_reader<R: Read>`, a function generic over a
  reader trait.
* `tests/nist_vectors.rs` defines `DripFeed`, an in-memory implementation of
  `std::io::Read` used as a test double for it.
* `src/fat32.rs` is 702 lines, the largest file in `src/`.

---

## 2. Decision

**A. Evidence access crosses a project-defined trait with one positional
method**, mirroring `EvidenceFile::read_exact_at`. `EvidenceFile` implements
it. An in-memory implementation backs unit tests.

**B. Directory enumeration is bounded at 65,536 entries.** A chain that
would exceed it is an error, not a truncated result.

**C. Long-filename entries are retained and counted, not decoded.**

**D. M5 lives in a new `src/directory.rs`.**

---

## 3. Why a Positional Trait

### 3.1 Alternatives considered

**Pass `&mut EvidenceFile` into the filesystem modules.** Simplest. Rejected
because every unit test then requires a file on disk, replacing 59 byte-array
tests with filesystem-dependent ones, and because it couples FAT parsing to
the evidence layer's concrete type for no gain.

**Keep parsers pure by reading the whole FAT into memory first.** This was
the project's first recommendation and it was wrong. See §3.3.

**Generic over `std::io::Read + Seek`.** Rejected. `hash_reader` uses
`Read` correctly, because hashing streams sequentially from offset zero.
Directory reading is random access by offset. `Read + Seek` is stateful, so
a caller could leave the cursor anywhere between calls, and `EvidenceFile`
would have to expose seeking that it does not currently expose. The access
pattern differs, so the trait differs.

### 3.2 Evidence for the positional shape

The Sleuth Kit's `tsk_img_read()` reads an arbitrary amount of data from an
arbitrary byte offset, and `tsk_fs_read()` takes a byte offset relative to
the start of the file system rather than a block address. Positional access
at both levels, with the filesystem layer working in filesystem-relative
offsets and the image layer in image-relative ones.

Taphonomy's split is the same: the trait works in evidence-relative offsets,
and FAT code converts from volume-relative using the `VolumeExtent` it
already holds. That is the reason `VolumeExtent` exists (ADR-0007 predates
no part of it; it was added for M4).

`ARCHITECTURE.md` §16 describes `read(offset, length)` and states that the
exact API will be determined during implementation. This is that
determination and does not contradict the document.

### 3.3 Why the pure-parser alternative was rejected

The original proposal was to keep every parser pure by having the caller read
the entire first FAT into memory, then interpret it with a pure function.
The stated justification was that a parser incapable of I/O cannot be driven
into a read loop by malformed evidence.

Two things were wrong with it.

**The bound fails.** A FAT32 volume may declare up to 268,435,445 clusters,
so a legal FAT reaches roughly 1 GiB and an explicit limit is required. A
16 MiB limit was proposed. Worked against Microsoft's documented default
cluster sizes, a 32 GB volume — the capacity ADR-0002 §3.4 names as the
project's motivating media — produces:

```text
16 KB clusters (Microsoft default, 16-32 GB)    8 MiB FAT
 8 KB clusters                                  16 MiB FAT
 4 KB clusters                                  32 MiB FAT
```

The proposed limit sat exactly on one plausible formatting of the target
media and failed another. No limit would have been both safe and comfortably
above it.

**The premise was unnecessary.** M5 does not need the allocation table. It
needs a handful of 4-byte entries within it. A root directory is commonly one
cluster; following its chain reads one entry per link. Loading 8 MiB to read
twelve bytes optimised for parser purity rather than for the problem.

The safety argument was also overstated. `SECURITY.md` §16 requires resource
limits, not structural impossibility. A chain walk with explicit cycle
detection and a cluster bound satisfies it.

Under the accepted decision, memory is bounded by the directory being read
rather than by the allocation table, which is a limit with a natural unit.

### 3.4 Cost of the first trait

This introduces the first `trait` in `src/`. `CLAUDE.md` §9 prohibits
abstractions without a demonstrated requirement.

The requirement is demonstrated by two implementations existing immediately:
`EvidenceFile`, and an in-memory double so chain-following stays unit-testable
against byte arrays like the other 59 unit tests. The project already runs
this exact pattern once — `hash_reader<R: Read>` with `DripFeed` — so what is
new is a project-defined trait, not the technique.

---

## 4. Why 65,536 Entries

Microsoft documents the maximum size of a FAT32 directory as 65,536 32-byte
entries, which is 2,097,152 bytes. A corroborating account reports a
practical limit of 21,844 files per directory, consistent with 65,532 entries
at three entries per file — one short entry and two long-name entries.

**Provenance, stated rather than glossed:** this limit appears in Microsoft's
support documentation, not in the on-disk format specification. The format
imposes no directory limit; a directory chain may in principle span the data
region. This is the limit Microsoft's implementations enforce, and adopting
it means the bound is documented rather than invented.

### 4.1 Exceeding the bound is an error

A chain that would exceed 65,536 entries fails rather than returning what fit.

`PROJECT.md` §5 forbids presenting guesses as verified recovery, and a
partial enumeration presented as a directory listing is that failure: the
caller cannot distinguish "these are the entries" from "these are the entries
we stopped at". ADR-0003 defines `PARTIAL` for genuinely partial results, and
if partial enumeration proves useful it becomes an explicit classification
rather than a silent truncation.

### 4.2 Cycle detection

A cluster chain may loop, whether through corruption or construction. The
walk maintains a visited set and terminates on revisiting a cluster. The set
is bounded by the same entry limit, so it cannot itself become the
exhaustion vector.

---

## 5. Why Long Filenames Are Retained But Not Decoded

A name that does not fit 8.3 is stored in preceding entries with attribute
`0x0F`, holding 13 UCS-2 characters each, ordinal-numbered, in reverse order,
protected by a checksum of the short name.

### 5.1 Three options

**Decode them in M5.** Rejected for M5. It roughly doubles the milestone:
UCS-2 handling, ordinal sequencing, checksum verification, and orphaned
fragments.

**Skip them silently.** Rejected. A directory listing that omits entries
without saying so is the false-positive class `PROJECT.md` §5 forbids, and no
precedent for silent omission exists anywhere in the codebase.

**Detect, retain, report, decline to interpret.** Accepted.

### 5.2 Precedent

The codebase already answers this shape of question four times:

| Structure | Treatment |
|---|---|
| GPT protective MBR | detected, reported unsupported, not parsed |
| exFAT and NTFS volumes | identified, reported unsupported, not parsed |
| FSInfo sector | located, bounds-checked, contents not read |
| Backup boot sector | located, bounds-checked, not read or compared |

This is the fifth instance of an established pattern.

### 5.3 The finding that nearly reversed this

Deletion overwrites the first byte of the short entry with `0xE5`,
destroying the first character of the name. The preceding long-name entries
survive, and their first byte is a **sequence number, not a name character**,
so the name characters remain intact. Forensic practice reconstructs the
short entry's destroyed first character by reading it from the long-name
entry.

**For a deleted long-named file, the long-name entries are the only
surviving source of the first character of the filename.** Long-name handling
is therefore load-bearing for recovery, not a display convenience.

That requirement lands at M6, which identifies deleted entries, not at M5,
which enumerates allocated ones. An allocated file's short name is intact and
truthful: `ANNUAL~1.TXT` is genuinely what the volume records.

The finding relocates the requirement rather than raising it, and argues that
long-name decoding deserves its own milestone with its own fixtures rather
than being folded into M5 as an afterthought.

### 5.4 Binding constraint on M5

**Enumeration must not discard `0x0F` entries.** They are retained in
on-disk order, classified, and counted, with their position relative to the
following short entry preserved. Filtering them would mean M6 has to
reintroduce them and the iterator gets rewritten.

Enumeration therefore yields entries in on-disk order, classified, rather
than a filtered list of files.

### 5.5 Open question before decoding

Microsoft held patents on the VFAT long-filename mechanism, and they were
litigated. Their current status has **not** been researched. Given that the
project selected Apache-2.0 partly for its patent grant, this must be checked
before long-name decoding is implemented. It does not affect M5, which counts
entries it does not interpret.

---

## 6. Why a New Module

`src/fat32.rs` is 702 lines, already the largest file in `src/`. Adding chain
walking and directory parsing would push it past a thousand. `CLAUDE.md` §34
lists "giant files" under what to avoid.

`src/directory.rs` also matches the boundary the code already has: `fat.rs`
holds structure shared across FAT variants, `fat32.rs` holds FAT32-only boot
sector fields, and directory entries are a third concern whose 32-byte format
is shared across FAT12, FAT16 and FAT32.

---

## 7. Consequences

### Positive

* M5 reads only the bytes it needs, rather than a table that may reach 1 GiB
* Memory is bounded in a natural unit, and by a documented limit
* Unit tests remain byte-array tests, consistent with the other 59
* The trait is the abstraction `ARCHITECTURE.md` §16 anticipated, arrived at
  when a caller existed rather than in advance
* M6 inherits a structure that already carries long-name entries

### Negative

* The first trait in the codebase, and a project-defined one
* Filesystem code can now perform I/O, so cycle detection and bounds become
  its responsibility rather than being structurally impossible
* Long names are visible but unreadable until a later milestone, so a
  directory listing shows `ANNUAL~1.TXT` where a user expects
  `annual report 2019.txt`
* The entry limit is documented by Microsoft but not by the on-disk format

---

## 8. Review Trigger

Revisit if:

* a second filesystem needs evidence access and the trait does not fit
* a directory legitimately exceeding 65,536 entries is encountered
* long-name decoding is implemented, which changes §5's treatment
* profiling shows per-read overhead dominates for large directories
* physical-device support introduces read semantics a file-backed
  implementation does not capture

---

## Appendix A: Correction to §1 (2026-08-31)

This appendix corrects two measurements in §1. The body above is left
unmodified, consistent with the treatment of superseded content
elsewhere in `docs/decisions/`.

### A.1 The errors

§1 states seven call sites of `read_exact_at`. Measured:

```text
grep -rn "read_exact_at" src/ tests/ | grep -v "pub fn read_exact_at"
  src/main.rs:57
  src/main.rs:113
  tests/filesystem_fixtures.rs:43
  tests/filesystem_fixtures.rs:58
  tests/partition_fixtures.rs:41
  tests/partition_fixtures.rs:191
  tests/fat32_fixtures.rs:50
  tests/fat32_fixtures.rs:76
```

Eight, not seven. The two in `tests/fat32_fixtures.rs` were added by
`47b622e`, before this ADR was written.

§1 states `src/fat32.rs` is 702 lines. Measured:

```text
wc -l src/fat32.rs
  727 src/fat32.rs
```

702 was correct when measured, before `e010bd0` added 25 lines. The ADR
was written after that commit using the earlier figure.

### A.2 Effect on the decision

**None.** Both figures support §6's argument that `src/fat32.rs` is the
largest file in `src/` and that M5 belongs in a separate module; 727
supports it more strongly than 702. The call-site count was cited as
evidence that every existing read is a single fixed-size buffer, which
remains true of all eight.

### A.3 Cause

The line count was carried across a commit that invalidated it. The
call-site count was transcribed from an audit without being re-derived.
Neither was checked against the working tree at the moment of writing.

`ADR-0002` Appendix A §A.5 already requires that future ADRs state the
command used to obtain any environmental claim. That was done here. What
was not done was re-running those commands after the tree changed. A
measurement is valid at an instant, and an ADR written later must
re-measure rather than quote.

## Appendix B: Corrections to §4 and §4.2 (2026-09-03)

Three factual errors, found while auditing this document at `7e00155` before
implementing M5. The body above is left unmodified.

The decisions taken in §2 are unaffected. Two of these corrections strengthen
the reasoning that produced them; the third replaces a justification without
changing what it justified.

The six implementation decisions arising from the same audit are recorded in
`ADR-0008`, not here, because an appendix corrects a fact and a new ADR makes
or amends a decision.

### B.1 Correction to §4: the entry limit is in the format specification

§4 states that the 65,536-entry limit appears in Microsoft's support
documentation but not in the on-disk format specification, and treats that as
a weakness in its own reasoning.

**That is wrong.** The FAT32 File System Specification version 1.03, dated
6 December 2000 — the document ADR-0002 §3.1 relies on for the entire
filesystem choice — states the limit directly in its closing notes on FAT
directories. A driver must not allow a directory to exceed 65,536 × 32 bytes,
which is 2,097,152. The document gives two reasons: FAT directories are
neither sorted nor indexed, so creating an entry requires checking every
allocated entry for a duplicate name and becomes very slow on large
directories; and many drivers and utilities, Microsoft's included, count
directory entries in a 16-bit word.

The specification also states explicitly that the limit constrains the size
of the directory rather than the number of files it contains, which is the
same distinction §4 draws when it converts entries to bytes.

The error was caused by relying on secondary accounts of the specification
rather than reading it. §4 was written from a Microsoft support article and a
corroborating third-party account, both of which report the limit accurately
but neither of which is the format specification. The primary document was
fetched and read in full on 2026-09-03.

The correction runs in the direction of greater authority, not less. The
bound §2B adopts is not a compatibility convention inherited from a support
page; it is stated in the same document that defines the on-disk structures
this project parses.

**Independent corroboration, not available when §4 was written.** The Linux
FAT driver carries a constant `FAT_MAX_DIR_SIZE` of 2,097,152 bytes, exactly
65,536 × 32. A 2020 patch to that driver proposed a sanity check in
`fat_calc_dir_size()` returning `EIO` when a corrupted directory's computed
size exceeds it, on the grounds that traversal would otherwise take a very
long time.

That is §4.1's decision — error rather than truncated result — reached
independently by a mature implementation. §4.1 arrived at it from
`PROJECT.md` §5 without knowing anyone else had.

### B.2 Correction to §4.2: the visited set is bounded in clusters

§4.2 states that the set of visited clusters is bounded by the same entry
limit.

Entries and clusters are not the same unit. Entries per cluster is the
cluster size divided by 32: sixteen at one sector per cluster, 1,024 at the
32 KiB maximum the specification permits. The 65,536-entry bound therefore
corresponds to between 64 and 4,096 clusters depending on geometry.

The visited set is bounded by `ceil(65536 / entries_per_cluster)`, at most
4,096 `u32` values, which is 16 KiB in the worst case.

That is a better bound than §4.2 implies, and stating it in the right unit
matters because the implementation allocates the set and the test asserts
against its limit. A set sized in entries would be up to 1,024 times larger
than necessary.

### B.3 Correction to §4.2: cycle detection is a truthfulness control

§4.2 justifies cycle detection as a control on a cluster chain that may loop,
whether through corruption or deliberate construction, and states that
enumeration terminates on revisiting a cluster.

The justification is wrong even though the measure is right.

The 65,536-entry bound in §2B already guarantees termination on its own. A
looping chain produces entries until the bound is reached, and enumeration
then fails under §4.1. Cycle detection does not prevent non-termination,
because non-termination is not reachable.

What cycle detection provides is a truthful diagnosis. Without it, a looping
chain and an oversized directory produce the same error, and the reader
cannot tell a corrupt volume from a large one. With it, the two are
distinguishable and each is reported as what it is.

That places the measure under `PROJECT.md` §5, which requires that Taphonomy
not present a guess as a verified result, rather than under `SECURITY.md`
§16, which the entry bound satisfies unaided.

The distinction is not academic. A control justified as preventing an
infinite loop is tested by constructing a loop and asserting that enumeration
returns. A control justified as producing a truthful diagnosis is tested by
constructing a loop and asserting *which* error is returned. The second test
is the one that would catch a regression here.

### B.4 Cause

§B.1 was caused by writing from secondary accounts of a specification the
project already depends on, rather than from the specification. §B.2 and
§B.3 were caused by reasoning about a bound and a control without working
through the units or the failure mode they were claimed to prevent.

ADR-0002 Appendix A §A.5 requires that an ADR state the command used to
obtain any factual claim about the environment. Appendix A of this document
adds that a measurement is valid only at the instant it was taken. Neither
covers a claim about what an external document says.

The requirement that does cover it — read the source, quote it, and do not
paraphrase from recollection — was not followed. Every claim in `ADR-0008`
§8.2 was read from the specification directly for that reason.
