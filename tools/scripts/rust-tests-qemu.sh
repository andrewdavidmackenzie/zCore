#!/usr/bin/env bash
#
# Run built-in Rust zircon-tests on bare-metal QEMU.
#
# Usage: tools/scripts/rust-tests-qemu.sh <arch>
#   arch: aarch64, x86_64, riscv64
#
# The kernel must be built with --features baremetal-test and
# ROOTPROC=__test__ in the cmdline.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT="${RUST_TESTS_TIMEOUT:-120}"

# Read core count from target config
source "$(dirname "$0")/get-target-cores.sh"
CORES=$(get_target_cores "$ARCH")

case "$ARCH" in
  aarch64)
    KERNEL="target/qemu-aarch64/release/kernel.bin"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72 -smp "$CORES"
      -kernel "$KERNEL"
      -serial mon:stdio
    )
    ;;
  x86_64)
    KERNEL="target/qemu-x86_64/release/boot.img"
    QEMU_CMD=(
      qemu-system-x86_64
      -m 4G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase -smp "$CORES"
      -drive "format=raw,file=$KERNEL"
      -serial mon:stdio
      -bios /usr/share/ovmf/OVMF.fd
    )
    ;;
  riscv64)
    KERNEL="target/qemu-riscv64/release/kernel.bin"
    QEMU_CMD=(
      qemu-system-riscv64
      -m 2G -display none -no-reboot -nographic
      -machine virt -smp "$CORES"
      -bios default
      -kernel "$KERNEL"
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: unsupported arch '$ARCH'"
    exit 1
    ;;
esac

echo "==> Running Rust zircon-tests on $ARCH QEMU (timeout=${TIMEOUT}s, cores=$CORES)..."

# Run QEMU, capture output, look for pass/fail markers.
LOG=$(mktemp /tmp/rust-tests-XXXXXX.log)
trap "rm -f $LOG" EXIT

# Use gtimeout on macOS, timeout on Linux
if command -v gtimeout &>/dev/null; then
  TIMEOUT_CMD=gtimeout
elif command -v timeout &>/dev/null; then
  TIMEOUT_CMD=timeout
else
  # Fallback: background QEMU and kill after timeout
  "${QEMU_CMD[@]}" > "$LOG" 2>&1 &
  QEMU_PID=$!
  sleep "$TIMEOUT" && kill "$QEMU_PID" 2>/dev/null &
  wait "$QEMU_PID" 2>/dev/null || true
  TIMEOUT_CMD=""
fi

if [ -n "$TIMEOUT_CMD" ]; then
  "$TIMEOUT_CMD" "$TIMEOUT" "${QEMU_CMD[@]}" > "$LOG" 2>&1 || true
fi

if grep -q "All tests passed" "$LOG"; then
  echo "PASS: Rust zircon-tests passed on $ARCH"
  exit 0
else
  echo "FAIL: Rust zircon-tests did not complete on $ARCH"
  echo "--- Full QEMU serial output ($ARCH) ---"
  cat "$LOG"
  echo "--- End serial output ---"
  exit 1
fi
