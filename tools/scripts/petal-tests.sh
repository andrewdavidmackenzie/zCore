#!/usr/bin/env bash
#
# Run the full petal user-mode test suite in a single QEMU boot.
#
# Boots the kernel with the test-runner program as init (the
# petal-tests product); the runner spawns each petal test binary from
# the petal rootfs sequentially and prints results to the serial
# console. This is the CI entry point for the petal test suite
# (issue #706) — one boot instead of one QEMU boot per test binary.
#
# Usage: tools/scripts/petal-tests.sh <arch>
#   arch: aarch64, x86_64
#
# Exit code 0 = all tests passed (no FAIL lines, runner completed)
# Exit code 1 = any test failed, the runner did not finish, or timeout

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
CONFIG="petal-tests.qemu-${ARCH}"
KERNEL_DIR="target/${CONFIG}/release"
TIMEOUT="${PETAL_TESTS_TIMEOUT:-1800}"
LOG="${PETAL_TESTS_LOG:-/tmp/petal-tests-${ARCH}.log}"

source "$(dirname "$0")/get-target-cores.sh"
CORES=$(get_target_cores "qemu-${ARCH}")

case "$ARCH" in
  aarch64) ;;
  x86_64) ;;
  *)
    echo "ERROR: petal-tests.sh does not support arch '$ARCH'"
    exit 1
    ;;
esac

echo "==> Building petal rootfs (all petal binaries) for $ARCH..."
ROOTFS_IMG=$(cargo xtask rootfs-image -m "$CONFIG" --print-path)
cargo xtask rootfs-image -m "$CONFIG"

echo "==> Building kernel ($CONFIG)..."
cargo bin -m "$CONFIG" > /dev/null

if [ "$ARCH" = "x86_64" ]; then
  # UEFI boot: create the boot image with the rootfs as ramdisk.
  if [ ! -f tools/x86-bootimage/target/release/x86-bootimage ]; then
    cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
  fi
  tools/x86-bootimage/target/release/x86-bootimage \
    "$KERNEL_DIR/kernel" "$KERNEL_DIR/boot.img" --ramdisk "$ROOTFS_IMG"
  KERNEL="$KERNEL_DIR/boot.img"
  source "$(dirname "$0")/find-ovmf.sh"
  OVMF=$(find_ovmf) || exit 1
  QEMU_CMD=(
    qemu-system-x86_64
    -m 4G -display none -no-reboot -nographic
    -machine q35 -cpu qemu64,+rdrand,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16,+fsgsbase -smp "$CORES"
    -serial mon:stdio
    -drive if=pflash,format=raw,readonly=on,file="$OVMF"
    -drive "format=raw,file=$KERNEL"
  )
else
  KERNEL="$KERNEL_DIR/kernel.bin"
  QEMU_CMD=(
    qemu-system-aarch64
    -m 2G -display none -no-reboot -nographic
    -machine virt -cpu cortex-a72 -smp "$CORES"
    -serial mon:stdio
    -kernel "$KERNEL"
    -initrd "$ROOTFS_IMG"
  )
fi

echo "==> Running petal test suite on $ARCH QEMU (timeout=${TIMEOUT}s)..."
"${QEMU_CMD[@]}" > "$LOG" 2>&1 &
QEMU_PID=$!

DONE=0
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "petal-test-runner: DONE" "$LOG" 2>/dev/null; then
    DONE=1
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    break
  fi
  sleep 5
  ELAPSED=$((ELAPSED + 5))
done

kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

# Strip ANSI colors for reliable parsing.
CLEAN="${LOG}.clean"
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' > "$CLEAN"

PASSED=$(grep -c " PASS$" "$CLEAN" || true)
FAILED=$(grep -c "^FAIL:" "$CLEAN" || true)
EXEC_FAILS=$(grep -c "petal-test-runner: EXEC-FAIL" "$CLEAN" || true)
STARTED=$(grep -c "petal-test-runner: START" "$CLEAN" || true)
ENDED=$(grep -c "petal-test-runner: END" "$CLEAN" || true)

echo "=== Petal test results ($ARCH, ${ELAPSED}s) ==="
echo "Tests passed: $PASSED  Failed: $FAILED  Exec failures: $EXEC_FAILS"
echo "Binaries started: $STARTED  completed: $ENDED"

if [ "$FAILED" -gt 0 ]; then
  echo "Failed tests:"
  grep "^FAIL:" "$CLEAN" | sed 's/^/  /'
fi
if [ "$EXEC_FAILS" -gt 0 ]; then
  echo "Exec failures:"
  grep "petal-test-runner: EXEC-FAIL" "$CLEAN" | sed 's/^/  /'
fi
if [ "$DONE" -ne 1 ]; then
  LAST_START=$(grep "petal-test-runner: START" "$CLEAN" | tail -1 | sed 's/.*START \[[0-9/]*\] //;s/ ===//')
  echo "HANG: runner did not finish (last started: $LAST_START)"
fi

if [ "$DONE" -eq 1 ] && [ "$FAILED" -eq 0 ] && [ "$EXEC_FAILS" -eq 0 ]; then
  echo "OK: all petal tests passed"
  exit 0
fi
exit 1
