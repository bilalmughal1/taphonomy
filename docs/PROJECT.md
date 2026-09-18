# Taphonomy Project Specification

**Project:** Taphonomy
**Status:** Foundation
**Version:** 0.1.0
**Owner:** Fahad Bilal Saleem
**Repository:** Private
**Primary Environment:** Linux via WSL2 on Windows
**Development Model:** Local-first, evidence-preserving, test-driven

---

## 1. Project Overview

Taphonomy is a local-first digital recovery and investigation platform designed to identify, preserve, analyze, reconstruct, and recover digital artifacts from damaged, deleted, corrupted, or otherwise inaccessible storage and devices.

The project is designed primarily for local execution. Sensitive user data must remain on the user's machine unless an explicitly authorized future feature states otherwise.

Taphonomy is intended to become a serious engineering project suitable for professional use, open-source development in the future, and inclusion in a professional portfolio.

---

## 2. Concept

The name Taphonomy is derived from the scientific discipline of taphonomy, which studies the processes affecting remains after their original state has ended.

The project's conceptual relationship is:

> What remains after digital loss, how it changed, what evidence survived, and what can be reliably reconstructed from that evidence.

The name does not imply that every recovery operation is possible. Taphonomy must distinguish between recoverable evidence, partially recoverable evidence, corrupted evidence, and evidence that cannot be reliably recovered.

---

## 3. Primary Objectives

Taphonomy aims to:

1. Identify storage devices and accessible digital sources safely.
2. Preserve source evidence before analysis whenever technically possible.
3. Create verifiable forensic-style images of supported storage media.
4. Analyze filesystem structures without modifying source evidence.
5. Recover deleted or damaged files where sufficient evidence remains.
6. Reconstruct filesystem metadata where technically feasible.
7. Recover application-level artifacts where supported.
8. Provide specialized recovery paths for different device classes.
9. Validate recovered data rather than assuming recovery success.
10. Record sufficient metadata to reproduce and audit recovery operations.
11. Operate locally without requiring cloud services.
12. Provide transparent explanations of recovery methods, limitations, and confidence.

---

## 4. Recovery Domains

Taphonomy will be divided into independent recovery domains.

### 4.1 Removable Storage

Initial targets:

* USB storage
* SD cards
* microSD cards

Potential capabilities:

* device identification
* partition detection
* filesystem identification
* disk imaging
* deleted-file recovery
* filesystem reconstruction
* file carving
* integrity validation

### 4.2 HDD

Potential targets:

* MBR
* GPT
* NTFS
* FAT32
* exFAT
* ext4
* HFS+

The exact supported filesystems will be determined through research and documented before implementation.

### 4.3 SSD

SSD recovery is treated as a separate technical domain.

The architecture must account for:

* TRIM
* garbage collection
* wear leveling
* controller behavior
* flash translation layers
* encryption
* filesystem behavior

Taphonomy must never imply that HDD recovery techniques apply equally to SSDs.

### 4.4 Mobile Devices

Mobile recovery is a separate domain.

Initial research targets:

* Android
* iOS

Potential acquisition classes:

* logical acquisition
* filesystem-level acquisition
* backup analysis
* application data extraction
* damaged-device recovery

Capabilities will only be implemented where technically and legally appropriate.

### 4.5 Application Data

Application recovery is treated independently from device recovery.

Potential artifact types include:

* SQLite databases
* SQLite WAL files
* SQLite journal files
* application caches
* thumbnails
* local backups
* application-specific databases
* application-specific media and metadata

The existence of an application on a device does not imply that its data is recoverable.

---

## 5. Non-Goals

Taphonomy will not:

* guarantee recovery of lost data.
* claim recovery where evidence is insufficient.
* modify original evidence without explicit authorization.
* upload user data to remote services by default.
* depend on a cloud service for core recovery functionality.
* silently overwrite source or recovered data.
* bypass device security without a legitimate and technically supported acquisition method.
* present guesses as verified recovery.
* begin as a graphical application before the recovery engine is technically mature.
* support every filesystem or device type simultaneously.

---

## 6. Core Engineering Principles

### 6.1 Evidence Preservation

The original source must be treated as immutable evidence whenever technically possible.

Analysis and recovery should operate against a verified image or other controlled copy.

### 6.2 Read-Only by Default

Operations against source media must default to read-only behavior.

Any operation capable of modifying source data must require explicit authorization and have documented justification.

### 6.3 Fail Closed

When Taphonomy cannot determine that an operation is safe, it must stop rather than guess.

### 6.4 Deterministic Behavior

Given identical evidence, configuration, and software version, Taphonomy should produce reproducible results wherever technically possible.

### 6.5 Verifiable Results

Recovered artifacts must be validated.

A file being extracted from storage does not by itself establish that the file is correct.

### 6.6 Auditability

Important operations must produce sufficient records to determine:

* what was processed
* when it was processed
* which software version performed the operation
* what method was used
* what evidence was accessed
* what hashes were produced
* what result was obtained
* what limitations were encountered

### 6.7 Separation of Domains

Device acquisition, evidence management, filesystem analysis, recovery algorithms, artifact analysis, and presentation must remain independently testable components.

### 6.8 Local-First Privacy

Sensitive data must remain local by default.

Network communication must not be required for core recovery functionality.

### 6.9 Explicit Uncertainty

Taphonomy must distinguish what it verified from what it inferred, and must
never present the second as the first.

The levels this distinction uses, and the evidence each requires, are
defined in `docs/decisions/ADR-0003-recovery-confidence-model.md`. The
six-term list this section previously held predated that ADR and is removed
rather than restated here, so that the levels have one definition.

---

## 7. Safety Priority

When engineering goals conflict, the priority order is:

1. Preserve source evidence.
2. Protect user data.
3. Maintain correctness.
4. Maintain reproducibility.
5. Maintain auditability.
6. Maintain performance.
7. Improve convenience.

Performance or convenience must never justify compromising evidence integrity.

---

## 8. Development Strategy

Development will proceed incrementally.

### Phase 0: Foundation

* repository
* documentation
* safety model
* threat model
* architecture decisions
* development standards

### Phase 1: Safe Device Discovery

* identify devices
* inspect device metadata
* distinguish source and destination
* prevent accidental writes

### Phase 2: Evidence Acquisition

* create disk images
* calculate hashes
* record acquisition metadata
* verify image integrity

### Phase 3: Evidence Inspection

* partition detection
* filesystem identification
* metadata inspection
* controlled analysis

### Phase 4: First Recovery Engine

The first recovery implementation will target a controlled filesystem and synthetic test evidence.

### Phase 5: Recovery Laboratory

Build reproducible fixtures covering:

* deletion
* fragmentation
* metadata corruption
* filesystem corruption
* partial damage
* known-good files

### Phase 6: Additional Filesystems

Additional filesystems will be added only after research, implementation planning, and test coverage.

### Phase 7: Specialized Recovery

* HDD
* SSD
* mobile
* application artifacts

Each domain will have its own architecture and documentation.

---

## 9. Quality Gates

A feature must not be considered complete merely because it works on one example.

A recovery capability should satisfy, as appropriate:

* unit tests
* integration tests
* deterministic fixtures
* negative tests
* corruption tests
* failure-path tests
* source-integrity tests
* output-validation tests
* documentation
* architecture review
* relevant ADR
* reproducibility verification

---

## 10. Definition of Done

A Taphonomy feature is complete only when:

1. Its purpose is documented.
2. Its architectural boundary is defined.
3. Its safety implications are understood.
4. Its implementation is tested.
5. Its failure modes are tested.
6. Its limitations are documented.
7. Its behavior is reproducible where possible.
8. Its relevant architectural decisions are recorded.
9. It does not violate the project's evidence-preservation model.
10. The repository remains understandable after the feature is added.

---

## 11. Current Status

Taphonomy has a validated read-only evidence layer, a FAT32 boot sector
parser, a FAT32 root directory reader that identifies deleted entries,
and recovery of the data of an unfragmented deleted file. Recovery
extracts to memory and reports a digest; no file is written. Where the
operator supplies a reference digest, the recovered digest is compared
against it, and the artifact carries the one confidence level ADR-0003's
model leaves reachable.

Milestones M1 to M9 of ADR-0002 section 8 are complete, which is the whole
of that sequence. The next engineering objective is item 5 of the README's
development sequence, validating recovery accuracy: the characteristic
false positive is measured and the rest of the space is not.

The technology stack is selected and recorded in ADR-0001. The safety
and development contracts this section previously described as pending
are established in `docs/SAFETY.md`, `SECURITY.md` and
`docs/development/DEVELOPMENT_ENVIRONMENT.md`.

