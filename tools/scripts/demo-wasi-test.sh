#!/usr/bin/env bash
#
# WASI demo test: boot petal shell (with Linux feature enabled), run
# /bin/hello.wasm, verify it produces expected output via wasi-runner.
#
# Usage: tools/scripts/demo-wasi-test.sh <arch>
#   arch: aarch64
#
# Exit code 0 = all tests passed
# Exit code 1 = any test failed

set -euo pipefail

ARCH="${1:?Usage: $0 <arch>}"
TIMEOUT=30

case "$ARCH" in
  aarch64)
    QEMU_BASE_CMD=(
      qemu-system-aarch64
      -m 2G -smp 4 -display none -no-reboot -nographic
      -machine virt -cpu cortex-a72
      -serial mon:stdio
    )
    ;;
  *)
    echo "ERROR: demo-wasi-test.sh does not yet support arch '$ARCH'"
    exit 1
    ;;
esac

# Build the kernel with Linux feature and petal shell as init
echo "==> Building kernel (petal shell + linux)..."
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/shell" \
  cargo bin -m "qemu-${ARCH}" --flavour linux

KERNEL="target/qemu-${ARCH}/release/kernel.bin"
ROOTFS="target/qemu-${ARCH}/release/${ARCH}-linux.img"

if [ ! -f "$KERNEL" ]; then
  echo "FAIL: kernel not found at $KERNEL"
  exit 1
fi
if [ ! -f "$ROOTFS" ]; then
  echo "FAIL: rootfs image not found at $ROOTFS"
  exit 1
fi

# Create temp files for QEMU I/O
TMPD=$(mktemp -d)
OUTPUT="$TMPD/output"
QEMU_IN="$TMPD/qemu_in"
touch "$OUTPUT"
mkfifo "$QEMU_IN"

cleanup() {
  exec 3>&- 2>/dev/null || true
  [ -n "${QEMU_PID:-}" ] && kill "$QEMU_PID" 2>/dev/null || true
  wait "$QEMU_PID" 2>/dev/null || true
  rm -rf "$TMPD"
}
trap cleanup EXIT

echo "==> Booting petal shell in QEMU..."
"${QEMU_BASE_CMD[@]}" \
  -kernel "$KERNEL" \
  -initrd "$ROOTFS" \
  < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for the petal shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "petal>" "$OUTPUT" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before petal prompt appeared"
    echo "--- QEMU output ---"
    cat "$OUTPUT"
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

if [ "$ELAPSED" -ge "$TIMEOUT" ]; then
  echo "FAIL: petal prompt not found within ${TIMEOUT}s"
  echo "--- QEMU output ---"
  cat "$OUTPUT"
  exit 1
fi

echo "==> Petal shell ready. Running WASI test..."
FAILED=0

# Test: /bin/hello.wasm (WASI binary via wasi-runner)
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
echo "/bin/hello.wasm" >&3
# Wait for prompt to return after WASI process exits
for i in $(seq 1 15); do
  CUR=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
  [ "$CUR" -gt "$PROMPTS_BEFORE" ] && break
  sleep 1
done
if grep -q "Hello from WASI on zCore!" "$OUTPUT" 2>/dev/null; then
  echo "PASS: /bin/hello.wasm"
else
  echo "FAIL: /bin/hello.wasm — expected 'Hello from WASI on zCore!'"
  FAILED=$((FAILED + 1))
fi

# Verify prompt returned by sending another command
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
echo "echo WASI_CHECK" >&3
for i in $(seq 1 10); do
  if grep -q "WASI_CHECK" "$OUTPUT" 2>/dev/null; then break; fi
  sleep 1
done
if grep -q "WASI_CHECK" "$OUTPUT" 2>/dev/null; then
  echo "PASS: prompt returned after hello.wasm"
else
  echo "FAIL: prompt did not return after hello.wasm"
  FAILED=$((FAILED + 1))
fi

# Check for panics
if grep -q "panic" "$OUTPUT" 2>/dev/null; then
  echo "FAIL: kernel panicked"
  FAILED=$((FAILED + 1))
fi

# Clean exit
echo "exit" >&3
sleep 2

echo ""
if [ "$FAILED" -eq 0 ]; then
  echo "========================================"
  echo "  WASI demo: All tests PASSED"
  echo "========================================"
  exit 0
else
  echo "========================================"
  echo "  WASI demo: $FAILED test(s) FAILED"
  echo "========================================"
  echo "--- QEMU output ---"
  cat "$OUTPUT"
  exit 1
fi
