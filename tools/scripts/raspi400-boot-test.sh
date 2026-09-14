#!/usr/bin/env bash
#
# Raspberry Pi 400 boot smoke test: build kernel in Zircon mode with
# petal shell, run in QEMU raspi400, check for shell self-test output.
#
# Usage: tools/scripts/raspi400-boot-test.sh
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
        for _ in $(seq 1 5); do
            if ! kill -0 "$QEMU_PID" 2>/dev/null; then
                break
            fi
            sleep 1
        done
        kill -KILL "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
    [ -n "$OUTPUT" ] && rm -f "$OUTPUT"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "==> Raspberry Pi 400 boot smoke test (Zircon mode)..."

# Build using the Makefile target (Zircon mode with petal shell)
make raspi400-build 2>&1 | tail -5

echo "Running in QEMU raspi4b..."
OUTPUT=$(mktemp)

qemu-system-aarch64 \
  -machine raspi4b -m 2G \
  -display none -no-reboot -nographic \
  -serial mon:stdio \
  -kernel target/aarch64-raspi400/release/zcore.bin > "$OUTPUT" 2>&1 &
QEMU_PID=$!

ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  # Check for shell self-test output (proves full boot chain works)
  if grep -q "shell: self-test PASS" "$OUTPUT" 2>/dev/null; then
    echo "PASS: raspi400 petal shell self-test passed"
    exit 0
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    wait "$QEMU_PID" || true
    if grep -q "shell: self-test PASS" "$OUTPUT" 2>/dev/null; then
      echo "PASS: raspi400 petal shell self-test passed"
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
