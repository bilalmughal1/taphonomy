# Taphonomy

Taphonomy is a local-first digital data recovery system written in Rust.

The project is designed around evidence preservation, correctness, security, reproducibility, and maintainability.

## Project Status

Taphonomy has a validated read-only evidence layer. No recovery capability is
implemented yet.

Milestone M1 of ADR-0002 §8 is complete: evidence images are opened
read-only, hashed with SHA-256, and their size is reported; hashing is
verified against NIST FIPS 180-4 vectors; evidence immutability is verified
by test.

The initial development sequence is:

1. ~~Establish project, safety, security, and architecture contracts.~~ Done.
2. ~~Build a read-only evidence abstraction.~~ Done.
3. Build a synthetic evidence laboratory.
4. Implement one narrowly defined recovery capability.
5. Validate recovery accuracy, including false positives.
6. Add regression, property, integration, and fuzz testing where appropriate.
7. Expand recovery capabilities based on research and measured results.

## Initial Recovery Scope

The first recovery path is intended to target:

* disk-image evidence
* RAW/DD images
* FAT32
* deleted controlled-test files
* read-only evidence access
* deterministic recovery output

The initial filesystem target is FAT32, not NTFS; see
`docs/decisions/ADR-0002-initial-filesystem-target.md`. The implementation
order is FAT32, then exFAT, then NTFS.

Physical-device recovery is outside the initial implementation scope.

## Design Priorities

Taphonomy prioritizes:

* evidence preservation
* correctness
* security
* reproducibility
* deterministic behavior
* maintainability

Performance and feature breadth come after correctness and validation.

## Architecture

The initial dependency direction is:

```text
CLI
 ↓
Application
 ↓
Domain
 ↓
Infrastructure
```

The CLI is an interface to the application layer. Recovery logic must remain independent of CLI parsing and presentation.

Infrastructure is responsible for controlled interaction with evidence and other system resources.

The architecture is intentionally kept small while the first recovery path is being validated.

As implemented, the separation is between the CLI binary and the library
crate. The library is not yet subdivided into the layers above; that
subdivision will be introduced when a capability requires it, not in
advance.

## Evidence Model

Taphonomy distinguishes between:

```text
Source Evidence
      ↓
Working Evidence
      ↓
Analysis
      ↓
Recovery Output
```

Source evidence must remain unmodified.

Development and testing use controlled evidence such as synthetic disk images, generated files, corrupted fixtures, disposable virtual disks, and known-good reference artifacts.

Real personal recovery evidence must not be committed to the repository.

## Safety

Taphonomy treats recovery evidence as potentially valuable and untrusted input.

Normal development must not:

* modify source evidence
* write recovered output to source evidence
* format or repartition source devices
* perform destructive filesystem operations
* transmit evidence to external services
* assume that a device path identifies the same physical device throughout an operation

Operations involving ambiguous or potentially destructive evidence access must fail explicitly rather than guessing.

See [docs/SAFETY.md](docs/SAFETY.md) for the detailed safety model.

## Security

Recovery evidence is treated as untrusted input.

The implementation must account for risks including:

* out-of-bounds reads
* integer overflow and underflow
* malformed filesystem metadata
* malicious filenames
* path traversal
* excessive memory allocation
* recursion exhaustion
* CPU exhaustion
* malicious compressed data
* symbolic-link attacks

See [SECURITY.md](SECURITY.md) for the security requirements.

## Development Environment

The initial development environment is:

* Rust 1.98.0
* Rustfmt
* Clippy
* Linux userspace under WSL2
* Windows host environment

Platform-specific behavior must be validated separately where WSL2 differs from native Linux hardware access.

## Testing Philosophy

Recovery functionality must be tested against controlled evidence.

Testing should verify both:

* successful recovery
* incorrect recovery and false positives

A recovery result that appears plausible but is incorrect is considered a failure.

Depending on the capability, testing may include:

* unit tests
* component tests
* integration tests
* filesystem fixture tests
* end-to-end tests
* property-based tests
* fuzz tests
* regression tests

Recovery experiments should record enough information to reproduce their results.

## Research

Recovery algorithms should be based on documented filesystem and storage behavior rather than assumptions.

Research may include established recovery software and filesystem implementations, including:

* TestDisk
* PhotoRec
* The Sleuth Kit
* Autopsy
* Foremost
* Scalpel
* filesystem-specific open-source implementations

Third-party software is used as a research reference unless its implementation is independently evaluated for licensing, security, correctness, and compatibility.

Material research findings are recorded in:

```text
docs/development/RESEARCH_LOG.md
```

## Experiments

Controlled experiments are recorded in:

```text
docs/development/EXPERIMENTS.md
```

Experiments should document:

* question
* hypothesis
* input
* method
* expected result
* actual result
* conclusion
* next action

Experiments must use controlled evidence.

## Documentation

Important project contracts are maintained in:

```text
docs/PROJECT.md
docs/SAFETY.md
SECURITY.md
docs/ARCHITECTURE.md
docs/development/RESEARCH_LOG.md
docs/development/EXPERIMENTS.md
```

Architecture decisions that materially affect the system are recorded as Architecture Decision Records under:

```text
docs/decisions/
```

## Current Limitations

At this stage:

* no recovery capability is implemented
* no filesystem parsing is implemented
* partition detection is not implemented
* physical-device recovery is not supported
* recovery accuracy has not yet been established
* filesystem support has not yet been validated
* production recovery workflows have not been established
* performance characteristics have not yet been measured

These limitations will change only when implementation and validation provide evidence for doing so.

## Contributing

Development follows the engineering and safety requirements documented in `CLAUDE.md` and `CONTRIBUTING.md`.

Changes should be focused, tested, documented where required, and reviewed before committing.

## License

Taphonomy is distributed under the license specified in `LICENSE`.

## Status Terminology

Project documentation distinguishes between:

* **Implemented**: code exists for the described behavior.
* **Tested**: automated or controlled tests have been run.
* **Experimentally validated**: controlled evidence supports the behavior.
* **Partially supported**: only a defined subset has been implemented or validated.
* **Untested**: implementation exists but relevant validation has not been performed.
* **Known limitation**: a documented limitation is understood.
* **Planned**: intended future work that has not been implemented.

Claims about recovery capability should use these terms accurately.

## Repository

Taphonomy is currently under active development.

The immediate goal is to establish a safe, reproducible foundation before implementing recovery functionality.

