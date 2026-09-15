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
