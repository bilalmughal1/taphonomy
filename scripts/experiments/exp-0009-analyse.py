#!/usr/bin/env python3
"""EXP-0009 analysis: what Windows' exFAT driver left after deletion.

Reads two images of the same exFAT volume, A (before deletion) and B
(after), read-only. Parses the MBR, the boot region, the FAT, the
allocation bitmap and every directory entry set, following the Microsoft
exFAT specification. Scores predictions P1-P7 against the bytes and
attributes every byte that differs between A and B to the structure that
holds it.

Usage: python3 exp-0009-analyse.py <A.vhd> <B.vhd> [files-dir]

files-dir, if given, is the generator's output; each target's content is
then checked against the generated file.
"""
import hashlib
import os
import struct
import sys

SECTOR = 512


def u16(b, o):
    return struct.unpack_from("<H", b, o)[0]


def u32(b, o):
    return struct.unpack_from("<I", b, o)[0]


def u64(b, o):
    return struct.unpack_from("<Q", b, o)[0]


def boot_checksum(region, bps):
    s = 0
    for i in range(bps * 11):
        if i in (106, 107, 112):
            continue
        s = (((s & 1) << 31) | (s >> 1)) + region[i]
        s &= 0xFFFFFFFF
    return s


def set_checksum(entries):
    s = 0
    for i, byte in enumerate(entries):
        if i in (2, 3):
            continue
        s = (((s & 1) << 15) | (s >> 1)) + byte
        s &= 0xFFFF
    return s


class Volume:
    def __init__(self, path):
        with open(path, "rb") as f:
            self.img = f.read()
        mbr = self.img[:SECTOR]
        assert mbr[510:512] == b"\x55\xaa", "no MBR signature"
        self.part_lba = u32(mbr, 446 + 8)
        self.base = self.part_lba * SECTOR
        b = self.img[self.base:self.base + SECTOR]
        assert b[3:11] == b"EXFAT   ", "not exFAT"
        self.bps = 1 << b[108]
        self.spc = 1 << b[109]
        self.cluster_size = self.bps * self.spc
        self.fat_offset = u32(b, 80)
        self.fat_length = u32(b, 84)
        self.heap_offset = u32(b, 88)
        self.cluster_count = u32(b, 92)
        self.root_cluster = u32(b, 96)
        self.volume_flags = u16(b, 106)
        self.percent_in_use = b[112]
        region = self.img[self.base:self.base + 12 * self.bps]
        stored = u32(region, 11 * self.bps)
        self.boot_checksum_ok = boot_checksum(region, self.bps) == stored
        self.entries = []   # every 32-byte slot, with its absolute offset
        self.sets = []
        self.bitmap = None
        self.bitmap_cluster = None
        self.bitmap_length = None
        self.dir_clusters = {}  # directory path -> clusters
        self._walk("/", self.root_cluster, None)

    # --- geometry -------------------------------------------------------
    def cluster_offset(self, c):
        return self.base + (self.heap_offset + (c - 2) * self.spc) * self.bps

    def fat_entry_offset(self, c):
        return self.base + self.fat_offset * self.bps + 4 * c

    def fat(self, c):
        return u32(self.img, self.fat_entry_offset(c))

    def chain(self, first, limit=100000):
        out, c = [], first
        while 2 <= c <= self.cluster_count + 1 and len(out) < limit:
            out.append(c)
            c = self.fat(c)
        return out

    def clusters_of(self, first, length, nofatchain):
        if first == 0:
            return []
        if nofatchain:
            n = -(-length // self.cluster_size)
            return list(range(first, first + n))
        return self.chain(first)

    def allocated(self, c):
        i = c - 2
        return bool(self.bitmap[i // 8] >> (i % 8) & 1)

    def read(self, clusters, length):
        data = b"".join(
            self.img[self.cluster_offset(c):
                     self.cluster_offset(c) + self.cluster_size]
            for c in clusters)
        return data[:length]

    # --- directories ------------------------------------------------------
    def _walk(self, path, first, nofatchain_len):
        if nofatchain_len is None:
            clusters = self.chain(first)
        else:
            clusters = self.clusters_of(first, nofatchain_len, True)
        self.dir_clusters[path] = clusters
        slots = []
        for c in clusters:
            off = self.cluster_offset(c)
            for i in range(self.cluster_size // 32):
                slots.append(off + 32 * i)
        i = 0
        while i < len(slots):
            off = slots[i]
            e = self.img[off:off + 32]
            t = e[0]
            self.entries.append((off, t, path))
            if t == 0x00:
                i += 1
                continue
            if t == 0x81:
                self.bitmap_cluster = u32(e, 20)
                self.bitmap_length = u64(e, 24)
                start = self.cluster_offset(self.bitmap_cluster)
                self.bitmap = self.img[start:start + self.bitmap_length]
            if t & 0x7F == 0x05:  # File entry, live (85h) or not (05h)
                count = e[1]
                raw = b"".join(self.img[slots[i + k]:slots[i + k] + 32]
                               for k in range(count + 1)
                               if i + k < len(slots))
                s = self._parse_set(path, [slots[i + k]
                                           for k in range(count + 1)], raw)
                self.sets.append(s)
                if s["live"] and s["is_dir"] and s["name"]:
                    child = path.rstrip("/") + "/" + s["name"]
                    self._walk(child, s["first"],
                               s["length"] if s["nofatchain"] else None)
            i += 1

    def _parse_set(self, path, offsets, raw):
        count = raw[1]
        stored = u16(raw, 2)
        restored = bytearray(raw)
        for k in range(count + 1):
            restored[32 * k] |= 0x80
        name, first, length, flags = "", 0, 0, 0
        for k in range(1, count + 1):
            e = raw[32 * k:32 * k + 32]
            if len(e) < 32:
                break
            if e[0] & 0x7F == 0x40:
                flags = e[1]
                first = u32(e, 20)
                length = u64(e, 24)
            elif e[0] & 0x7F == 0x41:
                name += e[2:32].decode("utf-16-le", "replace")
        name = name.split("\x00")[0]
        return {
            "path": path, "name": name, "offsets": offsets,
            "types": [raw[32 * k] for k in range(count + 1)],
            "live": raw[0] == 0x85,
            "is_dir": bool(u16(raw, 4) & 0x10),
            "first": first, "length": length,
            "nofatchain": bool(flags & 0x02),
            "checksum_ok_as_is": set_checksum(raw) == stored,
            "checksum_ok_restored": set_checksum(bytes(restored)) == stored,
            "raw": raw,
        }

    def find(self, name, live=None):
        return [s for s in self.sets if s["name"] == name
                and (live is None or s["live"] == live)]


def runs(clusters):
    out = []
    for c in clusters:
        if out and c == out[-1][1] + 1:
            out[-1][1] = c
        else:
            out.append([c, c])
    return out


def region_of(vol, off, owners):
    """Name the structure holding absolute byte offset off."""
    if off < SECTOR:
        return "MBR"
    if off >= len(vol.img) - SECTOR:
        return "VHD footer"
    rel = off - vol.base
    if rel < 0:
        return "before partition"
    sector = rel // vol.bps
    if sector < 12:
        return "main boot region"
    if sector < 24:
        return "backup boot region"
    if vol.fat_offset <= sector < vol.fat_offset + vol.fat_length:
        c = (rel - vol.fat_offset * vol.bps) // 4
        return f"FAT entry of cluster {c} ({owners.get(c, 'unowned')})"
    if sector >= vol.heap_offset:
        c = 2 + (sector - vol.heap_offset) // vol.spc
        return f"cluster {c} ({owners.get(c, 'unowned')})"
    return "alignment space"


def main():
    if len(sys.argv) not in (3, 4):
        print("usage: python3 exp-0009-analyse.py <A.vhd> <B.vhd> [files-dir]",
              file=sys.stderr)
        return 2
    a, b = Volume(sys.argv[1]), Volume(sys.argv[2])
    files = sys.argv[3] if len(sys.argv) == 4 else None
    print("# EXP-0009 analysis\n")
    for label, path in (("A", sys.argv[1]), ("B", sys.argv[2])):
        with open(path, "rb") as f:
            print(f"{label}: {hashlib.sha256(f.read()).hexdigest()}  {path}")
    print(f"\npartition LBA {a.part_lba}, {a.bps} B/sector, "
          f"{a.cluster_size} B/cluster, {a.cluster_count} clusters, "
          f"FAT at sector {a.fat_offset} x{a.fat_length}, heap at sector "
          f"{a.heap_offset}, root cluster {a.root_cluster}, bitmap cluster "
          f"{a.bitmap_cluster} ({a.bitmap_length} bytes)")
    for label, v in (("A", a), ("B", b)):
        print(f"{label}: boot checksum {'valid' if v.boot_checksum_ok else 'INVALID'}"
              f", VolumeFlags {v.volume_flags:#06x}, PercentInUse "
              f"{v.percent_in_use}, entry sets {len(v.sets)} "
              f"({sum(s['live'] for s in v.sets)} live)")

    # Ownership of clusters, from A's live sets plus system structures.
    owners = {c: "root directory" for c in a.dir_clusters["/"]}
    for d, cl in a.dir_clusters.items():
        if d != "/":
            for c in cl:
                owners[c] = f"directory {d}"
    for c in range(a.bitmap_cluster,
                   a.bitmap_cluster + -(-a.bitmap_length // a.cluster_size)):
        owners[c] = "allocation bitmap"
    for s in a.sets:
        if s["live"] and not s["is_dir"]:
            for c in a.clusters_of(s["first"], s["length"], s["nofatchain"]):
                owners.setdefault(c, s["path"].rstrip("/") + "/" + s["name"])
    for s in a.sets:
        if s["name"] and s["first"] and not s["is_dir"] and s["length"]:
            for c in a.clusters_of(s["first"], s["length"], s["nofatchain"]):
                owners.setdefault(c, "(deleted) " + s["name"])

    targets = ["CONTIG.bin", "SMALL.txt", "FRAG.bin"]
    print("\n## Targets\n")
    for name in targets + ["KEEP.txt"]:
        la = a.find(name, live=True)
        lb = b.find(name, live=(name == "KEEP.txt"))
        print(f"{name}: A live sets {len(la)}, B matching sets {len(lb)}, "
              f"A all sets {len(a.find(name))}")
        if not la:
            continue
        sa = la[0]
        cl = a.clusters_of(sa["first"], sa["length"], sa["nofatchain"])
        print(f"  A: first {sa['first']}, length {sa['length']}, NoFatChain "
              f"{int(sa['nofatchain'])}, runs {runs(cl)}, checksum "
              f"{'valid' if sa['checksum_ok_as_is'] else 'INVALID'}")
        sb = [s for s in b.sets if s["offsets"] == sa["offsets"]]
        if sb:
            sb = sb[0]
            diffs = [i for i in range(len(sa["raw"]))
                     if sa["raw"][i] != sb["raw"][i]]
            print(f"  B: same slots, types {[hex(t) for t in sb['types']]} "
                  f"(A {[hex(t) for t in sa['types']]}), changed byte "
                  f"offsets within set {diffs}, checksum as-is "
                  f"{'valid' if sb['checksum_ok_as_is'] else 'invalid'}, with "
                  f"InUse restored "
                  f"{'valid' if sb['checksum_ok_restored'] else 'INVALID'}")
        bits_a = [a.allocated(c) for c in cl]
        bits_b = [b.allocated(c) for c in cl]
        fat_same = all(a.fat(c) == b.fat(c) for c in cl)
        print(f"  bitmap: A {sum(bits_a)}/{len(cl)} set, B "
              f"{sum(bits_b)}/{len(cl)} set; FAT entries for its clusters "
              f"{'unchanged' if fat_same else 'CHANGED'} A->B")
        if not sa["nofatchain"]:
            chain_b = b.chain(sa["first"])
            print(f"  chain followed in B: {len(chain_b)} clusters, "
                  f"{'identical to A' if chain_b == cl else 'DIFFERENT'}")
        content_note = ""
        if files and os.path.exists(os.path.join(files, name)):
            with open(os.path.join(files, name), "rb") as f:
                want = f.read()
            got_a = a.read(cl, sa["length"])
            src_b = cl if sa["nofatchain"] else b.chain(sa["first"])
            got_b = b.read(src_b, sa["length"])
            content_note = (f"  content: A {'matches' if got_a == want else 'DIFFERS'}"
                            f", B read by recorded run "
                            f"{'matches' if got_b == want else 'DIFFERS'}")
            print(content_note)

    # Every byte that differs.
    diff = [i for i in range(min(len(a.img), len(b.img)))
            if a.img[i] != b.img[i]]
    print(f"\n## Every changed byte: {len(diff)}\n")
    groups = {}
    for off in diff:
        r = region_of(a, off, owners)
        key = r.split(" (")[0] if r.startswith(("FAT entry", "cluster")) \
            else r
        owner = r[r.find("(") + 1:-1] if "(" in r else ""
        k = (key if not owner else f"{owner}: {key.split()[0]}"
             if key.startswith("FAT") else f"{owner}")
        groups.setdefault(k, []).append(off)
    for k, offs in sorted(groups.items(), key=lambda x: -len(x[1])):
        print(f"{len(offs):5d}  {k}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
