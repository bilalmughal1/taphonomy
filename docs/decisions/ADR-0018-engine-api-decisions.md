# ADR-0018: The Engine Reports; the CLI Renders

* **Status:** Accepted
* **Date:** 2026-09-25
* **Decision owners:** Taphonomy project
* **Scope:** Where the analysis of a disk image runs, and what it hands to
  an interface
* **Related:** `docs/ARCHITECTURE.md` Invariant 8, §20, §26, §27, §47;
  `docs/development/KNOWN_ISSUES.md`, *Domain logic lives in the CLI
  binary*; `ADR-0017`

---

## 1. Context

The library parses, enumerates, assesses and extracts. The sequence that
joins those steps into a run does not live there. The partition loop, the
directory walk, the orphan search, the recovery loop and the run's counts
are in `src/main.rs`, interleaved with `println!`. Invariant 8 says the CLI
must not contain domain recovery logic, and §47 records that it does.

Two consequences follow. From outside the binary, the walk and the search
can be tested only by running it, and each run hashes a 64 MB image first.
And §27 requires that a desktop interface call the same layer the CLI
calls; today no such layer exists, so any second interface would repeat
the walk.

§20 leaves the result schema to be defined after implementation research.
This decision defines it.

---

## 2. Decisions

* **A.** The run moves into the library. `src/main.rs` keeps argument
  handling, the checks on an output directory it makes before the library
  is called, rendering, and the exit status.
* **B.** The library reports a run as an ordered stream of events, one per
  finding, delivered to a sink the caller supplies. The CLI's sink prints.
* **C.** The run's counts and its coverage move with it, and are returned
  when the run ends.
* **D.** Hashing the image stays outside the run. The run is given the
  image's length in bytes.
* **E.** The move changes no output. Its test is byte equality.

---

## 3. Why a stream, not a finished tree

A tree returned at the end would serve the CLI and a test equally well,
but not an interface. EXP-0008 timed about ten seconds on a 1 GiB image,
hashing included. A 32 GB card holds thirty-two times as much, and an
interface that shows nothing until the end is one an operator abandons. A
stream also keeps the CLI's output in the order it has today with no
reordering logic, which is what makes Decision E testable. A sink that
collects events into a tree is a few lines, and tests use one.

Every line the CLI prints today is rendered from an event's fields. No
event carries preformatted text.

---

## 4. Decision D: the image's length

`inspect` gives the partition parser the image's length as hashing
measured it, and the parser reports a partition that claims sectors past
that length. The run takes the length as an argument. The CLI passes the
hashed length, as now. A test that needs no digest passes the file's
reported size and skips the whole-image read, which is where its time
goes.

---

## 5. Decision E: how the move is proved

Before the move, the binary at the base commit is run on every fixture and
on the three CFReDS images of EXP-0008, with no options, with `--recover`,
and with `--recover --output` into the same empty directory. Standard
output, standard error, exit status and every written file are hashed.
After the move, the same runs must produce the same digests, every one.

Also required:

1. The test suite passes with no test file changed in the same commit.
2. `src/main.rs` calls none of `enumerate_root`, `enumerate_directory`,
   `enumerate_deleted_directory`, `begins_directory`, `assess_listed` or
   `extract`.
3. `Cargo.lock` is unchanged.

Once proved, the tests that run the binary only to reach the walk or the
search are rewritten against the library, in their own commit.

---

## 6. Not decided here

* **Machine-readable output.** JSON needs either a dependency or a
  hand-written encoder, and that choice is its own.
* **Extension points for private components.** A trait is added when its
  first implementation exists, not before.
* **Which components are published and which are not.** Decided with
  exFAT, whose patent status bears on it.
* **Any change to what a run reports.** This decision moves code only.
