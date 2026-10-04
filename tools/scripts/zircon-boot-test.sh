#!/usr/bin/env bash
#
# Zircon boot smoke test: build petal programs, package into ZBIs,
# build kernel, run each program in QEMU, check for expected output.
#
# Usage: tools/scripts/zircon-boot-test.sh <arch>
#   arch: aarch64, x86_64
#
# Exit code 0 = all tests passed
# Exit code 1 = any test failed

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30

# Per-architecture configuration.
case "$ARCH" in
  aarch64)
    RUST_TARGET="aarch64-unknown-none-softfloat"
    KERNEL="target/qemu-aarch64/release/kernel"
    ;;
  x86_64)
    RUST_TARGET="x86_64-unknown-none"
    KERNEL="target/qemu-x86_64/release/kernel"
    ;;
  *)
    echo "ERROR: zircon-boot-test.sh does not support arch '$ARCH'"
    exit 1
    ;;
esac

USERSTART="target/userstart/${RUST_TARGET}/release/userstart"

# Build userstart as a static PIE with PIC codegen.
# Must match xtask/src/petal.rs build_userstart() flags:
#   --pie (from build.rs), -Crelocation-model=pic, -Z build-std=core,alloc
echo "Building userstart for $ARCH..."
if ! CARGO_ENCODED_RUSTFLAGS="-Crelocation-model=pic" \
  cargo build -p userstart \
  --target "$RUST_TARGET" \
  --release --target-dir target/userstart \
  -Z build-std=core,alloc; then
  echo "ERROR: userstart build failed."
  exit 1
fi

# x86_64 needs the bootimage tool and OVMF firmware.
if [ "$ARCH" = "x86_64" ]; then
  echo "Building x86-bootimage tool..."
  cargo build --release --manifest-path tools/x86-bootimage/Cargo.toml

  # Locate OVMF firmware.
  # shellcheck source=tools/scripts/find-ovmf.sh
  . tools/scripts/find-ovmf.sh
  OVMF=$(find_ovmf)
  echo "Using OVMF: $OVMF"
fi

# launch_qemu: start QEMU for the current arch, writing output to $1.
launch_qemu() {
  local output="$1"
  case "$ARCH" in
    aarch64)
      qemu-system-aarch64 \
        -m 2G -display none -no-reboot -nographic \
        -machine virt -cpu cortex-a72 \
        -serial mon:stdio \
        -kernel "$KERNEL" > "$output" 2>&1 &
      ;;
    x86_64)
      tools/x86-bootimage/target/release/x86-bootimage \
        "$KERNEL" \
        target/qemu-x86_64/release/boot.img
      qemu-system-x86_64 \
        -m 4G -display none -no-reboot -nographic \
        -machine q35 -smp 1 \
        -cpu qemu64,+fsgsbase,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
        -serial mon:stdio \
        -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
        -drive "format=raw,file=target/qemu-x86_64/release/boot.img" \
        > "$output" 2>&1 &
      ;;
  esac
}

# Run a single petal test program.
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
  if ! USERSTART_ELF="$(pwd)/$USERSTART" \
    PETAL_ZBI="$(pwd)/$ZBI" \
    ZCORE_CMDLINE="LOG=${LOG:-info}" \
    cargo zcore-build -m "qemu-${ARCH}"; then
    echo "FAIL: kernel build failed for '$bin_name'"
    return 1
  fi

  # Run in QEMU
  local OUTPUT
  OUTPUT=$(mktemp)

  launch_qemu "$OUTPUT"
  local QEMU_PID=$!

  local ELAPSED=0
  while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
    # Check if the expected pattern appeared (QEMU may still be running).
    if grep -q "$expected_pattern" "$OUTPUT" 2>/dev/null; then
      kill "$QEMU_PID" 2>/dev/null || true
      wait "$QEMU_PID" 2>/dev/null || true
      echo "PASS: $bin_name (exit=0)"
      rm -f "$OUTPUT"
      return 0
    fi
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

run_test "hello" "Hello from Zircon on zCore!" || FAILED=$((FAILED + 1))
run_test "channel-test" "channel_test: PASS" || FAILED=$((FAILED + 1))
run_test "vmo-test" "vmo_test: PASS" || FAILED=$((FAILED + 1))
run_test "vdso-test" "vdso_test: PASS" || FAILED=$((FAILED + 1))
run_test "alloc-test" "alloc_test: PASS" || FAILED=$((FAILED + 1))
run_test "shell" "shell: self-test PASS" || FAILED=$((FAILED + 1))
run_test "process-mem-test" "process_mem_test: PASS" || FAILED=$((FAILED + 1))
run_test "exception-test" "exception_test: PASS" || FAILED=$((FAILED + 1))
# vdso_call_test blocked on #241 (petal ELF loading with data sections)

echo ""
if [ "$FAILED" -eq 0 ]; then
  echo "========================================"
  echo "  All petal tests PASSED ($ARCH)"
  echo "========================================"
  exit 0
else
  echo "========================================"
  echo "  $FAILED petal test(s) FAILED ($ARCH)"
  echo "========================================"
  exit 1
fi
