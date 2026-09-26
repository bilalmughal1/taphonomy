#!/usr/bin/env python3
"""Generate the EXP-0009 test files.

Every 512-byte block of every file starts with a header naming the file
and the block's index, so any cluster found on the volume identifies
itself. Content is deterministic: running this twice produces identical
files. Prints a SHA-256 manifest of everything it writes.

Usage: python3 exp-0009-generate.py <output-directory>
"""
import hashlib
import os
import sys

BLOCK = 512

# name, size in bytes
TARGETS = [
    ("KEEP.txt", 5000),      # stays live: the control
    ("CONTIG.bin", 262144),  # 256 KiB, written first, deleted later
    ("SMALL.txt", 1000),     # less than one cluster, deleted later
    ("FRAG.bin", 245760),    # 240 KiB, written into holes, deleted later
]
FILLERS = 300               # 64 KiB each, far more than a 16 MB volume holds
FILLER_SIZE = 65536


def content(name: str, size: int) -> bytes:
    out = bytearray()
    index = 0
    while len(out) < size:
        header = f"TAPHONOMY EXP-0009 FILE={name} BLOCK={index:06d}\n"
        block = header.encode("ascii")
        pad = (name.encode("ascii") + b" ") * BLOCK
        block += pad[: BLOCK - len(block)]
        out += block
        index += 1
    return bytes(out[:size])


def write(path: str, data: bytes) -> str:
    with open(path, "wb") as f:
        f.write(data)
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    root = sys.argv[1]
    fill = os.path.join(root, "fill")
    os.makedirs(fill, exist_ok=True)
    for name, size in TARGETS:
        digest = write(os.path.join(root, name), content(name, size))
        print(f"{digest}  {name}  {size}")
    for i in range(1, FILLERS + 1):
        name = f"FILL{i:03d}.bin"
        digest = write(os.path.join(fill, name), content(name, FILLER_SIZE))
        if i <= 3 or i == FILLERS:
            print(f"{digest}  fill/{name}  {FILLER_SIZE}")
    print(f"fillers   {FILLERS} x {FILLER_SIZE} bytes in {fill}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
