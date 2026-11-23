#!/usr/bin/env bash
#
# Automated verification that real Fuchsia userspace boots and runs
# (issue #523 Phase A). Boots the Fuchsia bringup image in QEMU
# headlessly, captures the serial log, and waits for userspace
# markers in order:
#
#   1. "welcome to Zircon"                        -- Fuchsia kernel up
#   2. "console: starting up"                     -- component_manager tree
#                                                    started the console
#   3. "launching /boot/bin/sh"                   -- serial shell running
#   4. "Bootup completed"                         -- driver_manager done
#
# Usage: tools/scripts/fuchsia-test.sh [timeout-seconds]
#
# The serial log is saved to target/fuchsia/serial.log.
#
# Exit code 0 = all markers found within the timeout (userspace running)
# Exit code 1 = timeout, missing markers, or setup error

set -euo pipefail

TIMEOUT="${1:-120}"

source "$(dirname "$0")/fuchsia-env.sh"

fuchsia_resolve_images
fuchsia_qemu_args

# TCG (no KVM) is ~10x slower than KVM; scale the timeout.
if [ "$FUCHSIA_KVM" != 1 ]; then
    TIMEOUT=$((TIMEOUT * 8))
    echo "fuchsia-test: no KVM, timeout scaled to ${TIMEOUT}s"
fi

LOG_DIR="$ZCORE_ROOT/target/fuchsia"
LOG="$LOG_DIR/serial.log"
mkdir -p "$LOG_DIR"
: > "$LOG"

# Markers, in required order. Each is an extended regex matched
# against the raw serial log.
MARKERS=(
    'welcome to Zircon'
    'console: starting up'
    'launching /boot/bin/sh'
    'Bootup completed'
)

echo "fuchsia-test: FUCHSIA_OUT=$FUCHSIA_OUT"
echo "fuchsia-test: log=$LOG timeout=${TIMEOUT}s"

QEMU_PID=""
cleanup() {
    if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

"$FUCHSIA_QEMU_BIN" "${FUCHSIA_QEMU_ARGS[@]}" \
    -display none -vga none \
    -serial file:"$LOG" \
    -append "$FUCHSIA_CMDLINE" &
QEMU_PID=$!

# Wait for each marker in order. Track the byte offset after each
# match and search only the log beyond it, so a log containing the
# markers out of order cannot pass: the next marker must appear
# *after* its predecessor.
NEXT=0
POS=0
START=$SECONDS
while [ "$NEXT" -lt "${#MARKERS[@]}" ]; do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        echo "fuchsia-test: FAIL — QEMU exited before marker ${#MARKERS[@]}/${#MARKERS[@]}: ${MARKERS[$NEXT]}" >&2
        exit 1
    fi
    if [ $((SECONDS - START)) -ge "$TIMEOUT" ]; then
        echo "fuchsia-test: FAIL — timed out after ${TIMEOUT}s waiting for marker $((NEXT+1))/${#MARKERS[@]}: ${MARKERS[$NEXT]}" >&2
        echo "fuchsia-test: last serial output:" >&2
        tail -5 "$LOG" | sed 's/^/  /' >&2 || true
        exit 1
    fi
    # -a treats the log as text (it contains ANSI escapes); -b gives
    # byte offsets relative to the tail slice starting after POS.
    MATCH="$(tail -c +"$((POS + 1))" "$LOG" | grep -abo -- "${MARKERS[$NEXT]}" | head -n 1 || true)"
    if [ -n "$MATCH" ]; then
        POS=$((POS + ${MATCH%%:*} + ${#MARKERS[$NEXT]}))
        NEXT=$((NEXT + 1))
        echo "fuchsia-test: marker $NEXT/${#MARKERS[@]} OK: ${MARKERS[$NEXT-1]} (${SECONDS}s)"
    else
        sleep 2
    fi
done

echo "fuchsia-test: PASS — Fuchsia userspace booted (kernel, component_manager, console shell, driver_manager)"
exit 0
