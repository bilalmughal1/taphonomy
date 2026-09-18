# Taphonomy

Taphonomy is a local-first digital data recovery system written in Rust.

The project is designed around evidence preservation, correctness, security, reproducibility, and maintainability.

## Project Status

Taphonomy has a validated read-only evidence layer, reads FAT32 root
directories, and recovers the data of an unfragmented deleted file.
Recovered content is extracted to memory and reported as a SHA-256 digest.
No file is written. Where the operator supplies a digest of the file they
are looking for, the recovered digest is compared against it and the result
reported as a match or a difference. Every run also states how much of the
evidence it analysed and what it did not.

Milestones M1 to M9 of ADR-0002 §8 are complete, which is the whole of that
sequence: evidence images are
opened read-only and hashed, MBR partition tables are parsed with every
declared extent validated against the true evidence size, GPT is detected
and reported as unsupported, filesystems are identified from volume
structure, FAT32 boot sectors are parsed and validated against the
partition extent they occupy, the root directory is enumerated by walking
its cluster chain, the deleted entries within it are identified, the
content of an eligible deleted file is read and hashed, and that digest is
compared against a reference the operator supplies, and the run reports
what it recovered, what it could not, and how much of the evidence it
reached.

A deleted entry is reported with whatever fields deletion left intact.
Where a long-name entry survives alongside it, the first character of its
short name is recovered from the checksum that entry carries, which
determines the destroyed byte rather than narrowing it. Where none
survives, that is reported rather than guessed.

Deletion zeroes the cluster chain in every FAT, so nothing in the evidence
says a deleted file was unfragmented. Taphonomy computes the run the entry
implies, reads every cluster of that run in the active FAT, and refuses the
recovery where any of them is in use, naming the cluster that caused the
refusal. A run of free clusters means only that nothing has claimed those
clusters since the deletion, which is not evidence that the content there
is the file's, and the tool says so rather than leaving it to be inferred.
Reading the content requires `--recover`.

A digest on its own says only what was read. Comparing it against a
reference the operator holds is the only evidence available that the run
read was the file's clusters, and it is evidence about that one recovery
rather than about the assumption in general. Where the content is not
distinctive, a match establishes less than it appears to, and the tool says
so. Where the digests differ, the four conditions that could have caused it
are listed and none is chosen. The tool never looks a reference up: it
compares against what it was given, and reports that nothing was validated
when it was given nothing. See
`docs/decisions/ADR-0013-m8-reference-validation-decisions.md`.

A run states its own coverage. A directory that was listed and not read, a
partition whose filesystem is not FAT32, a boot sector refused: each is
counted, named, and reported, and the run is described as having covered
the evidence completely, incompletely, or not at all. Coverage is about
reach and not about correctness. A run can cover everything it could reach
and still recover, for a fragmented deleted file, content that is not that
file's. Every artifact carries one confidence level, `RECONSTRUCTED`, and a
digest match is reported beside it rather than raising it. See
`docs/decisions/ADR-0014-m9-classification-decisions.md`.

The initial development sequence is:

1. ~~Establish project, safety, security, and architecture contracts.~~ Done.
2. ~~Build a read-only evidence abstraction.~~ Done.
3. ~~Build a synthetic evidence laboratory.~~ Done.
4. ~~Implement one narrowly defined recovery capability.~~ Done.
5. Validate recovery accuracy, including false positives.
6. Add regression, property, integration, and fuzz testing where appropriate.
7. Expand recovery capabilities based on research and measured results.

Item 5 is not complete, though the failure that matters most is now
measured. A deleted file that was fragmented, whose clusters have since
been freed, produces a run that passes the allocation check and a digest
that is plausible and wrong. One fixture now produces that case, built from
ordinary file operations rather than by editing an image, and on it three
of five deleted entries recover content that is not their own file's while
the tool reports all five identically. That is one arrangement of many.
`docs/development/KNOWN_ISSUES.md` records which remain unmeasured.

## Exit Status

| Code | Meaning |
| --- | --- |
| 0 | The evidence was analysed, completely or in part |
| 1 | The evidence could not be opened or hashed |
| 2 | An argument error; no evidence was opened |
| 3 | Nothing past the evidence digest was analysed |

A gap in coverage does not by itself change the status. While subdirectories
are not read, a volume holding one is analysed in part and exits 0, and the
gap is reported on standard output. A digest that differs from the reference
is a finding about the evidence and also exits 0.

---

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

* recovered content is reported as a digest and is not written to a file
* only FAT32 is parsed beyond the partition table; exFAT and NTFS are
  identified and reported as unsupported
* FAT32 support stops at the root directory; subdirectories are not read,
  and every directory listed is reported as a gap in the run's coverage
* only an unfragmented deleted file is recovered; a run reaching an
  allocated cluster is refused rather than reconstructed
* recovered content is compared only against a reference the operator
  supplies; the tool never discovers one
* one confidence level is reachable, so every artifact carries the same one
  and a reference match does not raise it
* the long name of a deleted entry is not decoded; only its short name is
  recovered, and only where a long-name entry survives to determine it
* only MBR partition tables are parsed; GPT is detected but not parsed
* only 512-byte sectors are supported
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

Copyright (c) 2026 Fahad Bilal Saleem.

Taphonomy is distributed under the GNU General Public License, version 3 or
later. See `LICENSE`.

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

