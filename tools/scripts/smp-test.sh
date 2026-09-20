#!/usr/bin/env bash
#
# SMP smoke test: boot with -smp 4, verify all secondary cores initialize
# and the shell works.
#
# Usage: tools/scripts/smp-test.sh <arch>
#   arch: aarch64 (only arch with SMP support currently)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30
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
    echo "SMP test not yet supported for arch '$ARCH'"
    exit 0
    ;;
esac

if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found. Run 'make build ARCH=$ARCH' first."
  exit 1
fi

echo "Starting QEMU with $NUM_CORES cores (timeout=${TIMEOUT}s)..."

OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"
trap 'rm -f "$OUTPUT" "$QEMU_IN"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q '/ # ' "$OUTPUT" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before shell prompt"
    cat "$OUTPUT"
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

if ! grep -q '/ # ' "$OUTPUT" 2>/dev/null; then
  echo "FAIL: no shell prompt within ${TIMEOUT}s"
  cat "$OUTPUT"
  exit 1
fi

echo "Shell prompt reached in ${ELAPSED}s"

# Verify all secondary cores initialized
CLEAN=$(sed 's/\x1b\[[0-9;]*m//g' "$OUTPUT")
CORES_INIT=$(echo "$CLEAN" | grep -c "secondary core .* initialized" || true)
EXPECTED=$((NUM_CORES - 1))

if [ "$CORES_INIT" -lt "$EXPECTED" ]; then
  echo "FAIL: only $CORES_INIT/$EXPECTED secondary cores initialized"
  echo "$CLEAN" | grep "core"
  exit 1
fi

echo "All $EXPECTED secondary cores initialized"

# Run a command to verify the shell works with SMP
printf '%s\n' "printf '%s%s\\n' smp_ ok" >&3
sleep 2
if ! grep -q "smp_ok" "$OUTPUT" 2>/dev/null; then
  echo "FAIL: shell command did not execute with SMP"
  exit 1
fi

echo "Shell command executed with SMP"

# Clean shutdown
echo "poweroff -f" >&3
exec 3>&-
for i in $(seq 1 10); do
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    wait "$QEMU_PID" || true
    echo "PASS: SMP boot + $NUM_CORES cores + shutdown"
    exit 0
  fi
  sleep 1
done

echo "FAIL: QEMU did not exit after poweroff"
exit 1
