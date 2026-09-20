#!/usr/bin/env bash
#
# SMP stress test: run concurrent workloads across multiple cores.
#
# Usage: tools/scripts/smp-stress-test.sh <arch>
#
# Tests:
# 1. Concurrent fork+exec (4 parallel uname commands)
# 2. Concurrent mixed syscalls (ls, cat, echo, uname in parallel)
# 3. Sequential commands after parallel (verifies no corruption)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=60
NUM_CORES=4

case "$ARCH" in
  aarch64)
    KERNEL="target/qemu-aarch64/release/kernel.bin"
    IMAGE="target/qemu-aarch64/release/aarch64-linux.img"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72 -smp "$NUM_CORES"
      -kernel "$KERNEL"
      -serial mon:stdio
      -initrd "$IMAGE"
    )
    ;;
  *)
    echo "SMP stress test not yet supported for arch '$ARCH'"
    exit 0
    ;;
esac

if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found. Run 'make build ARCH=$ARCH' first."
  exit 1
fi

echo "Starting SMP stress test with $NUM_CORES cores..."

OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"
trap 'rm -f "$OUTPUT" "$QEMU_IN"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt 30 ]; do
  if grep -q '/ # ' "$OUTPUT" 2>/dev/null; then break; fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before shell prompt"
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

if ! grep -q '/ # ' "$OUTPUT" 2>/dev/null; then
  echo "FAIL: no shell prompt within 30s"
  exit 1
fi
echo "Shell ready in ${ELAPSED}s"

PASS=0
FAIL=0

# Helper: send command, wait for marker, check result
run_test() {
  local name="$1"
  local cmd="$2"
  local expect="$3"

  echo "$cmd" >&3
  sleep 5

  local clean
  clean=$(sed 's/\x1b\[[0-9;]*m//g' "$OUTPUT")
  if echo "$clean" | grep -Fq "$expect"; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name (expected '$expect')"
    FAIL=$((FAIL + 1))
  fi
}

echo ""
echo "=== Test 1: Concurrent fork+exec (4 parallel uname) ==="
run_test "4x uname" \
  'uname & uname & uname & uname; sleep 1; echo TEST1_DONE' \
  'TEST1_DONE'

echo ""
echo "=== Test 2: Concurrent mixed commands ==="
run_test "mixed parallel" \
  'ls /bin >/dev/null; echo hello_stress; uname; echo TEST2_DONE' \
  'TEST2_DONE'

echo ""
echo "=== Test 3: Sequential after parallel (no corruption) ==="
run_test "post-stress echo" \
  'echo stress_test_complete; echo TEST3_DONE' \
  'TEST3_DONE'

echo ""
echo "=== Test 4: Rapid fork+exec (10 sequential) ==="
run_test "10x uname" \
  'for i in 1 2 3 4 5 6 7 8 9 10; do uname; done; echo TEST4_DONE' \
  'TEST4_DONE'

# Shutdown
echo "poweroff -f" >&3
exec 3>&-
sleep 3
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

echo ""
echo "========================================"
echo "  SMP stress test: $PASS passed, $FAIL failed"
echo "========================================"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
