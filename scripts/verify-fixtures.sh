#!/usr/bin/env bash
#
# Verify that fixture generation is deterministic.
#
# Generates the fixture set twice into separate directories and compares
# every image byte for byte.
#
# PROJECT.md section 6.4 requires deterministic behaviour where technically
# possible. DEVELOPMENT_ENVIRONMENT.md section 11 requires fixtures that are
# deterministic and reproducible. This script is the evidence for both.
#
# It also performs the measurement owed by ADR-0002 Appendix A section A.3,
# which recorded the determinism of FAT32 fixture generation as an unverified
# claim.
#
# Usage:  scripts/verify-fixtures.sh
# Exit:   0 if every fixture is byte-identical across runs, 1 otherwise.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly GENERATOR="$SCRIPT_DIR/generate-fixtures.sh"

[ -x "$GENERATOR" ] || {
    printf 'error: generator not found or not executable: %s\n' "$GENERATOR" >&2
    exit 1
}

WORK="$(mktemp -d)"
readonly WORK
trap 'rm -rf "$WORK"' EXIT

printf 'Run 1\n'
"$GENERATOR" "$WORK/run1" >/dev/null

printf 'Run 2\n'
"$GENERATOR" "$WORK/run2" >/dev/null

printf '\nComparing\n\n'

failures=0
total=0

for image in "$WORK/run1"/*.img; do
    name="$(basename "$image")"
    total=$((total + 1))

    if cmp --silent "$image" "$WORK/run2/$name"; then
        printf '  IDENTICAL   %s\n' "$name"
    else
        failures=$((failures + 1))
        printf '  DIFFERS     %s\n' "$name"
        printf '              first difference: %s\n' \
            "$(cmp "$image" "$WORK/run2/$name" 2>&1 | head -1)"
    fi
done

printf '\n'

if [ "$failures" -eq 0 ]; then
    printf 'Deterministic: %d/%d fixtures byte-identical across runs.\n' \
        "$total" "$total"
    exit 0
fi

printf 'NOT deterministic: %d of %d fixtures differ between runs.\n' \
    "$failures" "$total"
printf 'Fixture-based tests cannot be treated as reproducible until this\n'
printf 'is resolved. Record the finding in docs/development/EXPERIMENTS.md.\n'
exit 1
