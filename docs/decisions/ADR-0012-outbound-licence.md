# ADR-0012: Outbound Licence

* **Status:** Proposed
* **Date:** 2026-09-07
* **Decision owners:** Taphonomy project
* **Scope:** The licence Taphonomy is distributed under, and what it grants
* **Related:** `LICENSE`; `Cargo.toml`; `CONTRIBUTING.md`;
  `docs/development/DEVELOPMENT_ENVIRONMENT.md` §16; ADR-0004 §3;
  ADR-0007 §5.5; ADR-0011 §3

---

## 1. Context

The repository carries the Apache-2.0 licence text in `LICENSE`,
`license = "Apache-2.0"` in `Cargo.toml`, and a statement in
`CONTRIBUTING.md` §Authorship that contributions, "when accepted in future,
will be licensed under Apache-2.0 in accordance with section 5 of the
license, unless separately agreed in writing".

A `NOTICE` file holds two lines: the project name and "Copyright 2026 Fahad
Bilal Saleem".

The repository is private. Nothing has been distributed, so no licence has
reached anyone and the choice is still free.

The owner's stated goals are to publish the work so that it can be read and
run, and to retain the ability to license it commercially if a company ever
wants it. Both, not one.

### 1.1 Why Apache-2.0 does not serve the second goal

Apache-2.0 grants every recipient a perpetual, irrevocable, royalty-free
licence, including for commercial use. A company wanting to build a product
on Taphonomy would already have permission and would owe nothing. There
would be nothing to negotiate.

Ownership is not the issue and never was. Copyright vests in the author
automatically, and every licence considered here leaves it there. A licence
controls what others may do, not who owns the work.

### 1.2 Why not a noncommercial licence

PolyForm Noncommercial 1.0.0 was considered. It grants free use for any
noncommercial purpose and reserves commercial use, which matches the second
goal exactly.

It was rejected because it defeats the first. It is not an OSI-approved
licence, so the project could not honestly be described as open source.
Contributions become impractical, because merged code would need a separate
licensing arrangement with each contributor. And a company cannot evaluate
the software commercially without first asking permission, which reduces
rather than increases the chance that any company ever looks.

---

## 2. Decision

Taphonomy is licensed under the **GNU General Public License, version 3 or
later** (`GPL-3.0-or-later`).

Commercial licences are available from the copyright holder on request. No
separate document is needed to make that true: the copyright holder may
license the work on any terms, to anyone, at any time.

`NOTICE` is removed. It exists because Apache-2.0 §4(d) requires derivative
works to carry a NOTICE file forward. The GPL has no equivalent mechanism,
so under this decision the file is an artefact of a licence that no longer
applies.

The copyright line it holds moves to `README.md` §License, which currently
says only that the project is "distributed under the license specified in
`LICENSE`". Removing `NOTICE` without relocating that line would leave the
tree with no copyright statement anywhere, which is the opposite of what
this decision is for.

---

## 3. Why GPL-3.0

It is OSI-approved and universally recognised, so the project can be
published, described and assessed as open source. Anyone may read, run,
study, modify and redistribute it.

It preserves the commercial option. A derivative work distributed inside a
closed-source product must be released under the GPL. A company unwilling to
do that has one remaining route, which is a licence from the copyright
holder. The reluctance of commercial vendors to accept copyleft is the
mechanism here, not a side effect of it.

`-or-later` follows the Free Software Foundation's own recommendation and
allows a future version of the licence to apply without relicensing.

**Not AGPL.** The Affero clause covers software offered over a network.
Taphonomy is a local-first command-line tool, and `docs/PROJECT.md` §6.8
requires that network communication not be needed for core recovery
functionality. The clause would add obligations that never trigger.

**Not MIT or BSD.** Same defect as Apache-2.0 for the second goal, with less
patent clarity.

---

## 4. What this does not change

**Ownership.** Unchanged, and unchangeable by any licence here.

**The dependency policy.**
`docs/development/DEVELOPMENT_ENVIRONMENT.md` §16 says that "GPL, LGPL, CPL,
IPL, AGPL, SSPL, custom, source-available, or otherwise restrictive licenses
must not be introduced into the dependency graph without a documented
decision." That rule governs what Taphonomy consumes, not what Taphonomy is
licensed as. It is unaffected, and it is named here because a reader meeting
that line will otherwise think this decision contradicts it.

**Dependency compatibility.** ADR-0004 §3.4 records the whole tree as
`MIT OR Apache-2.0`, and both are compatible with the GPL in the combining
direction used here.

Version 3 matters for this and not only for §3's reasons. Apache-2.0 is
compatible with GPL version 3 and is **not** compatible with GPL version 2.
Had this decision chosen `GPL-2.0`, the `sha2` dependency ADR-0004 selected
could not have been combined with it. `-or-later` permits version 3 and
anything after it, never version 2, so the incompatibility cannot be
reached.

**The patent question of ADR-0007 §5.5.** Microsoft's VFAT long-filename
patents are a third party's, and no outbound licence affects exposure to
them. ADR-0007 notes that Apache-2.0 was selected "partly for its patent
grant"; that grant runs from contributors to users and never protected the
project against anyone else's patents. The question remains open on its own
terms, before long-name decoding.

---

## 5. Timing

Applied now, while the repository is private.

An Apache-2.0 grant is irrevocable for every version published under it.
Publishing first and switching later would leave those versions permanently
available for commercial use, which is the outcome this decision exists to
avoid. Nothing has been published, so nothing is lost.

The alternative, deferring the change until the day of publication, places
an irreversible step at the busiest moment and risks it being forgotten.

---

## 6. Contributions

**External contributions are not accepted.** This is a decision, not a
current state of affairs. `CONTRIBUTING.md` §Authorship presently says
contributions are anticipated "when accepted in future"; that wording is
changed, because they are not.

The copyright holder therefore holds the copyright in the entire work, and
§2's commercial option is unencumbered.

The rest of this section is a warning for whoever revisits the decision,
because the way to lose §2 is to open contributions and get the instrument
wrong. Without a contributor licence agreement, the copyright holder cannot
grant a commercial licence covering code contributed by someone else,
because they do not hold its copyright.

**A CLA grants permission. It does not create a claim.** A contributor who
signs one keeps the copyright in their own work and is owed nothing by this
project: no share of any commercial licence, no revenue, no partnership.
What the agreement gives is documented permission to ship and relicense the
combined work. A contributor who has *not* signed one cannot claim anything
either, but can prevent the copyright holder from licensing any work
containing their contribution, which is the risk this section exists to
avoid.

**A Developer Certificate of Origin is not sufficient here, and looks like
it is.** A DCO is a sign-off line on a commit certifying that the
contributor had the right to submit the code. It is lighter than a CLA and
widely used, and it does not grant the right to relicense. Adopting one
because it is easier would remove the ability to offer the commercial
licences §2 relies on, without any visible sign that it had done so. If
contributions are opened, the instrument must be a CLA or a copyright
assignment.

The operational rule that follows: no external contribution is merged before
its agreement is on file. A contribution merged without one can only be
removed and rewritten.

---

## 7. Consequences

### Positive

The project can be published, described and assessed as open source, which
is what the first goal requires.

A commercial route survives publication, which Apache-2.0 would have closed
permanently on the first public version.

The change is made while it is free to make.

### Negative

Some organisations avoid copyleft software as policy and will not evaluate
Taphonomy at all. That is the cost of the mechanism in §3 and is accepted.

A CLA becomes a prerequisite for accepting contributions, which is friction
at exactly the moment contributions would be most welcome.

`LICENSE` grows from 202 lines to roughly 675, and the GPL is harder to read
than Apache-2.0 for anyone assessing what they may do.

`NOTICE` is deleted, which is the first tracked file this project has
removed. Anyone who knows the Apache convention and looks for it will not
find one, and nothing explains its absence except this decision.

ADR-0004 §3.4 describes `sha2` as "compatible with the project's Apache-2.0
license". That sentence becomes wrong. ADR-0004 is a dated record under
ADR-0011 §3 and is corrected by appendix rather than edited.

---

## 8. Review trigger

Revisit if the project begins accepting external contributions, which
requires a CLA first.

Revisit if a commercial licence is actually requested, at which point the
terms of that licence are a separate decision and should be recorded.

Revisit if the goals in §1 change. This decision is downstream of them and
has no independent justification.

---

## 9. Not legal advice

This decision was reached by reading the licences and the project's own
documents. No lawyer was consulted. The licence text is used unmodified,
which is the safest available position, and no jurisdiction or governing law
is specified by GPL-3.0.

A solicitor should review the choice before the repository is made public,
particularly on governing law and on the enforceability of the arrangement
in §2 where the copyright holder resides.

---

## 10. Open after this decision

1. **Per-file licence headers.** The GPL's own "How to Apply" appendix
   recommends a short notice at the top of each source file. This decision
   does not add them. Whether to do so is a separate choice, and the
   argument for it is strongest at the moment of publication, when files
   begin circulating separately from the repository.

2. **A solicitor's review**, per §9, before publication.

3. **ADR-0004's appendix**, correcting its description of `sha2` as
   compatible with an Apache-2.0 project.

4. **The commercial offer's wording.** §2 states that commercial licences
   are available. Where and how that is said to a reader, and in what terms,
   is not decided here.
