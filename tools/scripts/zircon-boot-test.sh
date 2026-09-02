#!/usr/bin/env bash
#
# Zircon boot smoke test: build petal ZBI, build kernel in Zircon mode
# with ZBI embedded, start QEMU, wait for hello message and clean exit.
#
# Usage: tools/scripts/zircon-boot-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = hello message appeared and QEMU exited (boot success)
# Exit code 1 = timeout or error (boot failure)

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30
HELLO_PATTERN='petal: Hello from petal on zCore!'

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

# Build petal ZBI (cross-compile petal, strip, package)
echo "Building petal ZBI for $ARCH..."
cargo petal-zbi --arch "$ARCH" 2>&1 | tail -5

ZBI="target/petal/${ARCH}/petal.zbi"
if [ ! -f "$ZBI" ]; then
  echo "ERROR: $ZBI not found after petal-zbi build."
  exit 1
fi

# Build the kernel in Zircon mode with the ZBI embedded
# Build userstart (first userspace process)
echo "Building userstart for $ARCH..."
if ! cargo build --manifest-path zCore/userstart/Cargo.toml \
  --target "aarch64-unknown-none-softfloat" \
  --release --target-dir target/userstart; then
  echo "ERROR: userstart build failed."
  exit 1
fi

USERSTART="target/userstart/aarch64-unknown-none-softfloat/release/userstart"
if [ ! -f "$USERSTART" ]; then
  echo "ERROR: $USERSTART not found after build."
  exit 1
fi

echo "Building zCore in Zircon mode ($ARCH) with userstart + petal ZBI..."
if ! USERSTART_ELF="$(cd "$(dirname "$USERSTART")" && pwd)/$(basename "$USERSTART")" \
  PETAL_ZBI="$(cd "$(dirname "$ZBI")" && pwd)/$(basename "$ZBI")" \
  ZCORE_CMDLINE="LOG=warn" cargo build \
  -p zcore \
  --no-default-features --features zircon \
  --target "zCore/${ARCH}.json" \
  -Z json-target-spec \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release; then
  echo "ERROR: kernel build failed."
  exit 1
fi

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
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    QEMU_EXIT=0
    wait "$QEMU_PID" || QEMU_EXIT=$?
    if grep -q "$HELLO_PATTERN" "$OUTPUT" 2>/dev/null; then
      echo "PASS: Zircon boot + petal hello + clean shutdown (exit=$QEMU_EXIT)"
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
