# ADR-0014: M9 Classification and Reporting Decisions

**Status:** Accepted
**Date:** 2026-09-15
**Milestone:** M9 (`ADR-0002` §8)
**Supersedes:** nothing
**Amends:** nothing. `ADR-0003` is amended by its own appendix, an action of
this record.

---

## 1. Context

`ADR-0002` §8's ninth and final milestone is to classify and report the
result with a confidence level. `ADR-0003` defined the taxonomy a year
before any of it was built, and `ADR-0003` §8 reserved it for this
milestone: the taxonomy becomes binding on the first code that assigns a
confidence level.

This record is that code's decision document. It was prepared by auditing
`ADR-0003` against the tool at `e033ea9`, against the constraining documents
that cite it, and against the measurements in EXP-0003 and EXP-0004.

The audit's principal finding is that the taxonomy cannot be implemented as
written. Three of its six levels have no reachable code path, one of M8's
three outcomes has no level, and one level misdescribes the case it would be
assigned to. §3 records this in full. The decisions that follow are
constrained by it.

---

## 2. Basis

Read and quoted in preparing this record:

* `ADR-0003` §§1, 2, 3.1, 3.2, 3.3, 4.1–4.7, 5, 7, 8, Appendix A
* `ADR-0013` §§3, 7, and Appendix A
* `ADR-0002` §8
* `SAFETY.md` §§10, 12, 13, 14
* `SECURITY.md` §31
* `ADR-0011` §3
* EXP-0003, EXP-0004 and its Appendix A
* `src/main.rs` reporting path, `src/fat_recovery.rs`, `src/validation.rs`

External practice consulted:

* NIST CFTT, *Active File Identification & Deleted File Recovery Tool
  Specification*, Draft 1 of Version 1.1
* Jim Lyle, *Creating Deleted File Recovery Tool Testing Images*, NIST/CFTT,
  AAFS 2012
* Meyer and Roy, *Do Metadata-based Deleted-File-Recovery (DFR) Tools Meet
  NIST Guidelines?*, EAI Endorsed Transactions on Security and Safety, 2020
* ENFSI, *Guideline for Evaluative Reporting in Forensic Science*
* Piriform Recuva's published recovery states, and independent testing of
  their reliability

---

## 3. The taxonomy cannot be implemented as written

Each level in `ADR-0003` §3.2 carries an evidence requirement. Checked
against the tool at `e033ea9`:

| Level | Evidence `ADR-0003` requires | State |
| --- | --- | --- |
| `VERIFIED` | Cryptographic hash match against a reference artifact | reachable; contested by §4.5, resolved in Decision A |
| `STRUCTURALLY_VALID` | Format-aware validator completed without error over the entire artifact | **unreachable** |
| `PARTIAL` | Validator succeeded over a bounded prefix or region; the missing extent is known and recorded | **unreachable** |
| `RECONSTRUCTED` | Reconstruction method and its inputs recorded; validator result recorded separately | reachable, and universal |
| `UNVERIFIED` | Extraction succeeded; no validator exists for the format, or validation could not run | **misdescribes** |
| `UNRECOVERABLE` | Basis for the determination recorded | **unreachable** |

**3.1 `STRUCTURALLY_VALID`.** No format-aware validator exists. M9 is the
last of `ADR-0002` §8's nine milestones and none is scheduled. The level
cannot be assigned by any code path that exists or is planned.

**3.2 `PARTIAL`.** `extract` reads the whole declared size or the run is
refused before any content is read. No bounded-prefix extraction exists, so
no artifact of this shape can be produced.

**3.3 `UNRECOVERABLE`.** `ADR-0003` §3.1 makes a level a property of an
Artifact, and an Artifact is the Validator's output. A refused run produces
no Candidate and therefore never reaches the Validator. `SAFETY.md` §14
closes it independently: confidence classifies a recovered artifact, and
does not classify an operation or a structural finding such as the
identification of a deleted directory entry. A refused entry is a structural
finding.

**3.4 `UNVERIFIED`.** Its evidence column is that no validator exists for
the format, or validation could not run. M8's `NotAttempted` means no
reference was supplied. Validation could have run; nobody asked it to.
Assigning `UNVERIFIED` there would state something untrue about the run.

**3.5 The gap.** A comparison that ran and refuted — M8's `Differs` — has no
level. §4.4's fail-closed default resolves a tie between two applicable
levels; here none applies.

**3.6 Why.** `ADR-0003` §1 records the taxonomy's provenance. The six terms
came from `SAFETY.md` §14 and `PROJECT.md` §6.9, which a documentation audit
found to be the same taxonomy under different names. `ADR-0003`'s task was
to reconcile three incompatible models, and §2's separation of pipeline
state from evidence strength is correct and has held. But the list of levels
was adopted rather than derived, before any pipeline existed. §8 anticipated
half of this by reserving implementation; the speculation was in the list
itself.

---

## 4. Decisions

| | Decision |
| --- | --- |
| A | The classification states method; the comparison result is reported beside it |
| B | Entries that produce no Artifact carry no level |
| C | M9 defines the operation-status axis and records that it is defining it |
| D | Aggregate reporting counts levels, outcomes, and what was never classified |
| E | Unreachable levels are recorded as unreachable, not implemented |

---

## 5. Decision A: the classification states method

**Every artifact this tool produces is classified `RECONSTRUCTED`. A digest
match is reported on the outcome axis and does not promote the level.**

`ADR-0003` §4.2 says a level may only be raised by evidence satisfying the
higher level's requirement, and a match does satisfy `VERIFIED`'s. §4.5 says
`RECONSTRUCTED` describes the method used, not a position between `PARTIAL`
and `UNVERIFIED`, and that a reconstructed artifact records its validator
result separately from its classification. Both cannot govern: a level
cannot be raised out of a category the same ADR excludes from the ordering.

§4.5 governs, for four reasons.

**5.1 Every extraction infers its extent.** EXP-0003 measured that deletion
zeroes the cluster chain in every FAT. The run is computed from a first
cluster and a declared size; contiguity is assumed, never read. `ADR-0013`
Appendix A records that the CFTT specification's term for this is
*estimating content* — recovering beyond what residual metadata explicitly
identifies. That is §3.2's definition of `RECONSTRUCTED`, and it is true of
every recovery the tool performs.

**5.2 A match does not change the method.** It establishes that the
inference was correct for that entry. `ADR-0003` §4.6 and `SAFETY.md` §10
already bound what a match establishes, and `ADR-0013` Decision D bounds it
further where content is not distinctive.

**5.3 The tool cannot discriminate.** EXP-0004 Appendix A.2 measured five
deleted entries on one volume: three recover content that is not their own
file's, two recover correctly, and the tool reports all five identically.
The two that are correct are correct because nothing reused their clusters,
which is not a property of the method. A level that varied would assert a
discrimination the tool measurably does not have.

**5.4 The varying version exists and fails.** Recuva publishes a per-file
recovery state computed from the same evidence this tool has: its
`Excellent` means no clusters are overwritten by a live file, which is this
tool's *every cluster free*. Independent testing reports the indicator
unreliable in both directions, with files marked `Excellent` recovering
corrupt and files marked `Unrecoverable` recovering intact. Meyer and Roy
measured the mechanism: on a fragmented deleted file whose intervening
clusters are unallocated, Recuva is among the tools that recover data
belonging to other files. That is the case EXP-0004 fixtures.

**5.5 `SECURITY.md` §31, left unexercised deliberately.** §31 states that
validation is required before a result is classified as verified. That is a
conditional ordering constraint and not a requirement that `VERIFIED` be
assigned; it is satisfied where nothing is so classified. Under this
decision a match is reported only after validation has run, so the ordering
holds in substance. The phrase is recorded here as considered and not
overlooked.

§31's list of what a recovered file may contain includes *remnants from
another file*. EXP-0004 Appendix A.2 is the first measurement of that
condition in this tree.

**5.6 Consequence, stated plainly.** The level is constant. §4.7's counts
per level will report every artifact under one name. A field that never
varies carries no information, and that is the correct outcome: the
information is in the outcome axis and in what carries no level at all. A
level that varied would be the misleading version.

---

## 6. Decision B: entries that produce no Artifact carry no level

**No confidence level is assigned to an entry that is refused, ineligible,
or whose assessment or extraction failed.**

`ADR-0003` §3.1 makes a level a property of an Artifact. `SAFETY.md` §14
forbids classifying a structural finding such as the identification of a
deleted directory entry. `Assessment::Ineligible`, `Assessment::RunBroken`,
and the error arms of `assess` and `extract` all terminate before a
Candidate exists.

These entries are reported, and what is reported about them is unchanged by
this milestone. `ADR-0010` Decision C's named refusal stands.

This is a non-conformance with the CFTT specification, which requires a
Recovered Object for each deleted file system object accessible in residual
metadata. `ADR-0013` Appendix A already records it. This decision does not
introduce it and does not resolve it.

---

## 7. Decision C: M9 defines the operation-status axis

**M9 defines the operation-status vocabulary. It is not inherited from
`SAFETY.md` §13.**

`SAFETY.md` §13 is titled Partial Results. It requires that partial recovery
be explicitly represented, illustrates the requirement with two strings, and
states that a partially recovered file must not be presented as a complete
verified file. It defines no status set and names no failure status. `main.rs`
emits no status line; the only `SUCCESS` in it is `ExitCode::SUCCESS`.

Three documents nonetheless attribute a vocabulary to it: `ADR-0003` §5
refers to §13's `SUCCESS` / `PARTIAL` / failure statuses, `ADR-0013` §3.2
refers to §13's statuses, and the M9 handover carries the same reading
forward. The attribution is a pattern rather than a slip, and it is recorded
here so that M9 does not become its fourth instance.

The axis M9 defines:

* A run that completed reports `SUCCESS`, whatever its artifacts are
  classified and whatever its comparisons found. `ADR-0013` §3 settled that
  a differing digest is a finding and not a failed operation, and §3.1
  scoped `SAFETY.md` §12's `Hash mismatch` to source-versus-image integrity.
* A run in which any entry could not be assessed or extracted reports
  `PARTIAL`, which is the condition `SAFETY.md` §13 exists to require be
  explicitly represented.
* Fail-closed conditions are unchanged. `ADR-0013` §3.4's malformed
  reference digest remains the only one in this path, and it halts before
  evidence is read.

The two axes are never merged, per `ADR-0003` §5 and `SAFETY.md` §14.

---

## 8. Decision D: what aggregate reporting counts

**A run reports counts per level, counts per outcome, and a count of entries
that carry no level at all.**

`ADR-0003` §4.7 requires counts per level, forbids a single combined success
figure, and forbids describing a session as successful where any artifact is
below `STRUCTURALLY_VALID` without stating the distribution. Under Decision
E `STRUCTURALLY_VALID` is unreachable, so every artifact is below it and the
distribution is always stated.

The third count is the one the milestone most needs and the one nothing
currently supports: a run-level statement that some entries were never
classified, because they were refused, ineligible or errored. Without it a
reader of the per-level counts cannot tell how much of the volume they
describe.

This cannot be built on the current reporting path. `report_recovery`
returns a `Caveats` of three booleans, `report_partition` returns `()`, and
`print_caveats` runs per volume while §4.7 requires per session. Threading
return values through `report_recovery` → `report_partition` → `inspect` is
a prerequisite commit and carries no classification.

---

## 9. Decision E: unreachable levels are recorded, not implemented

**`STRUCTURALLY_VALID`, `PARTIAL` and `UNRECOVERABLE` are not implemented.
`UNVERIFIED` is not assigned.**

`ADR-0003` §8's reasoning applies to the variants as it applied to the enum:
implementing a level no code path can produce is speculative. A variant that
cannot be constructed is dead code that invites a future reader to construct
it wrongly, which is what `UNRECOVERABLE` would invite on a refused entry in
breach of `SAFETY.md` §14.

`ADR-0003` receives an appendix recording §3's findings, so that its
taxonomy and this tool's reachable subset are reconcilable by a reader who
has only the ADR series. Writing it is an action of this record.

This decision is a statement about this tool at this milestone, not a
rejection of the taxonomy. `STRUCTURALLY_VALID` becomes reachable on the
first format-aware validator. `PARTIAL` becomes reachable on the first
bounded extraction. Either would reopen Decision A, and §11 records that as
the review trigger.

---

## 10. What M9 does not do

* It does not write a recovered file. No output path exists and none is
  introduced here.
* It does not change what any entry currently reports about itself.
* It does not add a level that varies with the outcome. Decision A.
* It does not resolve the CFTT non-conformances recorded in `ADR-0013`
  Appendix A and EXP-0004's external practice section.
* It does not measure any fragmentation arrangement beyond the one EXP-0004
  built.

---

## 11. Review trigger

This record is reopened if any of the following becomes true:

* a format-aware validator is built, making `STRUCTURALLY_VALID` reachable;
* a bounded or partial extraction is built, making `PARTIAL` reachable;
* a means is found to establish a deleted file's extent from surviving
  evidence rather than assumption, which would make the classification vary
  on something measured;
* `ADR-0003` is amended in a way that changes §4.5.

The first three would each give the level something real to vary on, and
Decision A exists only because none of them does.

---

## 12. Implementation order

1. Thread return values through the reporting path. No classification.
2. Introduce the classification and assign it. Decisions A, B, E.
3. Operation status. Decision C.
4. Aggregate reporting. Decision D.
5. Tests, including that no entry under Decision B carries a level.
6. `ADR-0003` appendix; `README.md`, `CHANGELOG.md` and `PROJECT.md` §6.9
   reviewed against what was built.

---

## 13. Consequences

**Positive.** The tool states how it obtained every byte it reports, and
declines to assert a discrimination it has been measured not to have. The
two axes `ADR-0003` §5 separates stay separated in code. A reader of the
run-level counts can tell what fraction of a volume they cover.

**Negative.** The headline feature of `ADR-0002` §8's final milestone is a
constant. A reader expecting a varying confidence indicator will find the
information elsewhere in the output, and the ADR series must be read to
understand why. Three of six levels exist in a governing ADR with no code
behind them, which is a discrepancy a future maintainer will have to
re-derive if `ADR-0003`'s appendix is not written.

---

## 14. Open after this decision

* Whether `PROJECT.md` §6.9 and `ARCHITECTURE.md` §36 need amendment once
  the classification exists in code. Not assessed here.
* Whether the constant level should appear in output at all, or only in the
  aggregate. Decided during implementation, recorded by appendix.
* The sixteen CFTT test cases EXP-0004 does not cover.

---

## Appendix A: M9 as built, and three corrections to Decision C (2026-09-16)

Steps 1 and 2 of §12 are complete. Preparing step 3 required auditing
Decision C against the tool and measuring what it would fire on, which
found the decision mis-justified and its trigger set empty. §5's Decision A
and §9's Decision E are unaffected and were implemented as written.

The body is not rewritten.

---

### A.1 Coverage gaps, measured

Every fixture was run with `--recover` at `41df92c`, with `stdout` and
`stderr` captured separately.

| Gap | Images | Stream |
| --- | --- | --- |
| MBR table rejected, nothing analysed | `bad-signature`, `no-signature`, `partition-beyond-end` | `stderr` only |
| GPT detected, unsupported | `gpt-protective` | `stdout` |
| Boot sector rejected, volume not analysed | `fat32-bad-root-cluster`, `fat32-oversized-volume`, `fat32-undersized-fat` | `stdout` |
| Filesystem not identified, volume not analysed | `mbr-four-partitions`, all four partitions | `stdout` |
| An entry could not be assessed or extracted | none | — |

Eight of nineteen images carry a gap. Ten are analysed end to end.
`mbr-empty.img` is neither: it declares no partition entries, so nothing
was missed and nothing was uncovered.

---

### A.2 Decision C's trigger set is exercised by nothing

§7 fires the `PARTIAL` status on a run in which any entry could not be
assessed or extracted. That is the last row of the table above, and no
fixture produces it. It is the narrowest gap the tool can have and the
only one the fixture set does not contain.

Meanwhile the four kinds that do occur are not named by Decision C at all,
including three whole-image failures and one whole-image refusal.

The trigger set is therefore replaced. §A.7 states the new one.

---

### A.3 The attribution to `SAFETY.md` §13 is unsupported

§7 states that the `PARTIAL` status is the condition `SAFETY.md` §13
exists to require be explicitly represented. §13 in full is four
sentences: that partial recovery is allowed but must be explicitly
represented, two example `Status:` strings, and that a partially recovered
file must not be presented as a complete verified file.

It states no purpose. §7's claim about what §13 exists to do is the
record's own reading presented as the document's content.

§13 is also ambiguous. "Partial recovery" reads either as a run that
partly succeeded, which is coverage, or as recovery of part of a file,
which this tool never performs: §3.2 records that `extract` reads the whole
declared size or the run is refused before any content is read. Its only
unambiguous sentence concerns a file.

This appendix does not resolve that ambiguity and does not need to. §13 is
not the warrant for the axis. §A.4 is.

That §7 over-attributed to §13 is worth recording plainly, because §7's own
subject is that three documents attribute a vocabulary to §13 which §13
does not define. It became the fourth.

---

### A.4 What does warrant the axis

**`SAFETY.md` §14, which is unambiguous and constraining.** It states that
confidence classifies a recovered artifact, does not classify an operation,
and does not classify a structural finding such as the identification of a
deleted directory entry. Forbidding confidence from classifying an
operation presupposes that operations are classified. On a reading where
nothing classifies them, §14 reserves a job no part of the tool performs.

`ADR-0011` §3 makes `SAFETY.md` a constraining document, which is not
updated to match an implementation that violates it. So the axis is not
M9's to decline.

§14 sources that sentence to `ADR-0003` §5, and §5 attributes the
vocabulary to `SAFETY.md` §13, which defines none. The chain is circular
and terminates in an illustration. The presupposition is real; no document
supplies the vocabulary. Decision C was correct that M9 defines it.

**A measured harm in not having it.** Failures are split across streams.
`BOOT SECTOR REJECTED`, the GPT line and an unidentified filesystem print
to `stdout`; an MBR parse failure, a sector read failure and a partition
read failure print to `stderr`. `inspect` returns `Ok(())` in every one of
those cases and `main` maps that to `ExitCode::SUCCESS`
(`src/main.rs:107`), so the process exits zero.

An operator capturing `stdout` alone therefore receives, for
`bad-signature.img`, `no-signature.img` and `partition-beyond-end.img`, a
report in which nothing indicates that no partition was analysed, from a
process that succeeded. A status on `stdout` is the only element that would
surface it.

---

### A.5 An anomaly is not a gap

`mbr-type-mismatch.img` prints `MISMATCH: declared type 0x07 disagrees with
observed filesystem FAT32` and then analyses its volume completely.
`fat32-hidden-mismatch.img` behaves the same way. Both are findings about
evidence that *was* covered.

A status that fired on them would merge a finding about content with a
statement about coverage, which is the merging `SAFETY.md` §14 and
`ADR-0003` §5 forbid. Neither is a gap and neither affects the status.

---

### A.6 Coverage states the tool's reach, not what was missed

`mbr-four-partitions.img` was expected, from its declared partition types,
to contain two unsupported Linux volumes. Measured, all four of its
partitions report `no boot signature: found 0000, expected 55aa`,
including both `0x0c` entries: none was ever formatted.

So the true statement is that the tool could not identify what is in those
extents, not that it does not support what is there. From ground truth they
are empty, and nothing in the evidence tells the tool that.

A coverage statement therefore reports what the run did not analyse. It
must not imply anything about what was there, because the tool cannot know.
`mbr-empty.img` is the control that keeps the two apart: nothing found,
nothing uncovered, and any status other than a successful one would be
wrong.

---

### A.7 The trigger set, replacing §7's

A run has a coverage gap where any of the following occurred:

1. the partition table could not be parsed, so no partition was analysed;
2. a partition table was parsed but is GPT, which is unsupported;
3. a partition's first sector could not be read;
4. a partition's filesystem could not be identified, or was identified as
   one this tool does not analyse;
5. a FAT32 boot sector was rejected;
6. a root directory could not be enumerated;
7. an entry's assessment or extraction failed.

Kinds 6 and 7 are unexercised: no fixture produces either. A test for
them requires fixtures that do not exist, and building them is not this
milestone's work. They are included because the code paths exist and a
status that ignored them would be wrong on evidence the tool may meet.

`Assessment::Ineligible` and `Assessment::RunBroken` are **not** gaps. Both
are reported refusals with a stated basis, which `ADR-0010` Decision C
treats as correct behaviour, and both concern one entry rather than the
run's reach. Decision B already denies them a confidence level; this
appendix denies them a status effect for the same reason.

`Ok(None)` from `assess` is not a gap. The entry is not a deleted file and
nothing about it was skipped.

---

### A.8 The status is derived, not tracked

The status is computed from the coverage counts rather than maintained
alongside them, so that the two cannot disagree. Where the count of
uncovered partitions and entries is zero the run reports success; otherwise
it reports a gap and the counts state its extent.

This follows CFTT's disk imaging practice, where an acquisition over a
drive with unreadable sectors is reported as *qualified* rather than equal,
with the count and location of what could not be read recorded alongside.
The qualification is the enumeration; the word is its summary.

---

### A.9 §12's steps 3 and 4 become one

§12 listed the operation status and the aggregate reporting as separate
steps. Under §A.8 the status is a function of the counts, so building it
before them would mean building a status with nothing to derive from. They
are one commit.

---

### A.10 Steps 1 and 2 as built

**Step 1** (`4cc673c`) was listed as threading return values through the
reporting path. It also moved `print_caveats` from `report_root_directory`
to `inspect` and reduced its indentation to zero, because a block printed
once per run at volume indentation would read as a statement about the last
volume printed. `ADR-0013` Appendix C.6's measurement was preserved: a run
over `fat32-recover-run.img` still emits the free-run paragraph once.

**Step 2** (`41df92c`) implemented Decisions A and E as
`src/confidence.rs`. It is filesystem-independent and parallel to
`validation`, rather than inside `fat_recovery`, which is FAT-specific and
which exFAT and NTFS would then have to import a general concept from.
`classify` was rejected as a name: `src/fat_directory.rs:492` already
declares `pub fn classify` for deciding an entry's kind, and the word
appears 34 times in the crate with that meaning.

No derivation function was written. Under Decision A the level follows from
the method, which is fixed, so a function would take no argument that could
change its answer. `Confidence::of_inferred_run` is an associated constant
whose name states the premise instead.

**§14's open question on output is decided: the level appears in the
aggregate only, not on each entry.** Each entry already states the
inference in concrete terms — its run, its extent and every cluster's
allocation state — so a constant word per line adds a label rather than a
qualification. Warning research finds attention to a repeated identical
stimulus falling measurably after two or three exposures, and names the
failure mode of a uniform label as protecting the author rather than
informing the reader. That research concerns warnings meant to change
behaviour rather than data fields, so it is recorded as a reason for
caution and not as proof. The decisive point is the first: the per-entry
lines are already qualified.

---

### A.11 External practice recorded, changing no decision

Researched while auditing Decision C. None of it bears on M9, and it is
recorded here so it is not lost.

Deleted-file recovery tools are scored on precision, recall and F1 against
NIST CFTT ground truth, per test case, with the mean F1 across suites
forming an overall figure; `AutoDFBench 1.0` implements this over 63 test
cases. These are validation-time measures computed against known ground
truth, not runtime output, so they belong in this project's test suite and
experiment records rather than in anything M9 prints.

Three gaps against that frame:

* Taphonomy parses no directory entry timestamps, so the MAC-times
  parameter is unmeasurable against it rather than measured badly.
* The fragmentation test cases score how many complete file blocks match
  ground truth. This tool hashes the whole declared size and emits one
  binary verdict, so on `fat32-fragmented-deleted.img` it reports only that
  the digest differs, where two of three clusters are in fact the file's.
  Per-cluster digests would make the tool measurable on those cases.
* One test case of seventeen is built, so no score can be computed.

---

### A.12 What this appendix does not change

Decisions A, B, D and E stand as written. Decision C's axis stands; its
justification, its trigger set and its silence on anomalies are corrected
above. §11's review triggers are unaffected. No decision here concerns
what the tool recovers, refuses or validates.
