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
  x86_64)
    RUST_TARGET="x86_64-unknown-none"
    KERNEL="target/qemu-x86_64/release/boot.img"
    QEMU_CMD=(
      qemu-system-x86_64
      -m 4G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase -smp "$CORES"
      -serial mon:stdio
      -bios /usr/share/ovmf/OVMF.fd
    )
    ;;
  riscv64)
    RUST_TARGET="riscv64gc-unknown-none-elf"
    KERNEL="target/qemu-riscv64/release/kernel.bin"
    QEMU_CMD=(
      qemu-system-riscv64
      -m 2G -display none -no-reboot -nographic
      -machine virt -smp "$CORES"
      -bios default
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: unsupported arch '$ARCH'"
    exit 1
    ;;
esac

# Build the petal test binary as a ZBI, then build the kernel
# with ROOTPROC pointing to it.
cargo petal-zbi --arch "$ARCH" --bin "$BIN"
cargo clean -p kernel 2>/dev/null || true
PETAL_ZBI="$(pwd)/target/petal/$ARCH/petal.zbi" \
  ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/$BIN" \
  cargo zcore-build -m "qemu-$ARCH" --flavour ""

# Strip kernel ELF to raw binary for QEMU
if [ "$ARCH" != "x86_64" ]; then
  OBJCOPY=$(find "$(rustc --print sysroot)" -name llvm-objcopy 2>/dev/null | head -1)
  if [ -n "$OBJCOPY" ]; then
    "$OBJCOPY" -O binary "target/qemu-$ARCH/release/kernel" "$KERNEL"
  elif command -v llvm-objcopy >/dev/null 2>&1; then
    llvm-objcopy -O binary "target/qemu-$ARCH/release/kernel" "$KERNEL"
  fi
fi

echo "==> Running petal $BIN on $ARCH QEMU (timeout=${TIMEOUT}s)..."
LOG=$(mktemp /tmp/petal-test-XXXXXX.log)
trap "rm -f $LOG" EXIT

if [ "$ARCH" = "x86_64" ]; then
  QEMU_CMD+=(-drive "format=raw,file=$KERNEL")
else
  # The petal ZBI is embedded in the kernel binary (via PETAL_ZBI env var
  # during build). No separate initrd needed.
  QEMU_CMD+=(-kernel "$KERNEL")
fi

"${QEMU_CMD[@]}" > "$LOG" 2>&1 &
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
