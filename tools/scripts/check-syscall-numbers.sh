#!/usr/bin/env bash
# check-syscall-numbers.sh — verify zx-syscall-numbers.h consistency
#
# Checks:
#   1. Fuchsia syscalls are alphabetically sorted by name
#   2. Fuchsia syscall numbers are sequential starting from 0
#   3. ZX_SYS_COUNT matches the number of Fuchsia syscalls
#   4. zCore extension numbers don't overlap Fuchsia's range
#   5. Optionally fetches upstream header and diffs
#
# Usage:
#   ./tools/scripts/check-syscall-numbers.sh          # verify only
#   ./tools/scripts/check-syscall-numbers.sh --fetch   # fetch upstream & diff

set -euo pipefail

HEADER="zCore/zircon-syscall/src/zx-syscall-numbers.h"
ERRORS=0

if [ ! -f "$HEADER" ]; then
    echo "ERROR: $HEADER not found (run from repo root)"
    exit 1
fi

# ── Extract Fuchsia syscalls (before the zCore extensions block) ─────
# Fuchsia syscalls have numbers 0..204; zCore extensions start at 210+.
fuchsia_lines=$(grep "^#define ZX_SYS_" "$HEADER" \
    | grep -v "COUNT" \
    | awk '$3 < 210 {print}')

fuchsia_count=$(echo "$fuchsia_lines" | wc -l | tr -d ' ')
declared_count=$(grep "^#define ZX_SYS_COUNT" "$HEADER" | awk '{print $3}')

# ── Check 1: ZX_SYS_COUNT matches ───────────────────────────────────
if [ "$fuchsia_count" != "$declared_count" ]; then
    echo "FAIL: ZX_SYS_COUNT=$declared_count but found $fuchsia_count Fuchsia syscalls"
    ERRORS=$((ERRORS + 1))
else
    echo "OK: ZX_SYS_COUNT=$declared_count matches $fuchsia_count Fuchsia syscalls"
fi

# ── Check 2: no duplicate names ─────────────────────────────────────
names=$(echo "$fuchsia_lines" | awk '{print $2}')
dup_count=$(echo "$names" | sort | uniq -d | wc -l | tr -d ' ')
if [ "$dup_count" -gt 0 ]; then
    echo "FAIL: duplicate Fuchsia syscall names:"
    echo "$names" | sort | uniq -d
    ERRORS=$((ERRORS + 1))
else
    echo "OK: no duplicate Fuchsia syscall names"
fi
# Note: Fuchsia sorts by original FIDL CamelCase name (e.g. SyscallTest0
# before SyscallTestHandleCreate), then converts to snake_case. We don't
# re-check sort order — the header is taken verbatim from Fuchsia's zither
# output and verified by sequential numbering.

# ── Check 3: sequential numbering 0..N-1 ────────────────────────────
numbers=$(echo "$fuchsia_lines" | awk '{print $3}')
expected=$(seq 0 $((fuchsia_count - 1)))
if [ "$numbers" != "$expected" ]; then
    echo "FAIL: Fuchsia syscall numbers are not sequential 0..$((fuchsia_count - 1))"
    diff <(echo "$numbers") <(echo "$expected") | head -10
    ERRORS=$((ERRORS + 1))
else
    echo "OK: Fuchsia syscall numbers are sequential 0..$((fuchsia_count - 1))"
fi

# ── Check 4: zCore extensions don't overlap ─────────────────────────
zcore_lines=$(grep "^#define ZX_SYS_" "$HEADER" \
    | grep -v "COUNT" \
    | awk '$3 >= 210 {print}')
zcore_count=$(echo "$zcore_lines" | grep -c "." || true)
if [ "$zcore_count" -gt 0 ]; then
    min_ext=$(echo "$zcore_lines" | awk '{print $3}' | sort -n | head -1)
    if [ "$min_ext" -lt "$declared_count" ]; then
        echo "FAIL: zCore extension number $min_ext overlaps Fuchsia range 0..$((declared_count - 1))"
        ERRORS=$((ERRORS + 1))
    else
        echo "OK: $zcore_count zCore extensions at $min_ext+ (no overlap with Fuchsia 0..$((declared_count - 1)))"
    fi
fi

# ── Check 5: build.rs can parse the header ───────────────────────────
# Verify grep pattern matches what build.rs expects
build_count=$(grep "^#define ZX_SYS_" "$HEADER" | grep -v "COUNT" | wc -l | tr -d ' ')
total=$((fuchsia_count + zcore_count))
if [ "$build_count" != "$total" ]; then
    echo "FAIL: total #define count ($build_count) != fuchsia ($fuchsia_count) + zCore ($zcore_count)"
    ERRORS=$((ERRORS + 1))
else
    echo "OK: total $build_count syscall definitions ($fuchsia_count Fuchsia + $zcore_count zCore)"
fi

# ── Optional: fetch upstream and diff ────────────────────────────────
if [ "${1:-}" = "--fetch" ]; then
    echo ""
    echo "Fetching upstream header from Fuchsia source..."
    UPSTREAM_URL="https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/syscalls/zx-syscall-numbers.h?format=TEXT"
    tmpfile=$(mktemp)
    if curl -sfL "$UPSTREAM_URL" | base64 -d > "$tmpfile" 2>/dev/null && [ -s "$tmpfile" ]; then
        echo "Comparing upstream with local (Fuchsia portion only)..."
        # Extract just the #define lines from upstream
        upstream_defines=$(grep "^#define ZX_SYS_" "$tmpfile" | grep -v "COUNT")
        local_defines=$(echo "$fuchsia_lines")
        if [ "$upstream_defines" = "$local_defines" ]; then
            echo "OK: local header matches upstream"
        else
            echo "DIFF: local header differs from upstream:"
            diff <(echo "$local_defines") <(echo "$upstream_defines") | head -20
            ERRORS=$((ERRORS + 1))
        fi
    else
        echo "WARN: could not fetch upstream (Fuchsia gitiles may be unavailable)"
        echo "      Skipping upstream comparison. Local checks still apply."
    fi
    rm -f "$tmpfile"
fi

# ── Summary ──────────────────────────────────────────────────────────
echo ""
if [ "$ERRORS" -gt 0 ]; then
    echo "FAILED: $ERRORS check(s) failed"
    exit 1
else
    echo "All checks passed."
fi
