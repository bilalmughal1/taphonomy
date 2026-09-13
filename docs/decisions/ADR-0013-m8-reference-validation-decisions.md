# ADR-0013: M8 Reference Validation Decisions

* **Status:** Proposed
* **Date:** 2026-09-09
* **Decision owners:** Taphonomy project
* **Scope:** What a reference is and where it comes from; whether a reference
  may be a file; what a comparison outcome is and what it establishes; how a
  mismatch is reported and whether it stops the run; where the code lives;
  how the reference reaches the CLI; what M8 does not do
* **Related:** ADR-0002 §8 (M8, M9); ADR-0003 §2, §3.1, §3.2, §4.2, §4.4,
  §4.6, §4.7, §5, §8; ADR-0004 §5 conditions 1, 2; ADR-0010 §3.1, §3.2,
  Decision B, Decision C, Decision E, Appendix B3, Appendix C4, §14;
  ADR-0011 §3, §11; `ARCHITECTURE.md` §5.6; `PROJECT.md` §6.4, §6.9;
  `SAFETY.md` §10, §12, §13, §14, §18; `SECURITY.md` §5, §20, §31,
  §32, §42; `CLAUDE.md` §11, §14, §25, §26, §39; EXP-0003
* **Basis commit:** `16507a0`. Every line reference in this record is to the
  tree at that commit.

---

## 1. Context

`ADR-0002` §8 states M8 as: validate recovered data against a known-good
reference.

M7 ends with a digest. `src/fat_recovery.rs:384` defines `Extraction`, which
pairs a `Sha256Digest` with the byte count it covers, and `src/main.rs:347`
prints it. Nothing consumes it further. The digest is a true statement about
bytes that were read, and it says nothing about whether those bytes are the
deleted file's content.

The question this milestone turns on is not how to compare two digests. It
is where the second digest comes from, and what a comparison licenses the
tool to say. `ADR-0003` §3.2 has held the answer since before any recovery
code existed: `VERIFIED` requires a match against a known-good reference,
and `STRUCTURALLY_VALID` exists precisely for the case where no reference
exists for comparison.

An earlier framing of this milestone held that a reference is rarely
available in the field, on the reasoning that a file with a surviving
known-good copy would not need recovering. That reasoning is rejected here.
Recovery is not only retrieval. A substantial share of the questions put to
a forensic tool are questions of attribution — whether a deleted file on a
device is the same file as a known exhibit — and in those the reference
exists by construction, because it is the exhibit. Hash sets exist for the
same reason. M8 as literally worded in `ADR-0002` §8 is therefore a real
capability with a real user, and needs no reinterpretation.

## 2. Basis

### 2.1 Measured state at `16507a0`

* Nothing in `src/` outside `#[cfg(test)]` compares two digests. The only
  comparison mechanism is `#[derive(PartialEq, Eq)]` at `src/hash.rs:29`.
* Nothing in the crate reads the internal structure of a file format. Every
  signature constant is a FAT32 or MBR structure.
* Neither `src/` nor `tests/` contains any of the six confidence level names
  from `ADR-0003`, or the word `confidence`. `ADR-0003` is undelivered in
  full.
* `Sha256Digest::from_hex` does not exist. `to_hex` exists at
  `src/hash.rs:47` and emits lowercase. There is no hex parser anywhere in
  the crate.
* `Sha256Digest::from_bytes` exists at `src/hash.rs:37`, is `const`, and its
  doc comment states it is intended for test vectors and for reading
  recorded digests.
* `src/main.rs:34` is `for arg in args`, which consumes the argument
  iterator by value and therefore cannot read a flag's value.
* `recover: bool` is threaded through four functions: `inspect`
  (`src/main.rs:54`), `report_partition` (126), `report_root_directory`
  (228), `report_recovery` (282).
* `src/error.rs:15` defines `Error` with four variants, every one of which
  carries a `PathBuf`. The crate's convention is a per-module error enum:
  `Fat32Error`, `DirectoryError`, `RecoveryError`, `ParseError`.

### 2.2 What the CLI walks

`src/main.rs` iterates every partition, and within each, both
`root.entries` and `root.residue`. A single command-line reference is
therefore compared against every extraction the run performs. This is not
incidental to the design; §12 makes it a decision.

## 3. A mismatch is a finding, not a failed operation

This section resolves an apparent conflict with a constraining document, and
it precedes the decisions because every decision below depends on it.

`SAFETY.md:275` requires that Taphonomy fail closed, stopping rather than
continuing with assumptions when an operation meets uncertainty.
`SAFETY.md:287` lists `Hash mismatch` among the examples. `SAFETY.md` is a
constraining document under `ADR-0011` §3, which states that where a
constraining document and the code disagree, the code is wrong. Read in
isolation, `SAFETY.md:287` requires a reference mismatch to halt the run.

`CLAUDE.md:331` raises the same question from the other side, listing
`validation failure` among the conditions error handling must differentiate.

Both are read here as follows.

**3.1** `SAFETY.md` §12's example list is scoped to operations against a
device or an acquisition. Its neighbours are unknown source device,
ambiguous source path, unexpected device size, unexpected device removal and
corrupted acquisition. The hash mismatch it names is the integrity mismatch
between a source and its image, which is `SAFETY.md` §9's subject. A
reference digest that differs from a recovered digest is not a failure of
the operation that read the bytes; the read succeeded and its result is
exact.

**3.2** `ADR-0003:143` already draws this line for the project: an operation
may complete with status `SUCCESS` while producing artifacts classified
`UNVERIFIED`. Confidence classifies an artifact; `SAFETY.md` §13's statuses
classify an operation; the two axes are separate and must not be merged.

**3.3** `ADR-0010` Appendix B3 applies the same split in code. A fact about
the evidence travels in the `Ok` arm; only a fault in reading the evidence
becomes an error. `assess` at `src/fat_recovery.rs:348` already returns
`Ok(None)` for expected absence, and `CLAUDE.md:331`'s own list contains
`expected absence` alongside `validation failure`, which shows that list
asks for conditions to be distinguished rather than for all of them to be
errors.

**3.4** What does fail closed in M8 is a malformed reference digest. It
stops the run before a single byte of evidence is read, with a non-zero exit
status. This is the fail-closed behaviour `SAFETY.md` §12 requires of M8,
and it is the only one.

**3.5** `SECURITY.md:641` states that validation is required before a result
is classified as verified. That is the M8-then-M9 ordering stated in a
constraining document outside the ADR series, and it is the reason the
decisions below produce evidence and never a classification.

## 4. Decisions

* **A.** The reference is supplied by the operator. The tool never discovers
  one.
* **B.** In M8 a reference is a digest, never a file.
* **C.** A comparison has three outcomes, and none of them is a confidence
  level.
* **D.** A match is the only evidence this project can produce that the
  contiguity assumption held for a given file.
* **E.** A mismatch is reported as a mismatch, and its cause is not chosen.
* **F.** A validation record states what the comparison covers.
* **G.** The hex parser lives in `src/hash.rs`; the comparison lives in a
  new `src/validation.rs`.
* **H.** A reference supplied without `--recover` is an argument error.
* **I.** M8 reports per entry and prints no aggregate line.
* **J.** Argument parsing changes shape, and the ceiling at which it is
  replaced is stated.

## 5. Decision A: the operator supplies the reference

The tool accepts a reference from the command line. It does not consult a
hash set, query a service, search the host filesystem, or infer a reference
from anything in the evidence.

**Reason.** A reference derived from the evidence is not a reference; it is
the recovery restated, and comparing a recovery against itself is the
self-referential evidence `PROJECT.md` §6.4 rejects. A reference obtained
over a network is prohibited outright by `SAFETY.md` §19 and
`SECURITY.md` §22. A reference discovered on the host filesystem would make
the tool's finding depend on state the operator did not state, which
`CLAUDE.md:816` rules against in preferring explicit failure to a best
guess.

**Consequence.** When no reference is supplied, the tool has nothing to
validate against and says so. It does not treat the absence as a pass, per
`ADR-0003` §4.4.

## 6. Decision B: a digest, not a file

The flag is `--reference-digest <hex>`. `--reference-file` is not
implemented in M8.

**Reason.** `SECURITY.md:104` lists what must be treated as untrusted:
storage devices, images, filesystem metadata, filenames, recovered files,
archive contents. Operator-supplied command-line input is not on that list,
and `SECURITY.md:113` places validated configuration among the potentially
trusted. A 64-character hex string introduces no new class of untrusted
data. Opening and hashing a non-evidence file from the host filesystem does,
and `SECURITY.md:822` requires a documented review of trust boundaries,
parser surface, resource consumption and privileges before such a subsystem
is introduced. `--reference-file` is that review before it is a feature.

**Reason, second.** The constructor for this already exists. `from_bytes` at
`src/hash.rs:37` was written in M7 with the words "for reading recorded
digests" in its doc comment. M8 needs only the parse that feeds it.

### 6.1 What the parser accepts

Exactly 64 hexadecimal characters, upper or lower case, and nothing else. It
rejects: any other length, any non-hexadecimal character, leading or
trailing whitespace, a `0x` prefix, and a pasted `sha256sum` line consisting
of a digest, separator and filename.

**On the asymmetry.** Case is accepted in both forms because hexadecimal is
case-insensitive and no misreading is possible; `to_hex` at
`src/hash.rs:47` emits lowercase, so lowercase is canonical and uppercase is
a convenience with no cost. A trailing filename is rejected because
silently discarding part of the operator's input is how a tool ends up
comparing against something other than what the operator believed they
supplied. Strictness is applied where a leniency could change the meaning
of a finding, and not elsewhere.

### 6.2 The parse error type

`src/error.rs:15`'s `Error` is scoped to evidence access and every variant
carries a `PathBuf`. A hex parse failure has no path and does not belong
there. Following the crate's per-module convention, the parser gets its own
small error enum in `src/hash.rs`, distinguishing wrong length from invalid
character, and reporting the offending position for the latter so the
operator can find it in a long string.

It surfaces in `main` before `inspect` is called, so `inspect`'s return type
and `taphonomy::Error` are unchanged.

## 7. Decision C: three outcomes, none of them a level

A comparison yields exactly one of:

* the digests are equal;
* the digests differ;
* no comparison was attempted, because no reference was supplied.

The third is a first-class outcome, not a silence and not an error.

None of the three is one of `ADR-0003`'s six levels, and no level name
appears in M8's code. `ADR-0003:181` states the taxonomy becomes binding on
the first code that assigns a confidence level, which under `ADR-0002` §8 is
M9. `ADR-0003:42` says the classification is the validator's output; that is
read here as the validator producing the evidence a classification is
computed from, not the name itself. M8 produces the evidence. M9 assigns the
name, applies §4.1's single-level rule, §4.4's fail-closed default and
§4.7's aggregate reporting.

`ADR-0003` §2 should carry an appendix recording this reading, so that M9
does not inherit an ADR whose §2 can be read against the code it is about to
constrain. That appendix is an action of this record, not of M9.

## 8. Decision D: what a match establishes

A match establishes that the bytes read are byte-for-byte identical to the
reference supplied. It establishes nothing about the operator's reference
being the file they believe it to be.

`SAFETY.md:241` states this already: matching hashes establish byte equality
between the hashed objects, and do not establish that a recovered file
represents the user's intended original content. That sentence is quoted
rather than paraphrased because it is the ceiling on every output line this
milestone adds.

**8.1 The consequence M7 could not reach.** `ADR-0010` Decision C
established that the tool computes the run an entry implies and refuses to
assert that the run is the file's, because on FAT32 the cluster chain is
destroyed at deletion — measured in EXP-0003 — and nothing in the evidence
can confirm contiguity. A digest match confirms it, retroactively, from
outside the evidence: if the bytes read equal the known file, the clusters
read held the file's content.

This is the only mechanism available to this project for that confirmation,
and it is bounded in two ways. It is evidence about one recovery, never
about the assumption in general, and it can never be carried to the next
file. And it is only as strong as the content is distinctive.

**8.2 Degenerate content.** Where the content is not distinctive — an
all-zero region, or any byte sequence common enough to occur elsewhere on
the volume — a match establishes much less, because a different run could
hold identical bytes. `ADR-0003` §4.2 forbids raising a level on the
strength of absent contrary evidence; this is its mirror, and the tool must
not report a match on a kilobyte of zeroes as though it were a match on a
photograph. M8 does not attempt to measure distinctiveness. It states the
limitation in its output caveat, and leaves any measure of entropy or
commonality to a later milestone that can justify one.

## 9. Decision E: a mismatch names no cause

When the digests differ, the tool reports that they differ. It does not
attribute the difference.

A differing digest is consistent with at least four conditions, and the
evidence does not distinguish them: the file was fragmented and the implied
run was not its content; clusters of the run were reused after deletion
without the FAT recording it; the directory entry's size field does not
describe the content; or the reference is a different file. The tool may
enumerate these. It must not choose among them.

This is `ADR-0010` §10.6's withdrawn recommendation returning in the form it
belongs in. That recommendation would have written a wrong digest into a
fixture as an expected value, which was rejected. A real mismatch against an
operator's reference is the legitimate version of the same signal, and the
discipline is the same: report the observation, not an inferred cause.

## 10. Decision F: a record states what it covers

`Extraction` at `src/fat_recovery.rs:384` pairs a digest with
`bytes_hashed` so that a digest is never separated from a statement of its
extent. M8's validation record does the same: it carries the outcome, the
recovered digest, the reference digest, and the byte count the comparison
covers.

`SECURITY.md:645` requires that where cryptographic hashes are used, the
project documents exactly what is being hashed, and that hash values are not
confused with authenticity guarantees. `ADR-0003` §4.6 says the same of
`VERIFIED`. A record that says "matched" without saying over what could be
read as either.

## 11. Decision G: where the code lives

`Sha256Digest::from_hex` goes in `src/hash.rs`. The comparison goes in a new
module, `src/validation.rs`.

**Reason for the parser's home.** `ADR-0010` Appendix C4 established that a
shared helper lives in the module that owns the concept. `hash.rs` owns
`Sha256Digest` and already owns the outbound conversion at line 47.

**Reason for the module.** Comparing two digests is not a FAT32 operation.
`ADR-0010` Appendix B1 names modules for the structures they read, which
puts filesystem-agnostic code outside the `fat_` modules. `CLAUDE.md:268`
requires that the CLI contain no recovery algorithms and that logic be
callable independently of CLI parsing, so the comparison cannot live in
`src/main.rs`.

`ARCHITECTURE.md:193` already specifies a Validation component, and
`ARCHITECTURE.md:206` states that validation must remain separate from
extraction, with line 208 adding that successfully extracting bytes does not
automatically mean those bytes represent a valid recovered artifact. This
decision is a restatement of that section, not an invention.

**11.1 A documented expectation is falsified by this.** `ADR-0011:338`
records that nothing tracks which `ARCHITECTURE.md` sections have crossed
from intent into binding, and predicts §18's Output Writer will be the
first. Creating `src/validation.rs` makes **§5.6 the first**, ahead of §18.
`ARCHITECTURE.md:197` heads its list "Potential responsibilities", which is
why the crossing does not make M8 owe format validation, reconstruction
confidence or the rest of that list on the day the module appears. Both
facts are recorded here because `ADR-0011` §11 says nothing else tracks
them.

**11.2 The module performs no I/O.** It takes two digests and a byte count
and returns an outcome. It opens nothing, writes nothing, and touches no
evidence, so it adds no path to the read-only guarantee covered by
`tests/read_only.rs`.

## 12. Decisions H and I: how the reference reaches a finding

**H.** `--reference-digest` supplied without `--recover` exits non-zero with
a message. It does not imply `--recover`.

`--recover` exists because `ADR-0010` Decision B made reading a deleted
file's content opt-in, so that the default invocation reports what it found
without touching content. A flag that silently enabled content reading would
undo that decision without recording it. `CLAUDE.md:816` prefers explicit
failure to a best guess, and `CLAUDE.md:825` states the preference in those
words.

**I.** The reference is compared against every extraction the run performs,
and each comparison is reported on the entry it belongs to. No summary line
is printed.

The CLI walks every partition and both `root.entries` and `root.residue`, so
one reference meets many extractions. That is the useful behaviour — whether
a known exhibit is present as a deleted file anywhere on the volume is a
real question, and this answers it — but it has two consequences. A
non-match on one entry is one comparison among many and must not read as a
finding about that entry's identity beyond byte inequality with the
reference. And "no entry matched" is a statement about the whole run, which
is aggregate reporting under `ADR-0003` §4.7, which is M9's. M8 therefore
prints per-entry lines only, and the operator reads them.

## 13. Decision J: argument parsing

`src/main.rs:34`'s `for arg in args` becomes `while let Some(arg) =
args.next()`, so a flag's value can be read inside the loop. The
`recover: bool` parameter threaded through `inspect`, `report_partition`,
`report_root_directory` and `report_recovery` becomes a small options struct
carrying both the recover flag and the optional reference, in its own commit
with no behaviour change.

`src/main.rs:29` comments that one boolean does not call for an argument
parser. That reasoning holds for two flags and will not hold indefinitely.
**The ceiling stated here: when a third flag is added, or when any flag
takes more than one value, hand-rolled parsing is reconsidered against a
dependency under `CLAUDE.md` §20.** Recording the ceiling is the point; the
next milestone should not have to rediscover that the comment was load
bearing.

## 14. What M8 does not do

* No format parsing of any kind. `STRUCTURALLY_VALID` remains unreachable,
  and the first parser of recovered content is not written in this
  milestone. `SECURITY.md:626` requires deleted data to be treated as
  untrusted evidence, and a format parser over recovered bytes is the
  highest-risk code this project would have written; it gets its own ADR and
  its own §42 review.
* No hash-set integration, and no second hash algorithm. Public hash sets
  are keyed on MD5 and SHA-1, which would mean a second hash implementation
  to own and justify against `ADR-0004`.
* No confidence level, no enum, no level names in code.
* No file is written. `ADR-0010` Decision A is unchanged.
* No aggregate reporting.
* No reference file.

## 15. Output

Two lines on the recovery path where a reference was supplied, both labelled
so that the recovered-file hash and the validation hash are distinguishable.
`SAFETY.md:230` requires exactly that distinction, naming source hash, image
hash, recovered-file hash and validation hash as four kinds that must be
clearly distinguished, and `SAFETY.md:237` is the validation hash.

The existing standing caveat on the recovery path is unchanged and is not
weakened by a match. A match adds a statement of byte equality; it does not
retract the statement that a free run is not evidence that the content is
the file's.

No recovered byte is printed. `SAFETY.md:390` and `SECURITY.md:402` both
list hashes among permitted output and file contents among what must not be
emitted by default. This is unchanged from M7.

## 16. Tests

`CLAUDE.md:580` requires recovery testing to measure both successful and
incorrect recovery, and `CLAUDE.md:594` states that false positives are
important failures. A match-only test set does not satisfy that section, so
the mismatch case is mandatory.

**16.1 The reference for the match case is externally reproducible.**
`big_content` in `tests/fat32_recovery_fixtures.rs` builds 40 lines of
`taphonomy multicluster fixture line NNN\n`, 40 bytes each, 1600 bytes
total. Its SHA-256 is

```text
5ecddc870bcf7d8525574328f548af954ac1ab8d1d56555b40d01d2977a21a91
```

recomputed for this record outside the crate, with an independent SHA-256
implementation, and matching the value recorded on 2026-09-07. A reference
that can be regenerated without running Taphonomy is what
`CLAUDE.md:557`'s requirement — that tests verify externally meaningful
behaviour rather than reproducing the implementation's own assumptions —
asks for here.

**16.2 Required cases.**

* Parser: the canonical digest round-trips through `to_hex` and `from_hex`;
  uppercase accepted; 63 and 65 characters rejected; a non-hex character
  rejected with its position; a `sha256sum` line rejected; whitespace
  rejected.
* Comparison: equal digests report equality; digests differing in one hex
  character report difference; no reference reports not attempted.
* Fixture, match: `fat32-recover-run.img` with the digest above reports a
  match, and the byte count reported is 1600.
* Fixture, mismatch: the same image with one character of that digest
  changed reports a difference, exits zero, and does not stop the run.
* Fixture, refusal: `fat32-recover-collision.img` refuses before extraction,
  so no comparison is attempted even though a reference was supplied. This
  case exists to prove a reference cannot induce an extraction the FAT
  forbids.
* Argument handling: a reference without `--recover` exits non-zero; a
  malformed reference exits non-zero before any evidence is opened.

## 17. Security review

Recorded under `SECURITY.md:822`'s categories. The digest form introduces no
new class of untrusted data, so this review is precautionary rather than
required; it is written because the next form of this feature will require
one.

* **Trust boundaries.** The reference is operator-supplied command-line
  input, absent from `SECURITY.md:104`'s untrusted list, and becomes
  validated configuration once parsed. Evidence remains untrusted and is
  unchanged.
* **Parser surface.** One parser, fixed 64-character input, no recursion, no
  allocation beyond a 32-byte array, total over its input. Input length is
  bounded by `argv`, which the operating system bounds.
* **Resource consumption.** No allocation on the recovery path. The
  comparison is 32 bytes.
* **Privileges.** Unchanged. No new capability is required.
* **Output.** Digests only. No recovered byte is printed.
* **Temporary data.** None. `SAFETY.md` §17 unaffected.
* **Network.** None. `SAFETY.md` §19 and `SECURITY.md` §22 unaffected.
* **Dependencies.** None added. `ADR-0004` §5 conditions 1 and 2 remain
  satisfied, because `from_hex` names no `sha2` type.
* **Constant-time comparison is not required and must not be added.**
  Nothing compared here is secret. The reference is supplied by the operator
  and printed back to them, the recovered digest is printed, there is no
  oracle, and no party learns anything from timing. `#[derive(PartialEq)]`
  at `src/hash.rs:29` is the correct mechanism. This is recorded so that a
  later reviewer does not introduce a dependency to solve a problem the
  threat model does not contain.

## 18. Implementation order

One coherent change per commit.

1. `refactor:` options struct threaded through the four reporting functions,
   no behaviour change.
2. `feat:` `Sha256Digest::from_hex` and its error type, with unit tests.
3. `feat:` `src/validation.rs`, with unit tests.
4. `feat:` the CLI — the flag, the argument loop, the two output lines, the
   argument errors.
5. `test:` fixture tests for match, mismatch and refusal-with-reference.
6. `docs:` `CHANGELOG.md`, `README.md` status, `src/lib.rs` module doc, the
   `ADR-0003` §2 appendix from §7 of this record, and the
   `ARCHITECTURE.md` §5.6 crossing noted against `ADR-0011` §11.

## 19. Consequences

* The tool gains the ability to state byte equality with an operator-held
  reference, and with it the only retroactive evidence available that a
  computed run was the file's.
* `ARCHITECTURE.md` §5.6 crosses from intent into binding, first among that
  document's component sections.
* M9 inherits a validation record rather than a classification, and owns the
  taxonomy, the single-level rule and aggregate reporting alone.
* The argument-parsing decision in `ADR-0010` Decision B acquires a stated
  expiry condition.
* A reference file, hash-set lookup and structural validation are all
  deferred with reasons recorded, rather than left unmentioned.

## 20. Open items

1. **`Extraction::slack_bytes` is never read outside tests.**
   `src/main.rs:338` prints `run.slack_bytes`; `src/main.rs:347` prints
   `extracted.bytes_hashed`. `src/fat_recovery.rs:451` assigns
   `Extraction::slack_bytes` from `run.slack_bytes`, so the two are the same
   value by construction and printing either is identical. The field is a
   convenience copy, not an independent measurement. No output is wrong.
   Either the extraction line should print it for symmetry with
   `bytes_hashed`, or the duplication should be noted in
   `KNOWN_ISSUES.md`. Not resolved here.
2. **Whether any directory entry names cluster 3 of
   `fat32-deleted-entries.img` is unmeasured.** The cluster's content was
   measured on 2026-09-08 and survives; see the `ADR-0010` §14 appendix.
   Whether an entry references it decides whether that fixture contains data
   that entry-driven recovery structurally cannot reach, which is a limit
   statement M9's reporting may need. Settled by decoding every root entry's
   first cluster in that image.
3. **A reference is compared against every extraction, and nothing bounds
   the number of comparisons.** Bounded in practice by the entry count,
   which is bounded by the directory. Worth revisiting if a later milestone
   accepts many references.
4. **No measurement of content distinctiveness exists,** so §8.2's
   limitation is stated in prose and not enforced in code.

## Appendix A: The CFTT specification, and what Taphonomy is under it

Researched on 2026-09-08 for this record. Nothing in the repository
previously referenced it.

NIST's Computer Forensics Tool Testing programme publishes *Active File
Identification & Deleted File Recovery Tool Specification*. The published
document is Draft 1 of Version 1.1, dated 24 March 2009, and NIST's own
page still lists it as a draft for comment. It is nonetheless the
specification the programme's deleted-file-recovery test reports were run
against, so it is operative in practice while remaining a draft. Any use of
it must state both.

Three findings, recorded because they describe this project in the field's
vocabulary rather than its own.

**A.1 Taphonomy estimates content, by that specification's definition.** The
specification defines a tool as estimating content if it attempts to recover
the content of a deleted file beyond what is explicitly identified in the
residual metadata. On FAT32 the residual metadata identifies the first
cluster and the size, and nothing else, because deletion destroys the chain
— measured independently in EXP-0003. M7 computes a multi-cluster run from
the first cluster and the size, which is beyond what the metadata
identifies. The project's distinguishing property is that it says the
assumption is unconfirmed and refuses when the FAT contradicts it. The
classification still applies, and a reader who knows the vocabulary will
apply it whether or not the project does.

**A.2 M7 declines a core requirement of that specification, deliberately.**
The specification requires a tool to construct a recovered object for each
deleted entry accessible in residual metadata. `ADR-0010` Decision C
declines to construct one whenever a cluster of the implied run is
allocated. Against that requirement, the refusal is non-conformance by
design. A related optional requirement asks a content-estimating tool to
substitute benign filler of equal length for blocks reallocated since
deletion; Taphonomy produces nothing instead, which is the ignore-status
strategy `ADR-0010` Decision C rejected. Both differences are deliberate and
neither was previously recorded.

**A.3 The specification supports Decision A directly.** For estimated
content it states that there is no definitive expected result for the
content of the created recovered object. That is the governing specification
saying that no expected content exists to compare a recovered object
against. If a reference cannot come from the evidence and cannot come from
the recovery, the operator is the only remaining source, which is
Decision A.

## Appendix B: An error in the preparation of this record

The audit run on 2026-09-08 reported the content of cluster 3 of
`fat32-deleted-entries.img` as 31 ASCII bytes followed by 97 zero bytes. The
string `taphonomy reused slot fixture\n` is **30** bytes, and 30 plus 98
fills the 128-byte window that was examined. The audit's own hexadecimal
listing shows 30 bytes; only the count written beside it was wrong.

The measurement was otherwise sound: the offset arithmetic was independently
reproduced, and the bytes are those written by
`scripts/generate-fixtures.sh`.

Recorded because this is the same failure as the one corrected by
`ADR-0010` Appendix A — a count stated rather than counted — and because
that section of the audit is the part most likely to be quoted into a dated
record, where the wrong count would have become evidence.

## Appendix C: M8 as built (2026-09-09)

Recorded on completion of the milestone. Basis `4e09a9b`, the last commit
that changed code. Line references in the body of this record are to
`16507a0` as its header states, and several have since moved; this appendix
names a commit wherever it cites a line.

### C.1 A citation in this record is wrong

The Related list cites `PROJECT.md` §6.4, and §5's Decision A says that
comparing a recovery against itself is "the self-referential evidence
`PROJECT.md` §6.4 rejects."

§6.4 is Deterministic Behavior. It requires reproducible results given
identical evidence, configuration and software version, and says nothing
about the provenance of a reference. No section of `PROJECT.md` states the
requirement attributed to it here. The attribution was invented and should
never have entered the record.

Decision A is unchanged. It does not depend on the citation: a reference
derived from the evidence is the recovery restated, which is circular on
its own terms, and the network and host-filesystem halves of the reason
rest on `SAFETY.md` §19, `SECURITY.md` §22 and `CLAUDE.md` §39, all of
which say what they were cited for. What is withdrawn is the claim that a
constraining document already required it.

The same misattribution was used in conversation to justify pursuing the
CFReDS corpus. That work remains worth doing for the reason §5.4 gives —
ground truth this project did not generate — but it is a judgement this
project is making, not a requirement it is meeting.

`tests/fat32_recovery_fixtures.rs` cites §6.4 for determinism. That
citation is correct and predates this milestone.

### C.2 The requirement this record should have cited

`PROJECT.md` §6.5, Verifiable Results, states that recovered artifacts must
be validated, and that a file being extracted from storage does not by
itself establish that the file is correct.

That is M8's mandate, stated in a constraining document, and this record
argued the entire milestone without citing it. It belongs in the Related
list. It is recorded here rather than inserted there, because this record
is corrected by appendix and not rewritten.

### C.3 A refinement proposed and withdrawn

After the output was first exercised, a change was proposed: where one
entry matched the reference, narrow Decision E's four causes for any other
entry that differed, on the reasoning that the reference is now known to be
a file present on the volume, so a differing entry must be a different
file.

Withdrawn on two grounds.

It is false. The same file may have existed twice on the volume and both
copies been deleted. One copy's run may be contiguous and recover
correctly while the other was fragmented and does not. The differing entry
is then the same file, recovered wrongly, and fragmentation remains the
cause. Every one of the four causes stays live for every entry.

It is also the wrong mechanism. Gating one entry's caveat on another
entry's outcome makes a per-entry statement depend on a cross-entry
inference, which is aggregate reasoning. `ADR-0003` §4.7 and Decision I of
this record reserve that for M9.

Decision E stands as written. Enumerate, never choose.

### C.4 A fourth state, and why it is not a defect

`Outcome` has three variants, but four situations arise where a reference
was supplied: a comparison that matched, one that differed, no reference at
all, and a reference supplied against an entry that produced no extraction
— a refused run, an ineligible entry, a failed assessment or a failed read.

The fourth produces no comparison line. That is not the silence §7 rejects.
`ADR-0003` §3.1 makes validation something that happens to a Candidate, and
a run refused before any content is read never becomes one; there is
nothing to validate and nothing to classify. The per-entry line already
states that no content was read.

What is genuinely absent is a statement at the end of a run that some
entries could not be compared, so that an operator asking whether a file is
present knows the question went unanswered for part of the volume. That is
a count across a session, which is `ADR-0003` §4.7, which is M9's. It is
added to the open items rather than built.

### C.5 §16 and §18 disagreed

§16 requires tests that a reference without `--recover` and a malformed
reference each exit non-zero. §18's commit sequence does not mention them.

They are built, in `tests/cli_arguments.rs`, which is the first suite in
the tree to run the binary rather than call into the crate. Both ordering
claims are tested by passing a path that cannot exist, so that an argument
error rather than a file error is the evidence that the check ran first.

§18 was incomplete, not §16. Recorded so the gap is not read later as work
that was skipped.

### C.6 The caveat block was printing twice

Measured on `fat32-deleted-residue.img`, which holds a recoverable deleted
entry past the directory terminator:

```text
cargo run --quiet -- fixtures/partition/fat32-deleted-residue.img \
  --recover --reference-digest <64 hex> | grep -c "A free run means"
2
```

`report_recovery` printed the caveats itself and is called once for the
directory's entries and once for its residue. The duplication predates this
milestone for the free-run paragraph; M8 tripled the block's size, so a run
could emit twenty-two lines of caveat.

Fixed at `4e09a9b`: `report_recovery` returns a `Caveats` value, the two
results merge, and `print_caveats` prints once per volume. The measurement
above then returns `1`, and the two multi-line paragraphs gained a blank
line between them.

A caveat is not a findings summary, so Decision I is not breached. Each
paragraph states what a finding does not establish; none counts or
aggregates findings.

### C.7 External practice, researched after the decisions

Three findings from SWGDE, none of which changed a decision and all of
which bear on one.

Their position paper on MD5 and SHA-1 describes the purpose of hash lists
as file identification — scanning a digital object for specific items — and
names NIST's National Software Reference Library as the example. Decision A
and §12's behaviour of comparing one reference against every extraction
were reasoned from first principles here; that practice has a name and this
is it.

The same paper states that because of known limitations in MD5 and SHA-1,
only SHA-2 and SHA-3 are appropriate, and recommends transition as tools
add support. §14 rejected hash-set integration partly because the corpora
are keyed on algorithms this project does not implement. The better reason
is that they are keyed on algorithms the field is moving away from.

Their best practices for computer forensic examinations ask that
conclusions be reported concisely as well as completely. The output this
milestone produces is complete and is not concise, which is C.6's problem
stated from outside the project.

### C.8 What M8 does not establish

Validation against a reference detects a wrong recovery only where the
operator already holds the right answer. In the case where they do not —
which is the ordinary reason to recover a file — it cannot help.

`CLAUDE.md` §26 requires recovery testing to measure incorrect recovery as
well as successful recovery. The failure that produces incorrect
recoveries here is a deleted file that was fragmented, whose clusters have
since been freed, passing the allocation check and yielding a plausible
wrong digest. No fixture produces it, none of the eighteen the generator
builds is fragmented, and `mtools` offers no way to ask for one.

So M8 does not close item 5 of the README's development sequence.
`KNOWN_ISSUES.md` records what building that fixture requires. This is
stated here because a record of a validation milestone is exactly where a
later reader would otherwise assume accuracy had been measured.

### C.9 Open items added by this appendix

1. A run-level statement that some entries could not be compared, per C.4.
   `ADR-0003` §4.7, deferred to M9.
2. A fragmented-deleted-file fixture, per C.8, without which `CLAUDE.md`
   §26 is unmet for the case that matters most.
