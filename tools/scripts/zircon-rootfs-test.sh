#!/usr/bin/env bash
#
# Zircon rootfs boot test: build petal programs into an SFS rootfs
# image, boot QEMU with it via --rootfs-image, verify the init
# program runs successfully.
#
# Usage: tools/scripts/zircon-rootfs-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = test passed
# Exit code 1 = test failed

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    ROOTFS_IMG="zCore/${ARCH}-zircon.img"
    QEMU_BASE_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: zircon-rootfs-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

echo "==> Building Zircon rootfs image..."
cargo xtask zircon-rootfs --arch "$ARCH"

if [ ! -f "$ROOTFS_IMG" ]; then
  echo "FAIL: rootfs image not found at $ROOTFS_IMG"
  exit 1
fi

echo "==> Building kernel (Zircon mode)..."
# Build userstart and ZBI (still needed for the embedded fallback)
cargo build -p userstart \
  --target "aarch64-unknown-none-softfloat" \
  --release --target-dir target/userstart

USERSTART="target/userstart/aarch64-unknown-none-softfloat/release/userstart"

cargo petal-zbi --arch "$ARCH" --bin hello 2>&1 | tail -1

ZBI="target/petal/${ARCH}/petal.zbi"

# Build the kernel with rootfs ROOTPROC set to /bin/hello
USERSTART_ELF="$(cd "$(dirname "$USERSTART")" && pwd)/$(basename "$USERSTART")" \
  PETAL_ZBI="$(cd "$(dirname "$ZBI")" && pwd)/$(basename "$ZBI")" \
  ZCORE_CMDLINE="LOG=info:ROOTPROC=/bin/hello" cargo build \
  -p zcore \
  --no-default-features --features zircon \
  --target "zCore/${ARCH}.json" \
  -Z json-target-spec \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release

echo "==> Running Zircon rootfs boot test..."
OUTPUT=$(mktemp)

# Boot with the rootfs image via VirtIO block device
"${QEMU_BASE_CMD[@]}" \
  -kernel "$KERNEL" \
  -drive "file=${ROOTFS_IMG},if=none,format=raw,id=x0" \
  -device "virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0" \
  > "$OUTPUT" 2>&1 &
QEMU_PID=$!

ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  # Check for BOTH the rootfs mount message AND the petal output.
  # This ensures we're testing the rootfs path, not the ZBI fallback.
  if grep -q "Zircon rootfs boot: loading" "$OUTPUT" 2>/dev/null && \
     grep -q "petal: Hello from petal on zCore!" "$OUTPUT" 2>/dev/null; then
    echo "PASS: Zircon rootfs boot (pattern found after ${ELAPSED}s)"
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    rm -f "$OUTPUT"
    exit 0
  fi
  # Check if QEMU exited
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    QEMU_EXIT=0
    wait "$QEMU_PID" || QEMU_EXIT=$?
    if grep -q "Zircon rootfs boot: loading" "$OUTPUT" 2>/dev/null && \
       grep -q "petal: Hello from petal on zCore!" "$OUTPUT" 2>/dev/null; then
      echo "PASS: Zircon rootfs boot (exit=$QEMU_EXIT)"
      rm -f "$OUTPUT"
      exit 0
    else
      echo "FAIL: expected rootfs boot + petal output not found"
      echo "--- QEMU output ---"
      cat "$OUTPUT"
      rm -f "$OUTPUT"
      exit 1
    fi
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

echo "FAIL: pattern not found within ${TIMEOUT}s"
echo "--- QEMU output ---"
cat "$OUTPUT"
rm -f "$OUTPUT"
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true
exit 1
