# ADR-0009: M6 Implementation Decisions

* **Status:** Accepted
* **Date:** 2026-09-04
* **Decision owners:** Taphonomy project
* **Scope:** The representation of a deleted directory entry; treatment of
  entries beyond the `0x00` terminator; whether M6 decodes long names;
  recovery of the destroyed first byte of a short name; whether M6 assigns a
  confidence level; the fixtures required before M6 may claim to identify
  deleted entries
* **Completes:** ADR-0007 §5.4; ADR-0008 §5.4
* **Corrects:** `EXPERIMENTS.md` EXP-0003 Conclusion, final paragraph
* **Related:** ADR-0002 §8 (M6, M7, M9); ADR-0003 §3.1, §3.2, §4.5, §5;
  ADR-0006 §5.1; ADR-0007 Appendix C; ADR-0008 §5.4, §6, §8.1;
  `PROJECT.md` §5; `SAFETY.md` §12; `CLAUDE.md` §22, §44

---

## 1. Context

ADR-0002 §8 defines M6 as "Identify deleted directory entries." M7 is
"Recover the data of an unfragmented deleted file," and M9 is "Classify and
report the result with a confidence level." Each boundary matters below.

M5 is complete. `enumerate_root` walks the root directory's cluster chain and
classifies every entry up to the terminator. `EntryKind::Deleted` exists and
carries no payload; its doc comment at `fat_directory.rs:125-128` records
that "M6 will need the remaining bytes."

### 1.1 Basis

Every decision below rests on one of three things, and each is named where it
is used.

**Measurement.** EXP-0003 deleted files from a FAT32 volume with `mtools` and
compared the result against the volume it was made from, byte by byte. It
established what deletion destroys, what survives, and what a fixture built
with these tools can and cannot contain.

**Code read at `43e84ef`.** Line references are to that commit.

**The project's own documents, quoted.** ADR-0007 §5.3 was found to contain
two factual errors and is corrected by its Appendix C; the decisions below
depend on the corrected facts, not the originals.

---

## 2. Decisions

**A.** `EntryKind::Deleted` carries a `DeletedKind` payload in which the
fields deletion destroys are unrepresentable rather than absent or wrong.

**B.** Entries beyond the terminator are classified into their own vector.
`RootDirectory::entries` keeps its present meaning unchanged.

**C.** M6 does not decode long names.

**D.** The destroyed first byte of a short name is derived from the long-name
checksum where one survives, and reported as destroyed where none does. It is
never guessed.

**E.** M6 assigns no confidence level.

**F.** Two fixtures are required: one built with `mtools` alone, one built by
poking a single byte into a valid image.

---

## 3. Decision A: `EntryKind::Deleted` carries a `DeletedKind`

### 3.1 The shape

```rust
/// What a deleted entry was, as far as its surviving bytes establish.
pub enum DeletedKind {
    LongName {
        checksum: u8,
    },
    VolumeLabel {
        surviving_name: [u8; NAME_LEN - 1],
    },
    ShortName {
        surviving_name: [u8; NAME_LEN - 1],
        directory: bool,
        first_cluster: u32,
        file_size: u32,
        nt_res: u8,
    },
    Invalid {
        attr: u8,
    },
}
```

The variants correspond one to one with the live classification `classify`
already performs, minus every field the deletion marker destroys.

### 3.2 Why not reuse `EntryKind`

The obvious design is `Deleted { was: Box<EntryKind> }`, reusing the
classification that already exists. It is wrong, and EXP-0003 §4 measured why.

`0xE5` is `1110 0101`. Bit 6 is `LAST_LONG_ENTRY`. So for a deleted long-name
entry, the expressions `classify` evaluates at `fat_directory.rs:412-413`
produce:

```text
first & LDIR_ORD_LAST != 0   ->  last: true    on every component
first & !LDIR_ORD_LAST       ->  ordinal: 165  on every component
```

Neither value is evidence. Both are artefacts of the marker, and both are
plausible enough to be believed by a reader who does not know the entry is
deleted. Reusing `EntryKind::LongName` would place measured garbage into
typed fields, which is worse than absence: absence prompts a question and a
wrong value does not.

`DeletedKind::LongName` carries the checksum and nothing else, because the
checksum is the only field of a long-name entry that deletion leaves both
intact and meaningful.

### 3.3 Why ten name bytes and not eleven

`surviving_name` is `[u8; NAME_LEN - 1]`, holding bytes 1 to 10 of the name
field. The eleventh, byte 0, is not carried.

An eleven-byte array invites rendering a name whose first character is a
deletion marker, which is exactly the failure `PROJECT.md` §5 names as a
non-goal: presenting a guess as a verified result. A ten-byte array cannot be
rendered as a name by accident, because it is not a name.

Nothing is lost. The destroyed byte is the known constant `NAME_DELETED`, so
the on-disk entry remains exactly reconstructible from the parsed structure.

### 3.4 No rendered name in the payload

`EntryKind::ShortName` carries `name: Option<String>` alongside its raw
bytes. `DeletedKind::ShortName` carries no rendered name, because there is
not yet one to render: the first character comes from Decision D, which needs
a second entry to derive it and therefore cannot run inside `classify`.

ADR-0008 Decision D requires that classification be total and infallible over
a single 32-byte array. That property is preserved. Association and
derivation are a separate pass over the entry list, after classification.

### 3.5 Blast radius

Four sites, all read at `43e84ef`:

```text
fat_directory.rs:129   the variant definition
fat_directory.rs:405   construction, inside classify
fat_directory.rs:722   one unit test
main.rs:263            one match arm in report_root_directory
```

Nothing under `tests/` references it. `grep -rniIn "deleted" tests/` returns
no match at all, which is also the finding recorded in Decision F.

`short_entry_count` and `long_name_count` at `fat_directory.rs:371` and
`:379` filter on `EntryKind::ShortName` and `EntryKind::LongName`
respectively, so neither counts a deleted entry of either shape. That
behaviour is retained deliberately rather than inherited: both names describe
the allocated listing, and a deleted entry is not part of it.

---

## 4. Decision B: entries beyond the terminator get their own vector

### 4.1 What M5 does now

`enumerate_root` sets `terminated` at `fat_directory.rs:610` and, for every
later slot, reaches `fat_directory.rs:599-605`:

```rust
if terminated {
    if raw.iter().any(|b| *b != 0) {
        residue_first.get_or_insert(slot);
        residue_slots += 1;
    }
    continue;
}
```

`classify` is never called. The observation pushed at `:651-657` carries
`cluster`, `first_slot` and `slots` — a start and a population count. The
bytes do not leave the function, and the extent is not recoverable from the
count: residue in slots 5 and 12 with 6 to 11 empty reports `first_slot: 5,
slots: 2`, from which slot 12 cannot be derived.

### 4.2 The commitment already made

ADR-0008 §5.4 justified scanning the full cluster:

> Residue past a terminator is evidence, there is documented hardware that
> leaves it, and M6 will need exactly that region.

M6 is here, and the region is not in the output. This decision discharges
that commitment.

### 4.3 The shape

```rust
pub struct RootDirectory {
    pub entries: Vec<Entry>,
    pub residue: Vec<Entry>,
    pub clusters: Vec<u32>,
    pub observations: Vec<DirectoryObservation>,
}
```

`Entry` already carries `cluster` and `slot`, so a single vector accumulating
across clusters loses no position information.

### 4.4 Why not extend `entries`

Merging would change the meaning of a field whose doc comment states it ends
at the terminator, and would break the two counts asserted in
`tests/fat32_directory_fixtures.rs` at `:90` and `:277`.

The stronger reason is that the distinction is the finding. `entries` is what
the volume claims is present. `residue` is what is present anyway. Collapsing
them would present a reconstruction as a directory listing, which ADR-0007
§4.1 forbids in the same terms: the caller could not distinguish "these are
the entries" from "these are the entries and some bytes we found afterwards".

`DirectoryObservation::ContentAfterTerminator` is retained unchanged. It
remains the human-readable finding; `residue` carries the data. The
redundancy is accepted in preference to altering a committed observation.

### 4.5 The bound, stated rather than discovered

`MAX_ENTRIES` is 65,536 and the check at `fat_directory.rs:638` counts
`entries` only. `residue` is bounded only indirectly, by `max_clusters` at
`:557` capping the walk at `MAX_ENTRIES.div_ceil(slots_per_cluster)`
clusters. The worst case therefore approximately doubles: a directory may
yield up to `MAX_ENTRIES` entries and a comparable number of residue slots.

This is accepted rather than bounded separately, because a second explicit
limit would fail a directory that the first limit already permits, and the
combined figure is a known multiple of a value ADR-0007 §4 already justified.
It is recorded here so that it is a decision and not a surprise.

---

## 5. Decision C: M6 does not decode long names

### 5.1 The reason that was withdrawn

ADR-0007 §5.5 records that the VFAT long-filename patents were litigated and
that their status has not been researched, and requires the check before
decoding is implemented.

That question is **not** used to justify this decision, and this ADR takes no
position on it. §5.5 remains open exactly as written, for whoever does that
work properly against primary records. M6 does not decode, so the gate is not
reached.

### 5.2 The reason that stands

ADR-0007 §5.3 held that long-name handling is load-bearing for recovery
because the long-name entries are the only surviving source of the first
character of a deleted file's name. Appendix C.2 measured that claim false:
the checksum determines the byte uniquely, using no name character at all.

The correction splits the conclusion. **Retention** is load-bearing, because
the checksum byte exists nowhere else. **Decoding** is not, because Decision
D needs one byte from a long-name entry and no part of `LDIR_Name1`,
`LDIR_Name2` or `LDIR_Name3`.

ADR-0002 §8 gives M6 as identification. With decoding removed from the
recovery path, nothing in the milestone requires it.

### 5.3 A limit decoding could not escape

Deletion destroys `LDIR_Ord` on every component, so a deleted set carries no
count of its own members. Measured in EXP-0003: `mtools` reuses the earliest
deleted slot when a new file is written, and a subsequent write consumed the
`0x40`-flagged component of a two-entry set, leaving one orphan adjacent to
its short entry.

Nothing in the surviving evidence distinguishes that partial set from a
complete one-entry set. **A long name recovered from a deleted set can
therefore never be known to be complete**, and presenting one as a filename
would present a possibly-truncated string as a name. That is independent of
the patent question and independent of scope.

---

## 6. Decision D: the first byte is derived, or reported destroyed

### 6.1 The derivation

The long-name checksum is computed over the eleven bytes of the short name.
Ten survive. The specification's `ChkSum()` performs eleven rounds of a
rotation followed by an addition modulo 256; rotation is a bijection on 256
values, addition of a fixed byte modulo 256 is a bijection, and the first
byte enters as the initial value because the accumulator starts at zero. The
composition is a bijection, so exactly one candidate reproduces any given
checksum.

Measured in EXP-0003 §3 against a real deleted entry: surviving bytes
`LONGD~1TXT`, checksum `0x32`, candidate set `{0x41}`, and `0x41` is correct.

No `ChkSum()` implementation exists in the crate. `OFF_LDIR_CHKSUM` is
defined at `fat_directory.rs:56` and the byte is read at `:414`, but nothing
computes it. M6 adds the function. It takes `&[u8; NAME_LEN]`, performs no
I/O and cannot fail, matching the constraint ADR-0008 §6.2 places on
`classify` for the same reason.

### 6.2 The validity test is free

Two candidate values can never be correct. `0x00` is the terminator, and
`0xE5` is never stored literally because it is escaped to `0x05`. A
derivation producing either is proof that the long-name set does not belong
to the short entry that follows it, and the association is rejected rather
than reported.

A derivation producing `0x05` means the recovered character is `0xE5`.
`short_name` at `fat_directory.rs:456-462` already performs that mapping and
already rejects the result as not printable ASCII.

### 6.3 The association

A deleted short entry at slot *n* is associated with deleted long-name
entries at *n-1*, *n-2*, and so on, while each is deleted, carries the
long-name attribute, and carries the same checksum. Live entries terminate
the walk backwards: a live long-name component belongs to a live short entry
following it, which the deleted entry is not.

Position is the only ordering evidence available, because Appendix C.1
establishes that the ordinals are destroyed. In particular, the component
holding characters 1 to 13 is the one stored **immediately before the short
entry**, not the one flagged `LAST_LONG_ENTRY`, which holds the final
fragment and is stored first. ADR-0007 Appendix C.2 requires this ADR to
record that constraint, and this is the record.

### 6.4 When no long-name entry survives

A deleted 8.3 entry with no associated long-name set has nothing constraining
its first byte. All 254 remaining values are equally consistent with the
evidence.

**The byte is reported as destroyed.** M6 does not enumerate candidates, does
not select a likely one, and does not substitute a placeholder character that
could be mistaken for a name. `PROJECT.md` §5 forbids presenting guesses as
verified recovery, and `SAFETY.md` §12 forbids silently converting a failure
into a partial success. A name with an invented first character is both.

This is why Decision A carries ten name bytes rather than a rendered string:
the type makes the unrecovered case unrepresentable as a name.

---

## 7. Decision E: M6 assigns no confidence level

### 7.1 Confidence classifies content

ADR-0003 §3.1 defines the pipeline as recovery algorithm, then Candidate,
then validator, then Artifact, and states that an Artifact is a proposition
plus a verdict. Every level in §3.2 is defined over content: recovered
content byte-identical to a reference, content that parses, content
assembled by inference.

M6 runs no recovery algorithm and produces no content. A deleted directory
entry is not an Artifact, and ADR-0003 §5 forbids merging the two axes it
distinguishes.

### 7.2 The milestone list places this at M9

ADR-0002 §8 gives M9 as "Classify and report the result with a confidence
level." Assigning one at M6 would be doing M9's work three milestones early,
against evidence M9 has not yet been designed to weigh.

### 7.3 Correction to EXP-0003

The Conclusion of EXP-0003, committed at `4e95da5`, states:

> Any first character recovered by the checksum method is `RECONSTRUCTED`
> under ADR-0003 §3 and never `VERIFIED`.

That sentence is wrong in three ways and is withdrawn.

It applies a taxonomy defined over Artifacts to something that is not one.
`RECONSTRUCTED` is defined as content assembled by inference "rather than
surviving filesystem metadata", and the derivation uses nothing but surviving
filesystem metadata. And ADR-0003 §4.5 states that `RECONSTRUCTED` describes
the method used and is not a position between `PARTIAL` and `UNVERIFIED`,
which makes "and never `VERIFIED`" a strength ordering the rule forbids.

The remainder of that paragraph stands: the derivation is exact but rests on
assumptions the evidence does not establish, and its uniqueness must not be
reported as certainty about the name. That is a true statement about the
derivation and it does not require a confidence level to express.

The cause is recorded in §11.

### 7.4 What M6 reports instead

Uncertainty is carried by the type system and by observations, which is the
mechanism the codebase already uses. ADR-0008 §5.4 describes it as the
pattern applied four times: parsing succeeds and the finding is reported.

`DeletedKind` makes destroyed fields unrepresentable. An unassociated
long-name entry is reported as unassociated. A rejected association is
reported as rejected. None of these is a confidence level, and none of them
requires one.

---

## 8. Decision F: two fixtures

### 8.1 What `mtools` can produce, measured

EXP-0003 established that `mdel`, `mrd` and `mdeltree` are deterministic
under the exports `scripts/generate-fixtures.sh` already sets, and that
`mrd` and `mdeltree` are byte-identical in effect.

Producible with `mtools` alone:

```text
a deleted 8.3 entry
a deleted long-name set, complete
a deleted subdirectory, with its own cluster and dot entries intact
a partial long-name set, produced by slot reuse consuming a component
```

The fourth is the least obvious and the most valuable. Deleting a long-named
file and then copying a new one causes `mtools` to reuse the earliest deleted
slot, which is a long-name component, leaving an incomplete set behind. It
requires no poking.

### 8.2 What it cannot produce

```text
a deleted entry beyond a terminator
a zeroed first-cluster high word
```

The first is impossible because FAT directories do not shrink and `mtools`
reuses deleted slots before the directory grows, so the terminator never
moves backwards past a deleted entry. The second is recorded in
`KNOWN_ISSUES.md`: `mtools` preserves the high word.

### 8.3 The two fixtures

`fat32-deleted-entries.img`, built with `mtools` alone, covering the four
shapes in §8.1. Partition label-id `0xfa730007`.

`fat32-deleted-residue.img`, built by the same tools and then poked once: a
single `0x00` written into the first byte of a slot preceding a deleted
entry, making that slot a terminator and everything after it residue.
Partition label-id `0xfa730008`.

The generator's FAT32 fixtures currently use `0xfa730001` through
`0xfa730006` consecutively, read at `43e84ef`, so both values are free.

ADR-0006 §5.1 permits exactly this and no more:

> The narrow exception is deliberate corruption. Taking a valid image and
> poking one field, as the four FAT32 boot sector fixtures already do, does
> not encode a reading of the specification — the surrounding structure came
> from `mkfs.vfat`.

One byte is poked. Every other byte in the image was written by `mkfs.vfat`,
`mcopy`, `mmd` and `mdel`, none of which has seen this codebase, so the
fixture cannot agree with the parser by construction.

### 8.4 Why both are required

`grep -rniIn "deleted" tests/` returns no match at `43e84ef`. Not one test in
the repository exercises a deleted entry against real bytes; the only
coverage is synthetic arrays inside `src/fat_directory.rs`.

That is the condition ADR-0008 §8.1 identified for the chain walk and built
`fat32-root-multicluster.img` to fix. Adding Decisions A, B and D without
these fixtures would recreate it exactly, and Decision B would be the worse
case: residue classification whose only test constructs the residue with the
same understanding it is meant to check.

---

## 9. Consequences

### Positive

The fields deletion destroys cannot be read as data, because they cannot be
expressed. The `0xE5` ordinal trap is closed by the type rather than by a
comment.

The commitment ADR-0008 §5.4 made to M6 is discharged, and residue becomes
addressable evidence rather than a reported count.

The first character of a deleted long-named file is recovered exactly where
the evidence permits, and reported as destroyed where it does not, with no
middle case in which a guess is presented.

Long-name decoding leaves the recovery path, so ADR-0007 §5.5's unresearched
patent question no longer sits between the project and a working milestone.

Deleted entries gain fixture coverage from real tool output for the first
time.

### Negative

`RootDirectory` gains a field and `EntryKind::Deleted` gains a payload, so
every construction and match site changes. Four sites for the enum and one
struct literal at `fat_directory.rs:677`, but the public API changes shape.

Worst-case memory for enumeration approximately doubles, per §4.5.

A deleted 8.3 file with no long-name set yields a name that can never be
completed by any method M6 implements. This is a limit of the evidence, but
it will be visible to users as an incomplete result.

The residue fixture depends on a poked byte, so it proves the parser handles
the structure and not that the structure occurs in the field. The hardware
ADR-0008 §5.3 cites is not available to this project.

`src/lib.rs:12` states that "no deleted entry is interpreted." That becomes
false and must change in the same commit series.

---

## 10. Review trigger

Revisit Decision B if a directory is encountered whose residue exceeds what
the doubled bound permits, or if residue is found to require the cluster
bytes rather than the classified entries.

Revisit Decision C if long-name decoding is proposed. ADR-0007 §5.5 must be
answered first, and §5.3 of this ADR must be answered as well: a decoded name
from a deleted set cannot be known to be complete.

Revisit Decision E at M9, which is where the confidence model meets its first
real artifacts.

Revisit Decision F if a fixture built from a Windows-deleted volume becomes
available, which would close the gap `KNOWN_ISSUES.md` records.

---

## 11. Errors in the preparation of this decision

### 11.1 A taxonomy was applied without being read

The Conclusion of EXP-0003 assigned `RECONSTRUCTED` to a recovered filename.
ADR-0003 was not read that day; the level was recalled as the project's
vocabulary for inferred results. Reading it afterwards showed three separate
errors in one sentence, recorded in §7.3.

The record was already committed at `4e95da5` when the error was found. It is
corrected here rather than edited there, because an experiment record is
dated evidence.

### 11.2 A blast radius was quoted from a report already found unreliable

An agent report was rejected for dropping lines from code quotations, and a
count of call sites from the same report was then used in a design proposal.
Re-running the search directly found four sites rather than three: the
variant definition had been omitted.

### 11.3 The contents of two shell functions were asserted from a name search

`poke` and `poke_le32` were cited as available for building a poked fixture
on the strength of a grep that matched their names. Neither had been read.
They turned out to do what was claimed, which does not make the assertion
sound.

### 11.4 A defective search hid eight functions for two turns

`grep -n "^[a-z_]*()"` was used to enumerate the generator's functions. The
character class excludes digits, so every function whose name contains one —
including all six FAT32 fixtures — was silently absent from the result. The
omission was noticed only when a later count disagreed.

### 11.5 A legal question was nearly settled from secondary sources

Research into the VFAT patents was conducted and returned encyclopedia
entries, a wiki and a forum thread. Proposing to close ADR-0007 §5.5 on that
basis was withdrawn before it reached a document. Decision C is argued
without it.

### 11.6 Cause

Four of the five share one shape: a claim was formed from something that
resembled the source rather than the source itself. A report of a file, a
name in a search result, a recollection of a taxonomy, a summary of a patent
record. In each case the real thing was available and cheap to read.

`CLAUDE.md` §22 requires an ADR to state reasoning, and reasoning built on a
proxy for the evidence is not reasoning about the evidence. The measure that
caught all five was the same one: read it, quote it, count it.

---

## 12. Open after this decision

1. `PROJECT.md` §6.9 and `SAFETY.md` §14 still carry the six-term lists that
   ADR-0003 §7 requires be replaced by a reference to ADR-0003. Neither
   amendment has been made. This is documentation debt predating M6 and is
   not resolved here.

2. No fixture can exercise a zeroed first-cluster high word, per
   `KNOWN_ISSUES.md`. Code handling that case will be written against no real
   evidence until a Windows-produced image is available.

3. Deleted entries in subdirectories are out of scope. M6 identifies deleted
   entries in the root directory, because that is what M5 enumerates.

4. `EXP-0003` Limitation 3 records that only one cluster size was measured. A
   larger cluster changes how many entries a directory cluster holds and
   therefore where residue can appear.

5. Whether the CLI should report residue separately from the allocated
   listing, and in what form, is deferred to implementation.

---

## Appendix A: Decision D was recorded as delivered (2026-09-05)

The body above is left unmodified.

This ADR was accepted at `551f711` and M6 was declared complete at `894e713`.
At that point Decision D was half implemented, and §9 already described it as
delivered. The gap was found by a documentation audit run against `894e713`
and closed at `898067e` and `60332eb`.

Nothing in §2 changes. The decisions were right; one of them was not built.

### A.1 The arithmetic shipped and its caller did not

§6.1 specifies deriving the destroyed first byte from a long-name checksum.
§6.3 specifies how the entry holding that checksum is found: walk backwards
from the deleted short entry while each preceding entry is a deleted
long-name component carrying the same checksum. §6.4 specifies what to report
when no component survives.

`chksum` and `recover_first_byte` were implemented at `0fbea0c`.
`recover_first_byte` takes ten surviving name bytes and a checksum and returns
the byte that produced it. **Nothing implemented §6.3.** No function walked
backwards, `recover_first_byte` had no caller anywhere in `src/`, and the CLI
printed no name for a deleted entry at all.

§9 nonetheless listed among the positive consequences:

> The first character of a deleted long-named file is recovered exactly where
> the evidence permits, and reported as destroyed where it does not.

At `894e713` the shipped tool did neither. It reported neither a recovered
character nor an explicit statement that one was destroyed; it omitted the
name field and said nothing about it.

Closed at `898067e`, which added `FirstByte`, `associate` and
`recovered_name`, and made the CLI state the outcome in every case.

### A.2 The fixture test appeared to prove otherwise

`a_deleted_first_byte_is_recovered_from_its_long_name_set`, added at
`20044c2`, passed throughout. It read:

```rust
for (component, short, expected) in [(3, 4, b'P'), (7, 8, b'C')] {
```

The slot pairs were read from a manual decode of the fixture image and
written into the test. The test performed the association itself and then
checked that the arithmetic agreed. It could not have failed for the reason
§6.3 exists, because it never asked any code to find a component.

A test that supplies the answer it is checking proves the last step of a
derivation and hides the absence of every step before it.

Replaced at `60332eb` with a version that calls `associate` over every index
in the enumeration and asserts that the resulting set of names is exactly
`PARTIA~1.TXT` and `COMPLE~1.TXT`, in on-disk order. It is given the
enumeration and nothing else. Two further tests were added: one asserting the
three outcomes of §6.2 and §6.4 against the fixture, and one asserting that
association works on the residue vector, where the slice indices and the slot
numbers differ.

### A.3 Cause

The quality gates were treated as the completeness check.

`cargo fmt`, `cargo clippy -D warnings` and `cargo test --workspace` were run
after every change in the milestone and were green every time. They prove
that what exists compiles, is idiomatic, and passes its tests. **They cannot
detect a specified function that was never written**, because nothing calls
it and nothing tests it. An absent capability produces no warning.

The milestone was built as a sequence of briefs, each implementing part of
the ADR, in the order `CLAUDE.md` §44 requires: research, decision,
implementation, tests, documentation. Each step was verified against its own
brief. No step compared the finished tree against the ADR's list of
decisions.

§11.6 named the shape of the errors made while preparing this decision: a
claim formed from something that resembled the source rather than the source
itself. This is a variant. The claim that the milestone was complete was
formed from a sequence of green results that resembled completeness.

The measure that would have caught it, and which was eventually what did:
read the ADR's own §2 and check each decision against the code by name.

### A.4 An error in the correction

The brief that replaced the hand-association stated that
`recover_first_byte` was no longer named by
`tests/fat32_directory_fixtures.rs`, and removed it from the import list on
that basis. A third test still called it. The build broke.

The file had been read in full that session and was available to search. It
was not searched. This is §11.2 and §11.3 again, and it is recorded here
because it happened while correcting §11.6's own diagnosis.

The resolution is in the file rather than in the import: the third test was
performing the same hand-association, which
`association_works_past_the_terminator` now covers properly, so the block was
removed rather than the import restored.
