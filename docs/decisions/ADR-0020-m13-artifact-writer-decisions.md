# ADR-0020: M13 Artifact Writer Decisions

* **Status:** Accepted
* **Date:** 2026-10-06
* **Decision owners:** Taphonomy project
* **Scope:** Where the code that hashes and writes a recovered artifact
  lives, what it may assume about the filesystem the bytes came from, and
  how the move was proved
* **Related:** `ADR-0015` Decision C, Decision G, §9; `ADR-0016`
  Decision F; `ADR-0018` §5, §6; `ADR-0019` §5, Decision F;
  `docs/ARCHITECTURE.md` §5.5

---

## 1. Context

`ADR-0019` has exFAT recovery read a deleted file as a chain of runs, with
sizes wider than 32 bits, and report the content past a file's valid data
length as zeros without reading it. The write path `ADR-0015` specifies
(`create_new`, one pass that hashes and writes, a flush, a read-back,
removal of a partial file, a report when removal fails) is the most
reviewed code in the project, and exFAT recovery needs all of it.

At `aad7117` that path lived in `src/fat_recovery.rs`, beside the FAT32
code. An audit of the file found two kinds of item. `Destination`,
`Output`, `Extraction`, the private `Sink` and the functions `open_sink`,
`discard`, `remove_partial`, `settle` and `read_back` know nothing about
FAT32. `extract` and `stream` do: they take a `Fat32BootSector` and one
contiguous run whose `file_size` is 32 bits. Copying the first kind into an
exFAT module would have given the project two write paths to keep correct.

---

## 2. Decisions

* **A.** The filesystem-neutral part moves to a new module,
  `src/extraction.rs`, which is public. `Destination`, `Output` and
  `Extraction` move with it and are re-exported from `fat_recovery`, so
  every existing path keeps working.
* **B.** A small `ArtifactWriter` joins them, with four operations:
  `begin`, `push`, `finish` and `abort`. It takes the artifact's bytes in
  whatever pieces the caller has them, so it assumes neither a single run
  nor a size width. `fat_recovery::extract` keeps the cluster loop and the
  arithmetic that trims the last cluster, and maps a leftover file to
  `RecoveryError::PartialLeft` as before. The writer and the `Leftover`
  value `abort` returns are `pub(crate)`.
* **C.** The writer has no `Drop`. Dropping an open writer would leave a
  partial file with nobody told, and a removal that fails has to be
  reported beside the evidence failure that caused it (`ADR-0015` §9),
  which a destructor cannot do. The type is `#[must_use]`, and `extract`
  routes every error through `abort`.
* **D.** The read-back reads in 64 KiB chunks, from a buffer `finish`
  allocates, and no longer reuses the extractor's cluster-sized buffer.
  The alternative was to pass that buffer in, which keeps the read pattern
  and couples the writer to a caller-owned buffer. What is written, hashed
  and reported does not depend on the chunk size.
* **E.** Both removal behaviours are preserved exactly. `remove_partial`
  treats a file that is already gone as removed. The `Broken` and
  failed-flush arms of `settle` use `fs::remove_file(..).is_ok()`, so a file
  already gone counts as not removed. The two disagree, and this decision
  moves code and changes no behaviour; the disagreement is recorded in
  `KNOWN_ISSUES.md`.

---

## 3. How the move was proved

The method is `ADR-0018` §5. A binary built from the base commit and one
built after the move were each run on 36 images: the 31 `.img` fixtures,
the two exFAT images of `ADR-0019` Appendix A, and the three CFReDS images
of EXP-0008, whose digests were checked against that record first. Each
image was run three ways, with no options, with `--recover`, and with
`--recover --output` into a fresh empty directory, which is 108 runs. For
each run the standard output, the standard error, the exit status and every
file written were hashed, and a check recorded that nothing was written
outside the output directory.

The table has 464 rows. Captured twice against the base binary it is
byte-identical, and the table from the new binary is byte-identical to
both; the SHA-256 of all three is
`e1b74b0a369d4e1c5c4cf37db178c41d6d6b628d4b4f2a500f72b59935e487be`.
The runs wrote 32 files, none outside the output directory, and exited 0
in 78 cases and 3 in 30. The moved items, compared with the originals, are
identical, and so are the five tests that moved.

The runs do not reach `Output::Failed`, `NotCreated` or `Unverified`, and
reach `Exists` only on the collision fixtures. The unit tests cover those:
the five that moved, and four added for the writer, one of which fixes the
digest of every byte pushed after a write failure. One of the four injects
the failure by setting the sink to `Broken`, because a real one needs a
full disk. The library suite went from 185 tests to 189 and every other
suite is unchanged. `src/main.rs`, `tests/`, `Cargo.toml` and `Cargo.lock`
are untouched.

One difference is not covered by that identity and is stated plainly: the
read-back chunk size changed (Decision D).

---

## 4. Consequences

* exFAT recovery can write through `ArtifactWriter` and report through the
  same `Output` and `Extraction`, so the event stream and the CLI render
  an exFAT artifact the way they render a FAT32 one.
* `Destination::path` still builds the name `ADR-0016` Decision F and
  Appendix B give, `p<partition>-c<cluster>-s<slot>-first-<n>.bin`, with
  32-bit cluster numbers. Whether an exFAT artifact is named the same way
  is a question for `ADR-0019`.
* `ADR-0018` §6 left open which components are published, to be decided
  with exFAT. The writer stays crate-private, so no new commitment is made
  before exFAT shows what shape it needs.

---

## 5. Not decided here

* **Content past a file's valid data length.** `ADR-0019` Decision F
  reports it as zeros without reading it. The writer will need a way to
  take that, and it is added with the code that needs it.
* **A list of extents.** A chain of several runs is the caller's loop
  today, as a single run is. A type for it waits for the second caller.
* **The removal disagreement of Decision E.**
* **The label an exFAT artifact carries.** `ADR-0014` §11 names the
  discovery of a deleted file's extent from surviving evidence as a trigger
  to reopen its classification, and exFAT is that case. A separate record
  decides it.
