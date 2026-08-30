#!/usr/bin/env bash
#
# Generate synthetic evidence fixtures for partition table parsing.
#
# Fixtures are synthetic disk images with known layouts. They are NOT
# committed to the repository; this script is the committed artefact, and the
# fixtures are reproducible from it.
#
# See docs/development/DEVELOPMENT_ENVIRONMENT.md section 11.
#
# Requirements:
#   sfdisk       util-linux
#   sgdisk       gdisk
#   mkfs.vfat    dosfstools
#   mcopy, mmd   mtools
#
# No root privileges are required. No loop devices are used. No filesystem is
# mounted. Every image is a regular file.
#
# Determinism: mkfs.vfat --invariant fixes values that would otherwise derive
# from the clock or a random source. Repeated runs of this script must produce
# byte-identical images. That property is checked by verify-fixtures.sh.
#
# mtools writes wall-clock time into directory entries, and FAT stores
# those timestamps in local time with no timezone field. SOURCE_DATE_EPOCH
# and TZ are therefore both exported below: either alone leaves fixture
# bytes dependent on when or where the script runs. Measured in ADR-0006
# section 4.

set -euo pipefail

readonly SECTOR=512
readonly PART_START=2048          # 1 MiB, the conventional first-partition LBA
readonly IMAGE_SECTORS=131072     # 64 MiB
readonly FAT32_LABEL="TAPHFIX"
readonly VBR_OFFSET=$((PART_START * SECTOR))   # 1048576, byte offset of the
                                               # volume boot record

# Pins every timestamp mtools writes. See ADR-0006 section 4.
export SOURCE_DATE_EPOCH=1577836800   # 2020-01-01 00:00:00 UTC
export TZ=UTC

OUT_DIR="${1:-fixtures/partition}"
readonly OUT_DIR

require() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'error: required tool not found: %s (%s)\n' "$1" "$2" >&2
        exit 1
    }
}

require sfdisk "apt install util-linux"
require sgdisk "apt install gdisk"
require mkfs.vfat "apt install dosfstools"
require mcopy "apt install mtools"
require mmd "apt install mtools"
require sha256sum coreutils

mkdir -p "$OUT_DIR"

# Writes a zero-filled image of IMAGE_SECTORS sectors.
blank_image() {
    local path="$1"
    rm -f "$path"
    dd if=/dev/zero of="$path" bs="$SECTOR" count="$IMAGE_SECTORS" \
        status=none
}

# Writes a single byte value at a byte offset, without extending the file.
poke() {
    local path="$1" offset="$2" value="$3"
    printf "$(printf '\\x%02x' "$value")" |
        dd of="$path" bs=1 seek="$offset" conv=notrunc status=none
}

note() {
    printf '  %s\n' "$1"
}

# Writes a 16-bit little-endian value at a byte offset.
poke_le16() {
    local path="$1" offset="$2" value="$3"
    poke "$path" "$offset"       "$(( value & 0xff ))"
    poke "$path" "$((offset+1))" "$(( (value >> 8) & 0xff ))"
}

# Writes a 32-bit little-endian value at a byte offset.
poke_le32() {
    local path="$1" offset="$2" value="$3"
    poke "$path" "$offset"       "$(( value & 0xff ))"
    poke "$path" "$((offset+1))" "$(( (value >> 8) & 0xff ))"
    poke "$path" "$((offset+2))" "$(( (value >> 16) & 0xff ))"
    poke "$path" "$((offset+3))" "$(( (value >> 24) & 0xff ))"
}

# ---------------------------------------------------------------------------
# 1. Valid MBR, single FAT32 partition.
#    The expected-success case.
# ---------------------------------------------------------------------------
fixture_single_fat32() {
    local path="$OUT_DIR/mbr-single-fat32.img"
    printf 'mbr-single-fat32.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0x1a2b3c4d
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    # --mbr=n prevents mkfs.vfat writing its own fake MBR over ours.
    # --offset writes the filesystem into the partition, not at sector 0.
    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    note "one FAT32 partition, type 0x0c, bootable, LBA ${PART_START}"
}

# ---------------------------------------------------------------------------
# 2. Four primary partitions.
#    Exercises the full entry table.
# ---------------------------------------------------------------------------
fixture_four_partitions() {
    local path="$OUT_DIR/mbr-four-partitions.img"
    printf 'mbr-four-partitions.img\n'
    blank_image "$path"

    local span=$(( (IMAGE_SECTORS - PART_START) / 4 ))

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0x11223344
unit: sectors
${path}1 : start=${PART_START}, size=${span}, type=c
${path}2 : start=$((PART_START + span)), size=${span}, type=c
${path}3 : start=$((PART_START + span * 2)), size=${span}, type=83
${path}4 : start=$((PART_START + span * 3)), size=$((span - 1)), type=83
EOF

    note "four primary partitions, types 0x0c, 0x0c, 0x83, 0x83"
}

# ---------------------------------------------------------------------------
# 3. Valid signature, no partition entries.
#    A partitioned-but-empty disk is valid, not an error.
# ---------------------------------------------------------------------------
fixture_empty_table() {
    local path="$OUT_DIR/mbr-empty.img"
    printf 'mbr-empty.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0x55667788
EOF

    note "valid 0x55AA signature, zero partition entries"
}

# ---------------------------------------------------------------------------
# 4. No signature at all.
#    Must be rejected. SAFETY.md section 12: fail closed.
# ---------------------------------------------------------------------------
fixture_no_signature() {
    local path="$OUT_DIR/no-signature.img"
    printf 'no-signature.img\n'
    blank_image "$path"
    note "first sector entirely zero, no 0x55AA at offset 0x1FE"
}

# ---------------------------------------------------------------------------
# 5. Plausible entries, corrupted signature.
#    The dangerous case: everything looks right except the one field that
#    says the table is real. Must be rejected, not parsed optimistically.
# ---------------------------------------------------------------------------
fixture_bad_signature() {
    local path="$OUT_DIR/bad-signature.img"
    printf 'bad-signature.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0x99aabbcc
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c
EOF

    # 0x1FE = 510, 0x1FF = 511. Valid is 0x55 0xAA.
    poke "$path" 510 0x00
    poke "$path" 511 0x00

    note "valid partition entry, signature overwritten with 0x0000"
}

# ---------------------------------------------------------------------------
# 6. Partition extending beyond the end of the image.
#    Must be rejected. DEVELOPMENT_ENVIRONMENT.md section 18: validate
#    offsets and lengths against actual bounds, never trust declared values.
# ---------------------------------------------------------------------------
fixture_partition_beyond_end() {
    local path="$OUT_DIR/partition-beyond-end.img"
    printf 'partition-beyond-end.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xdeadbeef
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c
EOF

    # Overwrite the sector-count field of entry 1 with 0xFFFFFFFF.
    # Entry 1 begins at 0x1BE; the sector-count field is at entry+12,
    # i.e. 0x1BE + 12 = 0x1CA = 458.
    local i
    for i in 458 459 460 461; do
        poke "$path" "$i" 0xff
    done

    note "entry 1 declares 0xFFFFFFFF sectors, far beyond the 64 MiB image"
}

# ---------------------------------------------------------------------------
# 7. GPT protective MBR.
#    A GPT disk presents a valid MBR containing one 0xEE partition spanning
#    the disk. An MBR-only parser that does not recognise this will report a
#    single whole-disk partition for every GPT disk it sees.
#    ADR-0005 section 4 requires this be detected and reported.
# ---------------------------------------------------------------------------
fixture_gpt_protective() {
    local path="$OUT_DIR/gpt-protective.img"
    printf 'gpt-protective.img\n'
    blank_image "$path"

    sgdisk --disk-guid=00000000-0000-4000-8000-000000000000 \
           --new=1:2048:0 \
           --partition-guid=1:00000000-0000-4000-8000-000000000001 \
           --typecode=1:0700 \
           "$path" >/dev/null 2>&1

    note "GPT with protective MBR, entry type 0xEE at LBA 0"
}

# ---------------------------------------------------------------------------
# 8. Declared partition type disagrees with actual content.
#    The partition entry declares type 0x07 (NTFS/exFAT) but the volume is
#    FAT32. A partition type byte records intent, not content. Taphonomy must
#    identify the filesystem from the volume's own structure and report the
#    disagreement rather than trusting either source.
# ---------------------------------------------------------------------------
fixture_type_mismatch() {
    local path="$OUT_DIR/mbr-type-mismatch.img"
    printf 'mbr-type-mismatch.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0x0badf00d
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=7
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    note "declared type 0x07 (NTFS/exFAT), actual content FAT32"
}

# ---------------------------------------------------------------------------
# 9. FAT32 volume declares more sectors than its partition contains.
#    TotSec32 at BPB offset 0x20 is corrupted to claim 200,000 sectors
#    inside a 129,024-sector partition. Declared geometry must be checked
#    against the partition's actual extent, not trusted outright.
# ---------------------------------------------------------------------------
fixture_fat32_oversized_volume() {
    local path="$OUT_DIR/fat32-oversized-volume.img"
    printf 'fat32-oversized-volume.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xfa730001
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    poke_le32 "$path" $((VBR_OFFSET + 0x20)) 200000

    note "volume declares more sectors than its partition contains"
}

# ---------------------------------------------------------------------------
# 10. FAT32 hidden sector count disagrees with the partition start.
#     mkfs.vfat writes zero to BPB_HiddSec for a volume formatted at an
#     offset, so poking zero changes nothing. HiddSec at BPB offset 0x1C
#     is corrupted to 9999 instead, a value the volume could not have
#     started at.
# ---------------------------------------------------------------------------
fixture_fat32_hidden_mismatch() {
    local path="$OUT_DIR/fat32-hidden-mismatch.img"
    printf 'fat32-hidden-mismatch.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xfa730002
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    poke_le32 "$path" $((VBR_OFFSET + 0x1C)) 9999

    note "hidden sector count records a start the volume does not have"
}

# ---------------------------------------------------------------------------
# 11. FAT32 root directory cluster number below the first valid cluster.
#     RootClus at BPB offset 0x2C is corrupted to 1. Cluster numbers 0 and
#     1 are reserved; the first valid cluster is 2.
# ---------------------------------------------------------------------------
fixture_fat32_bad_root_cluster() {
    local path="$OUT_DIR/fat32-bad-root-cluster.img"
    printf 'fat32-bad-root-cluster.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xfa730003
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    poke_le32 "$path" $((VBR_OFFSET + 0x2C)) 1

    note "root directory cluster number below the first valid cluster"
}

# ---------------------------------------------------------------------------
# 12. FAT32 FAT too small to describe the volume's cluster count.
#     FATSz32 at BPB offset 0x24 is corrupted to 4 sectors, far too few to
#     hold entries for the clusters the volume otherwise declares.
# ---------------------------------------------------------------------------
fixture_fat32_undersized_fat() {
    local path="$OUT_DIR/fat32-undersized-fat.img"
    printf 'fat32-undersized-fat.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xfa730004
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    poke_le32 "$path" $((VBR_OFFSET + 0x24)) 4

    note "FAT too small to describe the volume's cluster count"
}

# ---------------------------------------------------------------------------
# 13. FAT32 volume with entries in its root directory.
#     Every other fixture is an empty filesystem. This one carries the four
#     root-entry shapes M5 must enumerate: a pure 8.3 name, a lowercase 8.3
#     name recorded through the NTRes flags, a name too long for 8.3 which
#     produces preceding attribute-0x0F entries, and a subdirectory.
#     The volume label written by mkfs.vfat -n is a fifth shape,
#     attribute 0x08.
#
#     File contents are fixed short strings so that later milestones have
#     known-good data to recover and hash against.
# ---------------------------------------------------------------------------
fixture_fat32_root_entries() {
    local path="$OUT_DIR/fat32-root-entries.img"
    printf 'fat32-root-entries.img\n'
    blank_image "$path"

    sfdisk --quiet --no-tell-kernel "$path" >/dev/null <<EOF
label: dos
label-id: 0xfa730005
unit: sectors
${path}1 : start=${PART_START}, size=$((IMAGE_SECTORS - PART_START)), type=c, bootable
EOF

    mkfs.vfat --invariant --mbr=n -F 32 -n "$FAT32_LABEL" \
        --offset="$PART_START" "$path" \
        $(( (IMAGE_SECTORS - PART_START) / 2 )) >/dev/null

    local work
    work="$(mktemp -d)"

    printf 'taphonomy hello fixture\n' > "$work/hello.txt"
    printf 'taphonomy readme fixture\n' > "$work/readme.md"
    printf 'taphonomy long name fixture\n' > "$work/annual.txt"

    # Order is fixed, so directory entry order is deterministic.
    MTOOLS_SKIP_CHECK=1 mcopy -i "${path}@@${VBR_OFFSET}" \
        "$work/hello.txt" ::/HELLO.TXT
    MTOOLS_SKIP_CHECK=1 mcopy -i "${path}@@${VBR_OFFSET}" \
        "$work/readme.md" ::/readme.md
    MTOOLS_SKIP_CHECK=1 mcopy -i "${path}@@${VBR_OFFSET}" \
        "$work/annual.txt" "::/annual report 2019.txt"
    MTOOLS_SKIP_CHECK=1 mmd -i "${path}@@${VBR_OFFSET}" ::/logs

    rm -rf "$work"

    note "root directory with 8.3, lowercase, long-name and directory entries"
}

# ---------------------------------------------------------------------------

printf 'Generating fixtures in %s\n\n' "$OUT_DIR"

fixture_single_fat32
fixture_four_partitions
fixture_empty_table
fixture_no_signature
fixture_bad_signature
fixture_partition_beyond_end
fixture_gpt_protective
fixture_type_mismatch
fixture_fat32_oversized_volume
fixture_fat32_hidden_mismatch
fixture_fat32_bad_root_cluster
fixture_fat32_undersized_fat
fixture_fat32_root_entries

# ---------------------------------------------------------------------------
# Manifest
#
# Records the SHA-256 of every fixture. A fixture whose digest changes has
# changed shape, and any test asserting against it is no longer testing what
# it was written to test.
# ---------------------------------------------------------------------------

MANIFEST="$OUT_DIR/MANIFEST.sha256"
( cd "$OUT_DIR" && sha256sum ./*.img > "$(basename "$MANIFEST")" )

printf '\nManifest written to %s\n' "$MANIFEST"
printf '\n'
cat "$MANIFEST"
