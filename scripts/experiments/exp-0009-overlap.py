#!/usr/bin/env python3
"""EXP-0009 Appendix A: which deleted files recover correctly, under which rule.

Every 512-byte block written by exp-0009-generate.py names its file, so the
data a recovery returns can be checked against the name it is recovered
under. For every deleted entry set with data on the image, this reads the
clusters its metadata records and classifies the result, then reports what
each of three rules would recover:

  1. metadata: checksum valid with InUse restored, every cluster free in
     the bitmap, cluster count equal to the recorded length
  2. rule 1, and where deleted entry sets claim the same cluster, only the
     most recently created may be recovered; tied creation times refuse all
  3. rule 2, and a run whose clusters are all zeros is refused

Usage: python3 exp-0009-overlap.py <B.vhd>
"""
import importlib.util
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "analyse", os.path.join(HERE, "exp-0009-analyse.py"))
analyse = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(analyse)

HEADER = re.compile(rb"TAPHONOMY EXP-0009 FILE=(\S+) BLOCK=\d+")


def created(s):
    """Creation time as a sortable tuple: the DOS timestamp, then 10 ms."""
    return (struct.unpack_from("<I", s["raw"], 8)[0], s["raw"][20])


def contents(vol, clusters, length):
    data = vol.read(clusters, length)
    names = set()
    for i in range(0, len(data), 512):
        m = HEADER.match(data[i:i + 64])
        names.add(m.group(1).decode() if m else None)
    return data, names


def main():
    if len(sys.argv) != 2:
        print("usage: python3 exp-0009-overlap.py <B.vhd>", file=sys.stderr)
        return 2
    v = analyse.Volume(sys.argv[1])
    deleted = [s for s in v.sets
               if not s["live"] and not s["is_dir"]
               and s["first"] and s["length"]]
    claims = {}
    for s in deleted:
        s["clusters"] = v.clusters_of(s["first"], s["length"],
                                      s["nofatchain"])
        for c in s["clusters"]:
            claims.setdefault(c, []).append(s)

    rows = []
    for s in deleted:
        cl = s["clusters"]
        in_range = all(2 <= c <= v.cluster_count + 1 for c in cl)
        metadata_ok = (in_range and s["checksum_ok_restored"]
                       and all(not v.allocated(c) for c in cl)
                       and len(cl) == -(-s["length"] // v.cluster_size))
        if not metadata_ok:
            rows.append((s["name"], "fails metadata", None, None))
            continue
        data, names = contents(v, cl, s["length"])
        truth = ("right" if names == {s["name"]}
                 else "zeros" if data == bytes(len(data)) else "wrong")
        rivals = [r for c in cl for r in claims[c] if r is not s]
        mine = created(s)
        overlap = ("beaten" if any(created(r) > mine for r in rivals)
                   else "tied" if any(created(r) == mine for r in rivals)
                   else "clear")
        rows.append((s["name"], truth, overlap,
                     data == bytes(len(data))))

    print(f"deleted entry sets with data: {len(deleted)}")
    failing = [r for r in rows if r[1] == "fails metadata"]
    print(f"refused by the metadata checks: {len(failing)} "
          f"{sorted(r[0] for r in failing)}")
    passing = [r for r in rows if r[1] != "fails metadata"]
    print(f"passing the metadata checks: {len(passing)}\n")

    def tally(recovered):
        out = {"right": 0, "wrong": 0, "zeros": 0, "refused": 0}
        for r in passing:
            out[r[1] if recovered(r) else "refused"] += 1
        return out

    rules = [
        ("1 metadata", lambda r: True),
        ("2 + latest created wins", lambda r: r[2] == "clear"),
        ("3 + all-zero refused", lambda r: r[2] == "clear" and not r[3]),
    ]
    print(f"{'rule':26s} {'right':>5s} {'wrong':>5s} {'zeros':>5s} "
          f"{'refused':>7s}")
    for label, rule in rules:
        t = tally(rule)
        print(f"{label:26s} {t['right']:5d} {t['wrong']:5d} {t['zeros']:5d} "
              f"{t['refused']:7d}")
    print("\nnot right under rule 1:")
    for name, truth, overlap, _ in sorted(passing):
        if truth != "right":
            print(f"  {name}: {truth}, overlap {overlap}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
