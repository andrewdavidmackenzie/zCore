#!/usr/bin/env bash
#
# Demo 2 test: boot petal shell (with Linux feature enabled), run both
# /bin/zircon-hello and /bin/linux-hello from the petal prompt, verify
# both produce the expected output.
#
# Usage: tools/scripts/demo-petal-test.sh <arch>
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
    echo "ERROR: demo-petal-test.sh does not yet support arch '$ARCH'"
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

echo "==> Petal shell ready. Running tests..."
FAILED=0

# Count prompts seen so far (from self-test boot).
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)

# Test 1: /bin/zircon-hello (Zircon binary from petal shell)
echo "/bin/zircon-hello" >&3
# Wait for prompt to return (new prompt after command output)
for i in $(seq 1 10); do
  CUR=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
  [ "$CUR" -gt "$PROMPTS_BEFORE" ] && break
  sleep 1
done
if grep -q "Hello from Zircon on zCore!" "$OUTPUT" 2>/dev/null; then
  echo "PASS: /bin/zircon-hello"
else
  echo "FAIL: /bin/zircon-hello — expected 'Hello from Zircon on zCore!'"
  FAILED=$((FAILED + 1))
fi
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)

# Test 2: /bin/linux-hello (Linux binary from petal shell)
echo "/bin/linux-hello" >&3
# Wait for prompt to return after Linux process exits
for i in $(seq 1 10); do
  CUR=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
  [ "$CUR" -gt "$PROMPTS_BEFORE" ] && break
  sleep 1
done
if grep -q "Hello from Linux on zCore!" "$OUTPUT" 2>/dev/null; then
  echo "PASS: /bin/linux-hello"
else
  echo "FAIL: /bin/linux-hello — expected 'Hello from Linux on zCore!'"
  FAILED=$((FAILED + 1))
fi

# Verify prompt returned by sending another command
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
echo "echo PROMPT_CHECK" >&3
for i in $(seq 1 10); do
  if grep -q "PROMPT_CHECK" "$OUTPUT" 2>/dev/null; then break; fi
  sleep 1
done
if grep -q "PROMPT_CHECK" "$OUTPUT" 2>/dev/null; then
  echo "PASS: prompt returned after linux-hello"
else
  echo "FAIL: prompt did not return after linux-hello"
  FAILED=$((FAILED + 1))
fi

# Check for panics in the output
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
  echo "  Demo 2 (petal): All tests PASSED"
  echo "========================================"
  exit 0
else
  echo "========================================"
  echo "  Demo 2 (petal): $FAILED test(s) FAILED"
  echo "========================================"
  echo "--- QEMU output ---"
  cat "$OUTPUT"
  exit 1
fi
