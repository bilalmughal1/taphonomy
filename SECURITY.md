# Taphonomy Security Specification

**Project:** Taphonomy
**Document:** Security Specification
**Version:** 0.1.0
**Status:** In force
**Last Updated:** 2026-09-24

---

## Reporting a Vulnerability

Please do not report a security problem in a public issue.

Report it privately, through this repository's **Report a vulnerability**
button on GitHub, or by email to <contact@fahadbilal.com>. Include the
Taphonomy commit you used, what you ran, and what happened. A crafted image
that reproduces the problem is the most useful thing you can send; do not
send real evidence belonging to anyone.

In scope, most of all:

* anything that modifies, or could modify, the evidence being read
* a crash, hang or unbounded memory use caused by a crafted image
* a recovered file written anywhere other than the directory given to
  `--output`
* a report that states something the evidence does not support

Taphonomy has one maintainer. Reports are acknowledged on a best-effort
basis, and there are no releases yet: fixes are made on `main`.

The rest of this document states the security requirements the tool is
built to.

---

## 1. Purpose

Taphonomy processes potentially sensitive and untrusted digital data.

A storage device may contain:

* personal documents
* photographs
* videos
* private messages
* authentication material
* application databases
* financial information
* credentials
* deleted information
* malicious files
* malformed filesystem structures

Taphonomy must therefore be designed as both:

1. a recovery system, and
2. a security-sensitive binary-data processing system.

This document defines the security requirements for the project.

---

# 2. Security Objectives

Taphonomy must protect:

1. Source evidence.
2. Recovered data.
3. The host operating system.
4. Taphonomy's own configuration and state.
5. Recovery metadata and logs.
6. Temporary processing data.
7. Cryptographic material where applicable.
8. User privacy.

---

# 3. Security Principles

The project follows these principles:

```text
Least privilege
Local-first operation
Secure defaults
Explicit trust boundaries
Defensive parsing
Memory safety
Input validation
Fail-closed behavior
Minimal data exposure
No unnecessary network communication
Reproducibility
Auditability
```

---

# 4. Threat Model

Taphonomy must consider the possibility that the input data is malicious, corrupted, or intentionally designed to exploit recovery software.

Potential threats include:

* malformed filesystem metadata
* malicious files
* malicious archive structures
* integer overflow
* integer underflow
* out-of-bounds reads
* excessive memory allocation
* excessive CPU consumption
* recursive structures
* path traversal
* symbolic-link abuse
* device impersonation
* unexpected device replacement
* malicious filenames
* malformed Unicode
* corrupted compressed data
* malicious application databases
* malicious media files
* parser vulnerabilities
* dependency vulnerabilities
* accidental information disclosure

---

# 5. Trust Boundaries

Taphonomy must explicitly distinguish between trusted and untrusted data.

### Trusted

Potentially trusted inputs include:

* Taphonomy's compiled code
* validated configuration
* controlled test fixtures
* cryptographic verification code

### Untrusted

The following must be considered untrusted:

* physical storage devices
* disk images
* filesystem metadata
* filenames
* directory structures
* recovered files
* application databases
* archive contents
* media files
* metadata embedded in files
* device-provided identifiers

A file recovered from evidence does not become trusted simply because Taphonomy successfully extracted it.

---

# 6. Host Protection

Taphonomy must minimize the ability of processed data to affect the host operating system.

Recovered files should initially be treated as inert data.

Taphonomy must not automatically:

* execute recovered programs
* execute scripts
* launch recovered documents
* open recovered URLs
* invoke recovered commands
* execute recovered macros
* load recovered dynamic libraries

Recovery and execution must remain separate concerns.

---

# 7. Path Traversal Protection

Recovered filenames may contain malicious or unexpected path components.

Examples include:

```text
../../secret.txt
..\..\secret.txt
/absolute/path
C:\Windows\System32\...
```

Taphonomy must sanitize or safely map recovered paths before writing them to the destination filesystem.

A recovered filename must never allow arbitrary writes outside the configured recovery destination.

---

# 8. Symbolic Links

Recovered filesystem structures may contain symbolic links or equivalent constructs.

Taphonomy must not blindly reproduce links during recovery if doing so could cause writes or access outside the intended destination.

The default recovery model should treat symbolic-link metadata as data rather than as instructions to the host filesystem.

---

# 9. Device Identity

Operating-system device paths can change.

For example:

```text
/dev/sdb
```

may refer to different physical devices at different times.

Taphonomy must not rely exclusively on transient device paths when determining source identity.

Where available, identity should incorporate stable information such as:

* vendor
* model
* serial number
* capacity
* connection information
* filesystem identifiers
* partition identifiers

The system must account for devices where some identifying information is unavailable.

---

# 10. Device Replacement

Taphonomy must defend against the following scenario:

```text
Device A
   ↓
/dev/sdb
   ↓
Device removed
   ↓
Device B inserted
   ↓
/dev/sdb
```

The reuse of an operating-system path must never be treated as proof that the same physical device remains connected.

Long-running operations should periodically validate the identity of the underlying source where technically possible.

---

# 11. Privilege Separation

Taphonomy should operate with the minimum privileges required for each operation.

Normal analysis should not require administrative privileges when they are unnecessary.

Operations requiring elevated privileges should be isolated and explicitly identified.

The project should avoid making the entire application run as root or administrator merely because one operation requires elevated access.

---

# 12. Linux and WSL Considerations

The initial development environment is Linux running through WSL2.

The project must distinguish between:

* Linux filesystem access
* Windows-mounted filesystems
* Windows device access
* WSL virtualized hardware access

A recovery operation requiring direct physical-device access must explicitly document the required access path.

Taphonomy must never assume that WSL device visibility is equivalent to native Linux hardware access.

---

# 13. Memory Safety

The core implementation should prioritize memory-safe technology.

Rust is the current preferred candidate for the core engine because it provides strong compile-time memory-safety guarantees without requiring a garbage collector.

Unsafe Rust code should be minimized.

Any `unsafe` block must have:

* a clear justification
* a narrow scope
* safety invariants documented
* dedicated tests where appropriate

---

# 14. Binary Parsing

Filesystem and file parsers must treat every byte from evidence as untrusted input.

Parsers must validate:

* offsets
* lengths
* alignment
* block numbers
* sector numbers
* counts
* indexes
* allocation sizes
* string lengths
* recursion depth

Arithmetic involving values originating from evidence must be checked for overflow and underflow.

---

# 15. Bounds Checking

No parser may read outside the available evidence.

Before accessing:

```text
offset + length
```

Taphonomy must establish that the complete requested range exists.

Arithmetic must be performed using overflow-safe operations.

Invalid ranges must result in controlled errors rather than undefined behavior.

---

# 16. Resource Exhaustion

Malformed evidence may attempt to consume excessive resources.

Potential attacks include:

* enormous declared file sizes
* enormous directory counts
* deeply nested structures
* pathological fragmentation
* highly compressed data
* enormous metadata tables
* repeated structures
* infinite-loop conditions

Taphonomy should establish resource limits where appropriate.

Limits may apply to:

* memory
* CPU time
* recursion depth
* file count
* directory depth
* output size
* temporary storage
* parser work

---

# 17. Denial of Service

Recovery software must remain operational when encountering malformed data.

A malformed filesystem must not be able to cause:

* infinite loops
* uncontrolled memory growth
* uncontrolled temporary-file growth
* unbounded recursion
* indefinite CPU consumption

Errors should be detected and reported.

---

# 18. Output Security

Recovered files must be written only inside the explicitly configured destination.

The recovery system must prevent:

* path traversal
* unintended overwrites
* writes outside the destination
* unexpected filesystem interpretation
* accidental execution

Output filenames should be treated as untrusted input.

---

# 19. Temporary Files

Temporary files can contain complete or partial recovered data.

They must therefore be treated as sensitive.

Taphonomy should:

* use controlled temporary directories
* avoid predictable temporary filenames
* minimize temporary copies
* clean temporary data when safe
* document temporary-data behavior
* prevent temporary files from entering Git

Temporary data must not be stored in world-readable locations.

---

# 20. Logging Security

Logs must not expose sensitive recovered content.

The following should not be logged by default:

* file contents
* message contents
* credentials
* authentication tokens
* encryption keys
* personal documents
* private media
* full database records

Logs may contain:

* operation identifiers
* status
* errors
* offsets
* sizes
* hashes
* identifiers
* timestamps
* software versions

Logging levels must be explicitly defined.

---

# 21. Secrets

Taphonomy must not hard-code:

* passwords
* API keys
* encryption keys
* access tokens
* private credentials

Secrets must not be committed to Git.

Test credentials must not be real credentials.

---

# 22. Network Security

The core recovery engine should operate without network access.

Network functionality should be absent unless there is a documented requirement.

If a future feature requires network communication, the feature must explicitly document:

* destination
* protocol
* data transmitted
* authentication
* encryption
* user consent
* failure behavior
* offline behavior

Core recovery must remain functional without that network feature.

---

# 23. Telemetry

Telemetry is disabled by default.

Taphonomy must not silently transmit:

* device identifiers
* file names
* hashes
* filesystem metadata
* recovery statistics
* crash reports
* recovered content

If telemetry is introduced in the future, it must be:

* opt-in
* documented
* privacy reviewed
* disableable
* free from recovered-content transmission by default

---

# 24. Dependency Security

Every dependency introduces additional attack surface.

Dependencies should be added only when they provide meaningful value.

Before introducing a dependency, evaluate:

* purpose
* maintenance status
* license
* security history
* transitive dependencies
* binary requirements
* required privileges
* network behavior
* alternatives

Unused dependencies should be removed.

---

# 25. Supply Chain Security

The project should eventually use:

* locked dependency versions
* reproducible builds where practical
* dependency auditing
* automated vulnerability checks
* verified release artifacts
* signed releases where appropriate

Build tooling must not silently download or execute arbitrary code as part of normal recovery operations.

---

# 26. External Tools

Some recovery functionality may eventually depend on existing tools.

External tools must be treated as separate trust boundaries.

Before integration, document:

* executable identity
* version
* source
* license
* privileges
* input handling
* output handling
* network behavior
* known limitations
* failure behavior

Taphonomy must not blindly trust external tool output.

---

# 27. AI and External Services

AI services must not receive evidence or recovered content by default.

If AI-assisted functionality is ever introduced, it must be architecturally separated from the core recovery engine.

The user must explicitly authorize any transmission of sensitive data.

The default system must remain capable of performing core recovery without external AI services.

---

# 28. Application Data

Application databases can contain highly sensitive information.

Taphonomy must treat application artifacts as untrusted and confidential.

This includes:

* messaging databases
* browser databases
* authentication databases
* SQLite databases
* application caches
* backups

Application parsers must follow the same defensive parsing requirements as filesystem parsers.

---

# 29. Mobile Security

Mobile devices introduce additional security boundaries.

Taphonomy must document:

* acquisition method
* device state
* authentication requirements
* encryption state
* key availability
* permissions
* data obtained
* data not obtained

Taphonomy must not claim that a device is fully recovered when only a limited logical acquisition was obtained.

---

# 30. Encryption

Encrypted evidence presents a fundamental distinction between:

```text
physical access
```

and:

```text
usable plaintext data
```

Possession of storage sectors does not imply possession of the keys required to decrypt them.

Taphonomy must explicitly represent encryption-related limitations.

It must not claim successful recovery when only encrypted or unusable ciphertext has been acquired.

---

# 31. Deleted Data

Deleted data must be treated as untrusted evidence.

A recovered file may contain:

* stale data
* partial data
* unrelated data
* corrupted data
* remnants from another file
* incomplete metadata

Extraction does not establish correctness.

Validation is required before a result is classified as verified.

---

# 32. Cryptographic Integrity

Where cryptographic hashes are used, Taphonomy must document exactly what is being hashed.

Examples:

```text
source image
working image
recovered artifact
metadata manifest
```

Hash values must not be confused with authenticity guarantees.

---

# 33. Crash Handling

Unexpected crashes must not cause:

* source modification
* silent output corruption
* misleading success states
* incomplete manifests being marked valid

Operations should record state sufficiently to determine whether an operation completed successfully.

---

# 34. Interrupted Operations

Taphonomy must safely handle:

* Ctrl+C
* process termination
* system shutdown
* device removal
* power loss
* destination exhaustion

An interrupted operation must be distinguishable from a successful operation.

---

# 35. Security Testing

Security testing must include:

* malformed filesystem images
* malformed headers
* invalid offsets
* integer boundary conditions
* oversized declarations
* path traversal attempts
* malicious filenames
* symbolic-link cases
* corrupted compressed data
* truncated images
* unexpected device removal
* resource exhaustion
* parser fuzzing

Fuzzing should become part of the project as the parser surface grows.

---

# 36. Fuzzing

Binary parsers are strong candidates for fuzz testing.

Future parser implementations should use fuzzing to discover:

* crashes
* panics
* hangs
* memory exhaustion
* unexpected states
* parser inconsistencies

Fuzz inputs must be isolated from real user evidence.

---

# 37. Security Regression Tests

Every confirmed security defect must result in a regression test where practical.

The desired process is:

```text
Vulnerability
     ↓
Reproduce
     ↓
Document
     ↓
Fix
     ↓
Regression test
     ↓
Security review
```

A fix without a regression test is incomplete when a deterministic test can reasonably be created.

---

# 38. Security Incident Response

If Taphonomy is found to expose, corrupt, execute, or mishandle sensitive data:

1. Stop the affected operation.
2. Preserve diagnostic information.
3. Do not continue testing against sensitive evidence.
4. Reproduce using synthetic fixtures.
5. Determine the affected versions.
6. Determine the affected components.
7. Fix the vulnerability.
8. Add regression tests.
9. Document the incident.
10. Review whether the architecture needs to change.

---

# 39. Security Severity

Future security issues should be classified according to their impact.

At minimum:

```text
CRITICAL
HIGH
MEDIUM
LOW
INFORMATIONAL
```

Severity should consider:

* source-data modification
* arbitrary code execution
* sensitive-data exposure
* privilege escalation
* denial of service
* recovery-result corruption
* integrity failures

---

# 40. Security Release Gate

A release must not be considered production-ready if it contains a known critical security issue.

High-severity issues affecting source integrity, arbitrary code execution, or sensitive-data exposure require explicit review before release.

---

# 41. Security and Recovery Are Separate

Security controls must not be bypassed simply because recovery is difficult.

Examples:

```text
Recovery difficulty ≠ permission to weaken security

Missing data ≠ permission to execute recovered files

Corrupted filesystem ≠ permission to trust metadata

Urgent recovery ≠ permission to modify source evidence
```

---

# 42. Security Review Requirement

Before introducing a subsystem that processes a new class of untrusted data, the project should review:

* trust boundaries
* parser attack surface
* resource consumption
* output handling
* temporary data
* logging
* privileges
* dependencies
* network behavior
* testing requirements

The review should be documented when it materially affects architecture.

---

# 43. Guiding Security Principle

> **Treat every byte from the recovery source as untrusted, every recovered artifact as sensitive, and every system privilege as something that must be earned.**

