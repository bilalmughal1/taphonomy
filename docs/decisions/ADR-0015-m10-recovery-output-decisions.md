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

---

## Appendix B: The output path's failures, measured and corrected (2026-09-22)

Section 9 and section 10 were implemented at `b935901`. An audit at
`a38a8cb` found three places where the code reported a destination failure
as something it was not, one decision the code had made without this ADR,
and one condition in section 13 that Appendix A left uncorrected. The body
and Appendix A stand as written; this records what was found, what changed,
and what remains.

### B.1 Three misreported failures, corrected at `7c127a2`

**An uncreatable file was reported as a partial file left behind.** One
private state stood for both a file that could not be created and a file
that was created and then failed. For the first, the run tried to remove a
path it never created, the removal failed with not-found, and the entry was
printed with ", partial file left". An unwritable `--output` directory
produced that false statement for every entry. `CLAUDE.md` section 14
requires a permission failure to be told apart from an I/O failure. The
outcome is now `Output::NotCreated`, printed as "could not be created",
counted under `artifacts not written`, and nothing is removed.

**A write failure followed by a read failure left a truncated file.**
`discard` removed the file only while it was still open. Where a write had
already failed and the evidence then failed too, the partial file stayed on
disk and the run returned the read error with no statement that a file
existed. Section 9 removes the file on any error after it is created, and
`SAFETY.md` section 12 forbids a failure becoming a silent partial success.
`discard` now removes it in both states.

**A failed flush was reported as a written file awaiting verification.**
`settle` treated an error from `sync_all` as `Output::Unverified`, leaving
the file in place and printing it as written. A failed flush is a failed
write. After a writeback error Linux commonly discards the affected pages
and marks them clean, so a later read may return something other than what
was written; the kernel behaviour and the PostgreSQL failure it caused are
recorded in LWN's "PostgreSQL's fsync() surprise" (April 2018) and on the
PostgreSQL wiki page "Fsync Errors". The outcome is now `Output::Failed`,
and the file is removed. `sync_all` itself stays: the Rust standard
library's documentation for `std::fs::File` states that dropping a file
ignores errors detected on closing and that `sync_all` is how to handle
them.

### B.2 `Output::Unverified`, which section 10 does not cover

Section 10 covers a read-back that completes and differs. It does not cover
a read-back that cannot be performed. Since `7c127a2` that case is narrowly
defined: every write and the flush succeeded, and the file could not be
re-opened or read. The file is left in place and printed as `NOT VERIFIED`
with the reason. Removing it would destroy an artifact that may be sound on
the strength of a failure that says nothing about the evidence. It is not
counted under `artifacts not written`, because a file was delivered; it is
reported as unverified rather than as matching.

### B.3 What the read-back proves

Section 10 says re-reading "proves what landed". That is stronger than the
mechanism supports. The file is flushed, closed, re-opened and read through
the ordinary file interface, and on Linux such a read is normally served
from the page cache. What it proves is what the destination filesystem
returns for that file after a successful flush. It detects a truncated or
transformed file, a filesystem or FUSE layer that alters bytes, and any
flush error the kernel reports. It does not establish what the storage
medium holds. This project has not measured whether a given read-back was
served from cache.

Reading past the cache needs `O_DIRECT`, with its alignment rules, or
`posix_fadvise` with `POSIX_FADV_DONTNEED`, which the Linux manual page
describes as an attempt to free cached pages rather than a guarantee. Both
need the `libc` crate or `unsafe` foreign calls. `CLAUDE.md` section 16
requires `unsafe` to be justified, section 20 excludes a dependency for a
capability of this size, and the crate's dependency count is zero. Neither
is adopted. The stdout wording, "matches what was read", claims only what
the read-back establishes.

### B.4 Section 13, condition 2, as Appendix A corrects it

Condition 2 still states the rule Appendix A withdrew: a destination on the
evidence's own filesystem stopping the run. The condition that has been
asserted since `16953a7` is Appendix A.3's: a destination that is the
directory holding the evidence stops the run with exit status 2 before the
evidence is opened.
`tests/recovery_output.rs::a_destination_holding_the_evidence_is_refused`
asserts it. Appendix A.4 should have said so.

### B.5 Tests, measured at `3f14a7f`

Five unit tests in `src/fat_recovery.rs` and one CLI test in
`tests/recovery_output.rs` cover the failures above. The suite is 269 tests
across thirteen `test result` lines, lib 155 and `recovery_output` 7.

| Test | Asserts |
| --- | --- |
| `a_read_failure_part_way_through_leaves_no_partial_file` | Evidence fails at cluster 5 after cluster 4 is written; no file remains. A control run on the whole image writes the file. |
| `a_file_whose_write_had_failed_is_removed_when_the_evidence_then_fails` | The second failure in B.1 |
| `a_file_this_run_did_not_create_is_never_removed` | A file the run did not create survives every failure path, per section 5 |
| `a_file_that_could_not_be_created_is_reported_not_created` | The first failure in B.1, using a missing directory so it holds under root |
| `a_write_failure_removes_the_partial_file_and_says_so` | `Output::Failed` with the file removed |
| `an_unwritable_destination_is_reported_without_a_partial_file` | The first failure in B.1 through the binary; the digest still stands. Unix only, with a control assertion as in `tests/read_only.rs` |

**Untested:** a failed flush, and `Output::Unverified`. Neither can be
reached without a failing device, and reaching them from a test would need
a failure seam in production code that exists only for the test. Both are
reasoned from the code, not measured.

### B.6 Where the code does not yet meet section 9

Section 9 states that if the removal also fails, both failures are reported
and the run continues. On the read-failure path that is not so. `discard`
removes the file on a best-effort basis and returns nothing, because the
error `extract` returns is the evidence's. If the evidence fails and the
removal fails too, a partial file remains and the run does not say so. It
needs two failures at once, one of them in the evidence, and is not tested.
Meeting section 9 here means carrying the removal's outcome alongside the
read error, which is a change to `RecoveryError` and is left for its own
commit.

### B.7 A failed copy is not a partial recovery

Section 9 removes a partial file, and `SAFETY.md` section 13 allows partial
recovery where it is explicitly represented. They do not conflict, because
the two concern different things. The file section 9 removes is an
incomplete copy of bytes the run read in full: the evidence still holds
every one of them and the digest of all of them is reported, so a run to a
working destination reproduces the whole artifact. A partial recovery is
one where the evidence cannot supply the whole file, because a cluster is
unreadable, reused or elsewhere.

That second kind has forensic value and the field keeps it: PhotoRec's
documentation describes an option to keep corrupted files and fragments,
named with a leading `b`. This tool does not produce it today. A refused
run, a run voided by a read error, and a fragmented run all yield no
partial artifact. Producing one would revisit `ADR-0010` Decision C's
refusal and the single verdict `ADR-0014` Appendix A.11 records, alongside
the per-cluster digests A.11 names, and needs its own ADR.

### B.8 What this appendix does not change

Decisions A to G stand, and Decision H stands with its claim read as B.3
states it. Section 9's distinction between a read failure,
which voids the digest, and a destination failure, which does not, is what
the corrections in B.1 implement. Appendix A stands in full.
