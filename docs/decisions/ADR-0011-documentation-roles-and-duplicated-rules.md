# ADR-0011: Documentation Roles and Duplicated Rules

* **Status:** Proposed
* **Date:** 2026-09-07
* **Decision owners:** Taphonomy project
* **Scope:** What kind of document each tracked document is, which of them
  binds the code and which follows it, and whether a rule stated in several
  documents is a defect
* **Related:** `docs/development/DOCUMENTATION_AUDIT.md` §3, §10 item 4,
  §11 item 4; `docs/PROJECT.md` §6.1, §6.9, §11; `docs/SAFETY.md` §4.1;
  `docs/ARCHITECTURE.md` §1 status notice, §46; `CLAUDE.md` §1, §22;
  ADR-0002 §8; ADR-0003 §8

---

## 1. Context

`DOCUMENTATION_AUDIT.md` was written on 2026-08-29, against nine non-empty
documents, one ADR, and no source code. Its §10 item 4 asked the project to
"decide a single owner document for each duplicated rule family". §11
deferred that item "until after the first recovery milestone".

M7 completed at `529f763`. The deferral has expired.

The audit's own tables are a dated record of that day and are not the basis
for this decision. The duplication was re-derived at `334dc7c` across all 23
tracked markdown files, and the measurement disagrees with the 2026-08-29
snapshot in both size and kind.

### 1.1 What was measured

**The duplication is larger.** Evidence immutability is stated in 18
sentences across 9 files, not the 7 documents the earlier audit listed.
Thirteen rule families are stated normatively in three or more files, not
four. `docs/development/DEVELOPMENT_ENVIRONMENT.md` contributes four
statements to the evidence-immutability family and was not cited in the
2026-08-29 table at all, despite existing that day.

**Most of it is not duplication.** The 18 statements do not bind one
subject. They divide three ways, and the same division holds for the
network, dependency, malformed-input, device-path and unverified-claim
families:

* the tool, as in `docs/SAFETY.md` "Taphonomy must never assume that a
  device is safe" and `docs/PROJECT.md` §6.2 "Operations against source
  media must default to read-only behavior"
* a coding agent, as in `CLAUDE.md` §3 "Never write recovered output to the
  source", binding because `CLAUDE.md` §1 states its audience
* development activity, undifferentiated, as in `README.md` "Normal
  development must not"

Two sentences that read alike constrain different things: what the shipped
software does, and what an agent may introduce while writing it.

**The subject is stated in one family of thirteen.** Only commit authority
names human and agent explicitly in every statement. Everywhere else the
subject is recoverable only by knowing a document's audience, and
`CLAUDE.md` §1 is the only place any document declares one.

### 1.2 An error in the finding this decision closes

The 2026-08-29 audit gave one example of modal drift: `PROJECT.md` §6.1
saying "should" and "whenever possible" against `SAFETY.md` §7 saying "must
never".

Those are two different rules. §6.1 is about treating the source as
immutable. §7 is about not writing recovered output onto the source. The
comparison establishes nothing.

A genuine contradiction does exist, between §6.1 and `docs/SAFETY.md` §4.1:

> The original source should be treated as immutable evidence whenever
> possible.

> The original source must be treated as immutable whenever technically
> possible.

Same rule, same subject, same qualifier, different obligation. Decision D
resolves it.

The dated record is not edited. This section records the correction.

### 1.3 The distinction that has been operating unwritten

The documents were written before any source code existed. Some of them
describe what the code does, and some of them constrain what it may do. The
two behave oppositely when they disagree with the code, and nothing states
which is which.

The consequence is not hypothetical. `ADR-0010` §3.3 cites
`docs/ARCHITECTURE.md` §18's Output Writer in support of a decision, without
noting that `docs/ARCHITECTURE.md` §1 declares the whole document non-binding
until corresponding code exists. The use was legitimate, because it was
describing what building one would entail rather than invoking a
requirement, but nothing in the tree marks the difference.

---

## 2. Decisions

**A.** Every tracked document has exactly one of four roles, and the role
determines what happens when the document and the code disagree.

**B.** Each tracked document is classified, by name, with the two documents
that hold sections of more than one role named as such.

**C.** A rule stated in several documents is not a defect where the
statements bind different subjects. The project does not deduplicate them.

**D.** `docs/PROJECT.md` §6.1 is aligned to `docs/SAFETY.md` §4.1.

**E.** Where a rule is a definition rather than an obligation, one document
defines it and the others cite it. This records existing practice.

**F.** Declaring each document's audience is considered and deferred, with a
stated trigger.

---

## 3. Decision A: four roles

**Constraining.** States what the code, or a person or agent working on the
repository, must do. When it disagrees with the code, **the code is wrong**.
A constraining document is not updated to match an implementation that
violates it.

**Descriptive.** States what exists. When it disagrees with the code, **the
document is wrong** and is updated. `README.md`'s status and
`docs/PROJECT.md` §11 are of this kind.

**Dated record.** States what was true on a given date. Its findings are
never rewritten. Later developments are recorded alongside them, and the
tree already does this three ways: `docs/development/RESEARCH_LOG.md` §8
annotates each closed question in place while retaining its text,
`docs/development/DOCUMENTATION_AUDIT.md` uses a separate resolution table
in its §11, and an ADR uses a numbered appendix. The mechanism varies; the
principle does not. `docs/development/EXPERIMENTS.md`,
`docs/development/RESEARCH_LOG.md`,
`docs/development/DOCUMENTATION_AUDIT.md` and every ADR body are of this
kind.

**Intent.** Describes a system that does not exist. It binds nothing until
corresponding code exists. `docs/ARCHITECTURE.md` already declares itself
this way in its own §1 and §46; this decision names the category rather than
creating it.

The distinction that matters most is the first against the second. A safety
rule that yields to the code it constrains is not a rule. A status paragraph
that outlives the code it describes is a false claim. Both failures look
like "the document and the code disagree", and they require opposite
responses.

---

## 4. Decision B: the classification

| Document | Role |
|---|---|
| `README.md` | Descriptive; its rules restate ones owned elsewhere |
| `CLAUDE.md` | Constraining, on a coding agent |
| `CONTRIBUTING.md` | Constraining, on a contributor |
| `SECURITY.md` | Constraining, on the tool |
| `docs/PROJECT.md` | Constraining, except §11 which is descriptive |
| `docs/SAFETY.md` | Constraining, on the tool |
| `docs/ARCHITECTURE.md` | Intent |
| `docs/decisions/ADR-*` | Dated record; their decisions are constraining |
| `docs/development/DEVELOPMENT_ENVIRONMENT.md` | Constraining |
| `docs/development/CHANGELOG.md` | Descriptive |
| `docs/development/KNOWN_ISSUES.md` | Descriptive |
| `docs/development/EXPERIMENTS.md` | Dated record |
| `docs/development/RESEARCH_LOG.md` | Dated record |
| `docs/development/DOCUMENTATION_AUDIT.md` | Dated record |

`docs/PROJECT.md` holds sections of two roles and is named above rather than
forced into one box: §6 and §7 constrain, §11 describes.

`README.md` is a different case. It carries normative statements in at least
seven sections, at lines 92, 117, 121, 127, 136, 144, 169, 173 and 236, so
calling it purely descriptive would be false. But every one of them restates
a rule owned by `docs/SAFETY.md`, `SECURITY.md` or
`docs/development/DEVELOPMENT_ENVIRONMENT.md`, for a reader arriving at the
repository. It is therefore classified as descriptive with restatements, and
the practical consequence is the one that matters: where `README.md` and a
constraining document disagree, the constraining document governs and
`README.md` is corrected.

An ADR is both: its body is a dated record of a decision made on a day, and
the decision it records constrains the code until superseded. That is why an
ADR is corrected by appendix and never by rewriting, and why a superseded
ADR keeps its text.

---

## 5. Decision C: duplication across subjects is not a defect

The 2026-08-29 audit asked for a single owner per rule family. This decision
declines, and the reason is the measurement in §1.1.

Deleting `CLAUDE.md` §3's "Never write recovered output to the source" in
favour of a reference to `docs/SAFETY.md` would remove a rule from the
document an agent reads at the moment it might break it, and would replace a
rule about agent conduct with a citation of a rule about tool behaviour.
Those are not the same rule.

**The test is the subject, not the wording.** Two normative statements of
one rule are permitted where they bind different subjects. Where they bind
the same subject, they must state the same obligation at the same strength,
and a difference is a defect to be fixed rather than a duplication to be
tolerated.

`DOCUMENTATION_AUDIT.md` §10 item 4 is closed by this decision, not by
implementing what it asked for.

---

## 6. Decision D: `PROJECT.md` §6.1 is aligned

The first sentence of `docs/PROJECT.md` §6.1 changes from "should be treated
as immutable evidence whenever possible" to "must be treated as immutable
evidence whenever technically possible", matching `docs/SAFETY.md` §4.1.

Both bind the tool. §6.1 is the outlier: `docs/PROJECT.md` §6.2, immediately
below it, already says "must default to read-only behavior", so the section
is weaker than its own neighbour.

The qualifier is kept. `docs/SAFETY.md` §4.1 has it, and removing it here
would create the same divergence in the other direction.

The second sentence of §6.1, "Analysis and recovery should operate against a
verified image or other controlled copy", is not changed. `docs/SAFETY.md`
§4.2 states it as "Taphonomy must prefer analysis of a verified image over
direct analysis of the original physical device", which is a preference
rather than a prohibition, so "should" is the correct strength for it.

---

## 7. Decision E: definitions have one owner

Where a rule is a definition rather than an obligation, one document defines
it and the others cite it.

This is recorded rather than introduced. `docs/PROJECT.md` §6.9 already
removed a six-term confidence list in favour of a citation of `ADR-0003`,
saying it was removed "so that the levels have one definition". §11 does the
same for the safety and development contracts.

The distinction from Decision C: an obligation can be owed by two different
subjects and stated twice. A definition has one referent, and two
definitions of one term are a defect however the subjects differ.

---

## 8. Decision F: audience declarations are deferred

Twelve of thirteen rule families leave the subject to be inferred. Adding one
sentence to each constraining document, declaring whom its normative
statements bind, would make the subject stated.

Two documents already do something of this kind. `CLAUDE.md` §1 states that
its instructions "apply to Claude Code and any other coding agent operating
in this repository", which names an actor. `CONTRIBUTING.md` states that it
"records the standards that apply to all work in this repository", which
names a scope rather than an actor. No other document does either.

It is deferred, because the prevalence is measured and the harm is not.

The evidence points the other way, in fact. Commit authority is the one
family whose subject is named explicitly in every statement, and it is also
the family that produced the 2026-08-29 audit's finding 4.1, a genuine
contradiction resolved at `2af499a`. Explicit subjects did not prevent it.
No instance of subject confusion causing an error in this project has been
found.

**Trigger.** The first measured instance of a rule being applied to the
wrong subject, or a second document acquiring an audience distinct from the
tool's.

---

## 9. Consequences

### Positive

The precedence rule that has been applied by hand all project is written
down, so a future session does not have to infer it. A safety rule can no
longer be quietly weakened to match an implementation that breaks it.

The 2026-08-29 audit's last open item closes, with the reason it was not
implemented recorded alongside it.

One real contradiction is removed, and the example the earlier audit gave
for it is corrected without editing the dated record that contains it.

`docs/ARCHITECTURE.md`'s existing self-declaration gains a name, so citing it
in an ADR is visibly a citation of intent.

### Negative

The classification table is a document about documents, and it is one more
thing that can go stale. A document that changes role, or a new document
that is not added to it, makes it wrong.

Two documents are classified by exception rather than cleanly. That is
honest about their contents and it means the table cannot be applied
mechanically.

Deciding not to deduplicate leaves 18 statements of evidence immutability in
the tree. A reader who does not know Decision C will read that as a defect,
as the 2026-08-29 audit did.

Decision F leaves twelve families with unstated subjects, which is a real
gap accepted deliberately rather than a gap that has been closed.

---

## 10. Review trigger

Revisit Decision B when a document is added, removed, or changes role.

Revisit Decision C if two statements binding the same subject are found to
disagree, which under this decision is a defect and not a duplication.

Revisit Decision F on its stated trigger.

Revisit Decision A if a case arises where a constraining document and the
code disagree and the document turns out to be the thing that is wrong. That
case is not impossible: a rule written before any code existed may prove
unimplementable. The correct response is to change the rule deliberately,
with a decision recorded, rather than to let the code silently redefine it.

---

## 11. Open after this decision

1. **`docs/ARCHITECTURE.md` holds 135 `must` statements**, the highest count
   of any file, all currently non-binding by its own §1. When code exists
   for a section, that section becomes binding, and nothing tracks which
   sections have crossed that line. The Output Writer of §18 is the first
   that will.

2. **`docs/development/DOCUMENTATION_AUDIT.md` §11 item 4** should record
   this ADR as its resolution. That is an append to its resolution table,
   which its role permits.

3. **The audience declarations of Decision F** remain undone, with their
   trigger stated.
