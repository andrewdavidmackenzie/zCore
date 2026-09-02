#!/usr/bin/env bash
#
# Zircon boot smoke test: build petal programs, package into ZBIs,
# build kernel, run each program in QEMU, check for expected output.
#
# Usage: tools/scripts/zircon-boot-test.sh <arch>
#   arch: aarch64 (others may be added later)
#
# Exit code 0 = all tests passed
# Exit code 1 = any test failed

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30

case "$ARCH" in
  aarch64)
    KERNEL="target/aarch64/release/zcore"
    QEMU_BASE_CMD=(
      qemu-system-aarch64
      -m 2G -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: zircon-boot-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Build userstart
echo "Building userstart for $ARCH..."
if ! cargo build -p userstart \
  --target "aarch64-unknown-none-softfloat" \
  --release --target-dir target/userstart; then
  echo "ERROR: userstart build failed."
  exit 1
fi

USERSTART="target/userstart/aarch64-unknown-none-softfloat/release/userstart"

# Run a single petal test program
# Args: bin_name expected_pattern
run_test() {
  local bin_name="$1"
  local expected_pattern="$2"

  echo ""
  echo "==> Testing petal '$bin_name'..."

  # Build and package
  cargo petal-zbi --arch "$ARCH" --bin "$bin_name" 2>&1 | tail -3

  local ZBI="target/petal/${ARCH}/petal.zbi"

  # Build kernel with this ZBI
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
    echo "FAIL: kernel build failed for '$bin_name'"
    return 1
  fi

  # Run in QEMU
  local OUTPUT
  OUTPUT=$(mktemp)

  "${QEMU_BASE_CMD[@]}" -kernel "$KERNEL" > "$OUTPUT" 2>&1 &
  local QEMU_PID=$!

  local ELAPSED=0
  while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      local QEMU_EXIT=0
      wait "$QEMU_PID" || QEMU_EXIT=$?
      if grep -q "$expected_pattern" "$OUTPUT" 2>/dev/null; then
        echo "PASS: $bin_name (exit=$QEMU_EXIT)"
        rm -f "$OUTPUT"
        return 0
      else
        echo "FAIL: $bin_name - expected pattern not found: '$expected_pattern'"
        echo "--- QEMU output ---"
        cat "$OUTPUT"
        rm -f "$OUTPUT"
        return 1
      fi
    fi
    sleep 1
    ELAPSED=$((ELAPSED + 1))
  done

  echo "FAIL: $bin_name - QEMU did not exit within ${TIMEOUT}s"
  echo "--- QEMU output ---"
  cat "$OUTPUT"
  rm -f "$OUTPUT"
  kill "$QEMU_PID" 2>/dev/null || true
  return 1
}

# Run all petal tests
FAILED=0

run_test "hello" "petal: Hello from petal on zCore!" || FAILED=$((FAILED + 1))
run_test "channel_test" "channel_test: PASS" || FAILED=$((FAILED + 1))
run_test "vmo_test" "vmo_test: PASS" || FAILED=$((FAILED + 1))

echo ""
if [ "$FAILED" -eq 0 ]; then
  echo "========================================"
  echo "  All petal tests PASSED"
  echo "========================================"
  exit 0
else
  echo "========================================"
  echo "  $FAILED petal test(s) FAILED"
  echo "========================================"
  exit 1
fi
