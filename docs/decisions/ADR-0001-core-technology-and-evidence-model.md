# ADR-0001: Core Technology and Evidence Model

* **Status:** Accepted
* **Date:** 2026-08-28
* **Decision owners:** Taphonomy project
* **Scope:** Core implementation language, evidence model, initial recovery target

---

## 1. Context

Taphonomy is intended to become a local-first data recovery and analysis system.

The system may eventually operate across:

* USB storage
* SD and microSD cards
* HDDs
* SSDs
* Android devices
* iOS devices
* application data
* damaged or partially accessible storage

These domains have substantially different technical and security characteristics.

The system must prioritize:

1. Evidence preservation
2. Correctness
3. Safety
4. Reproducibility
5. Testability
6. Maintainability
7. Performance

The project must also remain suitable as a serious professional engineering project.

The first implementation therefore needs a narrow scope and a strong architectural foundation.

---

# 2. Decision

Taphonomy will initially be implemented in **Rust** as a local-first command-line application.

The recovery and analysis engine will operate on a **read-only evidence abstraction** rather than directly depending on physical devices.

The initial implementation will support **RAW/dd disk images** and target **NTFS** as the first filesystem.

The first recovery capability will be:

> Identify and recover deliberately deleted test files from controlled NTFS disk images without modifying the source evidence.

The initial architecture will separate:

```text
Interface
    ↓
Application
    ↓
Recovery Domain
    ↓
Evidence Abstraction
    ↓
Evidence Source
```

Filesystem-specific functionality will remain separate from the evidence layer.

File carving will remain a separate recovery strategy from filesystem-aware recovery.

Physical-device acquisition will remain outside the initial recovery engine.

---

# 3. Technology Decision

## 3.1 Rust

Rust is selected as the primary implementation language.

### Reasons

Taphonomy will process malformed and potentially hostile binary data.

Rust provides:

* memory safety by default
* strong static typing
* explicit error handling
* deterministic ownership and lifetime rules
* good binary-data support
* strong testing facilities
* fuzzing support
* native performance
* cross-platform support

Memory safety does not guarantee parser correctness. Taphonomy will still require bounds validation, malformed-input testing, fuzzing, invariants, and independent validation.

---

# 4. Alternatives Considered

## 4.1 C

### Advantages

* mature forensic ecosystem
* extensive filesystem libraries
* direct access to low-level operating-system APIs
* existing recovery projects use C extensively

### Disadvantages

* memory safety must be manually maintained
* malformed binary input creates significant memory-safety risk
* ownership and lifetime errors are easier to introduce
* modern testing and dependency management are less ergonomic for this project

### Decision

Not selected as the core implementation language.

Existing C projects remain valuable research and validation references.

---

## 4.2 C++

### Advantages

* mature systems ecosystem
* extensive filesystem and storage libraries
* strong performance
* existing forensic tooling

### Disadvantages

* significantly greater language complexity
* memory-safety hazards remain
* dependency and build complexity
* less attractive safety baseline for a new recovery engine

### Decision

Not selected as the core implementation language.

---

## 4.3 Go

### Advantages

* memory safety
* straightforward tooling
* good concurrency
* simple deployment

### Disadvantages

* less suitable ecosystem for low-level filesystem and forensic parsing
* less control over some low-level representations
* less established forensic ecosystem than C/C++

### Decision

Not selected for the core.

---

## 4.4 Hybrid Rust + C/C++

A hybrid architecture remains possible.

Existing mature libraries may be integrated through carefully reviewed FFI boundaries when they provide significant functionality that would be unreasonable to reproduce.

However, FFI introduces:

* memory-management boundaries
* build complexity
* platform-specific dependencies
* additional security review
* licensing considerations

### Decision

Rust remains the default.

FFI is permitted only when there is a demonstrated technical reason.

---

# 5. Evidence Model

Taphonomy will not allow recovery algorithms to depend directly on physical devices.

The conceptual model is:

```text
Physical Device
      │
      ▼
Acquisition
      │
      ▼
Evidence Image
      │
      ▼
EvidenceSource
      │
      ▼
Analysis
```

The analysis layer must not require knowledge of whether the underlying bytes came from:

* a physical disk
* a RAW image
* an E01 image
* a virtual disk
* a synthetic fixture
* another supported evidence container

---

# 6. Read-Only Evidence Boundary

The initial evidence abstraction must expose only operations required to inspect evidence.

Conceptually:

```text
EvidenceSource
├── size
├── read
└── seek / random access
```

It must not expose a write operation to recovery algorithms.

The intended dependency direction is:

```text
Evidence
   │
   ▼
Read-only Analysis
   │
   ▼
RecoveryArtifact
   │
   ▼
Output
```

The source evidence remains separate from recovered output.

This is a deliberate safety boundary.

---

# 7. Why RAW/dd Is the Initial Image Format

RAW/dd images are selected for the first implementation because they provide:

* simple byte-addressable storage
* deterministic behavior
* minimal format complexity
* easy synthetic test generation
* easy hashing
* easy interoperability
* straightforward debugging

RAW support does not imply that RAW will remain the only supported format.

Future evidence-container support may include:

* E01/EWF
* VHD/VHDX
* AFF4
* VMDK
* QCOW2
* other formats where justified

Each format should be introduced through the same evidence abstraction.

---

# 8. Why NTFS Is the Initial Filesystem

NTFS is selected as the first filesystem because it provides a strong combination of:

* relevance to the initial Windows development environment
* mature technical documentation
* mature forensic tooling
* available Rust implementations
* metadata-rich filesystem structures
* deleted-file recovery opportunities
* fragmented-file recovery challenges
* useful opportunities for independent validation

The selection does not imply that NTFS is the most important filesystem overall.

It is the first controlled target.

---

# 9. Why We Are Not Starting With All Filesystems

Supporting every filesystem immediately would create a large surface area before the evidence model and recovery methodology are validated.

The project will instead follow:

```text
Evidence abstraction
        ↓
Synthetic laboratory
        ↓
One filesystem
        ↓
One recovery capability
        ↓
Validation
        ↓
Regression testing
        ↓
Additional filesystems
```

Additional filesystems require their own technical research.

---

# 10. Why Filesystem Recovery and Carving Are Separate

Filesystem-aware recovery uses structures such as:

* allocation metadata
* directory records
* file records
* extents
* filesystem-specific metadata

File carving can operate when filesystem structures are damaged or unavailable.

Carving may instead depend on:

* file signatures
* structural validation
* content patterns
* extent analysis
* reconstruction heuristics

These are different recovery strategies.

They will therefore remain separate components.

---

# 11. Initial Recovery Target

The first recovery capability is deliberately narrow.

Taphonomy will create controlled NTFS test images containing known files.

Some files will then be deliberately deleted or otherwise altered.

Taphonomy will attempt to:

1. open the evidence image
2. identify the partition/filesystem
3. parse the NTFS structures required for the experiment
4. enumerate relevant file records
5. identify deleted test files
6. recover their data where possible
7. write recovered data to a separate destination
8. validate recovered data against known expected artifacts
9. report the result

The source image must remain unchanged.

---

# 12. Validation Model

A successful byte extraction is not automatically a successful recovery.

Validation will distinguish:

```text
Detected
Recovered
Validated
```

For controlled fixtures, validation may compare:

* cryptographic hashes
* file sizes
* file structure
* known content
* expected metadata
* expected recovery boundaries

A recovered file that differs from the known reference must not be reported as fully validated.

---

# 13. Testing Strategy

Testing will use controlled evidence rather than real personal data.

Initial testing layers:

```text
Unit Tests
     ↓
Component Tests
     ↓
Synthetic Disk Images
     ↓
Recovery Tests
     ↓
Golden Results
     ↓
Regression Tests
```

Fuzzing will be introduced for parsers and other components that process untrusted binary structures.

Real physical devices will not be required for ordinary development testing.

---

# 14. Cross-Validation

Where practical, Taphonomy results should be compared against established tools or independently generated expected results.

Potential reference systems include:

* TestDisk
* PhotoRec
* The Sleuth Kit
* Autopsy
* filesystem-specific tools

Reference-tool output must not automatically be treated as ground truth.

Independent expected results and controlled fixtures remain important.

---

# 15. Existing Projects

Existing recovery and forensic projects will be treated as research references.

They may provide:

* algorithmic insight
* filesystem knowledge
* edge cases
* test ideas
* validation references
* possible integration opportunities

Code reuse requires separate review of:

* license
* dependency tree
* security
* maintenance
* architecture
* compatibility with Taphonomy's goals

No project will be copied wholesale into Taphonomy merely because it already solves part of the problem.

---

# 16. Platform Decision

Development will initially occur on:

```text
Windows
WSL2
Linux userspace
```

WSL2 is considered a development environment, not a required architectural dependency.

Hardware access will be isolated behind platform-specific acquisition functionality.

The recovery engine must remain capable of operating entirely against evidence images.

---

# 17. Mobile Recovery

Android and iOS recovery are explicitly outside the initial implementation.

Mobile recovery involves additional constraints including:

* device security
* encryption
* authentication
* acquisition methods
* filesystem differences
* application sandboxing
* backups
* vendor-specific behavior
* operating-system protections

These will become separate research and architectural tracks.

They must not contaminate the initial desktop-storage recovery architecture.

---

# 18. SSD Recovery

SSD-specific recovery will also remain a separate research track.

Important factors include:

* TRIM
* garbage collection
* wear leveling
* controller behavior
* encryption
* filesystem behavior

A deleted file on an SSD cannot be assumed to have the same recoverability characteristics as a deleted file on a traditional HDD.

Taphonomy must not promise recovery where the underlying storage technology has already made the data unavailable.

---

# 19. Consequences

### Positive

This decision provides:

* a memory-safe core
* a clear evidence boundary
* deterministic testing
* manageable initial scope
* strong separation between acquisition and analysis
* a foundation for future filesystems
* a foundation for future desktop interfaces
* a credible professional engineering architecture

### Negative

The initial implementation will have limited filesystem and evidence-format coverage.

Some mature C/C++ functionality will not be immediately available natively.

Rust FFI may eventually be necessary for selected capabilities.

Supporting additional filesystems will require significant independent engineering work.

---

# 20. Rejected Approaches

The following approaches are rejected for the initial implementation:

### "Build everything at once"

Rejected because it would create a large unvalidated system.

### "Start with a GUI"

Rejected because the recovery engine and evidence model should be validated independently of presentation.

### "Start with a web application"

Rejected because evidence should remain local and the initial problem does not require a web architecture.

### "Start by supporting physical disks"

Rejected because disk-image testing provides a safer and more reproducible development environment.

### "Copy PhotoRec/TestDisk"

Rejected because Taphonomy needs its own architecture, safety model, testing methodology, and maintainable implementation.

### "Use a database for the first version"

Rejected unless a concrete requirement emerges.

Recovery analysis does not initially require a persistent database.

---

# 21. Future Architecture Direction

The long-term conceptual architecture is:

```text
                    TAPHONOMY
                        │
              ┌─────────┴─────────┐
              │                   │
             CLI            Future UI/API
              │                   │
              └─────────┬─────────┘
                        │
                 Application
                        │
                 Recovery Domain
                        │
        ┌───────────────┼────────────────┐
        │               │                │
     Evidence       Filesystem        Carving
       Layer           Layer           Layer
        │               │                │
        └───────────────┼────────────────┘
                        │
                Read-only Evidence
                        │
        ┌───────────────┼────────────────┐
        │               │                │
       RAW             EWF             Future
```

This diagram represents the architectural direction, not a requirement to implement every component immediately.

---

# 22. Decision Review Conditions

This ADR should be revisited if:

* Rust becomes technically unsuitable
* required filesystem support cannot reasonably be implemented or integrated
* a mature dependency changes the integration strategy
* mobile recovery becomes a primary requirement
* a new evidence format becomes a primary requirement
* performance measurements demonstrate a fundamental architectural problem
* the read-only evidence model proves insufficient

Any material change should result in a new ADR rather than silently rewriting this decision.

---

# 23. Final Principle

Taphonomy will prioritize:

```text
Evidence integrity
        ↓
Correctness
        ↓
Validation
        ↓
Safety
        ↓
Maintainability
        ↓
Performance
        ↓
Feature breadth
```

The project will grow from validated capabilities rather than from a large initial feature set.

