# Taphonomy Safety Specification

**Project:** Taphonomy
**Document:** Safety Specification
**Version:** 0.1.0
**Status:** Foundation
**Last Updated:** 2026-08-28

---

## 1. Purpose

Taphonomy operates on data that may be damaged, deleted, corrupted, or otherwise difficult or impossible to replace.

A recovery system can cause irreversible data loss if it writes to the wrong device, modifies filesystem metadata, overwrites recoverable sectors, or destroys evidence during analysis.

Safety is therefore a primary system requirement.

This document defines the safety rules that govern Taphonomy's architecture, implementation, testing, and operation.

These rules apply to the core system, CLI, recovery engines, plugins, scripts, tests, and development tooling.

---

# 2. Fundamental Safety Principle

> **Taphonomy must never make the recovery situation worse by default.**

When there is a conflict between recovery speed and evidence preservation, evidence preservation takes priority.

When there is uncertainty about the identity, state, or safety of a source device, Taphonomy must stop rather than guess.

---

# 3. Evidence Model

Taphonomy distinguishes between:

### 3.1 Source Evidence

The original device, storage medium, filesystem, backup, or other data source from which information is being recovered.

Examples:

* USB drive
* SD card
* microSD card
* HDD
* SSD
* Android device
* iPhone
* filesystem image
* application database
* backup archive

### 3.2 Working Evidence

A verified copy or image created from the source and used for analysis.

Examples:

* raw disk image
* segmented disk image
* verified filesystem copy

### 3.3 Recovery Output

Files or artifacts produced by Taphonomy during analysis.

Recovery output must never be written into the source evidence.

### 3.4 Metadata

Information describing an acquisition, analysis, or recovery operation.

Examples:

* timestamps
* device information
* filesystem information
* software version
* operation parameters
* hashes
* offsets
* recovery method
* validation results

---

# 4. Source Preservation

## Rule 4.1

The original source must be treated as immutable whenever technically possible.

## Rule 4.2

Taphonomy must prefer analysis of a verified image over direct analysis of the original physical device.

## Rule 4.3

Recovery output must be stored on a separate destination.

## Rule 4.4

Taphonomy must never assume that a device is safe to write to simply because the operating system permits writes.

## Rule 4.5

A filesystem being mounted read-only is not by itself sufficient evidence that the underlying acquisition is safe.

---

# 5. Read-Only Default

All source-device operations must default to read-only behavior.

Examples of read-only operations:

* device identification
* capacity detection
* partition discovery
* filesystem identification
* sector reading
* metadata inspection
* hashing
* imaging

Operations that can modify source data must not be part of the default recovery path.

---

# 6. Source and Destination Separation

Taphonomy must maintain an explicit distinction between:

```text
SOURCE
```

and:

```text
DESTINATION
```

The system must never infer a destination from an ambiguous path.

Before any operation that creates output, Taphonomy must establish:

1. source identity
2. destination identity
3. source accessibility
4. destination accessibility
5. destination capacity
6. whether source and destination are the same physical device
7. whether the destination overlaps the source

If these conditions cannot be established safely, the operation must stop.

---

# 7. Never Recover Onto the Source

Recovered data must never be written onto the source device.

This applies even when:

* the source has available free space
* the source contains an existing recovery directory
* the user explicitly specifies a directory on the source
* the filesystem appears healthy
* the source is mounted read-only by the operating system

The default recovery destination must be external to the source evidence.

---

# 8. Imaging First

When physical storage can be safely imaged, Taphonomy should create a working image before performing complex recovery operations.

Conceptually:

```text
SOURCE DEVICE
     │
     │ read-only acquisition
     ▼
EVIDENCE IMAGE
     │
     │ verification
     ▼
ANALYSIS
     │
     ▼
RECOVERY OUTPUT
```

The original device should remain untouched during subsequent analysis.

---

# 9. Evidence Hashing

Acquisition operations must support cryptographic integrity verification.

At minimum, Taphonomy should support SHA-256 for evidence verification.

Where appropriate, additional hashes may be supported.

An acquisition record should include:

* source identity
* source size
* acquisition start time
* acquisition completion time
* Taphonomy version
* acquisition method
* hash algorithm
* resulting hash
* errors encountered
* bytes successfully acquired
* acquisition status

A failed acquisition must not be represented as a successful verified image.

---

# 10. Hash Semantics

Taphonomy must clearly distinguish between:

* source hash
* image hash
* recovered-file hash
* validation hash

Hashes must never be represented as proof of correctness beyond what they actually establish.

For example, matching hashes establish byte equality between the hashed objects. They do not establish that a recovered file represents the user's intended original content.

---

# 11. Destructive Operations

Destructive operations are prohibited by default.

Examples include:

* formatting
* partition modification
* filesystem repair
* metadata modification
* writing recovery structures
* deleting source content
* overwriting source sectors

If destructive functionality is ever introduced, it must have:

1. explicit user authorization
2. prominent warnings
3. source identification
4. destination or target identification
5. a documented reason
6. a documented rollback strategy where technically possible
7. dedicated tests
8. an ADR
9. separate implementation boundaries from normal recovery operations

---

# 12. Failure Policy

Taphonomy must fail closed.

When an operation encounters uncertainty, it should stop rather than continue with assumptions.

Examples:

```text
Unknown source device
Ambiguous source path
Source/destination collision
Unexpected device size
Unreadable critical metadata
Hash mismatch
Insufficient destination capacity
Unexpected device removal
Unexpected write capability
Corrupted acquisition
Unsupported filesystem
```

A failure must never be silently converted into a partial success.

---

# 13. Partial Results

Partial recovery is allowed, but it must be explicitly represented.

For example:

```text
Status: PARTIAL
```

is different from:

```text
Status: SUCCESS
```

A partially recovered file must not be presented as a complete verified file.

---

# 14. Recovery Confidence

Taphonomy should eventually classify recovery results.

Initial conceptual categories:

```text
VERIFIED
HIGH_CONFIDENCE
PARTIAL
RECONSTRUCTED
UNVERIFIED
UNRECOVERABLE
```

The exact confidence model will be defined separately before implementation.

Confidence must be based on measurable evidence rather than subjective language.

---

# 15. No Silent Overwrites

Taphonomy must never silently overwrite existing recovery output.

If:

```text
recovered/photo.jpg
```

already exists, the system must either:

* generate a unique output name
* explicitly ask for overwrite authorization
* stop

The default behavior must preserve the existing output.

---

# 16. Recovery Output Isolation

Recovery output should be separated from:

* source evidence
* working evidence
* application state
* logs
* configuration
* temporary processing data

A future implementation should establish explicit directories for these categories.

---

# 17. Temporary Data

Temporary files can contain sensitive recovered information.

Taphonomy must:

* document where temporary data is created
* avoid unnecessary temporary copies
* clean temporary data when safe
* avoid placing temporary recovery data in the Git repository
* avoid logging sensitive file contents

Temporary data must never be treated as disposable merely because it is temporary.

---

# 18. Logging

Logs must contain sufficient information to diagnose failures without unnecessarily exposing recovered data.

Logs should prefer:

```text
file identifier
offset
size
hash
operation
status
error code
```

over:

```text
file contents
personal messages
authentication tokens
private documents
```

Sensitive content must not be logged by default.

---

# 19. Network Isolation

The core recovery engine must not require network connectivity.

Taphonomy must not transmit user data, recovered data, filesystem contents, or evidence images to remote services without explicit user authorization and a separately documented feature.

Network-dependent functionality must remain outside the core recovery path.

---

# 20. Telemetry

Taphonomy must not collect telemetry by default.

No background analytics, tracking, crash uploads, or usage reporting may be introduced without explicit architectural approval.

If telemetry is ever introduced, it must be:

* opt-in
* documented
* transparent
* independently disableable
* designed to avoid transmitting sensitive information

---

# 21. Malformed and Hostile Data

Recovery software processes untrusted binary data.

A corrupted or malicious file must be treated as potentially hostile input.

Parsers must not assume that:

* headers are valid
* lengths are sane
* offsets are within bounds
* metadata is internally consistent
* strings are properly terminated
* compression ratios are reasonable

The implementation must use bounds checking, overflow-safe arithmetic, controlled resource usage, and defensive parsing.

---

# 22. Device Removal

Taphonomy must expect devices to disappear during operations.

Examples:

* USB cable disconnected
* SD card removed
* device power loss
* storage controller failure
* filesystem becoming inaccessible

Unexpected device removal must not cause undefined behavior or accidental writes to another device that later receives the same operating-system path.

Device identity must not rely exclusively on volatile paths such as:

```text
/dev/sdb
```

---

# 23. Device Identity

Physical-device identification should use stable identifying information where available.

Potential identifiers include:

* vendor
* model
* serial number
* capacity
* connection information
* filesystem identifiers
* partition identifiers

The system must account for situations where some identifiers are unavailable.

---

# 24. Mount State

Taphonomy must inspect the operating system's view of the source before performing sensitive operations.

A mounted filesystem can introduce external modifications through normal operating-system behavior.

Where appropriate, Taphonomy should recommend or require controlled unmounting before imaging.

The exact implementation will be established through platform-specific research.

---

# 25. SSD-Specific Safety

SSD recovery requires special treatment.

Taphonomy must not assume that sectors behave like magnetic HDD sectors.

Research and implementation must account for:

* TRIM
* garbage collection
* wear leveling
* flash translation layers
* controller behavior
* encryption
* over-provisioning

SSD limitations must be clearly communicated to users.

---

# 26. Mobile Device Safety

Mobile recovery must be implemented as an independent domain.

Taphonomy must not:

* modify a user's device without authorization
* claim to bypass security mechanisms it cannot legitimately bypass
* imply that deleted encrypted data is recoverable without required cryptographic material
* present logical acquisition as equivalent to physical acquisition

Mobile recovery capabilities must be documented per platform and acquisition method.

---

# 27. Application Data Safety

Application recovery must preserve the distinction between:

* application deletion
* application database deletion
* database corruption
* filesystem deletion
* backup availability
* encryption
* key availability

Taphonomy must not assume that uninstalling an application means all of its data is recoverable.

---

# 28. Testing Safety

Real user data must not be required for automated testing.

Testing should use:

* synthetic files
* synthetic filesystems
* controlled disk images
* intentionally corrupted fixtures
* known-good reference artifacts
* reproducible damage scenarios

Real personal recovery cases should never be committed to the repository.

---

# 29. Development Safety

Developers and coding agents must never experiment against irreplaceable user data.

Development experiments must use disposable images and fixtures.

A recovery experiment should follow:

```text
CREATE FIXTURE
      ↓
DOCUMENT ORIGINAL STATE
      ↓
INTRODUCE CONTROLLED DAMAGE
      ↓
HASH FIXTURE
      ↓
RUN TAPHONOMY
      ↓
COMPARE RESULT
      ↓
DOCUMENT FINDINGS
```

---

# 30. Coding-Agent Safety

AI coding agents working on Taphonomy must follow the same safety model as human contributors.

Agents must not:

* execute recovery operations against unknown devices
* modify source evidence
* create destructive scripts without explicit approval
* add network transmission of evidence
* bypass safety checks for convenience
* remove safety validation to make tests pass
* claim functionality that has not been tested

Agent instructions are maintained in `AGENTS.md`.

---

# 31. Safety Exceptions

A safety rule may only be changed through a documented engineering decision.

The change must include:

* reason
* risk assessment
* alternatives considered
* affected components
* testing requirements
* migration requirements
* rollback considerations

Safety exceptions must not be introduced through undocumented code changes.

---

# 32. Incident Handling

If Taphonomy causes unexpected modification, corruption, data loss, or exposure of evidence:

1. Stop the operation.
2. Preserve logs and diagnostic information.
3. Do not continue experimentation against the affected source.
4. Document the incident.
5. Identify the last known safe state.
6. Determine the affected operation.
7. Reproduce the issue using synthetic evidence if possible.
8. Create an incident record.
9. Fix the underlying cause.
10. Add a regression test.
11. Document the architectural lesson.

The objective is not merely to fix the immediate bug, but to prevent recurrence.

---

# 33. Safety Review Gate

Before a recovery capability is considered production-ready, the following must be reviewed:

* source handling
* destination handling
* write behavior
* device identity
* error handling
* interruption behavior
* hash verification
* output isolation
* malformed input handling
* resource exhaustion
* logging
* temporary files
* test coverage
* documentation
* known limitations

A feature that fails a critical safety requirement must not be released as stable.

---

# 34. Guiding Rule

The most important operational rule of Taphonomy is:

> **When uncertain, preserve the evidence and stop.**

Recovery can only be useful if the evidence survives the recovery attempt.

