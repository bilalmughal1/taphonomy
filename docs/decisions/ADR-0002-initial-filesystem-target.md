# ADR-0002: Initial Filesystem Target

* **Status:** Accepted
* **Date:** 2026-08-29
* **Decision owners:** Taphonomy project
* **Scope:** Selection and ordering of filesystem implementation targets
* **Supersedes:** ADR-0001 §8 and §11
* **Amends:** ADR-0001 §9 (ordering only)

---

## 1. Context

ADR-0001 §8 selected NTFS as the first filesystem implementation target, and
§11 defined a nine-step first recovery capability against NTFS evidence.

Two facts not weighed in ADR-0001 have since been established.

### 1.1 The documentation requirement

`README.md` §Research and `RESEARCH_LOG.md` conclusion 7 both require that
recovery algorithms be based on documented filesystem behaviour rather than
assumptions.

ADR-0001 §8 lists "mature technical documentation" as a reason for selecting
NTFS. This is not accurate in the sense the project requires.

Microsoft has never published an authoritative NTFS on-disk specification.
Every open implementation, including the Linux NTFS project, ntfs-3g, and The
Sleuth Kit, is derived from reverse engineering. These implementations do not
agree on all edge cases.

Microsoft has published a FAT32 on-disk specification. FAT32 structures are
therefore documented in the sense the project's own standard requires.

Selecting NTFS first would mean the project's first parser is built on
inference, contradicting the standard on the first artefact produced.

### 1.2 Language proficiency

The implementer is writing their first Rust program. ADR-0001 was written
before this was stated.

NTFS parsing requires handling the Master File Table, resident and
non-resident attributes, attribute lists, data runs, `$Bitmap`, alternate data
streams, compression, and sparse files. FAT32 requires a boot sector, a file
allocation table, and 32-byte directory entries.

Learning Rust and reverse-engineered NTFS semantics simultaneously means a
defect cannot be attributed to either with confidence.

---

## 2. Decision

**FAT32 is the initial filesystem implementation target.**

The implementation order is:

```text
FAT32
  ↓
exFAT
  ↓
NTFS
  ↓
ext4
```

NTFS is not rejected. It is deferred to third position.

Each filesystem requires its own research, its own fixtures, and its own
validation before implementation begins, as required by ADR-0001 §9.

---

## 3. Reasons

### 3.1 Specification availability

FAT32 has a published on-disk specification. NTFS does not. This is the
primary reason and it derives directly from an existing project requirement
rather than from convenience.

### 3.2 Fixture reproducibility

`PROJECT.md` §6.4 requires deterministic behaviour, and
`DEVELOPMENT_ENVIRONMENT.md` §11 requires fixtures that are deterministic and
reproducible.

FAT32 fixtures can be produced with `mkfs.vfat` from `dosfstools`, which is
present in a default Ubuntu installation. NTFS fixtures require `ntfsprogs`,
and `mkntfs` output includes non-deterministic elements that complicate
byte-reproducible fixture generation.

### 3.3 Parser surface area

FAT32 has a materially smaller structure count. This reduces the amount of
untested parsing code in the trusted computing base at the point where the
surrounding architecture is least validated.

### 3.4 Relevance to motivating cases

The originating data-loss incidents involved SD cards, USB storage, and mobile
devices. SD cards up to 32 GB use FAT32 by SD Association specification.
Larger SD cards use exFAT. NTFS is the filesystem of internal Windows drives,
which is the one storage class not represented in the motivating incidents.

### 3.5 Forcing function on the confidence model

FAT32 deletion clears the cluster chain from the file allocation table. For
any fragmented file, the metadata required to reassemble it is destroyed at
deletion time.

This makes `RECONSTRUCTED` and `UNRECOVERABLE` (ADR-0003) real classifications
in the first implementation rather than unexercised enum variants. The
confidence model is validated by the first filesystem rather than deferred.

---

## 4. Accepted Costs

### 4.1 NTFS offers better recovery outcomes

This is the strongest argument against this decision and it is accepted, not
dismissed.

NTFS frequently retains the Master File Table record of a deleted file,
including its data runs. Recovery of fragmented files from NTFS is therefore
often possible where the equivalent FAT32 case is not.

Choosing FAT32 first means the project's first recovery capability will
recover less than an NTFS-first capability would have.

This cost is accepted because the purpose of the first implementation is to
validate the evidence model, the fixture laboratory, the validation pipeline,
and the confidence taxonomy. It is not to maximise recovery yield.

### 4.2 Windows development environment relevance

ADR-0001 §8 cited relevance to the Windows development environment. This is
reduced. It is not eliminated, because evidence images are filesystem-agnostic
and the development host filesystem is not the analysis target.

---

## 5. Alternatives Considered

### 5.1 NTFS first, as originally decided

Rejected. Requires building on reverse-engineered structures in violation of
the project's documented-behaviour standard, at the highest-risk point in the
project.

### 5.2 exFAT first

Rejected for first position. exFAT is specified and is the correct second
target, but it introduces the allocation bitmap, the upcase table, and the
no-FAT-chain case for contiguous files. These are better approached after
FAT32 concepts are established.

### 5.3 FAT32 and NTFS simultaneously

Rejected. Implementing two filesystems before either is complete means the
evidence abstraction is designed to fit both before either validates it. This
is speculative generalisation, prohibited by `CLAUDE.md` §9.

The value of the second filesystem is that it tests whether the abstraction
derived from the first was correct. That test is only available if the first
is finished.

### 5.4 File carving before any filesystem

Rejected for now, consistent with ADR-0001 §11. Carving does not exercise
filesystem metadata handling, which is the larger architectural risk.

---

## 6. Consequences

### Positive

* First parser is built on published specification rather than inference
* Fixture generation requires no additional tooling
* Smaller untested surface area during the highest-risk phase
* Confidence taxonomy is exercised by the first implementation
* Directly addresses the storage classes in the motivating incidents

### Negative

* Lower recovery yield than an NTFS-first implementation
* Fragmented-file recovery from FAT32 is largely not possible from metadata
* NTFS support is deferred, and NTFS is the most common desktop filesystem

---

## 7. Effect on ADR-0001

| ADR-0001 section | Effect |
|---|---|
| §8 "Why NTFS Is the Initial Filesystem" | **Superseded.** Retained for history. |
| §9 "Why We Are Not Starting With All Filesystems" | **Amended.** The staged approach stands; the filesystem is FAT32. |
| §11 "Initial Recovery Target" | **Superseded.** See §8 below. |
| All other sections | **Unaffected.** Rust, RAW/dd, read-only evidence boundary, validation model, testing strategy all stand. |

---

## 8. Revised Initial Recovery Target

ADR-0001 §11 defined nine steps as a single capability. This is revised into
sequenced milestones, each independently testable.

```text
M1  Open a RAW image read-only, hash it, report size
M2  Detect the partition table and locate partitions
M3  Identify the filesystem type of a partition
M4  Parse the FAT32 boot sector and validate its fields
M5  Enumerate the root directory
M6  Identify deleted directory entries
M7  Recover the data of an unfragmented deleted file
M8  Validate recovered data against a known-good reference
M9  Classify and report the result with a confidence level
```

Fragmented deleted files are explicitly out of scope for M7. Recovering them
from FAT32 requires carving or heuristic reconstruction, which is a separate
capability and a later decision.

The source image must remain unchanged throughout, as required by
`SAFETY.md` §4.

---

## 9. Open Questions Closed by This Decision

`RESEARCH_LOG.md` §8 question 4 ("Which filesystem should be the first
implementation?") is closed by this ADR.

`RESEARCH_LOG.md` §8 question 5 ("Should the first capability be filesystem
recovery or file carving?") remains closed by ADR-0001 §11 in favour of
filesystem recovery.

---

## 10. Review Trigger

This decision should be revisited if:

* an authoritative NTFS specification becomes available
* FAT32 implementation reveals that the evidence abstraction is unsuitable
* a real recovery case requires NTFS before the sequence reaches it

---

## Appendix A: Correction to §3.2 (2026-08-29)

This appendix corrects a factual error in §3.2. The body of the ADR above is
left unmodified, consistent with the treatment of superseded content
elsewhere in `docs/decisions/`.

### A.1 The error

§3.2 states:

> FAT32 fixtures can be produced with `mkfs.vfat` from `dosfstools`, which is
> present in a default Ubuntu installation. NTFS fixtures require `ntfsprogs`

The claim that `dosfstools` is present in a default Ubuntu installation is
**false**.

Measured on the development host, Ubuntu under WSL2, on 2026-08-29:

```text
which mkfs.vfat mkfs.fat mkntfs mkfs.ntfs sfdisk sgdisk
  /usr/sbin/sfdisk
  /usr/sbin/sgdisk

dpkg -l dosfstools ntfs-3g | grep '^ii'
  (no output)

apt-cache policy dosfstools
  dosfstools:
    Installed: (none)
    Candidate: 4.2-1.1build1
```

Neither `dosfstools` nor `ntfs-3g` is installed. `sfdisk` (util-linux) and
`sgdisk` (gdisk) are.

### A.2 What this invalidates

§3.2 presented tooling availability as a differentiator between FAT32 and
NTFS. It is not. Both require a package installation on this host:

```text
apt install dosfstools    # mkfs.vfat
apt install ntfs-3g       # mkntfs
```

Both are in Ubuntu main. Neither is burdensome. The asymmetry §3.2 claimed
does not exist.

### A.3 What §3.2 was also asserting without evidence

§3.2 further states that `mkntfs` output "includes non-deterministic elements
that complicate byte-reproducible fixture generation."

This was not measured. It is plausible — NTFS volumes carry volume GUIDs,
`$LogFile` contents, and MFT timestamps — but no comparison of repeated
`mkfs.vfat` and `mkntfs` output was performed before the claim was written.

The claim is therefore **unverified** and must not be cited as evidence until
tested. Determinism of fixture generation will be measured during milestone
M2, and the result recorded in
`docs/development/EXPERIMENTS.md`.

### A.4 Effect on the decision

**The decision is unchanged. FAT32 remains the initial filesystem target.**

§3.1, specification availability, is the primary reason and is unaffected:
Microsoft has published a FAT32 on-disk specification and has never published
an authoritative NTFS specification. That argument is decisive on its own,
and derives from an existing project requirement rather than from
convenience.

§3.3 (parser surface area), §3.4 (relevance to motivating cases) and §3.5
(forcing function on the confidence model) are also unaffected.

§3.2 is reduced from a supporting argument to a null one. It should not be
cited.

### A.5 Cause

The claim was written from assumption rather than measurement, and was not
checked against the development host before the ADR was committed.

`README.md` §Research and `RESEARCH_LOG.md` conclusion 7 require that the
project's technical claims rest on documented or measured behaviour. That
standard applies to decision records as much as to recovery algorithms.

Future ADRs must state the command used to obtain any environmental claim, as
this appendix does.
