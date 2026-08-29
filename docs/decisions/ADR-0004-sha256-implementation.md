# ADR-0004: SHA-256 Implementation and First Dependency

* **Status:** Accepted
* **Date:** 2026-08-29
* **Decision owners:** Taphonomy project
* **Scope:** Cryptographic hashing implementation; first third-party dependency
* **Related:** `SAFETY.md` §9, §10; ADR-0003 §3.2; `DEVELOPMENT_ENVIRONMENT.md` §15, §16, §17

---

## 1. Context

Cryptographic hashing is a stated requirement, not an optional capability.

| Source | Requirement |
|---|---|
| `SAFETY.md` §9 | Acquisition operations must support cryptographic integrity verification, at minimum SHA-256 |
| `SAFETY.md` §10 | Source, image, recovered-file and validation hashes must be distinguishable |
| ADR-0003 §3.2 | `VERIFIED` is defined as a cryptographic hash match against a reference artifact |
| ADR-0002 §8 (M1) | The first milestone requires hashing a RAW evidence image |

Milestone M1 cannot be implemented without SHA-256.

The Rust standard library provides no cryptographic hash functions. `std`
offers only `DefaultHasher`, which is a non-cryptographic hasher intended for
hash maps, is explicitly documented as unspecified and unstable across
releases, and must not be used for integrity verification.

An implementation must therefore be either written or adopted.

---

## 2. Decision

**Adopt the `sha2` crate (RustCrypto), version 0.11.0.**

This is the project's first third-party dependency.

Its use will be confined behind an internal hashing abstraction in the
infrastructure layer so that the implementation can be replaced without
changes to the domain layer.

---

## 3. Evaluation Against DEVELOPMENT_ENVIRONMENT.md §15

Answered from measurement, not assumption.

### 3.1 Does the standard library provide it?

No. See §1.

### 3.2 Is it actively maintained?

Yes. RustCrypto is the primary maintainer of cryptographic primitives in the
Rust ecosystem. Version 0.11.0 is current.

### 3.3 Is it widely used or technically credible?

Yes. `sha2` is the de-facto SHA-2 implementation in the Rust ecosystem and is
a transitive dependency of a large fraction of the crate registry, including
Cargo itself.

### 3.4 What is its license?

`MIT OR Apache-2.0`. Compatible with the project's Apache-2.0 license and
present on the preferred list in `DEVELOPMENT_ENVIRONMENT.md` §16.

### 3.5 What are its transitive dependencies?

Nine crates, measured with `cargo tree --format "{p} {l}"`:

```text
taphonomy v0.1.0                     Apache-2.0
└── sha2 v0.11.0                     MIT OR Apache-2.0
    ├── cfg-if v1.0.4                MIT OR Apache-2.0
    ├── cpufeatures v0.3.1           MIT OR Apache-2.0
    └── digest v0.11.3               MIT OR Apache-2.0
        ├── block-buffer v0.12.1     MIT OR Apache-2.0
        │   └── hybrid-array v0.4.14 MIT OR Apache-2.0
        │       └── typenum v1.20.1  MIT OR Apache-2.0
        ├── const-oid v0.10.2        Apache-2.0 OR MIT
        └── crypto-common v0.2.2     MIT OR Apache-2.0
            └── hybrid-array v0.4.14 MIT OR Apache-2.0
```

Every crate in the tree is permissively dual-licensed. No license in the graph
requires review by exception under `DEVELOPMENT_ENVIRONMENT.md` §16.

`cargo add` reported `libc` during resolution. It does not appear in the
resolved tree for `x86_64-unknown-linux-gnu` and is a platform-conditional
dependency of `cpufeatures`. It is recorded in `Cargo.lock` and may become
active on other targets.

### 3.6 Does it introduce unsafe code?

**Yes.** This is the principal cost of the decision and is recorded rather
than minimised.

Occurrences of the token `unsafe` in `.rs` files in each crate's registry
source:

| Crate | Occurrences |
|---|---|
| `sha2` 0.11.0 | 53 |
| `hybrid-array` 0.4.14 | 44 |
| `block-buffer` 0.12.1 | 22 |
| `cpufeatures` 0.3.1 | 11 |
| `const-oid` 0.10.2 | 1 |
| `digest` 0.11.3 | 0 |
| `crypto-common` 0.2.2 | 0 |
| `typenum` 1.20.1 | 0 |
| `cfg-if` 1.0.4 | 0 |
| **Total** | **131** |

This is an **upper bound**. The measurement counts the token wherever it
appears, including comments, documentation, and safety-invariant annotations,
and does not distinguish an `unsafe` block from an `unsafe fn` declaration.
`cargo-geiger` was attempted and could not be installed without adding system
packages; a more precise count was not obtained.

The project's `unsafe_code = "deny"` lint applies only to the `taphonomy`
crate. It does not and cannot constrain dependencies.

`DEVELOPMENT_ENVIRONMENT.md` §17 requires that unsafe code have a clear
reason, small scope, documented invariants, and test coverage. The project
cannot enforce this on third-party code. It is accepted here on the basis that
these crates are widely reviewed and heavily exercised across the ecosystem,
which is a weaker guarantee than direct review and is stated as such.

### 3.7 Does it materially increase attack surface?

Limited. Hashing is a pure function over a byte sequence. It performs no I/O,
allocates no unbounded buffers on input, and parses no untrusted structure.

The threat model in `SECURITY.md` concerns malformed evidence reaching
parsers. A hash function has no parser. Malformed input produces a different
digest, which is the correct behaviour.

### 3.8 Does it lock Taphonomy into a specific architecture?

No. `sha2` implements the `Digest` trait from `digest`. Substituting another
implementation of the same trait is mechanical.

### 3.9 Can it be isolated behind an internal abstraction?

Yes, and it must be. `ARCHITECTURE.md` places external interaction in the
infrastructure layer. The domain layer must depend on a project-defined
hashing interface, never on `sha2` directly.

This is a binding condition of this decision.

### 3.10 Is it required now, or only for a hypothetical future feature?

Required now. M1 cannot be implemented without it.

---

## 4. Alternatives Considered

### 4.1 Hand-written SHA-256

Rejected.

Approximately 150 lines and testable against NIST FIPS 180-4 vectors. It would
avoid all third-party unsafe code and provide implementation understanding.

Rejected because ADR-0003 §3.2 makes a hash match the sole basis of the
`VERIFIED` classification. A defect in a hand-written implementation produces a
confidently incorrect verdict with no observable symptom, which is precisely
the false-positive failure mode `PROJECT.md` §5 and `ARCHITECTURE.md` §36
exist to prevent.

The implementer is writing their first Rust program. Cryptographic primitive
implementation is not an appropriate first-week task for code that evidence
classification depends on.

### 4.2 A different SHA-256 crate

Considered and not pursued. `ring` and `openssl` both bind to C libraries,
adding build-time system dependencies and a substantially larger unsafe
surface. `sha2` is pure Rust with no system dependencies, which also preserves
`DEVELOPMENT_ENVIRONMENT.md` §28's reproducibility requirement.

### 4.3 Deferring hashing

Rejected. `SAFETY.md` §9 makes integrity verification part of acquisition, and
ADR-0002 §8 places it in M1. Deferring it would mean the first milestone
produces an unverifiable result.

---

## 5. Conditions of Acceptance

1. `sha2` is used only within the infrastructure layer, behind a
   project-defined hashing interface.
2. The domain layer does not name `sha2` or `digest` types.
3. `Cargo.lock` remains tracked, per `DEVELOPMENT_ENVIRONMENT.md` §6.
4. Version updates are deliberate and reviewed, not automatic.
5. Hashing behaviour is verified against published NIST FIPS 180-4 test
   vectors in the project's own test suite. Correctness of the dependency is
   not assumed.

Condition 5 is the mitigation for §3.6. The project cannot audit the
dependency's unsafe code, but it can verify that the dependency produces
correct output for known inputs.

---

## 6. Consequences

### Positive

* M1 is unblocked
* All licenses in the tree are permissive and compatible
* No system dependencies; build remains reproducible
* Implementation is replaceable behind a trait

### Negative

* 131 occurrences of `unsafe` (upper bound) enter the dependency graph,
  none reviewed by the project
* Nine crates enter the build for one function
* The project depends on external maintenance for a component that
  evidence classification rests on

---

## 7. Precedent

This ADR establishes the pattern for dependency adoption in Taphonomy:
measured evidence against the §15 checklist, licenses obtained from tooling
rather than memory, unsafe surface quantified and stated as a cost, and
conditions of acceptance recorded.

Future dependency decisions follow this form.

---

## 8. Review Trigger

Revisit if:

* `sha2` is unmaintained or a security advisory is issued against it
* the RustCrypto license terms change
* a `std` cryptographic hashing API becomes available
* NIST vector tests fail after a version update
