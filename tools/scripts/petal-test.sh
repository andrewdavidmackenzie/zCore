#!/usr/bin/env bash
#
# Run a petal test binary on bare-metal QEMU.
#
# Usage: tools/scripts/petal-test.sh <arch> <binary_name>
#   arch: aarch64
#   binary_name: e.g., vmar_test, vmo_test
#
# Builds the petal binary, creates a rootfs with it, boots QEMU,
# and checks for PASS/FAIL output.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch> <binary_name>}"
BIN="${2:?Usage: $0 <arch> <binary_name>}"
TIMEOUT="${PETAL_TEST_TIMEOUT:-30}"

source "$(dirname "$0")/get-target-cores.sh"
CORES=$(get_target_cores "$ARCH")

echo "==> Building petal $BIN for $ARCH..."

case "$ARCH" in
  aarch64)
    RUST_TARGET="aarch64-unknown-none-softfloat"
    KERNEL="target/qemu-aarch64/release/kernel.bin"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72 -smp "$CORES"
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: unsupported arch '$ARCH'"
    exit 1
    ;;
esac

# Build the petal binary first
cargo build -p petal --bin "$BIN" --target "$RUST_TARGET" --release

# Copy test binary into rootfs dir BEFORE building the kernel
# (cargo zcore-build creates the SFS rootfs from this directory)
ROOTFS_DIR="target/rootfs/zircon/$ARCH"
mkdir -p "$ROOTFS_DIR/bin"
cp "target/$RUST_TARGET/release/$BIN" "$ROOTFS_DIR/bin/$BIN"

# Build kernel with ROOTPROC pointing to the test binary
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/$BIN" \
  cargo zcore-build -m "qemu-$ARCH"

ROOTFS_IMG="target/qemu-$ARCH/release/$ARCH-zircon.img"

# Strip kernel
OBJCOPY=$(find "$(rustc --print sysroot)" -name llvm-objcopy 2>/dev/null | head -1)
if [ -n "$OBJCOPY" ]; then
  "$OBJCOPY" --strip-all -O binary "target/qemu-$ARCH/release/kernel" "$KERNEL"
elif command -v llvm-objcopy >/dev/null 2>&1; then
  llvm-objcopy --strip-all -O binary "target/qemu-$ARCH/release/kernel" "$KERNEL"
fi

echo "==> Running petal $BIN on $ARCH QEMU (timeout=${TIMEOUT}s)..."
LOG=$(mktemp /tmp/petal-test-XXXXXX.log)
trap "rm -f $LOG" EXIT

"${QEMU_CMD[@]}" \
  -kernel "$KERNEL" \
  -initrd "$ROOTFS_IMG" \
  > "$LOG" 2>&1 &
QEMU_PID=$!

ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "PASS" "$LOG" 2>/dev/null; then
    echo "PASS: $BIN completed successfully"
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    exit 0
  fi
  if grep -q "FAIL" "$LOG" 2>/dev/null; then
    echo "FAIL: $BIN reported failure"
    cat "$LOG"
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

echo "TIMEOUT: $BIN didn't complete in ${TIMEOUT}s"
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true
tail -20 "$LOG"
exit 1
