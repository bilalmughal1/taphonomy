# ADR-0016: M11 Subdirectory Traversal Decisions

* **Status:** Accepted
* **Date:** 2026-09-22
* **Decision owners:** Taphonomy project
* **Scope:** Which directories below the root are read; how a live and a
  deleted directory are read; how `.` and `..` are treated; what bounds the
  walk; what a directory that cannot be read, or may continue, reports; what
  a written artifact is named once entries come from more than one
  directory
* **Related:** EXP-0005; `ADR-0014` Appendix A.6, Appendix B.2, Appendix
  B.5, Appendix B.10, §11; `ADR-0015` Decision D; `ADR-0010` Decision C and
  §5.4; `ADR-0007` §4; `SECURITY.md` §7, §16; `CLAUDE.md` §9, §26

---

## 1. Context

The tool reads the root directory and lists each directory it names without
reading it. `ADR-0014` Appendix B.2 counts every such directory as a
coverage gap and records the non-conformance with DFR-CR-01 that follows:
deleted entries inside a directory are not identified, and on a DCF camera
card no image file sits in the root.

EXP-0005 measured what reading them would find. A deleted subtree survives
whole, with four recoverable files at depths one and two on two volumes; a
deleted directory's first cluster names itself in its `.` entry; `..`
holding 0 names the root; a later cluster of a deleted directory is
unreachable; a terminator in the first cluster proves a listing complete;
and the cluster adjacent to a deleted directory's first is not its
continuation. On both volumes the tool at `e9d0037` reports one unread
directory and none of the four files.

Two facts about the code at `30eeb44` shape the decisions.

`enumerate_root` walks a directory's FAT chain from `boot.root_cluster`,
with range checks, a cycle check within the chain, and `ADR-0007` §4's
65,536-entry bound. For a live directory it needs only a different start.
For a deleted one it fails: the chain is zeroed, so the first chain link
read is 0 and the walk returns `DirectoryError::ReservedChainLink`.

`ADR-0015` Decision D names a written artifact
`slot-<slot>-cluster-<first>.bin`. `Entry.slot` counts from zero within a
cluster, so the name omits which directory cluster the entry was in. The
run's own position label, `c<cluster> s<slot>`, includes it. Two entries at
the same slot in different directory clusters, pointing at the same first
cluster, receive the same name, and the second is reported as an existing
file. Two deleted entries name the same first cluster when a freed cluster
is reused and the second file is deleted too. EXP-0004 measured the reuse,
not such a pair. Today a collision needs a root longer than one cluster as
well; once every directory's slots start again at 0 it needs only two
directories.

---

## 2. Decisions

* **A.** Every directory reachable from the root is read. A live directory
  is read along its FAT chain. A deleted directory is read from its first
  cluster only.
* **B.** `.` and `..` are recognised by their raw names, listed, and never
  followed. They are never a gap. `..` holding 0 names the root.
* **C.** A deleted directory's first cluster is read only if its FAT entry
  is 0, slot 0 is `.` naming that cluster, and slot 1 is `..`. Otherwise it
  is not read, and is a gap with the reason stated.
* **D.** A deleted directory whose first cluster holds no terminator is a
  gap of a new kind: its listing may continue somewhere the evidence does
  not say. No other cluster is read in its place.
* **E.** No directory cluster is read twice in one run, and no directory
  deeper than 128 levels below the root is read. Either is a gap. The walk
  holds one directory's entries at a time.
* **F.** A written artifact is named from the entry's own location and its
  first cluster: `c<cluster>-s<slot>-first-<first>.bin`.
* **G.** Each directory is reported under a path built from the names the
  run recovered. The path is printed and never used on disk.

---

## 3. Decision A: live by the chain, deleted by the first cluster

A live directory's chain is intact, so reading it is what `enumerate_root`
already does from a different first cluster, and its bounds carry over.

A deleted directory has no chain. EXP-0005 finding 4 measured that its
later clusters are unreachable: the FAT entries that named them are zero,
they hold no `.` or `..`, and no entry points to them. Its first cluster is
the one part the evidence locates, through the entry in its parent. Reading
that cluster alone is the most the evidence supports.

**Rejected:** inferring a deleted directory's extent. `ADR-0010` §5 infers
a deleted file's run from its size, and a directory entry's size is always
0, so that inference has nothing to work from. EXP-0005 finding 6 measured
what the obvious substitute does: the adjacent cluster, read as
continuation, returned sixteen entries that look live and point billions
of clusters beyond a 127,006-cluster volume.

---

## 4. Decision B: the dot entries are listed and never followed

`classify` returns `.` and `..` as ordinary directory entries whose first
cluster is the directory itself and its parent. Followed, `.` re-enters the
directory being read and `..` climbs back to the one that led there, or, at
depth one, reads cluster 0, which is out of range.

EXP-0005 finding 3 and the FAT specification agree that `..` holds 0 where
the parent is the root. It is a correct entry, not a malformed one, and
reporting it as an error or a gap would state something false about the
evidence. They are recognised by their raw 11-byte names together with the
directory attribute.

Decision E would stop a followed dot entry from looping. It is not relied on
here, because a loop the bound stops is reported as a gap, and these are
not gaps.

---

## 5. Decision C: a deleted directory's first cluster must identify itself

EXP-0005 finding 2 measured that a deleted directory's first cluster
carries `.` naming itself at slot 0 and `..` at slot 1. That is the check.

The FAT entry must also be 0, for the reason `ADR-0010` Decision C refuses
an allocated cluster in a deleted file's run: a cluster in use belongs to
something else now. The two checks cover different failures. A cluster
reused for file data fails the dot check. A cluster reused for a new
directory passes it, because the new directory's `.` names the same
cluster, and only the FAT entry shows it is live.

A failed check is a gap under `ADR-0014` Appendix A.6, and the report says
which check failed.

**What the check cannot catch,** stated as `ADR-0010` §5.4 states it for
runs: a cluster reused by a directory that was itself later deleted passes
both checks, and its listing is the later directory's. The evidence does
not distinguish them. The check can only refuse.

---

## 6. Decision D: a listing that may continue is its own gap

EXP-0005 finding 5 measured that a terminator in a deleted directory's
first cluster proves the listing complete, and that its absence proves only
that it may have continued. `split.img`'s `BIG` is the case: its first
cluster is full, and its second, which led to `TAIL` and `OMEGA.TXT`, is
unreachable.

Under `ADR-0014` Appendix A.6 a coverage statement reports what the run did
not analyse. Here that is whatever followed the first cluster, which the
tool cannot locate and must not guess. The first cluster is still read and
reported in full.

It is a new kind rather than kind 8, because the directory was read. Kind 8
narrows to directories that were not read at all: a failed Decision C
check, an error along a live chain, or a Decision E bound.

This changes what `ADR-0014` Appendix B.5 measured. `incomplete` is reached
on the fixture set only through kind 8. Every kind-8 directory in the set
is empty, and each is expected to read cleanly under Decision C; Condition
8 measures whether it does. If they do, then without a new fixture
Appendix B.10's decision that `incomplete` exits zero would have no fixture
exercising it. The fixture built from EXP-0005's `split.img` carries this
kind, which is why it is required.

---

## 7. Decision E: each cluster once, and 128 levels

**Once.** Every cluster read as a directory is recorded for the whole run,
not only within one chain. A directory reached a second time, whether by a
cross-linked chain or a deleted entry naming an ancestor, is not read again
and is a gap. The walk therefore reads each data cluster as a directory at
most once, and terminates.

**128 levels.** Termination does not bound output. A chain of N nested
directories, each reported under its full path, prints on the order of N²
bytes, and `SECURITY.md` §16 lists deeply nested structures among its
attacks and directory depth and output size among its limits. The Sleuth
Kit's directory walk carries the same two mechanisms, a stack of
directories seen and a `MAX_DEPTH` of 128, and this decision adopts its
number. A directory deeper than that is not read and is a gap. A DCF card
nests two levels.

**One directory at a time.** The walk keeps a worklist of directories still
to read. Each directory is enumerated, reported and dropped before the next
is read, so memory holds one directory's entries, bounded by `ADR-0007` §4,
plus the worklist and the set of clusters seen, each bounded by the volume's
cluster count.

**Rejected:** recursion on the call stack. A depth bound would limit it,
but a worklist needs no bound for its own sake, and the one bound this
decision sets is then about output alone.

---

## 8. Decision F: the name carries the entry's location

`ADR-0015` Decision D's principle stands: the name is composed from
integers the run established, and no byte of evidence reaches the path.
`SECURITY.md` §7 is met the same way. What changes is which integers.

An entry's location on the volume is its directory cluster and its slot
within that cluster. That pair is unique, and it is what the run already
prints as `c<cluster> s<slot>`. The name becomes:

```text
c<cluster>-s<slot>-first-<first_cluster>.bin
```

`first` is kept because it is what an operator compares against the run's
report of the implied run. With the location in the name, a second file of
the same name can only mean the destination already held one, which is what
`ADR-0015` Decision C reports it as.

**Consumers.** `Destination::path` builds the name and `main` builds the one
`Destination`. `Destination` gains the entry's cluster. Seven name strings
in `src/fat_recovery.rs`'s tests and seven in `tests/recovery_output.rs`
change with it. No descriptive document states the format.

---

## 9. Decision G: paths are reported, not trusted

Each directory's listing is headed by a path of the names the run
recovered, with a destroyed first character shown as the tool already
shows it. `/?ONE/?EEP` is `DEEP` inside the deleted `GONE`. The names are
evidence bytes. They are printed, as recovered names already are, and never
reach a path the tool writes, which Decision F guarantees.

---

## 10. What M11 does not do

* It does not read a deleted directory beyond its first cluster, per
  Decision A.
* It does not find a directory no surviving entry names. The Sleuth Kit's
  orphan handling does; this tool reports only what the walk reached.
* It does not decode long names, so paths use 8.3 names.
* It does not distinguish a reused-then-deleted directory cluster, per
  Decision C.
* It does not change what a recovered file's confidence is. `ADR-0014`
  Decision A's single level applies unchanged, and a directory listing is
  not an artifact.

---

## 11. Conditions before M11 may claim to read subdirectories

1. `scripts/generate-fixtures.sh` builds EXP-0005's two volumes as
   fixtures, and `verify-fixtures.sh` reports every fixture byte-identical.
2. On the subtree fixture, `ALPHA.TXT` and `BETA.TXT` at depth one and
   `GAMMA.TXT` at depth two are listed as deleted files, and with
   `--recover` each digest equals the digest of the content EXP-0005 wrote.
3. On the split fixture, `BIG`'s first cluster is read and its fourteen
   deleted entries are listed; the run reports one listing that may
   continue and exits 0; no entry from cluster 4 appears; `OMEGA.TXT` does
   not appear.
4. No `.` or `..` entry is followed, and none is reported as an error or a
   gap, on any fixture.
5. Each way Decision C refuses is asserted: a first cluster in use, a slot
   0 that is not `.` naming its cluster, and a slot 1 that is not `..`.
6. Each Decision E bound is asserted: a directory reached twice, and a
   directory at depth 129.
7. Two entries at the same slot in different directory clusters with the
   same first cluster produce two files.
8. Every fixture whose coverage changes is measured before and after, and
   the two tests `ADR-0014` Appendix B.2's gap made necessary,
   `a_live_directory_that_was_not_entered_is_a_gap` and
   `a_directory_that_was_not_read_leaves_coverage_incomplete_and_exits_zero`,
   are replaced by tests of the gaps that remain rather than deleted.

---

## Appendix A: what M12 changed of §10 and §11 (2026-09-24)

`ADR-0017` added a search, after the walk, for directories no read entry
names. It changes two statements here and no decision. The body is not
rewritten.

### A.1 §10's second item is narrowed

§10 records that M11 does not find a directory no surviving entry names.
The walk still does not. The search now finds such a directory wherever its
first cluster still names itself and the FAT marks it free (`ADR-0017`
Decision A), and reports it on its own rather than under a path.

### A.2 §11 condition 3 is revised

Condition 3 required that `OMEGA.TXT` not appear on the split fixture.
Under `ADR-0017` it appears: `TAIL`'s first cluster, 19, carries `.`→19 and
`..`→3, as EXP-0005 recorded, so the search finds it and the listing there
offers `OMEGA.TXT` for recovery. The test asserting the condition changed
in the commit that added the search, `8315886`, and
`tests/orphaned_directories.rs` asserts the recovery. The rest of the
condition stands: cluster 4 is not read, and one listing may continue.

### A.3 §10's third item, on 8.3 paths

Paths still use 8.3 names. A deleted directory's first character is now
recovered in its path from the long-name component before its entry, as
its entry in the listing already was; EXP-0008 found the two disagreeing on
a NIST image, and `3ff77f7` corrected the path. Long names are still not
decoded.
