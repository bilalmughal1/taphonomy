# ADR-0010: M7 Implementation Decisions

* **Status:** Proposed
* **Date:** 2026-09-06
* **Decision owners:** Taphonomy project
* **Scope:** Whether M7 writes recovered content to a path; whether extraction
  is performed by default; how the absence of a cluster chain is handled;
  which deleted entries are eligible; the bounds on an extraction; where the
  code lives; the fixtures required before M7 may claim to recover a file
* **Related:** ADR-0001 §11 (superseded); ADR-0002 §8 (M7, M8, M9), §1.1;
  ADR-0003 §3.1, §3.2, §4.2, §4.4, §8; ADR-0004 §5 conditions 1, 2, 5;
  ADR-0006 §5.1; ADR-0008 §8.1; ADR-0009 §2, Appendix A;
  `ARCHITECTURE.md` §18; `PROJECT.md` §5; `SAFETY.md` §7, §15, §16;
  `SECURITY.md` §7, §16, §18; EXP-0003

---

## 1. Context

`ADR-0002` §8 defines M7 as "Recover the data of an unfragmented deleted
file." M8 is "Validate recovered data against a known-good reference" and M9
is "Classify and report the result with a confidence level." Every boundary
below is drawn from those three lines.

M6 is complete. `enumerate_root` classifies every entry in the root
directory, `EntryKind::Deleted` carries a `DeletedKind` payload, and a deleted
short name's destroyed first byte is derived where a long-name entry survives
to determine it. No code in the crate reads a data cluster, and no code in the
crate writes to a path.

M7 is the first milestone that acts on the location a directory entry names
rather than on the entry itself.

### 1.1 Basis

Every decision below rests on one of four things, and each is named where it
is used.

**Measurement.** EXP-0003 measured what `mtools` deletion destroys. A
fixture audit run on 2026-09-06 measured what the two existing deleted
fixtures contain, cluster by cluster and FAT entry by FAT entry, with every
number carrying the `od` command that produced it.

**Code read at `9bbc625`.** Line references are to that commit.

**The project's own documents, quoted.** In particular `ADR-0003` §3.1 and
§8, which had not been read in full before this decision and which contradict
the framing this milestone was handed.

**External sources, named.** M7 is the first milestone whose behaviour the
FAT32 specification does not define. §3.1 records why that matters and what
was consulted instead.

---

## 2. Decisions

**A.** M7 writes no file. It extracts to memory, hashes what it extracted,
and reports.

**B.** Extraction is opt-in. The default output reports only what the volume
states.

**C.** Contiguity is never affirmed. The implied run is checked against the
active FAT, and any allocated cluster in it refuses the recovery.

**D.** Eligibility is narrow, and every refusal is named rather than omitted.

**E.** The extraction is streamed and bounded. Nothing allocates
`DIR_FileSize`. Slack is excluded from the content and from the digest, and
its extent is reported.

**F.** `hash.rs` gains a project-owned incremental hasher, and `hash_reader`
is rewritten over it.

**G.** M7 is a new module. `cluster_offset` and `read_fat_entry` become
`pub(crate)` and neither moves.

**H.** Three cases must be covered by fixtures, in one new image pair.

---

## 3. Decision A: M7 writes no file

### 3.1 The milestone does not ask for it

`ADR-0002` §8's M7 line is five words long and none of them is "write". The
only milestone list in the project's history that contained a write step is
`ADR-0001` §11, whose item 7 reads "write recovered data to a separate
destination". `ADR-0002` §7 marks §11 **Superseded**.

A fixture audit of `docs/` on 2026-09-06 found no other passage assigning an
output step to M7. What it found instead was requirements on output, in
`SAFETY.md` §3.3, §4.3, §7, §15 and §16, in `SECURITY.md` §7, §8, §18
and §19, and in `ARCHITECTURE.md` §18, none of which is assigned to a
milestone.

### 3.2 `ADR-0003` §3.1 forbids it as things stand

> A Candidate must never be presented to a user as a recovered file, written
> to the recovery output directory, or counted in a recovery result summary.

M7 has no validator. M8 is the validator. Everything M7 produces is therefore
a Candidate in `ADR-0003` §3.1's sense, and writing it to a recovery output
directory is forbidden without amending an accepted ADR.

Note that `ADR-0003` §8 says the confidence taxonomy "becomes binding on the
first code that assigns a confidence level, which under ADR-0002 §8 is
milestone M9", and that implementing it earlier "would be speculative". Both
statements are correct and they are about §3.2. The constraint that binds M7
is §3.1's pipeline boundary, which is not a confidence classification and
does not wait for M9.

### 3.3 Writing is a component, not a call

`ARCHITECTURE.md` §18 specifies an Output Writer enforcing destination
boundaries, path safety, overwrite protection, output integrity and metadata
recording. The audit at `9bbc625` found none of it: no output directory, no
destination argument, no file-creation helper anywhere in `src/`. The only
filesystem writes in the tree are test scaffolding in `tests/read_only.rs`.

Building that alongside the recovery algorithm makes one milestone into two.

### 3.4 The reference implementation makes the same split

The Sleuth Kit's `icat` writes recovered content to standard output; the
caller redirects it. TSK is the library and the command-line tools, and
Autopsy is the layer above that provides case management. Extraction and
output are separated there, in the same place `ARCHITECTURE.md` §18 separates
them.

### 3.5 The write side is where the defect was

CVE-2026-40024, disclosed against The Sleuth Kit through 4.14.0, is a path
traversal in `tsk_recover`: a crafted filesystem image containing path
traversal sequences in filenames causes files to be written outside the
intended recovery directory.

This is `SECURITY.md` §7 exactly, and it occurred in the most examined
open-source forensic toolkit there is. It occurred in the component that
writes. `icat`, which extracts, has no such surface.

M7 additionally derives part of a filename rather than reading it:
`ADR-0009` Decision D recovers a deleted short name's first character from a
checksum. `SECURITY.md` §18 requires that output filenames be treated as
untrusted input. A partly derived filename, written by an unbuilt writer,
before any validator exists, is three unfinished things meeting at once, and
nothing requires them to meet at M7.

### 3.6 What is given up

A digest of bytes the user cannot obtain is less useful than a file. M7 ends
with the tool able to demonstrate that it read the right bytes and unable to
hand them over. That is a real cost and it is accepted deliberately: M7 is the
algorithm and the Output Writer is the delivery.

This decision does not hold that a recovery tool should never write.
`tsk_recover` exists because bulk extraction is useful. The decision is about
sequence.

---

## 4. Decision B: extraction is opt-in

Without an explicit request, the CLI reports what the volume states about a
deleted entry: its surviving fields, its first cluster, its size, and the
allocation status of the run those imply. With the request, it additionally
reads the run and reports the digest.

Two reasons.

**The guess stays behind an explicit act.** TSK reached the same
arrangement: without its recovery flag, `icat` returns only the first
cluster, because the first cluster is stated on disk and everything after it
is inferred. That is `ADR-0003`'s Candidate boundary expressed at the
interface.

**Cost.** The default enumeration is bounded by the size of the directory.
Reading every deleted file's run is bounded by the size of the data those
entries name, which on a real volume is unbounded in practice.

`src/main.rs:28` currently rejects a second argument. This decision changes
that and nothing else about argument handling.

---

## 5. Decision C: contiguity is never affirmed

### 5.1 The evidence that is missing

Deletion zeroes the cluster chain in every FAT. EXP-0003 measured it: of
thirty changed bytes, twenty-four were three FAT entries in each of two FATs.

For an unfragmented file the chain is not needed, because the clusters follow
the first. But **nothing in the evidence says the file was unfragmented.**
The entry that would have said so is the entry deletion destroyed.

### 5.2 The three available behaviours

There is no specification for this. Brian Carrier, documenting his own
implementation, states that the process "is not defined by any 'official'
specification" and that there is no "generally accepted" and documented
procedure for FAT file recovery. The FAT32 specification documents structure
and is silent on deletion, as EXP-0003 already recorded.

Three behaviours are documented in practice:

1. **Skip.** TSK advances by consecutive clusters, counts unallocated ones
   towards the file, and skips allocated ones. This reconstructs a fragmented
   file, which `ADR-0002` §8 places explicitly out of scope for M7.
2. **Ignore.** Other tools take the number of clusters the size requires and
   disregard allocation status. Carrier names this strategy and publishes the
   incorrect digests it produces against his test corpus, alongside the
   correct ones.
3. **Refuse.** Neither of the above.

M7 refuses.

### 5.3 The check

For an eligible entry, the implied run is

```text
first_cluster ..= first_cluster + file_size.div_ceil(cluster_bytes) - 1
```

Every cluster in the run is looked up in the active FAT. A zero entry is
unallocated. Any non-zero entry means the cluster is claimed by something
else, so the implied run is broken and the bytes at that offset are not this
file's. The recovery is refused, and the first offending cluster is named.

Carrier refuses on the starting cluster for a reason worth recording,
because it is not the reason above: an allocated starting cluster also occurs
when a file was moved within the same partition, in which case another
directory entry describes the same clusters with a better size. This decision
refuses that case too, and now does so knowingly.

### 5.4 The check can only refuse

A run of unallocated entries is the **absence of contrary evidence**, not
evidence. Clusters can be written and freed again, leaving the FAT zero and
the content foreign.

`ADR-0003` §4.2 states that a level "may only be raised by evidence that
satisfies the higher level's requirement" and "may never be raised by absence
of contrary evidence." A free run therefore never raises anything. It is a
necessary condition for the extraction to be attempted and not a sufficient
one for the result to be believed, and M7's report must say so in those
terms.

This is where `ADR-0003` first constrains real output, and it is §4.2 and
§3.1 that do it, not §3.2.

---

## 6. Decision D: eligibility is narrow and refusals are named

Eligible: `DeletedKind::ShortName` with `directory == false`,
`file_size > 0`, and `first_cluster >= FIRST_DATA_CLUSTER`
(`src/fat.rs:48`), with the implied run falling inside the volume's declared
cluster count.

Refused by name, never by omission:

* a deleted directory, which M7 does not recover
* a deleted volume label
* a deleted long-name component
* `DeletedKind::Invalid`
* `file_size == 0`, which locates no content
* `first_cluster` of 0 or 1, which are reserved
* a run extending beyond the declared cluster count

An omitted field reads as an absence of interest rather than an absence of
evidence. `ADR-0009` §3.3 made this argument about a name and it applies
unchanged to a recovery.

**The first cluster itself is unverified.** `KNOWN_ISSUES.md` records that no
fixture can exercise a deleted file whose first-cluster high word has been
zeroed, and EXP-0003 Limitation 2 states that the experiment measures
`mtools` and cannot measure a Microsoft implementation. A `first_cluster`
below 65,536 read from a deleted entry is indistinguishable from one whose
high word was destroyed. M7 reports this once rather than implying otherwise
by silence.

---

## 7. Decision E: the extraction is streamed, bounded, and excludes slack

### 7.1 Nothing allocates `DIR_FileSize`

`DIR_FileSize` is untrusted evidence and `SECURITY.md` §16 names "enormous
declared file sizes" as a resource-exhaustion vector. The extraction reads one
cluster at a time into a reusable buffer, exactly as `enumerate_root` does at
`src/fat_directory.rs:861`.

`src/evidence.rs:94` applies no bounds check of its own; an over-read fails
through `read_exact`'s `UnexpectedEof`. That fails the read but only after an
oversized allocation would already have been made, if one were made. The run
is therefore range-checked against the declared cluster count before any read,
in the same shape as `src/fat_directory.rs:844-849`.

### 7.2 Slack is excluded and reported

The final cluster is read whole and truncated to the remaining byte count. The
bytes past the logical end are the previous occupant's, not this file's, and
they appear in neither the extraction nor the digest.

They are not discarded silently. File slack is a recognised artefact in its
own right, so its extent is reported. Recovering it is a separate capability
and is not M7.

---

## 8. Decision F: `hash.rs` gains an incremental hasher

`src/hash.rs:85` exposes `hash_reader<R: Read>` and nothing else. Decisions A
and E together require hashing a cluster-by-cluster stream without holding it,
and there is no surface for that.

The alternative considered was a `Read` adapter over the cluster run, feeding
the existing `hash_reader` unchanged. It is rejected: the adapter would have
to launder a typed error through `io::Error`, and `src/error.rs:3` states the
principle it would break, that "a recovery failure that does not identify what
failed is not diagnosable."

A project-owned incremental type satisfies `ADR-0004` §5 conditions 1 and 2 in
the same way `Sha256Digest` does: `sha2` is named only inside `hash.rs` and no
caller sees its types.

Blast radius, measured at `9bbc625`. `hash_reader` has six call sites: one in
production at `src/evidence.rs:87`, two unit tests, and three in
`tests/nist_vectors.rs`. Rewriting `hash_reader` over the new type leaves
every signature unchanged, so no call site changes. `ADR-0004` §5 condition 5
is preserved, and `chunk_boundary_does_not_affect_digest` already drip-feeds a
reader, so the incremental path is covered by NIST vectors on the day it
lands.

---

## 9. Decision G: a new module, and two helpers become `pub(crate)`

M7 is a new module in `src/`. `src/fat_directory.rs` is 1,865 lines and
enumeration is a different concern from extraction.

The module needs `cluster_offset` (`src/fat_directory.rs:764-777`) and
`read_fat_entry` (`src/fat_directory.rs:969-980`). Both are private. Both
become `pub(crate)`.

Neither moves. `read_fat_entry` reads the FAT and is arguably misplaced in the
directory module, but moving it edits a path M5's tests cover for a gain that
is organisational rather than measured. The misplacement is recorded in
`KNOWN_ISSUES.md` instead.

M7's unit tests get their own in-memory `EvidenceReader`. The existing
`MemoryImage` is declared inside `src/fat_directory.rs`'s `#[cfg(test)]`
module at 1455-1467, is nameable nowhere else, and its helpers are
directory-shaped: `write_cluster` zero-fills unused 32-byte slots, which is not
what a data cluster is. Promoting it to shared test support is the wrong trade
for two users; a third user is the trigger.

No name collisions exist: `recover`, `recovery`, `run`, `extraction`,
`candidate` and `artifact` are declared as no module, type, function or
constant anywhere in `src/` at `9bbc625`.

---

## 10. Decision H: three cases, one new image pair

### 10.1 What the existing fixtures prove, measured

`fat32-deleted-entries.img` was decoded on 2026-09-06. Three deleted entries
qualify for M7, in slots 4, 8 and 10, at clusters 4, 6 and 8, with sizes 30,
31 and 31 bytes. The volume is formatted at one 512-byte sector per cluster.

For all three, the FAT entry reads `0x00000000` and the cluster still holds the
exact bytes the generator wrote.

So M7 would succeed on three files out of three; every run is one cluster
long, so no run is walked; nothing has been reallocated, so Decision C's
refusal could be deleted from the source without a test failing.

This is `ADR-0008` §8.1's "proves versus illustrates" problem in its fourth
instance, and it is now a measurement rather than a concern.

### 10.2 What `mtools` will not produce

`mtools` allocates forward. Measured: after four deletions freeing clusters 3,
4, 5 and 6, the next two files written took clusters 9 and 10.

A fixture built with `mtools` alone therefore cannot contain a deleted entry
whose clusters have been reused, which is the ordinary condition on any volume
that has been used since the deletion.

### 10.3 The three cases

1. **A multi-cluster deleted file with an intact free run.** Proves the run
   is walked and the content assembled across cluster boundaries. `mtools`
   alone.
2. **A deleted entry whose implied run collides with live clusters.** Proves
   Decision C's refusal fires. Requires a poke.
3. **A deleted directory.** Proves Decision D's refusal fires. Already
   available in the existing fixtures and reproduced in the new volume.

### 10.4 The poke, and the one rejected

`ADR-0006` §5.1 permits it: "Taking a valid image and poking one field, as the
four FAT32 boot sector fixtures already do, does not encode a reading of the
specification — the surrounding structure came from `mkfs.vfat`."

The field poked is a deleted entry's `DIR_FileSize`, enlarged so that the run
it implies extends into clusters that live files hold. One field, on an image
`mkfs.vfat` and `mtools` built, producing the shape a reused volume produces.

Rejected alternative: poking a FAT entry inside the run from zero to an
allocated value. Also one field, but it manufactures a cluster allocated to
no directory entry, which is a different condition, a lost cluster, and it
would test the refusal against a volume state that does not match the story
the fixture is telling.

### 10.5 A new pair, not an edit to the existing one

`tests/fat32_directory_fixtures.rs` asserts entry positions and justifies that
by the order `mcopy` and `mmd` are invoked. Adding a file to
`build_deleted_volume` would move every slot index in those assertions and
change both existing manifest digests. A new builder and a new image pair
leave M6's fixtures untouched.

### 10.6 A recommendation withdrawn

It was proposed during preparation that the fixture record, alongside the true
content digest, the digest a naive contiguous read would produce, following
Carrier's practice of publishing both.

Withdrawn. That practice guards against a tool silently returning the wrong
content. Under Decision C the tool refuses instead of returning anything, so a
test asserting the refusal already fails if the refusal is removed. The second
digest would add a value to maintain and detect nothing the first assertion
does not.

The practice becomes relevant again if Decision C is ever revisited in favour
of the ignore-status strategy.

---

## 11. Consequences

### Positive

The first milestone that reads file content adds no code that writes to a
path, so the class of defect CVE-2026-40024 represents cannot be introduced by
it.

The absence of a cluster chain is handled by refusing rather than by
assuming, and the refusal is grounded in a measurement of the FAT rather than
in a claim about what deletion usually leaves.

`ADR-0003`'s pipeline boundary becomes load-bearing for the first time, at
§3.1 and §4.2, ahead of the taxonomy becoming binding at M9.

The extraction is bounded before any read, so a hostile `DIR_FileSize`
allocates nothing.

Deleted files gain fixture coverage for a run, a collision and a directory,
none of which the existing set contains.

### Negative

M7 produces no artefact a user can open. The milestone's visible output is a
run, a status and a digest.

`hash.rs` changes, which is the first edit to the module `ADR-0004` isolated.
The signature does not change and the NIST vectors cover it, but the module was
previously untouched since M1.

A second in-memory `EvidenceReader` enters the test suite, duplicating part of
`MemoryImage`.

Two helpers widen from private to `pub(crate)`, and one of them stays in a
module it does not belong to.

`src/lib.rs:11-14`, `README.md:9-11`, `README.md:250`, `README.md:253-254` and
`PROJECT.md:345-348` all assert in the present tense that no file content is
read and no recovery capability exists. Every one becomes false and must
change in the same commit series. `ADR-0009:365-366` says the same and is
corrected by appendix rather than edited, under the rule that an ADR is dated
evidence.

The collision fixture depends on a poked field, so it proves the refusal
handles the structure and not that the structure arises in the field. The
measured `mtools` allocation behaviour is why no alternative exists here.

---

## 12. Review trigger

Revisit Decision A when the Output Writer of `ARCHITECTURE.md` §18 is built,
which is M9 at the earliest, and record the exception to `ADR-0003` §3.1 that
writing a Candidate would require if it is ever proposed before then.

Revisit Decision C if fragmented recovery is brought into scope, which
`ADR-0002` §8 currently excludes. The skip strategy becomes available at that
point and this decision's refusal does not.

Revisit Decision G if a third module needs an in-memory `EvidenceReader`, or
if a second module needs `read_fat_entry`, at which point moving it out of
`fat_directory.rs` becomes a measured gain rather than an organisational
preference.

Revisit Decision H if a fixture built from a Windows-deleted volume becomes
available, which would also close the high-word gap `KNOWN_ISSUES.md` records.

---

## 13. Errors in the preparation of this decision

### 13.1 A claim formed from a filename

It was predicted that the single-cluster FAT read M7 needs already existed in
`src/fat.rs`, on the reasoning that M5 walks a cluster chain and `fat.rs` is
the FAT module.

`src/fat.rs` contains no FAT-table access at all. It is BIOS parameter block
parsing and variant determination, and its own module doc comment says so in
its first line. The read is `read_fat_entry` in `src/fat_directory.rs`.

The prediction was formed from the file's name rather than from its contents,
which is `ADR-0009` §11.6's shape again: a claim formed from something that
resembled the source.

### 13.2 A precondition written from an assumption about a file's contents

An audit brief instructed that `sha256sum -c` be run against
`fixtures/partition/MANIFEST.sha256` from the repository root. The manifest
records relative paths, so the command fails there and must be run from the
manifest's own directory.

The line was written from an assumption about how the manifest was structured.
The manifest was available to read and was not read. This is the rule that new
briefs must be written from current file contents, applied to a file the brief
itself was about to verify.

### 13.3 An expectation formed without being stated

Before the fixture audit ran, an unstated expectation was held that the two
deleted fixture images differed by more than the single poked byte. They differ
by exactly one byte, and the FAT tables and data clusters are byte-identical.

The expectation cost nothing because it was never acted on, and it is recorded
because it was formed from a general sense that a poke has knock-on effects
rather than from the script, which pokes one byte and says so.

---

## 14. Open after this decision

1. **Cluster 3 of `fat32-deleted-entries.img` is unmeasured.** `REUSED.TXT`
   was the first file written and its directory slot was later taken by
   `REUSE1.TXT`, so if its content survives, the fixture contains a file that
   no directory entry names and that entry-driven recovery can never reach.
   That cluster 3 is the one is an inference from the write order, not a
   measurement, and the claim is deliberately not made in this ADR.

2. **No FAT32 reference corpus has been identified for M8.** Carrier's
   published FAT undelete test image documents both correct and incorrect
   digests for six deleted files, but it is a 6 MB volume, which cannot be
   FAT32 under the cluster-count threshold at `src/fat.rs:38`. Whether a
   FAT32 equivalent exists is unresearched, and its licence would need
   examining before use.

3. **The zeroed first-cluster high word remains untestable**, unchanged from
   `KNOWN_ISSUES.md`. Decision D reports the first cluster as unverified
   because of it.

4. **`ADR-0007` §5.5's patent question remains open.** M7 does not decode long
   names, so the gate is not reached.

5. **Deleted entries in subdirectories remain out of scope**, because M5
   enumerates the root directory only.

---

## Appendix A: Two factual errors in §10.1 and §11 (2026-09-06)

The body above is left unmodified. Neither error changes a decision. §2's
decisions A to H stand unaltered.

Both errors were present when this ADR was committed at `34de009` and were
found the same day, by checking two claims against the sentences they were
drawn from rather than against the lists they had been taken from.

### A.1 §10.1 gives the wrong ordinal

§10.1 states:

> This is `ADR-0008` §8.1's "proves versus illustrates" problem in its fourth
> instance, and it is now a measurement rather than a concern.

**It is the third instance, not the fourth.** The repository records two
prior instances and no more:

* `ADR-0008` §8.1, "The fixture set contains no chain-walk coverage", which
  is the first and where the phrasing originates.
* `EXPERIMENTS.md:515`, in EXP-0003, which names itself: "That is the 'what a
  fixture proves versus what it illustrates' problem recorded in ADR-0008
  §8.1, in its second instance."

No third instance is recorded anywhere in the tree. The fixture trap §10
addresses is therefore the third.

The ordinal was written from a recollection rather than from a count, and no
source was consulted before it was recorded. A number that is not counted is
not a measurement.

### A.2 §11 wrongly lists `ADR-0009` among the statements M7 falsifies

§11's Negative section states:

> `src/lib.rs:11-14`, `README.md:9-11`, `README.md:250`, `README.md:253-254`
> and `PROJECT.md:345-348` all assert in the present tense that no file
> content is read and no recovery capability exists. Every one becomes false
> and must change in the same commit series. `ADR-0009:365-366` says the same
> and is corrected by appendix rather than edited, under the rule that an ADR
> is dated evidence.

**The final sentence is wrong.** `ADR-0009:365-366` reads:

> M6 runs no recovery algorithm and produces no content. A deleted directory
> entry is not an Artifact, and ADR-0003 §5 forbids merging the two axes it
> distinguishes.

That statement is scoped to M6. M7 running a recovery algorithm does not make
it false, and it requires no appendix and no change of any kind.

The five statements named before it are unscoped assertions about the tool as
a whole, and those five do become false. The list is correct up to the point
where `ADR-0009` is introduced.

### A.3 Cause

Both errors share the shape recorded in §13 of this ADR and in `ADR-0009`
§11.6: a claim formed from something that resembled the source rather than
from the source.

A.2 came from a list. A prior audit had returned every passage in the
repository matching a search for statements about recovery and file content,
and had explicitly noted that some hits were included because they matched
the search rather than because they asserted what was searched for.
`ADR-0009:365-366` was one of those. Membership in the list was treated as
equivalent to the claim the list was gathered to support, and the sentence
itself was not re-read for its scope. The audit was correct and complete; it
was used carelessly.

A.1 came from a recollection. The ordinal was carried from a document read
earlier and then altered without reference to anything.

The general form: a list assembled for one question does not answer a
narrower question drawn from it, and every member has to be checked against
the narrower question individually. A search result is a candidate, not a
finding.

---

## Appendix B: Interface decisions for the recovery module (2026-09-06)

This appendix adds decisions. The three appendices written in this project
before it corrected facts, and nothing in `CLAUDE.md` §22 restricts an
appendix to that use. The choice to record these here rather than in a
separate ADR is deliberate: §2's decisions A to H and this appendix's B1 to
B7 are both M7's, and the rule that a milestone is not complete until its
ADR's decisions are checked by name points at one document. Two documents
would mean two checklists and only one of them would be checked.

Decisions here are numbered B1 to B7 so that a reference to "Decision D"
continues to mean §2's Decision D and nothing else.

These decisions determine the shape of the interface. They do not alter any
decision in §2, and each of them is downstream of one.

### B.0 Basis

Line references are to `ef15a1b`, and every one of them was confirmed by
reading the line at that number rather than by recollection.

An earlier draft of this appendix stated that exactly two public structs in
the crate have a private field. The count was three. The draft was written
after `Sha256Hasher` was added at `ef15a1b` and did not account for it. The
rule B5 records was not affected; only the count was wrong.

### B1. The module is `src/fat_recovery.rs`

Named for the structures it reads, matching `src/fat_directory.rs`. The
module reads FAT32 FAT entries and FAT32 directory entries specifically, so
the `fat` prefix is accurate rather than decorative, and a later recovery
module for another filesystem does not collide with it.

Rejected: `src/recovery.rs`, which would claim generality the module does not
have.

### B2. `assess` returns `Result<Option<Assessment>, RecoveryError>`

`None` means the entry is not a deleted file, so the question does not
arise.

This follows `associate` at `src/fat_directory.rs:672`, whose doc comment
already settled the same question for M6:

> Returns `None` when the entry at `index` is not a deleted short entry, in
> which case the question does not arise. A live entry's first byte is not
> destroyed, and reporting it as such would be false.

The alternative was an `Ineligible::NotADeletedFile` variant sitting
alongside genuine refusals. It was rejected because it makes every live entry
in a directory produce a refusal reason that the caller must remember to
suppress, and a reason that must be suppressed is a reason that will
eventually be printed.

`assess` takes `&Entry` rather than a pre-filtered payload, so that every
eligibility rule lives in one function. Splitting them between the caller and
the module is how a rule gets applied in one place and forgotten in the
other.

### B3. Ineligibility is an outcome, not an error

`Ineligible` travels in the `Ok` arm. A deleted directory, an empty file, or
a reserved first cluster is a fact about the evidence, not a failure of the
tool.

The crate already draws this line in a type. `ParseOutcome`
(`src/partition.rs:228-233`) returns `table`, "what the sector was found to
contain", alongside `anomalies`, "interpretable but unusual observations".
Something unusual about the evidence is reported in the success value; only
a failure to parse at all becomes an error. `Assessment` follows the same
shape.

`RecoveryError` is reserved for what stops the assessment from being made at
all: a read that fails, an offset that overflows, a cluster outside the data
region.

Decision D (§6) requires that refusals are named rather than omitted. A
refusal that arrives as an error is named, but it is named in the same
channel as a broken image, and the caller cannot tell a fact about the
evidence from a fault in reading it.

### B4. `UnallocatedRun` is a witness type

`extract` takes `&UnallocatedRun`. The type has a private field, is
constructible only within the module, and is produced only by `assess` after
every FAT entry in the implied run has read zero. There is therefore no way
to write a call that extracts a run the FAT says is in use, because there is
no way to obtain the argument.

This is Decision C (§5) enforced by the compiler rather than by discipline.
Decision C is the substance of the milestone, and a refusal that a later
caller can bypass by accident is a refusal that will eventually be bypassed.

The pattern is not new to this crate. Three public structs declared outside
`#[cfg(test)]` have a field that is not public, and all three exist to carry
an invariant:

* `EvidenceFile` (`src/evidence.rs:19-23`), whose existence proves that a
  read-only open succeeded. Its doc: the handle "is opened without write,
  append, create, or truncate access" and "there is no method on this type
  that writes."
* `Sha256Digest` (`src/hash.rs:30`), whose existence proves 32 bytes came
  out of a computation.
* `Sha256Hasher` (`src/hash.rs:90-93`), whose private `bytes_read` is why
  the count it reports can only be the number of bytes it hashed.

Every other public struct in `src/` has every field public: `FatGeometry`,
`Fat32BootSector`, `HashResult`, `Entry`, `RootDirectory`, `MbrPartition`
and `ParseOutcome`.

`UnallocatedRun` is the fourth of the first kind, and the same shape as all
three: an abstract newtype whose constructor is the only way in.

**No escape hatch.** `Sha256Digest::from_bytes` exists and is labelled with
what it does not do. `UnallocatedRun` gets no equivalent. Unit tests obtain
one by calling `assess` against a memory image holding a real FAT, which
costs more setup than fabricating one and is the point: a test that
manufactures the precondition it is testing under supplies its own answer,
which is what Appendix A.2 of `ADR-0009` records.

A read-only accessor returning `&ClusterRun` is provided, because the caller
must report the run and read access cannot weaken the invariant. No mutable
access and no consuming conversion are provided. Nothing needs either, and
adding them before anything does is the escape hatch by another name.

### B5. `ClusterRun` is a report with public fields

`ClusterRun` carries the first cluster, the cluster count, the file size and
the slack byte count, all public.

This follows a rule that holds across every struct in `src/` and is written
down nowhere: **private fields where a type carries an invariant, public
fields where a type reports what was found.** Seven public structs report and
have every field public: `FatGeometry`, `Fat32BootSector`, `HashResult`,
`Entry`, `RootDirectory`, `MbrPartition`, `ParseOutcome`. Three carry an
invariant and do not: `EvidenceFile`, `Sha256Digest`, `Sha256Hasher`.

The rule is recorded here because it is real, it has been followed
consistently, and a contributor who has to infer it is a contributor who will
eventually infer it wrongly.

### B6. `RecoveryError` carries a `DirectoryError` variant

`cluster_offset` (`src/fat_directory.rs:764`) and `read_fat_entry`
(`src/fat_directory.rs:969`) both return `Result<_, DirectoryError>`.
Decision G (§9) keeps them where they are, so the recovery module's error
type must carry their errors, and those errors are FAT-level failures wearing
a directory-level name: `OffsetOverflow`, `ClusterOutOfRange`, `BadCluster`.

This is recorded so that it reads as a consequence rather than as
carelessness, and so that the `KNOWN_ISSUES.md` entry Decision G requires
says what the misplacement actually costs. It is not a reason to reopen
Decision G: moving the helpers would edit a path M5's tests cover for an
organisational gain.

### B7. The active FAT index is derived inside the module

The module computes the index from `BPB_ExtFlags` itself. It is not a
parameter.

`src/fat_directory.rs:831` does exactly this:

```text
let fat_index = boot.active_fat().unwrap_or(0);
```

`active_fat` (`src/fat32.rs:207-217`) returns `None` when mirroring is
enabled, "because then every FAT is current and the active-FAT bits carry no
meaning", and otherwise masks bits 0 to 3 of `ext_flags`, which
`src/fat32.rs:178` documents as the raw `BPB_ExtFlags`. The `unwrap_or(0)` is
therefore not a fallback for a missing answer: when mirroring is on, FAT 0 is
as current as any other.

A recovery that read FAT 0 on a volume where FAT 1 is authoritative would
report allocation status confidently and wrongly, and Decision C (§5) rests
entirely on that status being right. Making the index a parameter would let a
caller supply the wrong one.

**The test cannot use a fixture.** `src/fat_directory.rs:1656-1659` records
why: `KNOWN_ISSUES.md` notes that no fixture exercises FAT mirroring, because
`mkfs.vfat` offers no option to set `BPB_ExtFlags` and writes both FATs
identically. M5's test at `:1661` therefore builds a synthetic boot sector
through a `boot(ext_flags)` helper at `:1538` and reads it with the in-memory
`MemoryImage`. M7's equivalent test does the same, with its own helper and
its own in-memory reader, per Decision G (§9).

### B.8 Shape

The following is the interface these decisions describe. It records the
shape, not the source; where the two ever differ, the source is what exists
and this is what was intended.

```text
pub fn assess(entry, boot, extent, reader)
    -> Result<Option<Assessment>, RecoveryError>

pub fn extract(run: &UnallocatedRun, boot, extent, reader)
    -> Result<Extraction, RecoveryError>

pub enum Assessment {
    Ineligible(Ineligible),
    RunBroken { run: ClusterRun, first_allocated: u32 },
    Recoverable(UnallocatedRun),
}

pub enum Ineligible {
    Directory,
    VolumeLabel,
    LongNameComponent,
    InvalidEntry { attr: u8 },
    EmptyFile,
    ReservedFirstCluster { cluster: u32 },
    RunOutOfRange { last_cluster: u64, data_clusters: u32 },
}

pub struct ClusterRun {
    pub first_cluster: u32,
    pub cluster_count: u32,
    pub file_size: u32,
    pub slack_bytes: u32,
}

pub struct UnallocatedRun(ClusterRun);

pub struct Extraction {
    pub digest: Sha256Digest,
    pub bytes_hashed: u64,
    pub slack_bytes: u32,
}
```

---

## Appendix C: Decision G is amended on the in-memory reader (2026-09-06)

Decision G (§9) ruled that M7's unit tests get their own in-memory
`EvidenceReader` and that promoting the existing `MemoryImage` to shared test
support was "the wrong trade for two users; a third user is the trigger."

That ruling is amended. The reader is promoted now, before the second copy is
written. The rest of Decision G stands: the module is new, `cluster_offset`
and `read_fat_entry` widen to `pub(crate)`, and neither moves.

### C.1 The estimate the decision rested on was wrong

Decision G was written on an estimate of about fifteen duplicated lines.

Measured: the `MemoryImage` block in `src/fat_directory.rs` runs 71 lines, of
which `write_cluster` is 13. The recovery module wants the other 58. A
further 21 lines of geometry constants, `extent()` and `boot()` were already
duplicated into `src/fat_recovery.rs` at `6a49d94`. The second copy would
therefore stand at roughly 83 lines rather than fifteen.

### C.2 The criterion the decision used was the weaker one

Decision G applied a count: two users is not enough, three is. That
heuristic exists because with two similar pieces of code it is not yet known
whether they are the same thing or merely look alike, and abstracting on a
resemblance produces something worse than the duplication.

The count is a proxy. The question it stands in for is whether the two uses
are the same concept, and where that question can be answered directly the
proxy is not needed.

It can be answered directly here, and it answers differently for the two
halves of the block:

**The reader is one concept and cannot diverge.** The struct, `new`, `write`
and the `EvidenceReader` impl exist to satisfy a trait with a single method,
`read_exact_at`, declared at `src/evidence.rs:123-126`. Both users need byte
regions read at an offset, and both need a read past the end to fail rather
than pad, which `src/fat_directory.rs:1460-1463` records as the reason the
double is shaped this way: a double that padded would let tests pass against
behaviour `EvidenceFile` does not have. There is no future in which one
module needs that to mean something different from the other.

**`write_cluster` is not.** It zero-fills 32-byte slots because directory
entries are 32 bytes. It is domain-shaped, it is exactly what acquires a
parameter and then a flag when it is shared, and it does not move.

Decision G's own objection to promotion was that "its helpers are
directory-shaped". That objection was correct and it applies to 13 of the 71
lines. It was applied to all of them.

### C.3 The trigger Decision G named could not have fired

Decision G said a third user is the trigger. A third unit-test user is
unlikely to arrive: the extraction work is in the same module and so is the
same user, and the fixture tests live in `tests/`, where each file compiles
as its own crate and cannot see a `#[cfg(test)]` item in `src` at all.

A trigger that cannot fire is not a deferral. It is a decision to duplicate
permanently, which is not what Decision G was weighing.

### C.4 Where the reader goes

Into `src/evidence.rs`, in a `#[cfg(test)] pub(crate) mod tests` added for
the purpose, as `pub(crate) struct MemoryImage`.

Not into a new support module. The crate already shares a test helper across
modules: `fat32_sector()` lives in `src/fat.rs`'s `#[cfg(test)] pub(crate)
mod tests` and is used by `src/fat_directory.rs:1544` and by
`src/fat_recovery.rs`. The established practice is that a shared test helper
lives in the module that owns the concept. `MemoryImage` implements
`EvidenceReader`, and `EvidenceReader` is declared in `evidence.rs`.

`write_cluster` and `write_fat` stay as methods, in an inherent `impl
MemoryImage` block in the test module that uses them. An inherent impl may
live in any module of the crate that defines the type, so all 38 existing
call sites are unchanged. `write_fat` is three lines of FAT32 entry arithmetic
and will exist in two test modules. That duplication is deliberate: the
alternative puts the size of a FAT32 entry into `evidence.rs`, which knows
nothing about filesystems and should continue not to.

### C.5 Consequences

The move edits the `#[cfg(test)]` block that M5's and M6's unit tests run
through. Nothing outside `#[cfg(test)]` changes, and the gates cover it: if
the move is wrong, those tests fail rather than something subtler happening.

`src/evidence.rs` gains its first `#[cfg(test)]` module. Its tests otherwise
live in `tests/read_only.rs`, which stays where it is, because it tests the
real type against the real filesystem and is not affected.

The count criterion is not abandoned. It remains the right default where the
question it proxies for cannot be answered directly. It was the wrong tool
for a 45-line implementation of a one-method trait.
