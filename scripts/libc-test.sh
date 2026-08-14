#!/usr/bin/env bash
#
# Run musl libc-test functional tests inside zCore on QEMU.
#
# Usage: scripts/libc-test.sh <arch>
#
# This script:
#   1. Builds static libc-test binaries (if not already built)
#   2. Copies them into the rootfs
#   3. Rebuilds the rootfs image
#   4. Boots QEMU and runs each test, collecting pass/fail results
#   5. Prints a summary and exits with 0 if any tests pass
#
# Always exits 0 — reports pass rate as a progress metric.
# The pass rate is expected to improve as more syscalls are implemented.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT_PER_TEST=10
BOOT_TIMEOUT=30

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    IMAGE="zCore/aarch64.img"
    CROSS_COMPILE="aarch64-linux-musl-"
    # Find musl cross-compiler: macOS uses Homebrew, Linux has it in PATH
    MUSL_BIN=""
    if command -v brew >/dev/null 2>&1; then
      MUSL_PREFIX="$(brew --prefix musl-cross 2>/dev/null || true)"
      if [ -n "$MUSL_PREFIX" ] && [ -d "$MUSL_PREFIX/libexec/bin" ]; then
        MUSL_BIN="$MUSL_PREFIX/libexec/bin"
      fi
    fi
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
    echo "ERROR: libc-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Step 1: Build libc-test static binaries
echo "==> Building libc-test static binaries..."
if [ ! -f libc-test/src/functional/argv-static.exe ]; then
  cp libc-test/config.mak.def libc-test/config.mak
  echo 'CFLAGS += -static' >> libc-test/config.mak
  echo 'LDFLAGS += -static' >> libc-test/config.mak
  PATH="${MUSL_BIN:+$MUSL_BIN:}$PATH" \
    make -C libc-test ARCH="$ARCH" CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)" 2>&1 | tail -3
fi

# Collect all static test binaries
TESTS=()
for exe in libc-test/src/functional/*-static.exe; do
  [ -f "$exe" ] && TESTS+=("$exe")
done
echo "   Found ${#TESTS[@]} static test binaries"

# Step 2: Copy into rootfs
echo "==> Copying tests into rootfs..."
TEST_DIR="rootfs/$ARCH/bin/libc-test"
mkdir -p "$TEST_DIR"
for exe in "${TESTS[@]}"; do
  name=$(basename "$exe" -static.exe)
  cp "$exe" "$TEST_DIR/$name"
  "${CROSS_COMPILE}strip" "$TEST_DIR/$name" 2>/dev/null || \
    PATH="${MUSL_BIN:+$MUSL_BIN:}$PATH" "${CROSS_COMPILE}strip" "$TEST_DIR/$name" 2>/dev/null || true
done

# Step 3: Rebuild image
echo "==> Rebuilding rootfs image..."
rm -f "$IMAGE"
cargo image --arch "$ARCH" 2>&1 | tail -2

# Step 4: Verify kernel exists
if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found. Run 'make build ARCH=$ARCH' first."
  exit 1
fi

# Step 5: Build the test command sequence
# Run each test with a per-test timeout, print result, then poweroff.
# Busybox provides the 'timeout' command.
CMDS=""
for exe in "${TESTS[@]}"; do
  name=$(basename "$exe" -static.exe)
  CMDS+="timeout ${TIMEOUT_PER_TEST} /bin/libc-test/$name && echo PASS:$name || echo FAIL:$name;"
done
CMDS+="poweroff -f"

# Step 6: Run in QEMU
echo "==> Running ${#TESTS[@]} tests in QEMU..."
OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"
trap 'rm -f "$OUTPUT" "$QEMU_IN"; kill "$QEMU_PID" 2>/dev/null || true' EXIT

"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$BOOT_TIMEOUT" ]; do
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

if [ "$ELAPSED" -ge "$BOOT_TIMEOUT" ]; then
  echo "FAIL: boot timeout"
  cat "$OUTPUT"
  kill "$QEMU_PID" 2>/dev/null || true
  exit 1
fi

# Send all test commands
echo "$CMDS" >&3
exec 3>&-

# Wait for QEMU to exit (poweroff should terminate it)
WAIT=0
while [ "$WAIT" -lt 120 ]; do
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    break
  fi
  sleep 1
  WAIT=$((WAIT + 1))
done
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

# Step 7: Parse results
PASSED=0
FAILED=0
ERRORS=""

for exe in "${TESTS[@]}"; do
  name=$(basename "$exe" -static.exe)
  if grep -q "PASS:$name" "$OUTPUT" 2>/dev/null; then
    PASSED=$((PASSED + 1))
  else
    FAILED=$((FAILED + 1))
    ERRORS+="  FAIL: $name\n"
  fi
done

TOTAL=${#TESTS[@]}
if [ "$TOTAL" -gt 0 ]; then
  PCT=$(( PASSED * 100 / TOTAL ))
else
  PCT=0
fi

echo ""
echo "========================================"
echo "  libc-test results: $PASSED/$TOTAL passed ($PCT%)"
echo "========================================"

if [ "$FAILED" -gt 0 ]; then
  echo ""
  echo "Failed tests:"
  printf "$ERRORS"
fi

# Always exit 0 — this test reports progress, not pass/fail.
# The pass rate is expected to improve as more syscalls are implemented.
exit 0
