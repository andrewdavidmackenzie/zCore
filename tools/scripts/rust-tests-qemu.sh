#!/usr/bin/env bash
#
# Run built-in Rust zircon-tests on bare-metal QEMU.
#
# Usage: tools/scripts/rust-tests-qemu.sh <arch> [product.board]
#   arch: aarch64, x86_64, riscv64
#   selection: defaults to rust-tests.qemu-<arch>
#
# The kernel must be built via `make rust-tests-build` (the rust-tests
# product adds the baremetal-test feature).

set -euo pipefail

ARCH="${1:?Usage: $0 <arch> [product.board]}"
CONFIG="${2:-rust-tests.qemu-${ARCH}}"
BOARD="${CONFIG##*.}"
TIMEOUT="${RUST_TESTS_TIMEOUT:-120}"

# Read core count from board config
source "$(dirname "$0")/get-target-cores.sh"
CORES=$(get_target_cores "$BOARD")

case "$ARCH" in
  aarch64)
    KERNEL="target/${CONFIG}/release/kernel.bin"
    QEMU_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72 -smp "$CORES"
      -kernel "$KERNEL"
      -serial mon:stdio
    )
    ;;
  x86_64)
    KERNEL="target/${CONFIG}/release/boot.img"
    # Find OVMF firmware — location varies by distro
    OVMF=""
    for candidate in \
      /usr/share/ovmf/OVMF.fd \
      /usr/share/OVMF/OVMF_CODE.fd \
      /usr/share/edk2/ovmf/OVMF_CODE.fd \
      /usr/share/qemu/OVMF.fd; do
      if [ -f "$candidate" ]; then
        OVMF="$candidate"
        break
      fi
    done
    if [ -z "$OVMF" ]; then
      echo "ERROR: OVMF firmware not found"
      exit 1
    fi
    QEMU_CMD=(
      qemu-system-x86_64
      -m 4G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase -smp "$CORES"
      -drive "format=raw,file=$KERNEL"
      -serial mon:stdio
      -bios "$OVMF"
    )
    ;;
  riscv64)
    KERNEL="target/${CONFIG}/release/kernel.bin"
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
