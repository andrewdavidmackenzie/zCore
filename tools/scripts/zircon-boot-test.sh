#!/usr/bin/env bash
#
# Zircon boot smoke test: build petal programs, run each in QEMU,
# check for expected output.
#
# aarch64: builds kernel once + rootfs with all petals, runs QEMU 8
#          times with -append to select the test binary at runtime.
# x86_64:  builds kernel per-petal (UEFI has no -append equivalent).
#          With rust-cache, the relink is near-instant.
#
# Usage: tools/scripts/zircon-boot-test.sh <arch>
#   arch: aarch64, x86_64
#
# Exit code 0 = all tests passed
# Exit code 1 = any test failed

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
# On macOS, QEMU runs x86_64 via emulation (no KVM) on aarch64 hosts,
# which is significantly slower. Increase timeout to avoid flaky failures.
if [ "$(uname -s)" = "Darwin" ]; then
  TIMEOUT=144
else
  TIMEOUT=72
fi

# Read core count from target config
source "$(dirname "$0")/get-target-cores.sh"
CORES=$(get_target_cores "$ARCH")

# Per-architecture configuration.
case "$ARCH" in
  aarch64)
    RUST_TARGET="aarch64-unknown-none-softfloat"
    KERNEL="target/qemu-aarch64/release/kernel"
    KERNEL_BIN="target/qemu-aarch64/release/kernel.bin"
    ROOTFS_IMG="target/qemu-${ARCH}/release/${ARCH}-zircon.img"
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

# --- aarch64: build once, run many ---
if [ "$ARCH" = "aarch64" ]; then
  echo "==> Building kernel once (aarch64 rootfs mode)..."

  # Build all petal binaries into the rootfs
  cargo xtask zircon-rootfs --arch "$ARCH"

  # Build one petal ZBI (needed as fallback, but rootfs overrides)
  cargo petal-zbi --arch "$ARCH" --bin hello 2>&1 | tail -1

  ZBI="target/petal/${ARCH}/petal.zbi"

  # Build kernel with default ROOTPROC
  USERSTART_ELF="$(pwd)/$USERSTART" \
    PETAL_ZBI="$(pwd)/$ZBI" \
    ZCORE_CMDLINE="LOG=${LOG:-info} ROOTPROC=/bin/hello" \
    cargo zcore-build -m "qemu-${ARCH}"

  # Strip ELF to raw binary (QEMU needs raw binary for DTB passthrough)
  OBJCOPY=$(find "$(rustc --print sysroot)" -name llvm-objcopy 2>/dev/null | head -1)
  if [ -n "$OBJCOPY" ]; then
    "$OBJCOPY" --strip-all -O binary "$KERNEL" "$KERNEL_BIN"
  elif command -v llvm-objcopy >/dev/null 2>&1; then
    llvm-objcopy --strip-all -O binary "$KERNEL" "$KERNEL_BIN"
  elif command -v rust-objcopy >/dev/null 2>&1; then
    rust-objcopy --strip-all -O binary "$KERNEL" "$KERNEL_BIN"
  else
    echo "WARNING: no objcopy found, using ELF (DTB may not work)"
    cp "$KERNEL" "$KERNEL_BIN"
  fi
fi

# launch_qemu: start QEMU for the current arch, writing output to $1.
# For aarch64, $2 is the ROOTPROC path (overrides compile-time default).
launch_qemu() {
  local output="$1"
  local rootproc="${2:-}"
  case "$ARCH" in
    aarch64)
      local append_args="ROOTPROC=${rootproc} LOG=${LOG:-warn}"
      qemu-system-aarch64 \
        -m 2G -display none -no-reboot -nographic \
        -machine virt -cpu cortex-a72 -smp "$CORES" \
        -serial mon:stdio \
        -kernel "$KERNEL_BIN" \
        -initrd "$ROOTFS_IMG" \
        -append "$append_args" \
        > "$output" 2>&1 &
      ;;
    x86_64)
      if ! tools/x86-bootimage/target/release/x86-bootimage \
        "$KERNEL" \
        target/qemu-x86_64/release/boot.img; then
        echo "ERROR: x86-bootimage failed" >&2
        return 1
      fi
      qemu-system-x86_64 \
        -m 4G -display none -no-reboot -nographic \
        -machine q35 -smp "$CORES" \
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

  if [ "$ARCH" = "x86_64" ]; then
    # x86_64: rebuild kernel per petal (no -append in UEFI boot)
    cargo petal-zbi --arch "$ARCH" --bin "$bin_name" 2>&1 | tail -3

    local ZBI="target/petal/${ARCH}/petal.zbi"

    if ! USERSTART_ELF="$(pwd)/$USERSTART" \
      PETAL_ZBI="$(pwd)/$ZBI" \
      ZCORE_CMDLINE="LOG=${LOG:-info}" \
      cargo zcore-build -m "qemu-${ARCH}"; then
      echo "FAIL: kernel build failed for '$bin_name'"
      return 1
    fi
  fi

  # Run in QEMU
  local OUTPUT
  OUTPUT=$(mktemp)

  launch_qemu "$OUTPUT" "/bin/$bin_name"
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
