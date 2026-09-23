# ADR-0017: M12 Orphaned Directory Decisions

* **Status:** Accepted
* **Date:** 2026-09-23
* **Decision owners:** Taphonomy project
* **Scope:** Which clusters are searched for a directory no read entry
  names; how such a directory is read and reported; which entries it
  lists are offered for recovery; what the search adds to coverage
* **Related:** EXP-0005; EXP-0007; `ADR-0016` Decisions A, C, D and E,
  §10, §11; `ADR-0010` Decisions C and D; `ADR-0014` Decision A,
  Appendix A.6, Appendix B.4; `ADR-0001` §10; `ADR-0002` §8

---

## 1. Context

`ADR-0016` §10 records that the walk does not find a directory no
surviving entry names. Two measurements show what that leaves behind.

EXP-0005 finding 4: a deleted directory's later cluster is reachable from
nothing, and on the split fixture it held the only entry leading to
`TAIL` at cluster 19 and `OMEGA.TXT`. `TAIL`'s own first cluster survives
with `.` naming 19 and `..` naming 3.

EXP-0007: a quick format with the same geometry leaves every directory
below the root intact. Each first cluster passes `ADR-0016` Decision C's
three checks unchanged, the files they list keep their first cluster and
size with the first byte unmarked, and every run holds its original
bytes. The tool at `8504cd8` reports the volume as the volume label
alone, coverage complete, exit 0. One write after the format puts a new
tree on the same clusters, and nothing in any directory then names the
old files.

Two facts about the code at `dd0b6bd` shape the decisions.
`enumerate_deleted_directory` already applies Decision C's checks to a
cluster it is given. `eligibility` in `src/fat_recovery.rs` returns `None`
for any entry that is not `EntryKind::Deleted`, so a file listed in an
orphaned directory would be skipped even once the directory is read.

A directory found this way is read from directory records, so it is
filesystem-aware recovery under `ADR-0001` §10. The volume in EXP-0007
that was written to after the format is not: no record names its old
files, and reaching them is carving, which `ADR-0002` §8 leaves to a later
decision.

---

## 2. Decisions

* **A.** After the walk, every data cluster the walk did not name as a
  directory is searched. A cluster whose slot 0 is `.` naming that cluster
  is read under `ADR-0016` Decision C unchanged, first cluster only.
* **B.** Each directory found is reported on its own, headed by its
  cluster and the cluster its `..` names. Directory entries it lists are
  not followed.
* **C.** A file an orphaned directory lists is offered for recovery
  whether or not its entry is marked deleted, under `ADR-0010` Decisions C
  and D otherwise unchanged.
* **D.** A search stopped by an evidence error is a coverage gap of a new
  kind. A directory an orphaned directory names and the run never read is
  kind 8.
* **E.** Run-wide cluster claims are not decided here.

---

## 3. Decision A: the search, and what it skips

**Every data cluster** from 2 to the last is visited once, in ascending
order, after the walk has finished. Slots 0 and 1 are read first, and
only a cluster whose slot 0 is a `.` entry naming that cluster goes on to
`enumerate_deleted_directory`, which reads the FAT entry and the rest of
the cluster. A free cluster that is not a directory therefore costs one
64-byte read.

**The walk's clusters are skipped:** every cluster the walk read, and
every first cluster it named and declined, under a Decision C refusal or a
Decision E bound. A refusal stands; the search does not reverse it.

**The signature is Decision C's, unchanged.** EXP-0007 measured it on
both of its orphaned directories, and EXP-0005 on `TAIL`. It is narrower
than external practice. The Sleuth Kit treats every sector of the data
area as possibly holding directory entries and keeps those that look
valid; issue 2900 in its repository asks for orphaned FAT directories to
be found by their dot entries instead. This decision requires more than
either: `.` must name the cluster it sits in, which a copied or displaced
directory cluster does not, and the FAT must mark the cluster free.

**Rejected: an opt-in search.** EXP-0007's formatted volume is reported
complete with three recoverable files on it. `ADR-0014` Appendix A.6 makes
that report correct, since coverage states what the run did not analyse,
but a default that leaves the tool's reach short where the measurement
shows files is the wrong default. The search is part of every run.

---

## 4. Decision B: reported flat, never followed

Every directory the search can find has its own signature, so the search
visits it whatever names it. Following an orphaned directory's entries
would read the same clusters a second time by another route and need
`ADR-0016` Decision E's machinery again. Reporting flat needs neither,
and no orphaned directory is reported under a path that grows with depth,
so Decision E's output bound is not needed here.

Each is headed `orphaned directory c<cluster>, .. names c<parent>`, and
listed exactly as a walked directory is. `..` holding 0 names the root,
as EXP-0005 finding 3 measured. The parent is printed as the cluster the
evidence states, not resolved to a name: it may be the walk's, another
orphan's, or overwritten.

`ADR-0016` Decision D applies unchanged: a first cluster with no
terminator reports a listing that may continue.

---

## 5. Decision C: what an orphaned listing offers for recovery

EXP-0007 measured that the files an orphaned directory lists are not
marked deleted. `ADR-0010` Decision D's eligible set is
`DeletedKind::ShortName`; this decision adds a short-name file entry in
an orphaned listing, with the same conditions on size, first cluster and
run, and the same named refusals. A deleted entry in an orphaned listing
is eligible as it is anywhere.

Directory entries in an orphaned listing, `.` and `..` included, are not
offered: Decision B reports them. Long-name components and volume labels
locate no content, as in a walked listing.

`ADR-0010` Decision C's check applies unchanged: the run is inferred from
the first cluster and size, and refused if any cluster is in use.
`ADR-0014` Decision A §5.1's reason for classifying every artifact
`RECONSTRUCTED`, that every extraction infers its extent, is true of these
recoveries in the same way. No new level is needed.

The recovery block under an orphaned directory is headed `orphaned
content`, because its entries are not deleted. `assess`'s existing
callers keep their behaviour.

---

## 6. Decision D: what the search adds to coverage

A search that visits every cluster adds no gap. A read error stops the
search for that volume, is reported with the cluster reached, and is the
fourteenth kind `RunCounts::gaps` counts. Like kinds 3, 6 and 7 in
`ADR-0014` Appendix B.4, no static fixture reaches it, so its test goes
on the derivation.

A directory an orphaned listing names, whose first cluster the run did not
read by the walk or the search, is kind 8 with the reason
`not found by the search`. The listing says it existed; the run has
nothing of it.

---

## 7. Decision E: claims across runs are left open

EXP-0004 Appendix A.2 measured two clusters each recovered into two
objects in one run, and EXP-0006 Conclusion 4 a cluster recovered twice
with the same digest. Nothing reports either. An orphaned listing can
claim clusters a deleted entry also claims, so the question becomes
likelier here. No fixture this milestone adds needs it, and it is
separable, so it gets its own decision rather than riding on this one.

---

## 8. What M12 does not do

* It does not read an orphaned directory beyond its first cluster, as
  `ADR-0016` Decision A does not for a deleted one.
* It does not find a directory whose later clusters carry no dot entries,
  such as cluster 20 on the split fixture.
* It does not find a directory formatted under different geometry.
  EXP-0007 Limitation 2: the data area moves, and `.` no longer names the
  cluster it is read as.
* It does not reach files no directory record names. That is carving.
* It does not resolve an orphaned directory's parent to a path.
* It does not change any confidence level.

---

## 9. Consequences for earlier decisions

`ADR-0016` §10's second item is narrowed: the walk still finds nothing no
surviving entry names, and the search now does. `ADR-0016` §11 condition
3 required that `OMEGA.TXT` not appear on the split fixture. Under this
decision it appears, found through `TAIL`, and
`a_deleted_directory_is_read_no_further_than_its_first_cluster` changes
in the same commit as the code. Cluster 4 and cluster 20 remain unread.
`ADR-0016` records this in an appendix.

`ADR-0010` Decision D's eligible set gains one case, recorded here.

---

## 10. Conditions before M12 may claim to find orphaned directories

1. `scripts/generate-fixtures.sh` builds EXP-0007's two volumes as
   fixtures, and `verify-fixtures.sh` reports every fixture byte-identical.
2. On the formatted fixture, clusters 3 and 4 are reported as orphaned
   directories whose `..` names 0 and 3, and the three files are recovered
   with the digests EXP-0007 recorded. Coverage complete, exit 0.
3. On the reused fixture, no orphaned directory is reported and nothing is
   recovered.
4. On the split fixture, cluster 19 is reported with `..` naming 3, and
   `OMEGA.TXT` is recovered with the digest of its content. Clusters 4 and
   20 are not read, and one listing may continue.
5. Every other fixture is measured before and after. One is predicted to
   change: in `fat32-deleted-residue.img` the poke makes `/gone`'s root
   slot the terminator, so the walk never names it (`ADR-0014` Appendix
   B.2), and its cluster, freed by `mrd` and not reused, should be found
   as an empty orphaned directory whose `..` names 0. Any other orphaned
   directory reported is explained before the milestone closes.
6. A directory the walk declined is not read by the search, and a
   directory an orphaned listing names that the run never read is a gap;
   each is asserted.
7. The four slowest harnesses are timed before and after, and
   `KNOWN_ISSUES.md` records the difference.
