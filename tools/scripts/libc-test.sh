#!/usr/bin/env bash
#
# Run musl libc-test functional tests inside zCore on QEMU.
#
# Usage: tools/scripts/libc-test.sh <arch>
#
# This script:
#   1. Builds static libc-test binaries (if not already built)
#   2. Copies them into the rootfs
#   3. Rebuilds the rootfs image
#   4. Runs ALL tests in a single QEMU session (batched for speed)
#   5. Prints a summary
#
# All tests run in one QEMU boot, with a host-side timeout on the
# entire session. This avoids ~70 separate QEMU boots.
#
# Always exits 0 — reports pass rate as a progress metric.

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
# Timeout for the entire QEMU session (all tests combined).
# Should be generous enough for boot + all tests.
SESSION_TIMEOUT=300

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore.bin"
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
      -initrd "$IMAGE"
    )
    ;;
  x86_64)
    KERNEL_ELF="target/x86_64/release/zcore"
    BOOT_IMG="target/x86_64/release/boot.img"
    IMAGE="zCore/x86_64.img"
    CROSS_COMPILE="x86_64-linux-musl-"
    # Find musl cross-compiler: macOS uses Homebrew, Linux has it in PATH
    MUSL_BIN=""
    if command -v brew >/dev/null 2>&1; then
      MUSL_PREFIX="$(brew --prefix musl-cross 2>/dev/null || true)"
      if [ -n "$MUSL_PREFIX" ] && [ -d "$MUSL_PREFIX/libexec/bin" ]; then
        MUSL_BIN="$MUSL_PREFIX/libexec/bin"
      fi
    fi
    # x86_64 uses a BIOS disk image with embedded ramdisk
    KERNEL="$BOOT_IMG"
    QEMU_CMD=(
      qemu-system-x86_64
      -m 2G -display none -no-reboot -nographic
      -machine q35 -cpu qemu64,+fsgsbase,+rdrand
      -serial mon:stdio
      -drive "format=raw,file=$BOOT_IMG"
    )
    # Flag to rebuild boot image after rootfs changes
    X86_REBUILD_BOOTIMG=1
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
TEST_DIR="rootfs/linux/$ARCH/bin/libc-test"
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

# Step 3b: For x86_64, rebuild the boot image with the updated rootfs
if [ "${X86_REBUILD_BOOTIMG:-}" = "1" ]; then
  echo "==> Rebuilding x86_64 boot image with test rootfs..."
  BOOTIMAGE_TOOL="tools/x86-bootimage/target/release/x86-bootimage"
  if [ ! -f "$BOOTIMAGE_TOOL" ]; then
    cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml
  fi
  "$BOOTIMAGE_TOOL" "$KERNEL_ELF" "$BOOT_IMG" --ramdisk "$IMAGE"
fi

# Step 4: Verify kernel exists
if [ ! -f "$KERNEL" ]; then
  echo "ERROR: $KERNEL not found. Run 'make build ARCH=$ARCH' first."
  exit 1
fi

# Step 5: Run ALL tests in a single QEMU session
echo "==> Running ${#TESTS[@]} tests in QEMU (one session)..."

OUTPUT=$(mktemp)
QEMU_IN=$(mktemp -u)
mkfifo "$QEMU_IN"

"${QEMU_CMD[@]}" < "$QEMU_IN" > "$OUTPUT" 2>&1 &
PID=$!
exec 3>"$QEMU_IN"

# Wait for shell prompt
ELAPSED=0
BOOT_TIMEOUT=10
prompt_found=false
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
  echo "ERROR: QEMU failed to boot (no shell prompt after ${BOOT_TIMEOUT}s)"
  exit 0
fi

echo "Shell prompt reached in ${ELAPSED}s"

# Send a single for-loop command that runs all tests sequentially.
# Each test is run directly -- if it crashes or exits non-zero, we
# report FAIL. If the whole session times out, remaining tests are
# reported as HANG.
#
# We send the command as a single line to avoid pipe-buffering issues
# with the busybox shell reading character-by-character.

# Build the test list as a space-separated string
TEST_NAMES=""
for exe in "${TESTS[@]}"; do
  TEST_NAMES+=" $(basename "$exe" -static.exe)"
done

# Send a compact one-liner for-loop
echo "for t in$TEST_NAMES; do /bin/libc-test/\$t >/dev/null 2>&1 && echo PASS:\$t || echo FAIL:\$t; done; echo ALL_TESTS_DONE; poweroff -f" >&3 2>/dev/null || true
exec 3>&- 2>/dev/null || true

# Wait for QEMU to exit or session timeout
W=0
while [ "$W" -lt "$SESSION_TIMEOUT" ]; do
  if ! kill -0 "$PID" 2>/dev/null; then break; fi
  # Check if all tests completed
  if grep -q "ALL_TESTS_DONE" "$OUTPUT" 2>/dev/null; then
    # Give poweroff a moment to terminate QEMU
    sleep 2
    if kill -0 "$PID" 2>/dev/null; then
      kill "$PID" 2>/dev/null || true
    fi
    break
  fi
  sleep 1
  W=$((W + 1))
done

# Kill QEMU if still running (session timeout)
timed_out=false
if kill -0 "$PID" 2>/dev/null; then
  timed_out=true
  kill "$PID" 2>/dev/null || true
fi
wait "$PID" 2>/dev/null || true

# Step 6: Parse results from the combined output
# Strip ANSI escape sequences for reliable parsing
CLEAN_OUTPUT=$(sed 's/\x1b\[[0-9;]*m//g' "$OUTPUT")

PASSED=0
FAILED=0
HUNG=0
FAIL_LIST=""
TOTAL=${#TESTS[@]}

for exe in "${TESTS[@]}"; do
  name=$(basename "$exe" -static.exe)
  if echo "$CLEAN_OUTPUT" | grep -q "^PASS:$name"; then
    PASSED=$((PASSED + 1))
  elif echo "$CLEAN_OUTPUT" | grep -q "^FAIL:$name"; then
    FAILED=$((FAILED + 1))
    FAIL_LIST+="  FAIL: $name\n"
  else
    HUNG=$((HUNG + 1))
    FAIL_LIST+="  HANG: $name\n"
  fi
done

rm -f "$OUTPUT" "$QEMU_IN"

if [ "$TOTAL" -gt 0 ]; then
  PCT=$(( PASSED * 100 / TOTAL ))
else
  PCT=0
fi

if $timed_out; then
  echo ""
  echo "WARNING: QEMU session timed out after ${SESSION_TIMEOUT}s"
  echo "Some tests may not have run."
fi

echo ""
echo "========================================"
echo "  libc-test results: $PASSED/$TOTAL passed ($PCT%)"
echo "  ($FAILED failed, $HUNG hung/not-run)"
echo "========================================"

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "Failed/hung tests:"
  printf '%b' "$FAIL_LIST"
fi

# Always exit 0 — this test reports progress, not pass/fail.
exit 0
