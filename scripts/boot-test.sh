#!/usr/bin/env bash
#
# Boot smoke test: start zCore in QEMU, wait for the shell prompt, exit.
#
# Usage: scripts/boot-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = shell prompt reached (boot success)
# Exit code 1 = timeout or error (boot failure)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=60
PROMPT_PATTERN='/ # '

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    IMAGE="zCore/aarch64.img"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -kernel "$KERNEL"
      -serial mon:stdio
      -drive "file=$IMAGE,if=none,format=raw,id=x0"
      -device "virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0"
    )
    ;;
  *)
    echo "ERROR: boot-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Verify required files exist
for f in "$KERNEL" "$IMAGE"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: $f not found. Run 'make build ARCH=$ARCH' first."
    exit 1
  fi
done

echo "Starting QEMU (timeout=${TIMEOUT}s)..."

# Create a temp file for QEMU output
OUTPUT=$(mktemp)
trap 'rm -f "$OUTPUT"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

# Start QEMU in the background
"${QEMU_CMD[@]}" > "$OUTPUT" 2>&1 &
QEMU_PID=$!

# Poll for the shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "$PROMPT_PATTERN" "$OUTPUT" 2>/dev/null; then
    echo "PASS: shell prompt reached in ${ELAPSED}s"
    kill "$QEMU_PID" 2>/dev/null || true
    exit 0
  fi

  # Check if QEMU exited early (crash / panic)
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before shell prompt was reached"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    exit 1
  fi

  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

# Timeout
echo "FAIL: shell prompt not reached within ${TIMEOUT}s"
echo "--- QEMU output ---"
cat "$OUTPUT"
kill "$QEMU_PID" 2>/dev/null || true
exit 1
