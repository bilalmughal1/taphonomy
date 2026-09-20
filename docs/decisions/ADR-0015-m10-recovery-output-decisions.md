# ADR-0015: M10 Recovery Output Decisions

* **Status:** Accepted
* **Date:** 2026-09-21
* **Decision owners:** Taphonomy project
* **Scope:** Whether and how a recovered artifact is written to a file;
  where it may be written; what it is named; how many bytes it contains;
  what happens to a partial write; whether what was written is verified;
  whether a third flag changes how arguments are parsed
* **Related:** `SAFETY.md` §3.3, §4, §5, §12, §15, §16; `SECURITY.md` §7;
  `ADR-0003` §3.1; `ADR-0010` Decision A, Decision B; `ADR-0013` §13
  (Decision J), Decision H; `ADR-0014` Appendix B.10; `CLAUDE.md` §9, §20,
  §26

---

## 1. Context

Nine milestones have produced a tool that identifies a deleted file in a
FAT32 root directory, establishes that the run its entry implies reads as
free, streams that run, and reports a digest. It writes no file.
`ADR-0010` Decision A deferred the output path, and `ADR-0014` §10 restated
the deferral. Nothing scheduled it.

The consequence, measured at `3f87efb`: `extract` at
`src/fat_recovery.rs:420` streams one cluster at a time into a reused
buffer, feeds each into the hasher, and discards it. `Extraction` carries
`digest`, `bytes_hashed` and `slack_bytes`. No recovered byte survives the
function, and the only `OpenOptions` in the crate is at
`src/evidence.rs:45` with `.write(false)` and `.create_new(false)`.

An operator who points the tool at an image holding their deleted file is
told its digest and cannot obtain the file. This milestone closes that.

---

## 2. Decisions

* **A.** `--output <directory>` writes each extracted artifact to a file in
  that directory. It requires `--recover`.
* **B.** The destination must not be on the filesystem that holds the
  evidence. The run stops before the evidence is opened if it is.
* **C.** An existing file is never overwritten. That artifact is not
  written and the run says so.
* **D.** The tool composes the filename from the entry's slot and first
  cluster. A recovered name is never a path component.
* **E.** Writing shares `extract`'s single streaming pass. No artifact is
  held in memory.
* **F.** Exactly `file_size` bytes are written. Slack is read, counted and
  not written.
* **G.** A failure part way through an artifact removes the partial file. A
  read failure in the evidence voids the extraction; a write failure leaves
  the digest standing and reports that nothing was delivered.
* **H.** Every written file is re-read and hashed, and the run reports
  whether it matches the digest taken from the evidence.

---

## 3. Decision A: opt-in, and only with `--recover`

`ADR-0010` Decision B put reading a deleted file's content behind an
explicit request, so that the default invocation reports what the volume
states without reading any content. Writing is strictly more than reading,
and inherits that reasoning rather than replacing it.

`--output` without `--recover` is an argument error, on the same grounds
`ADR-0013` Decision H gives for `--reference-digest`: implying `--recover`
would read content the operator did not ask to read.

**Rejected:** writing by default when a destination is configured
elsewhere. There is no configuration file, and introducing one to hold a
destructive default is the wrong place to start.

---

## 4. Decision B: the destination is never the evidence

`SAFETY.md` §3.3 states that recovery output must never be written into the
source evidence, and §4 that it must be stored on a separate destination.
`SAFETY.md` §12 lists "Source/destination collision" among the conditions
the tool must fail closed on.

The reason is stronger than collision with existing files, which a
filesystem allocator already prevents. Free space and unrecovered evidence
are the same bytes: writing a recovered artifact back to the evidence
allocates clusters from the free list, which is where every deleted file
not yet recovered still resides. The first recovery would succeed and
silently destroy the ones after it. A write also changes the evidence
digest every finding in the run is anchored to.

**Enforcement, not documentation.** The run calls `stat` on the destination
directory and on the evidence file and compares `st_dev`. Equal device
identifiers stop the run with an argument error, before the evidence is
opened. `SAFETY.md` §4 states that Taphonomy must never assume a device is
safe to write to simply because the operating system permits writes; a
check the operating system will not perform is exactly what that requires.

Today the evidence is always an image file, so the common case this catches
is a destination on the same host filesystem as the image, which is
harmless, and the degenerate case of writing over the image itself, which
is not. The check is written now because it becomes load bearing the moment
a block device may be named as evidence, and because `SAFETY.md` §16 asks
for output isolated from source evidence, working evidence, application
state, logs, configuration and temporary data.

**Rejected:** comparing canonical path prefixes. A path prefix does not
establish which device a file resides on, and bind mounts and symbolic
links defeat it.

---

## 5. Decision C: an existing file is never overwritten

`SAFETY.md` §15 requires that Taphonomy never silently overwrite existing
recovery output, offering three permitted behaviours — generate a unique
name, ask for overwrite authorisation, or stop — and states that the
default must preserve the existing output.

This milestone stops, per artifact rather than per run. The file is opened
with `create_new(true)`, so the check and the creation are one operation
and nothing can be written between them. The entry is reported as not
extracted, with the reason, and the run continues to the next entry.

**Rejected:** generating a unique name by appending a counter. It hides a
collision that an operator needs to see, because under Decision D two
colliding names mean two entries claiming the same slot and cluster, which
is a finding about the evidence rather than a naming inconvenience.

**Rejected:** an `--overwrite` flag. It is a fourth flag serving no
measured need, which `CLAUDE.md` §9 excludes.

---

## 6. Decision D: the tool names the file, the evidence does not

`SECURITY.md` §7 states that recovered filenames may contain malicious or
unexpected path components, that Taphonomy must sanitise or safely map
recovered paths before writing them, and that a recovered filename must
never allow arbitrary writes outside the configured destination. The bytes
of an 8.3 name in a deleted entry are attacker-controlled input.

A second reason is specific to FAT32 deletion: the first byte of the name
is overwritten with `0xE5`, so every recovered name has lost its first
character. `IMG_0042.JPG` and `AMG_0042.JPG` both recover as `?MG_0042.JPG`.
Naming output by the recovered name would make near-collisions the normal
case rather than the exception.

The name is therefore composed by the tool from facts it established:

```text
slot-<slot>-cluster-<first_cluster>.bin
```

Both are decimal, neither comes from evidence bytes, and the result cannot
contain a separator, a `..`, or a leading `/`. The recovered name, with its
destroyed first character shown as the tool already shows it, is reported on
stdout beside the written path, so nothing is lost — it is reported rather
than trusted.

The `.bin` extension states that the content was not identified. `ADR-0003`
§3.1 makes a level a property of an artifact a validator has seen, and no
validator exists; an extension asserting a format would be a claim the run
cannot support.

**Rejected:** sanitising the recovered name into a safe filename. A
sanitiser is a security boundary that must be correct for every input, and
composing a name from two integers has no such boundary to get wrong.

---

## 7. Decision E: one pass, shared with the hasher

`extract` allocates one cluster-sized buffer and reuses it, so memory does
not scale with file size. The writer takes the same slice the hasher takes,
in the same loop, before the next cluster is read.

This is why the output path belongs inside `fat_recovery` rather than in
the CLI: the alternative is returning the bytes so the caller can write
them, which is the only design that requires holding an artifact in memory.
`CLAUDE.md` §11 keeps recovery logic out of the CLI, and the CLI supplies
the destination and receives a result.

**Rejected:** writing from a second pass over the run. It doubles the reads
of the evidence and admits the possibility of the two passes seeing
different bytes.

---

## 8. Decision F: `file_size` bytes, and no slack

`extract` hashes exactly `run.file_size` bytes: `remaining` starts at
`file_size` and each iteration takes `remaining.min(cluster_bytes)`. Slack
is read as part of the final cluster, counted in `slack_bytes`, and not
hashed.

Writing exactly the bytes that were hashed is what makes Decision H
meaningful: a file containing slack would not match the digest the run
reported, and the mismatch would be an artefact of this decision rather
than a finding about the evidence.

Slack remains reported and unwritten. `Extraction`'s own documentation
records that it belongs to whatever held the cluster before this file and
is evidence in its own right; recovering it is a separate capability.

---

## 9. Decision G: the partial file goes, and the two failures differ

`SAFETY.md` §12 requires failing closed and states that a failure must
never be silently converted into a partial success. A truncated file in the
destination is exactly that conversion: it is indistinguishable, on disk,
from a short file that was recovered whole. So on any error after the file
is created, the file is removed. If the removal also fails, both failures
are reported and the run continues; the tool states what it knows rather
than claiming a cleanliness it could not achieve.

What is reported afterwards depends on which side failed, because the two
are not the same finding.

**A read failure in the evidence voids the extraction.** The hasher did not
see every byte, so there is no digest to report. The entry is reported as
not extracted, with the reason, and counts as a gap under `ADR-0014`
Appendix A.6.

**A write failure leaves the digest standing.** The run read the evidence
and hashed it; a full destination, which §12 names as "Insufficient
destination capacity", or a failing disk says nothing about the evidence.
Discarding a correct measurement because the destination misbehaved would
make the tool quieter about the evidence than it has grounds to be. The
digest is reported as it would be without `--output`, together with the
statement that nothing was delivered and why.

This is the same shape as Decision C, where an existing target leaves the
digest reported and only the delivery withheld. An artifact that was read
but not delivered still counts as a gap, because the operator asked for a
file and has none.

---

## 10. Decision H: the written file is verified against the evidence

After the file is closed, it is re-opened, read, and hashed. The run reports
whether that digest equals the digest computed from the evidence.

This is the decision this milestone exists to make. Every tool in the
comparison EXP-0004 records returns a recovered file without stating any
relationship between the file on disk and the bytes it read. Hashing during
the write proves only what was sent to the kernel. Re-reading proves what
landed, and it is the difference between reporting a digest and standing
behind a file.

It also closes the gap between what M8 does and what an operator holds.
`ADR-0013` established that a digest may be compared against a reference
the operator supplies. Under this decision the tool additionally compares
its own output against its own reading, which requires no reference and is
available on every write.

A mismatch is a finding, not a crash, following `ADR-0013` §3's treatment
of a differing reference: it is reported, the exit status is unchanged, and
the file is left in place so the operator can examine it. A mismatch means
the destination misreported a write, which is information about the
destination.

**Cost, stated:** one extra read and hash of each written artifact.
`ADR-0010`'s bounds already cap what may be extracted, so this is bounded
too. The cost is accepted because an unverified write makes the tool's
central claim unprovable.

**Rejected:** verifying only when asked. A verification the operator must
remember to request is one that will be absent from the runs that matter.

---

## 11. Decision J of `ADR-0013` discharged: parsing stays hand-rolled

`ADR-0013` §13 set a ceiling in terms: when a third flag is added, or when
any flag takes more than one value, hand-rolled parsing is reconsidered
against a dependency under `CLAUDE.md` §20. `--output <directory>` is the
third flag and it takes a value, so the trigger fires here and is
discharged here rather than rediscovered.

Reconsidered, and hand-rolled parsing is retained. `CLAUDE.md` §20 states
that dependencies must not be added for functionality that can reasonably
be implemented with existing project capabilities. Three flags, one of
which takes a value, is fourteen lines of `match` in a loop that already
exists. The evaluation §20 requires — purpose, maintenance, licence,
security history, transitive dependencies — is disproportionate to what a
parser would replace, and a parser crate brings transitive dependencies
into a tool whose dependency count is currently zero.

**The ceiling moves rather than disappears.** It is now: a flag that may be
given more than once, a subcommand, or a fifth flag. Any of those is where
this is reconsidered again.

---

## 12. What M10 does not do

* It does not recover slack, per Decision F.
* It does not identify the format of what it wrote, per Decision D.
* It does not read subdirectories, so a DCF camera card still yields
  nothing to write. That is the next milestone and the measurements for it
  are recorded in `EXPERIMENTS.md`.
* It does not emit per-cluster digests, so a partially correct recovery
  still reports one binary verdict, as `ADR-0014` Appendix A.11 records.
* It does not accept a block device as evidence. Decision B's device
  comparison is written for the milestone that does.

---

## 13. Conditions before M10 may claim to write a recovered file

1. A fixture-backed test writes an artifact from `fat32-recover-run.img`
   and asserts the written file's digest equals the digest the run reported.
2. A test asserts that a destination on the evidence's own filesystem stops
   the run with exit status 2, before the evidence is opened.
3. A test asserts that an existing target file is preserved, that the entry
   is reported as not written, and that the run continues.
4. A test asserts that the written byte count equals `file_size` and that
   slack is absent from the file.
5. A test asserts that a refused entry writes nothing, using
   `fat32-fragmented-live-gap.img`, whose slot 2 is refused at cluster 5.
6. `CLAUDE.md` §26 requires incorrect recovery to be measured, so a test
   asserts that writing an artifact from `fat32-fragmented-deleted.img`
   produces a file whose digest differs from the file that entry named.
   The tool writes a wrong file correctly; the record must say so.

---

## Appendix A: Decision B corrected before it was implemented (2026-09-21)

Section 4 was written from reasoning about what could go wrong and not from
what the field guards against. Implementing it exposed the defect before any
code was committed. The section stands as written; this records what
replaces it.

### A.1 The rule as written refuses every ordinary run

Section 4 requires that the destination not be on the filesystem holding
the evidence, enforced by comparing `st_dev` on both. With the evidence an
image file at `~/evidence/card.img` and the destination `~/recovered`, those
device identifiers are equal on any single-disk machine, so the run is
refused. That is the ordinary invocation, and the rule would have made the
feature unusable on the day it shipped.

Section 4 also contradicts itself. It prescribes the device comparison, then
states two paragraphs later that the common case it catches is a destination
on the same host filesystem as the image, "which is harmless". A rule whose
own justification calls the case it catches harmless is the wrong rule.

### A.2 Where established practice draws the line

Researched after the defect was found, rather than before section 4 was
written, which is the process failure here.

PhotoRec's documentation states the constraint in terms of the source
filesystem: recovered files must not be stored on the source filesystem, or
lost data may be overwritten and definitively lost, and its project page
puts the same rule as not writing recovered files to the partition they
were stored on. The mechanism it gives is the one section 4 gives: writing
recovered files creates new data that can land on blocks the tool has not
yet scanned.

Two further points bear directly on this decision.

First, PhotoRec's guidance recommends imaging a critical drive with `dd` or
similar and running the tool on the image, and places no constraint on where
the output goes relative to that image. Once the analysis runs against an
image, the source filesystem is inside the image and the host filesystem is
categorically not it.

Second, the destination properties that documentation does warn about are
capacity and the destination filesystem's own limits, not which device it
sits on. Capacity is already among the conditions `SAFETY.md` section 12
requires the tool to fail closed on, and Decision G already handles it where
it occurs, so it needs no decision here.

Nothing found imposes the restriction section 4 imposes.

### A.3 What replaces section 4's enforcement

The destination must not be, and must not contain, the evidence. Two checks,
both before the evidence is opened:

* The resolved destination directory must not be the directory holding the
  evidence file, so that no artifact can be written over the evidence.
  `create_new` alone does not cover this: it would report the collision as
  `Output::Exists` and continue, where the run should refuse.
* Where the evidence is a block device, the destination must not reside on
  that device, compared as the destination's `st_dev` against the evidence's
  `st_rdev`. This is the check that enforces `SAFETY.md` section 3.3 in
  substance. It is unreachable until a block device may be named as
  evidence, and it is written now so that the milestone which allows one
  does not have to discover it.

A destination that merely shares a filesystem with an image file is
permitted, with no warning. It is not a case established practice guards
against, and a warning printed on every ordinary run is one operators stop
reading.

### A.4 What this does not change

Decisions A and C through H stand unchanged, as does section 4's reasoning
about why writing into the evidence is destructive: free space and
unrecovered evidence are the same bytes, and a write changes the digest
every finding is anchored to. What changed is only which check establishes
that the destination is not the evidence.
