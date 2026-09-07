# Taphonomy Documentation Audit

**Date:** 2026-08-29
**Scope:** All tracked documentation
**Method:** Mechanical extraction plus manual review
**Status:** Findings only. No fixes applied, no files modified.

---

## 0. Basis and Limitations

This audit was performed against the document set as it stood before commits
`e81732c` through `c8c4e91`. Three findings arising from that snapshot have
already been resolved by those commits and are marked `RESOLVED` below.

`CONTRIBUTING.md`, `CHANGELOG.md`, `EXPERIMENTS.md` and `KNOWN_ISSUES.md` were
empty at time of audit and are reported as such.

Every finding cites document and section so it can be independently verified.

---

## 1. Inventory

| Document | Lines | Words | Purpose |
|---|---|---|---|
| `README.md` | 265 | 885 | Public entry point and project summary |
| `CLAUDE.md` | 952 | 2,359 | Coding-agent operational contract |
| `SECURITY.md` | 844 | 2,439 | Threat model and security requirements |
| `docs/PROJECT.md` | 351 | 1,280 | Project specification, scope, non-goals |
| `docs/SAFETY.md` | 698 | 2,152 | Evidence-preservation safety rules |
| `docs/ARCHITECTURE.md` | 1,141 | 2,534 | Intended system architecture |
| `docs/decisions/ADR-0001` | 670 | 1,942 | Core technology and evidence model |
| `docs/development/DEVELOPMENT_ENVIRONMENT.md` | 658 | 1,818 | Toolchain, quality gates, policy |
| `docs/development/RESEARCH_LOG.md` | 342 | 1,088 | Recovery-ecosystem research |
| `CONTRIBUTING.md` | 0 | 0 | **EMPTY** |
| `docs/development/CHANGELOG.md` | 0 | 0 | **EMPTY** |
| `docs/development/EXPERIMENTS.md` | 0 | 0 | **EMPTY** |
| `docs/development/KNOWN_ISSUES.md` | 0 | 0 | **EMPTY** |

**Total: 5,921 lines, 16,497 words across 9 non-empty documents.**

---

## 2. Normative Rule Density

Sentences containing `must`, `must not`, `should`, `should not`, `never`, or
`always`:

| Document | Count |
|---|---|
| `SECURITY.md` | 92 |
| `docs/ARCHITECTURE.md` | 91 |
| `docs/SAFETY.md` | 77 |
| `docs/development/DEVELOPMENT_ENVIRONMENT.md` | 73 |
| `CLAUDE.md` | 57 |
| `docs/development/RESEARCH_LOG.md` | 33 |
| `docs/PROJECT.md` | 19 |
| `ADR-0001` | 18 |
| `README.md` | 15 |

**Total: 475 normative statements governing 0 lines of source code.**

This is the headline finding. Every one of these is a constraint that future
code must satisfy, and none is currently enforced by a test, a lint, or a type.

---

## 3. Duplicated Rules

### 3.1 Evidence immutability / never write to source — **7 documents**

Status: **COMPATIBLE** (no contradiction, but no single source of truth)

| Location | Wording |
|---|---|
| `README.md` §Safety | "modify source evidence" listed under prohibited |
| `CLAUDE.md` §3 (L40, L42) | "Never write recovered output to the source." |
| `PROJECT.md` §6.1 (L166) | "should be treated as immutable evidence whenever possible" |
| `PROJECT.md` §6.2 (L172) | "must default to read-only behavior" |
| `SAFETY.md` §3.3 (L70) | "must never be written into the source evidence" |
| `SAFETY.md` §4.1 (L94) | "must be treated as immutable whenever technically possible" |
| `SAFETY.md` §5 (L116) | "All source-device operations must default to read-only" |
| `SAFETY.md` §7 (L165) | "must never be written onto the source device" |
| `ARCHITECTURE.md` §2 (L28) | "Read-only source handling." |
| `ADR-0001` §6 (L227) | "Read-only Evidence Boundary" |

Note the modal drift: `PROJECT.md` §6.1 says *should* and *whenever possible*;
`SAFETY.md` §7 says *must never*. These are not the same strength of rule.
The weaker wording in `PROJECT.md` is the one a reader meets first.

### 3.2 Network isolation and telemetry — **5 documents**

Status: **COMPATIBLE**

- `PROJECT.md` §6.8 (L211), `SAFETY.md` §19–20 (L420–436),
  `SECURITY.md` §22–23 (L455–484), `DEVELOPMENT_ENVIRONMENT.md` §20–21
  (L423–431), `ARCHITECTURE.md` §31 (L781–787)

`SAFETY.md` §20 and `SECURITY.md` §23 both define telemetry policy in full,
including near-identical "if telemetry is ever introduced it must be" lists.
One of these should reference the other.

### 3.3 Dependency and license policy — **3 documents**

Status: **COMPATIBLE**

- `DEVELOPMENT_ENVIRONMENT.md` §15–16, `CLAUDE.md` §16–17,
  `ARCHITECTURE.md` §41

### 3.4 Malformed-input / defensive parsing — **4 documents**

Status: **COMPATIBLE**

- `SAFETY.md` §21, `SECURITY.md` (multiple), `DEVELOPMENT_ENVIRONMENT.md` §18,
  `README.md` §Security

---

## 4. Contradictions

### 4.1 Agent commit authority — **DIVERGENT, action required**

`CLAUDE.md` §0.2 (added 2026-08-29):

> Coding agents must never run: `git commit` ...

But:

- `CLAUDE.md` §31 (L677) — the prescribed agent cycle ends `→ commit`
- `CLAUDE.md` §31 (L667) — "The agent should not commit automatically after
  every edit", which implies committing is otherwise permitted
- `DEVELOPMENT_ENVIRONMENT.md` §24 (L502) — agent cycle ends in `Commit`

Three statements grant an authority that §0.2 categorically removes. Because
§0.2 declares itself overriding, the document is self-resolving in principle,
but an agent reading §28 in isolation will act on it.

`CLAUDE.md` §29 ("Before **recommending** a commit") is already compatible and
needs no change.

### 4.2 Three different confidence models — **DIVERGENT**

| Location | Model |
|---|---|
| `SAFETY.md` §14 (L319–336) | Six values: `VERIFIED`, `HIGH_CONFIDENCE`, `PARTIAL`, `RECONSTRUCTED`, `UNVERIFIED`, `UNRECOVERABLE` |
| `PROJECT.md` §6.9 (L214–222) | Six values, different names: verified recovery, probable recovery, partial recovery, reconstructed data, unverified extraction, unrecoverable evidence |
| `ARCHITECTURE.md` §36 (L914) | Two values: `Candidate` vs `Verified Artifact` |

`PROJECT.md`'s "probable recovery" and `SAFETY.md`'s `HIGH_CONFIDENCE` appear
to be the same concept under two names. `ARCHITECTURE.md`'s binary model is a
different axis entirely and is not reconciled with either.

This must be one model before any code writes a status field.

### 4.3 SSD/HDD grouping

`PROJECT.md` §4.2 and §4.3 separate HDD and SSD into distinct domains on the
grounds that TRIM, garbage collection and wear levelling make them
technically different. `PROJECT.md` §4.1 groups USB/SD/microSD as one domain
despite those spanning both flash and controller behaviours. The partition
criterion is not stated consistently.

---

## 5. Unresolved Versus Decided

`RESEARCH_LOG.md` §8 lists 14 "important unknowns". At least four are
contradicted by decisions recorded elsewhere.

| `RESEARCH_LOG.md` §8 question | Contradicted by |
|---|---|
| Q1: "Which language should implement the core engine?" | `ADR-0001` — Rust, decided. `rust-toolchain.toml` committed. |
| Q3: "Should Taphonomy use or integrate The Sleuth Kit?" | `DEVELOPMENT_ENVIRONMENT.md` §16 excludes CPL/IPL/GPL from the dependency graph without a documented decision. TSK is CPL/GPL. Effectively pre-answered *no*. |
| Q4: "Which filesystem should be the first implementation?" | `ADR-0001` §8/§11 and `README.md` §Initial Recovery Scope — NTFS. |
| Q5: "Should the first capability be filesystem recovery or file carving?" | `ADR-0001` §11 — filesystem recovery. |

Q4 is the most consequential. The project's originating use cases are SD
cards, USB drives and phones, which are FAT32/exFAT/ext4. NTFS is the one
storage class not represented in the motivating incidents.

---

## 6. Claims About Things That Do Not Exist

### 6.1 Clearly aspirational (acceptable)

`ARCHITECTURE.md` is written almost entirely in "should" future tense and is
internally consistent about describing an intended system. Sections covering
device discovery, acquisition, plugin architecture, mobile acquisition,
reporting, and observability describe roughly 40 subsystems, none of which
exist.

### 6.2 Ambiguously present-tense (needs marking)

| Location | Text |
|---|---|
| `README.md` §Architecture | "The initial dependency direction **is**: CLI → Application → Domain → Infrastructure" — no such code exists |
| `README.md` §Evidence Model | "Taphonomy **distinguishes** between: Source Evidence → Working Evidence → ..." — present tense, nothing implemented |
| `README.md` §Development Environment | "The initial development environment **is**" — accurate |
| `PROJECT.md` §11 | "No recovery functionality is considered implemented" — accurate and well-stated |

`README.md` §Current Limitations does correctly disclaim the above. The
tension is between that section and the present-tense sections preceding it.

### 6.3 Directory structures described but not created

`ARCHITECTURE.md` describes a crate/module layout. `DEVELOPMENT_ENVIRONMENT.md`
§29 describes a Cargo workspace milestone. Neither `src/`, `tests/`,
`fixtures/`, `Cargo.toml` nor `Cargo.lock` exists.

---

## 7. Reference Integrity

| Reference | Occurrences | Target exists |
|---|---|---|
| `SECURITY.md` | 5 | Yes |
| `docs/SAFETY.md` | 4 | Yes |
| `docs/development/RESEARCH_LOG.md` | 4 | Yes |
| `docs/development/EXPERIMENTS.md` | 4 | Yes (empty) |
| `docs/PROJECT.md` | 2 | Yes |
| `docs/ARCHITECTURE.md` | 2 | Yes |
| `docs/development/KNOWN_ISSUES.md` | 1 | Yes (empty) |
| `docs/decisions/` | 1 | Yes |
| `CONTRIBUTING.md` | 1 | Yes (empty) |
| `Cargo.toml` | 1 | **No** |
| `Cargo.lock` | 2 | **No** |
| `rust-toolchain.toml` | 1 | Yes |
| `docs/adr/` | 1 | **No** — RESOLVED in `e81732c` |
| `AGENTS.md` | 3 | **No** — RESOLVED in `e81732c` / `9646699` |

`Cargo.toml` and `Cargo.lock` references are forward-looking and appropriate.

Two external URLs in `RESEARCH_LOG.md` (`docs/api-docs/4.15.0-develop/`,
`docs/user-docs/4.23.0/`) are fragments of Sleuth Kit and Autopsy URLs, not
internal paths. Not defects.

---

## 8. Structural Inconsistency

Markdown heading conventions differ between documents at the same structural
level:

| Document | `#` | `##` | `###` | Convention |
|---|---|---|---|---|
| `README.md` | 1 | 17 | 0 | Correct: single H1 title |
| `PROJECT.md` | 1 | 11 | 22 | Correct: single H1 title |
| `RESEARCH_LOG.md` | 5 | 7 | 22 | Mixed |
| `ADR-0001` | 23 | 6 | 19 | Numbered sections as H1 |
| `DEVELOPMENT_ENVIRONMENT.md` | 31 | 1 | 0 | Numbered sections as H1 |
| `SAFETY.md` | 34 | 6 | 4 | Numbered sections as H1 |
| `SECURITY.md` | 43 | 1 | 2 | Numbered sections as H1 |
| `ARCHITECTURE.md` | 46 | 8 | 10 | Numbered sections as H1 |
| `CLAUDE.md` | 48 | 1 | 0 | Numbered sections as H1 |

Seven of nine documents use `#` for numbered sections; two use `##`. This
produces the 49 markdownlint MD025 violations currently reported in VS Code.

`SAFETY.md` is internally inconsistent: sections 1 uses `##`, sections 2
onward use `#`.

---

## 9. Size Assessment

```
Documentation:   16,497 words / 5,921 lines
Source code:          0 words /     0 lines
Normative rules:    475
Tests:                0
Enforced rules:       0
```

---

## 10. Summary of Findings Requiring a Decision

1. Reconcile the three confidence models (§4.2). **Blocking** before any
   recovery code writes a status field.
2. Resolve agent commit authority in `CLAUDE.md` §28/§31 and
   `DEVELOPMENT_ENVIRONMENT.md` §24 against `CLAUDE.md` §0.2 (§4.1).
3. Close or re-open `RESEARCH_LOG.md` §8 questions 1, 3, 4, 5 (§5).
   Q4 (first filesystem) warrants an explicit ADR decision.
4. Decide a single owner document for each duplicated rule family (§3).
5. Mark `ARCHITECTURE.md` sections as aspirational, or move them behind a
   status header (§6.1).
6. Choose one heading convention and apply it, or configure markdownlint to
   accept the existing one (§8).
7. Populate or remove the four empty documents (§1).

No fixes have been applied. Nothing in this document has been committed.

---

## 11. Resolution Status

The findings above are a dated record of what was true on 2026-08-29 and are
left unmodified. This section tracks resolution against the §10 numbering.

| Item | Status | Resolving commit |
|---|---|---|
| 1. Reconcile the three confidence models | RESOLVED | `9e87f85` |
| 2. Resolve agent commit authority | RESOLVED | `2af499a` |
| 3. Close or re-open `RESEARCH_LOG.md` §8 questions 1, 3, 4, 5 | RESOLVED | `1686525` |
| 4. Decide a single owner document for each duplicated rule family | RESOLVED | `ADR-0011`, which declines it; see its section 5 |
| 5. Mark `ARCHITECTURE.md` sections as aspirational | RESOLVED | `45d9b3f` |
| 6. Choose one heading convention or configure markdownlint | RESOLVED | `d2d2d30` |
| 7. Populate or remove the four empty documents | RESOLVED | `c913b6b`, `d148466` |

Finding 4.1's section citations (`CLAUDE.md` §28/§31 and
`DEVELOPMENT_ENVIRONMENT.md` §24) were corrected in a later commit,
`d8654f4`, after the citations were found not to match the sections they
named.

Item 4 was re-derived at `334dc7c` before being answered. The duplication is
larger than §3 recorded, and most of it binds different subjects, so
`ADR-0011` declines to assign a single owner and records why. `ADR-0011`
§1.2 also corrects the example of modal drift this document gives in §3.1:
the two statements it compares are different rules. A genuine contradiction
exists between `docs/PROJECT.md` §6.1 and `docs/SAFETY.md` §4.1, and is
fixed separately.
