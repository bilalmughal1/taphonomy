# ADR-0003: Recovery Confidence Model

* **Status:** Accepted
* **Date:** 2026-08-29
* **Decision owners:** Taphonomy project
* **Scope:** Classification of recovery results and the pipeline stage that assigns it
* **Supersedes:** `SAFETY.md` §14 and `PROJECT.md` §6.9 as normative sources
* **Reconciles:** `ARCHITECTURE.md` §36

---

## 1. Context

`DOCUMENTATION_AUDIT.md` §4.2 identified three incompatible models for
expressing recovery certainty.

| Source | Model |
|---|---|
| `SAFETY.md` §14 | `VERIFIED`, `HIGH_CONFIDENCE`, `PARTIAL`, `RECONSTRUCTED`, `UNVERIFIED`, `UNRECOVERABLE` |
| `PROJECT.md` §6.9 | verified recovery, probable recovery, partial recovery, reconstructed data, unverified extraction, unrecoverable evidence |
| `ARCHITECTURE.md` §36 | `Candidate` vs `Verified Artifact` |

The first two are the same taxonomy under different names. The third is not a
taxonomy at all.

`SAFETY.md` §14 states that the exact confidence model will be defined
separately before implementation. This ADR is that definition.

---

## 2. Root Cause

Two orthogonal concepts were being expressed on one axis.

**Pipeline state** answers: has this thing been through the validator yet?
This is what `ARCHITECTURE.md` §36 describes.

**Evidence strength** answers: how strong is the claim the evidence supports?
This is what `SAFETY.md` §14 and `PROJECT.md` §6.9 describe.

A result cannot have an evidence-strength classification before validation,
because the classification is the validator's output. Treating `Candidate` as
a peer of `VERIFIED` conflates a stage with a verdict.

---

## 3. Decision

### 3.1 Two-stage pipeline

```text
Recovery algorithm
        ↓
   Candidate            (no confidence level; not yet assessed)
        ↓
    Validator
        ↓
    Artifact            (carries exactly one confidence level)
```

A Candidate is a proposition. An Artifact is a proposition plus a verdict.

A Candidate must never be presented to a user as a recovered file, written to
the recovery output directory, or counted in a recovery result summary.

`ARCHITECTURE.md` §36's distinction is retained in full and is understood as
this pipeline boundary, not as a confidence classification.

### 3.2 Confidence taxonomy

Exactly one of the following is assigned to every Artifact.

| Level | Definition | Evidence required |
|---|---|---|
| `VERIFIED` | Recovered content is byte-identical to a known-good reference | Cryptographic hash match against a reference artifact |
| `STRUCTURALLY_VALID` | Content parses completely and consistently as its claimed format; no reference exists for comparison | Format-aware validator completed without error over the entire artifact |
| `PARTIAL` | Content is incomplete, and the portion recovered is structurally sound | Validator succeeded over a bounded prefix or region; the missing extent is known and recorded |
| `RECONSTRUCTED` | Content was assembled using inference or heuristics rather than surviving filesystem metadata | Reconstruction method and its inputs recorded; validator result recorded separately |
| `UNVERIFIED` | Bytes were extracted; no validation was possible | Extraction succeeded; no validator exists for the format, or validation could not run |
| `UNRECOVERABLE` | Evidence is insufficient to attempt recovery | Basis for the determination recorded |

### 3.3 Renaming of HIGH_CONFIDENCE

`SAFETY.md` §14's `HIGH_CONFIDENCE` is renamed `STRUCTURALLY_VALID`.

`SAFETY.md` §14 itself requires that confidence be based on measurable
evidence rather than subjective language. "High confidence" is subjective
language and states a degree of belief rather than an observation.
"Structurally valid" states what was measured.

`PROJECT.md` §6.9's "probable recovery" maps to the same level and is
likewise replaced.

---

## 4. Rules

### 4.1 Single level

An Artifact carries exactly one confidence level. Levels are not combined,
averaged, or expressed as ranges or percentages.

### 4.2 No upgrade without evidence

A level may only be raised by evidence that satisfies the higher level's
requirement. It may never be raised by absence of contrary evidence.

### 4.3 Downgrade is always permitted

Any level may be lowered when evidence is found to be weaker than assessed.

### 4.4 Default on uncertainty

When a validator cannot determine which of two levels applies, the lower
applies. This follows the fail-closed requirement of `SAFETY.md` §12.

### 4.5 RECONSTRUCTED is not a strength ordering

`RECONSTRUCTED` describes the method used, not a position between `PARTIAL`
and `UNVERIFIED`. A reconstructed artifact records its validator result
separately from its classification.

### 4.6 Hash semantics

`VERIFIED` establishes byte equality with a reference. It does not establish
that the artifact is the file the user intended to recover. This restates
`SAFETY.md` §10 and must not be weakened in user-facing output.

### 4.7 Aggregate reporting

A recovery session reports counts per level. It must not report a single
combined success figure, and must not describe a session as successful when
any artifact is below `STRUCTURALLY_VALID` without stating the distribution.

---

## 5. Relationship to Operation Status

Confidence classifies an artifact. `SAFETY.md` §13's `SUCCESS` / `PARTIAL` /
failure statuses classify an operation. These are separate axes and must not
be merged.

An operation may complete with status `SUCCESS` while producing artifacts
classified `UNVERIFIED`.

---

## 6. Consequences

### Positive

* One normative taxonomy across all documents
* `ARCHITECTURE.md` §36 is preserved rather than discarded
* Every level is defined by a measurable condition
* Fail-closed default is explicit

### Negative

* `SAFETY.md` §14 and `PROJECT.md` §6.9 require amendment to reference this ADR
* `STRUCTURALLY_VALID` requires a format-aware validator per supported format,
  which is more work than a magic-byte check
* Formats with no validator yield `UNVERIFIED`, which may appear pessimistic

---

## 7. Required Amendments

| Document | Change |
|---|---|
| `SAFETY.md` §14 | Replace the candidate list with a reference to this ADR |
| `PROJECT.md` §6.9 | Replace the six-term list with a reference to this ADR |
| `ARCHITECTURE.md` §36 | Add a note that Candidate/Artifact is the pipeline boundary defined here |

These amendments are documentation-only and carry no implementation
dependency.

---

## 8. Implementation Note

No part of this model is implemented. The taxonomy becomes binding on the
first code that assigns a confidence level, which under ADR-0002 §8 is
milestone M9.

Implementing the enum earlier than M9 would be speculative. The definition
exists now so that earlier milestones do not encode a conflicting model by
accident.
