# ADR-0006: Fixture File Creation and the mtools Dependency

* **Status:** Accepted
* **Date:** 2026-08-30
* **Decision owners:** Taphonomy project
* **Scope:** Creating files inside FAT32 fixtures; first development-time tool
  carrying a licence excluded by `DEVELOPMENT_ENVIRONMENT.md` §16
* **Related:** ADR-0002 §8 (M5); ADR-0004 (dependency adoption pattern);
  `DEVELOPMENT_ENVIRONMENT.md` §11, §13, §15, §16; `PROJECT.md` §6.4

---

## 1. Context

ADR-0002 §8 defines milestone M5 as "Enumerate the root directory."

Every fixture the project currently generates is an empty filesystem.
`mkfs.vfat` creates a volume; it cannot place files inside one. A root
directory with no entries cannot demonstrate that enumeration works, and M6
("Identify deleted directory entries") additionally requires that entries be
created and then removed.

M5 is therefore blocked on fixture capability, not on parser design.

Three properties are required of any solution:

1. It must not require root. `DEVELOPMENT_ENVIRONMENT.md` §13 states that
   physical devices must not be required for ordinary development, and the
   existing script uses no loop devices and mounts no filesystem.
2. It must be deterministic. `PROJECT.md` §6.4 requires reproducible
   behaviour, and `DEVELOPMENT_ENVIRONMENT.md` §12 treats
   `MANIFEST.sha256` as a golden result.
3. It must not encode the project's own reading of the FAT specification.
   See §5.1.

---

## 2. Decision

**Adopt `mtools` as a development-time fixture tool.**

`scripts/generate-fixtures.sh` will export `SOURCE_DATE_EPOCH` and `TZ=UTC`
before invoking it. Both are required for byte-reproducible output; see §4.

No `mtools` code enters Taphonomy's dependency graph, its build, or any
distributed artefact. It is invoked as an external executable by a shell
script, in the same way as `sfdisk`, `sgdisk` and `mkfs.vfat`.

---

## 3. Licence

### 3.1 Measured, not recalled

Licences were read from each package's own copyright file on the development
host, following the practice ADR-0004 §3.4 established:

```text
for p in util-linux gdisk dosfstools mtools; do
  printf '%-12s ' "$p"
  grep -m1 '^License:' /usr/share/doc/$p/copyright
done

util-linux   License: GPL-2+
gdisk        License: LGPL-2.0+
dosfstools   License: GPL-3+
mtools       License: GPL-3
```

`mtools` is GPL-3, per `/usr/share/doc/mtools/copyright`, upstream
<https://ftp.gnu.org/gnu/mtools/>.

### 3.2 Why §16 does not prohibit this

`DEVELOPMENT_ENVIRONMENT.md` §16 states that GPL, LGPL and similar licences
"must not be introduced into the dependency graph without a documented
decision."

The section governs the **dependency graph**: crates resolved by Cargo,
recorded in `Cargo.lock`, and linked into the compiled binary. `mtools` is
none of these. It is an executable a shell script runs to produce test data,
and no line of its source or object code reaches Taphonomy's build output.
A user running Taphonomy does not need it installed.

§16 does not draw that distinction in its own words. This ADR is the
documented decision it requires, and it records the distinction so that a
future reader encountering GPL tooling in a project whose dependency policy
excludes GPL finds the reasoning rather than having to infer it.

### 3.3 This is not the first such tool

Three tools already used by `scripts/generate-fixtures.sh` carry licences
named in §16's exclusion list:

| Tool | Package | Licence | In the script since |
|---|---|---|---|
| `sfdisk` | util-linux | GPL-2+ | initial fixture set |
| `sgdisk` | gdisk | LGPL-2.0+ | initial fixture set |
| `mkfs.vfat` | dosfstools | GPL-3+ | initial fixture set |

`mtools` is the fourth, not the first. The precedent already spans both GPL
and LGPL. What was missing was any record that the distinction between a
linked dependency and an invoked executable was deliberate rather than
overlooked.

---

## 4. Determinism

### 4.1 The claim was measured before it was made

ADR-0002 Appendix A exists because a determinism claim was written from
assumption and later found to be unsupported. That failure is not repeated
here: `mcopy` output was measured before this ADR was drafted.

Method. Two 64 MiB images built by identical procedure — `dd`, `sfdisk` with
an explicit `label-id`, `mkfs.vfat --invariant --mbr=n -F 32 --offset=2048` —
then a 26-byte payload copied into each with
`mcopy -i IMAGE@@1M payload.txt ::/payload.txt`. Compared with `cmp`.

Environment:

```text
Host       Ubuntu 24.04 (noble) under WSL2 on Windows
mtools     4.0.43-1build1
TZ         Asia/Dubai (UTC+4) unless stated
```

### 4.2 `mcopy` writes wall-clock time

Two runs in the same second are byte-identical. Two runs five seconds apart
are not:

```text
cmp a.img c.img
  a.img c.img differ: byte 2081839, line 5
   2081839 263 320
   2081847 263 320
```

Both differing bytes lie in the root directory entry for the payload, at
byte offsets `0x0E` (`DIR_CrtTime`) and `0x16` (`DIR_WrtTime`) within the
32-byte entry. The values moved from `0x98B3` to `0x98D0` — 19:05:38 to
19:06:32.

Only the time fields moved because both runs fell on the same day. Across
midnight the date fields would move as well.

The payload file's own mtime had been pinned with
`touch -d '2020-01-01 00:00:00 UTC'`. The recorded date was the current
date, not the pinned one, so `mcopy` does not propagate the source file's
modification time by default.

**`mtools` is therefore not deterministic as invoked.** Without mitigation,
file-bearing fixtures cannot be golden results and
`DEVELOPMENT_ENVIRONMENT.md` §12 does not hold for them.

### 4.3 `SOURCE_DATE_EPOCH` pins the timestamps

With `SOURCE_DATE_EPOCH=1577836800` exported, two runs three seconds apart
are byte-identical, and the directory entry records the pinned date:

```text
001fc420: 5041 594c 4f41 4420 5458 5420 1800 0020  PAYLOAD TXT
001fc430: 2150 2150 0000 0020 2150 0300 1a00 0000
```

`DIR_CrtDate` and `DIR_WrtDate` read `0x5021`: year field 40, so 1980+40 =
2020, month 1, day 1.

The identical result was confirmed by decoding the entry rather than by
`cmp` alone. Two runs inside one clock tick would also compare equal, and
`cmp` cannot distinguish that from a genuine fix.

### 4.4 FAT timestamps are local time, so `TZ` matters

`SOURCE_DATE_EPOCH=1577836800` is 2020-01-01 00:00:00 **UTC**. On a UTC+4
host the entry recorded `DIR_CrtTime` `0x2000`, which decodes to 04:00:00.

Re-run with `TZ=UTC`:

```text
001fc420: 5041 594c 4f41 4420 5458 5420 1800 0000  PAYLOAD TXT
001fc430: 2150 2150 0000 0000 2150 0300 1a00 0000

cmp d.img f.img
  d.img f.img differ: byte 2081840, line 5
```

`DIR_CrtTime` is `0x0000`, 00:00:00. Same instant, different bytes.

FAT directory entries store local time with no timezone field. **Fixture
bytes therefore depend on the host's timezone**, which is a far more likely
difference between two developers than the architecture difference
`EXPERIMENTS.md` EXP-0001 already records as unmeasured.

Both `SOURCE_DATE_EPOCH` and `TZ=UTC` are required. Either alone is
insufficient.

---

## 5. Alternatives Considered

### 5.1 Construct directory entries with the existing poke helpers

Rejected, and this is the alternative that deserved the most weight. It adds
no tool, no licence question, and `poke_le16` and `poke_le32` already exist.
A 32-byte FAT directory entry is a small structure.

It is rejected because a hand-built entry would encode the project's own
reading of the FAT specification — the same reading the parser encodes. If
that reading is wrong, the fixture is wrong in the same direction as the
parser, and the test passes while the code is incorrect. A fixture built by
an implementation that has never seen this codebase can disagree with the
parser, and a disagreement is a signal.

This is the reason `mkfs.vfat` builds the volumes rather than the script
writing a BIOS parameter block directly, and the same reasoning applies one
level down. `ARCHITECTURE.md` §36 and `PROJECT.md` §5 both exist to prevent
plausible-but-incorrect results being accepted; a self-confirming fixture is
that failure moved into the test suite.

The narrow exception is deliberate corruption. Taking a valid image and
poking one field, as the four FAT32 boot sector fixtures already do, does not
encode a reading of the specification — the surrounding structure came from
`mkfs.vfat`. That practice continues.

### 5.2 Loop device and mount

Rejected. Requires root, which `DEVELOPMENT_ENVIRONMENT.md` §13 and the
existing script both avoid. It would also make fixture generation depend on
kernel filesystem drivers whose version is not recorded, replacing a
measurable tool version with an unmeasured one.

### 5.3 `faketime`

Rejected. It would pin the clock, but it is a second additional package
solving a problem `SOURCE_DATE_EPOCH` already solves, and it does not
address §4.4's timezone dependence.

### 5.4 Defer M5 until a deterministic alternative appears

Rejected. The determinism problem is solved, measurably, by two environment
variables. Deferring would stall the milestone sequence for a difficulty
that has already been resolved.

---

## 6. Conditions of Acceptance

1. `scripts/generate-fixtures.sh` exports both `SOURCE_DATE_EPOCH` and
   `TZ=UTC` before any `mtools` invocation, with a comment stating why both
   are required.
2. `mtools` is added to the script's `require` checks and its documented
   requirements, so a contributor is told what to install rather than
   discovering it through a failure.
3. `verify-fixtures.sh` must continue to report every fixture byte-identical
   after file-bearing fixtures are added. A non-deterministic fixture is not
   committed.
4. `EXPERIMENTS.md` records the timezone dependence as a stated limitation of
   the fixture laboratory, alongside the existing architecture limitation.
5. `mtools` is never invoked from Rust code, from a build script, or from
   anything that runs during a normal build or test. It is a fixture-time
   tool only.
6. No `mtools` source is copied into the repository.

Condition 1 is the mitigation for §4.2. Condition 3 is how a regression in it
would be caught.

---

## 7. Consequences

### Positive

* M5 and M6 are unblocked
* Fixtures containing files are produced by an independent implementation,
  so parser errors are not masked by matching fixture errors
* No root, no loop devices, no mounted filesystems
* Zero additional packages: `apt install mtools` installed one package,
  197 kB, with no dependencies pulled in
* No change to the dependency graph, the build, or the shipped binary

### Negative

* A GPL-3 tool is required for fixture generation, and this is the first
  time the project has documented that as acceptable
* One more package a contributor must install
* Fixture determinism is now conditional on two environment variables rather
  than intrinsic to the tooling. If either is dropped, fixtures silently
  become non-reproducible and only `verify-fixtures.sh` will catch it
* Fixture bytes depend on the host timezone, a new limitation not previously
  present
* `mcopy` does not preserve the source file's modification time, so payload
  mtimes cannot be used to carry meaning into a fixture

---

## 8. Review Trigger

Revisit if:

* `mtools` becomes unmaintained or a security advisory is issued against it
* a future `mtools` release changes its `SOURCE_DATE_EPOCH` handling and
  `verify-fixtures.sh` reports a non-identical fixture
* the project needs to create files in a filesystem `mtools` does not
  support, such as ext4
* a licence-compatible alternative offering the same independence appears
* fixture generation must run somewhere `mtools` cannot be installed
