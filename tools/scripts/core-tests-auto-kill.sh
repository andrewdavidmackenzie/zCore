#!/bin/bash
# Run x86_64 core-tests QEMU, wait for the gtest summary, allow a
# grace period to capture post-test kernel WARN output (stuck-task
# evidence), then kill QEMU and print results.
#
# Usage: core-tests-auto-kill.sh [LOGFILE] [GRACE_SECONDS] [CORES]
set -u
LOG=${1:-/tmp/qemu-core-tests.log}
GRACE=${2:-90}
CORES=${3:-8}
BOOT_IMG=target/core-tests.qemu-x86_64/release/boot.img

# Do not pkill other sessions' QEMU; only manage our own child via $PID below.
sleep 1

. "$(dirname "$0")/find-ovmf.sh"
OVMF=$(find_ovmf) || { echo "ERROR: no OVMF found"; exit 1; }

echo "==> Running QEMU (log=$LOG, grace=${GRACE}s after summary, cores=$CORES)..."
qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
    -machine q35 -smp "$CORES" \
    -cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
    -serial mon:stdio \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive "format=raw,file=$BOOT_IMG" \
    > "$LOG" 2>&1 &
PID=$!

START=$(date +%s)
SUMMARY_SEEN=0
SELF_EXIT=yes
while true; do
    if ! kill -0 $PID 2>/dev/null; then
        echo "==> QEMU exited on its own"
        break
    fi
    # gtest prints the final summary line "[==========] N tests from M cases ran (X ms total)."
    if [ $SUMMARY_SEEN -eq 0 ] && grep -q '\[==========\].*ran' "$LOG" 2>/dev/null; then
        echo "==> Test summary detected, waiting ${GRACE}s for post-test output..."
        SUMMARY_SEEN=1
        SUMMARY_AT=$(date +%s)
    fi
    NOW=$(date +%s)
    if [ $SUMMARY_SEEN -eq 1 ] && [ $((NOW - SUMMARY_AT)) -ge $GRACE ]; then
        echo "==> Grace period over, killing QEMU (did NOT exit on its own)"
        SELF_EXIT=no
        kill $PID 2>/dev/null
        wait $PID 2>/dev/null
        break
    fi
    # Hard cap: 40 minutes total.
    if [ $((NOW - START)) -ge 2400 ]; then
        echo "==> Hard timeout (2400s), killing QEMU"
        SELF_EXIT=no
        kill $PID 2>/dev/null
        wait $PID 2>/dev/null
        break
    fi
    sleep 5
    if [ $(( ($(date +%s) - START) % 60 )) -lt 6 ]; then
        P=$(grep -c '\[       OK \]' "$LOG" 2>/dev/null || true)
        echo "    ${P:-0} tests passed ($(( $(date +%s) - START ))s)..."
    fi
done

ELAPSED=$(( $(date +%s) - START ))
CLEAN="$LOG.clean"
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' > "$CLEAN"
echo "=== Core-tests results (${ELAPSED}s, qemu_self_exit=$SELF_EXIT) ==="
P=$(grep -c '\[       OK \]' "$CLEAN" || true)
F=$(grep -c '\[  FAILED  \]' "$CLEAN" || true)
R=$(grep '\[==========\]' "$CLEAN" | tail -1 | sed -n 's/.*\] \([0-9]*\) test.*ran\./\1/p')
echo "Passed: ${P:-0}  Failed: ${F:-0}  Ran: ${R:-no-summary}"
grep '\[  FAILED  \]' "$CLEAN" | head -20 || true
echo "=== Last lines ==="
tail -8 "$CLEAN"
echo "=== Post-summary scan lines (stuck-task evidence) ==="
grep -n 'scan: cpu=' "$CLEAN" | tail -10 || echo "(none)"
echo "=== Shutdown markers ==="
grep -n 'userstart: init process exited\|shutting down\|shutdown' "$CLEAN" | tail -5 || echo "(none)"
