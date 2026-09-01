#!/usr/bin/env bash
#
# Zircon boot smoke test: build zCore in Zircon mode, start QEMU,
# wait for the userstart hello message and clean shutdown.
#
# Usage: tools/scripts/zircon-boot-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = hello message appeared and QEMU exited (boot success)
# Exit code 1 = timeout or error (boot failure)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30
HELLO_PATTERN='userstart: Hello from zCore Zircon mode!'

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -kernel "$KERNEL"
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: zircon-boot-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Build the kernel in Zircon mode
echo "Building zCore in Zircon mode ($ARCH)..."
ZCORE_CMDLINE="LOG=warn" cargo build \
  -p zcore \
  --no-default-features --features zircon \
  --target "zCore/${ARCH}.json" \
  -Z json-target-spec \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release 2>&1 | tail -2

# Verify kernel exists
if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found after build."
  exit 1
fi

echo "Starting QEMU (timeout=${TIMEOUT}s)..."

OUTPUT=$(mktemp)
trap 'rm -f "$OUTPUT"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

"${QEMU_CMD[@]}" > "$OUTPUT" 2>&1 &
QEMU_PID=$!

# Wait for QEMU to exit (userstart calls process_exit -> kernel resets)
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  # Check if QEMU has exited
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    QEMU_EXIT=0
    wait "$QEMU_PID" || QEMU_EXIT=$?
    # Check for the hello message
    if grep -q "$HELLO_PATTERN" "$OUTPUT" 2>/dev/null; then
      echo "PASS: Zircon boot + userstart hello + clean shutdown (exit=$QEMU_EXIT)"
      exit 0
    else
      echo "FAIL: QEMU exited (code=$QEMU_EXIT) but hello message not found"
      echo "--- QEMU output ---"
      cat "$OUTPUT"
      exit 1
    fi
  fi

  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

# Timeout
echo "FAIL: QEMU did not exit within ${TIMEOUT}s"
echo "--- QEMU output ---"
cat "$OUTPUT"
kill "$QEMU_PID" 2>/dev/null || true
exit 1
