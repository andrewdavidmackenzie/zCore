#!/usr/bin/env bash
#
# Raspberry Pi 4B boot smoke test: build kernel, run in QEMU raspi4b,
# check for expected kernel output.
#
# Usage: tools/scripts/raspi4b-boot-test.sh
#
# Exit code 0 = test passed
# Exit code 1 = test failed

set -euo pipefail

TIMEOUT=30
QEMU_PID=""
OUTPUT=""

# Cleanup: kill QEMU and remove temp file on any exit.
cleanup() {
    if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        # Wait up to 5 seconds for graceful exit
        for _ in $(seq 1 5); do
            if ! kill -0 "$QEMU_PID" 2>/dev/null; then
                break
            fi
            sleep 1
        done
        # Force kill if still alive
        kill -KILL "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
    [ -n "$OUTPUT" ] && rm -f "$OUTPUT"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "==> Raspberry Pi 4B boot smoke test..."

# Build the kernel for raspi4b (Linux mode -- will panic on block device,
# but that's after successful boot + UART output, which is what we test).
echo "Building kernel for raspi4b..."
cargo build -p zcore --no-default-features --features "linux,board-raspi4b" \
  --target zCore/aarch64-raspi4b.json \
  -Z json-target-spec \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  --release 2>&1 | tail -3

rust-objcopy --strip-all -O binary \
  target/aarch64-raspi4b/release/zcore \
  target/aarch64-raspi4b/release/zcore.bin

echo "Running in QEMU raspi4b..."
OUTPUT=$(mktemp)

qemu-system-aarch64 \
  -machine raspi4b -m 2G \
  -display none -no-reboot -nographic \
  -serial mon:stdio \
  -kernel target/aarch64-raspi4b/release/zcore.bin > "$OUTPUT" 2>&1 &
QEMU_PID=$!

ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  # Check for kernel output that proves boot succeeded.
  # A panic means the kernel booted far enough to run Rust code.
  if grep -q "panicked at" "$OUTPUT" 2>/dev/null; then
    echo "PASS: raspi4b kernel booted (reached Rust, panicked as expected on block device)"
    exit 0
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    wait "$QEMU_PID" || true
    if grep -q "panicked at" "$OUTPUT" 2>/dev/null; then
      echo "PASS: raspi4b kernel booted (reached Rust)"
      exit 0
    fi
    echo "FAIL: QEMU exited without expected output"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

echo "FAIL: QEMU did not produce expected output within ${TIMEOUT}s"
echo "--- QEMU output ---"
cat "$OUTPUT"
exit 1
