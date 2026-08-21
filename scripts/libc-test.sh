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
#   4. Runs each test in its own QEMU session (avoids in-guest timeout
#      issues since setitimer/SIGALRM is not yet implemented)
#   5. Prints a summary
#
# Always exits 0 — reports pass rate as a progress metric.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
# Per-test QEMU session timeout (seconds). Includes boot + test execution.
# On macOS (Apple Silicon, native aarch64 QEMU) most tests finish in <5s.
# On Linux x86_64 (cross-arch emulation) QEMU is ~4x slower, so tests
# need a more generous timeout. Detect the platform and adjust.
if [ "$(uname -s)" = "Darwin" ]; then
  TIMEOUT_PER_TEST=20
else
  TIMEOUT_PER_TEST=60
fi
BOOT_TIMEOUT=15

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

# Step 5: Run each test in its own QEMU session
# We cannot use busybox `timeout` inside the guest because it relies on
# setitimer/SIGALRM which is not yet implemented. Instead, each test
# gets its own QEMU session with a host-side timeout.
echo "==> Running ${#TESTS[@]} tests in QEMU (one session per test)..."

run_test() {
  local name=$1
  local OUTPUT
  OUTPUT=$(mktemp)
  local QEMU_IN
  QEMU_IN=$(mktemp -u)
  mkfifo "$QEMU_IN"

  "${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
  local PID=$!
  exec 3>"$QEMU_IN"

  # Wait for shell prompt
  local ELAPSED=0
  local prompt_found=false
  while [ "$ELAPSED" -lt "$BOOT_TIMEOUT" ]; do
    if grep -q '/ # ' "$OUTPUT" 2>/dev/null; then prompt_found=true; break; fi
    if ! kill -0 "$PID" 2>/dev/null; then break; fi
    sleep 1
    ELAPSED=$((ELAPSED + 1))
  done

  if ! $prompt_found; then
    exec 3>&- 2>/dev/null || true
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    rm -f "$OUTPUT" "$QEMU_IN"
    echo "HANG"
    return
  fi

  # Send test command + poweroff
  echo "/bin/libc-test/$name && echo PASS:$name || echo FAIL:$name; poweroff -f" >&3 2>/dev/null || true
  exec 3>&- 2>/dev/null || true

  # Wait for QEMU to exit (poweroff terminates it)
  local W=0
  while [ "$W" -lt "$TIMEOUT_PER_TEST" ]; do
    if ! kill -0 "$PID" 2>/dev/null; then break; fi
    sleep 1
    W=$((W + 1))
  done

  # Check if QEMU is still running (test hung or poweroff failed)
  local timed_out=false
  if kill -0 "$PID" 2>/dev/null; then
    timed_out=true
    kill "$PID" 2>/dev/null || true
  fi
  wait "$PID" 2>/dev/null || true

  # Parse result
  local result
  result=$(sed 's/\x1b\[[0-9;]*m//g' "$OUTPUT" | grep -oE "(PASS|FAIL):$name" | head -1 || true)
  rm -f "$OUTPUT" "$QEMU_IN"

  if $timed_out; then
    echo "HANG"
  elif [ -z "$result" ]; then
    echo "HANG"
  elif echo "$result" | grep -q "^PASS:"; then
    echo "PASS"
  else
    echo "FAIL"
  fi
}

PASSED=0
FAILED=0
HUNG=0
FAIL_LIST=""

for exe in "${TESTS[@]}"; do
  name=$(basename "$exe" -static.exe)
  result=$(run_test "$name" || echo "HANG")
  case "$result" in
    PASS)
      PASSED=$((PASSED + 1))
      ;;
    FAIL)
      FAILED=$((FAILED + 1))
      FAIL_LIST+="  FAIL: $name\n"
      ;;
    HANG)
      HUNG=$((HUNG + 1))
      FAIL_LIST+="  HANG: $name\n"
      ;;
  esac
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

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "Failed/hung tests:"
  printf '%b' "$FAIL_LIST"
fi

# Always exit 0 — this test reports progress, not pass/fail.
exit 0
