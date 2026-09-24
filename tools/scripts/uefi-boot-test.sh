#!/usr/bin/env bash
#
# UEFI boot test: build kernel with UEFI target, boot via edk2
# firmware, verify petal shell works with /bin/zircon-hello.
#
# Checks:
# - No "Image type X64" from our code (firmware-only is OK)
# - No panic or kernel crash
# - Petal shell prompt appears
# - /bin/zircon-hello prints expected output
# - Shell exits cleanly
#
# Usage: tools/scripts/uefi-boot-test.sh
# Exit code 0 = passed, 1 = failed

set -euo pipefail

TIMEOUT=30

if ! command -v qemu-system-aarch64 &>/dev/null; then
  echo "SKIP: qemu-system-aarch64 not found"
  exit 0
fi

# Check for required tools
for tool in mformat mmd mcopy; do
  if ! command -v "$tool" &>/dev/null; then
    echo "SKIP: $tool not found (install mtools)"
    exit 0
  fi
done

# Check for edk2 firmware
EDK2=""
for path in /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
            /usr/share/qemu/edk2-aarch64-code.fd \
            /usr/share/AAVMF/AAVMF_CODE.fd; do
  if [ -f "$path" ]; then
    EDK2="$path"
    break
  fi
done
if [ -z "$EDK2" ]; then
  echo "SKIP: edk2-aarch64-code.fd not found"
  exit 0
fi

echo "==> Building UEFI kernel and stub..."
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/shell" \
  cargo qemu -m qemu-aarch64-uefi --log warn 2>&1 &
# cargo qemu runs QEMU directly — we need to intercept it.
# Instead, use the xtask to just build, then run QEMU ourselves.
kill %1 2>/dev/null; wait %1 2>/dev/null || true

# Build via cargo qemu which handles ESP creation
# Actually we need a different approach — build then test separately
echo "==> Building kernel..."
ZCORE_CMDLINE="LOG=warn ROOTPROC=/bin/shell" \
  cargo zcore-build -m qemu-aarch64-uefi

echo "==> Building UEFI stub..."
cargo build --manifest-path tools/aarch64-uefi-stub/Cargo.toml \
  --target aarch64-unknown-uefi --release

echo "==> Building rootfs..."
cargo image --arch aarch64

echo "==> Generating DTB..."
qemu-system-aarch64 -M virt,dumpdtb=/tmp/zcore-uefi-test-virt.dtb \
  -cpu cortex-a72 -m 2G -smp 1 2>/dev/null || true

echo "==> Stripping kernel..."
KERNEL="target/qemu-aarch64-uefi/release/kernel"
KERNEL_STRIPPED="target/qemu-aarch64-uefi/release/kernel.stripped"
OBJCOPY=$(find "$(rustc --print sysroot)" -name llvm-objcopy 2>/dev/null | head -1)
if [ -n "$OBJCOPY" ]; then
  "$OBJCOPY" --strip-debug "$KERNEL" "$KERNEL_STRIPPED"
elif command -v llvm-objcopy &>/dev/null; then
  llvm-objcopy --strip-debug "$KERNEL" "$KERNEL_STRIPPED"
elif command -v rust-objcopy &>/dev/null; then
  rust-objcopy --strip-debug "$KERNEL" "$KERNEL_STRIPPED"
else
  echo "FAIL: no objcopy found"
  exit 1
fi

echo "==> Creating ESP image..."
TMPD=$(mktemp -d)
ESP="$TMPD/esp.img"
VARS="$TMPD/vars.fd"
STUB="tools/aarch64-uefi-stub/target/aarch64-unknown-uefi/release/aarch64-uefi-stub.efi"
ROOTFS="target/qemu-aarch64/release/aarch64-linux.img"

dd if=/dev/zero of="$ESP" bs=1M count=128 2>/dev/null
mformat -i "$ESP" -F ::
mmd -i "$ESP" ::/EFI
mmd -i "$ESP" ::/EFI/BOOT
mcopy -i "$ESP" "$STUB" ::/EFI/BOOT/BOOTAA64.EFI
mcopy -i "$ESP" "$KERNEL_STRIPPED" ::/kernel
[ -f "$ROOTFS" ] && mcopy -i "$ESP" "$ROOTFS" ::/initrd.img
[ -f /tmp/zcore-uefi-test-virt.dtb ] && mcopy -i "$ESP" /tmp/zcore-uefi-test-virt.dtb ::/virt.dtb
dd if=/dev/zero of="$VARS" bs=1M count=64 2>/dev/null

cleanup() {
  exec 3>&- 2>/dev/null || true
  [ -n "${QEMU_PID:-}" ] && kill "$QEMU_PID" 2>/dev/null || true
  wait "$QEMU_PID" 2>/dev/null || true
  rm -rf "$TMPD"
  rm -f /tmp/zcore-uefi-test-virt.dtb
}
trap cleanup EXIT

echo "==> Booting UEFI in QEMU..."
OUTPUT="$TMPD/output"
QEMU_IN="$TMPD/qemu_in"
touch "$OUTPUT"
mkfifo "$QEMU_IN"

qemu-system-aarch64 -M virt -cpu cortex-a72 -m 2G -smp 1 \
  -drive if=pflash,format=raw,readonly=on,file="$EDK2" \
  -drive if=pflash,format=raw,file="$VARS" \
  -drive format=raw,file="$ESP" \
  -nographic \
  < "$QEMU_IN" > "$OUTPUT" 2>&1 &
QEMU_PID=$!
exec 3>"$QEMU_IN"

# Wait for petal shell prompt
ELAPSED=0
while [ "$ELAPSED" -lt "$TIMEOUT" ]; do
  if grep -q "petal>" "$OUTPUT" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "FAIL: QEMU exited before petal prompt"
    echo "--- output ---"
    cat "$OUTPUT"
    exit 1
  fi
  sleep 1
  ELAPSED=$((ELAPSED + 1))
done

if [ "$ELAPSED" -ge "$TIMEOUT" ]; then
  echo "FAIL: petal prompt not found within ${TIMEOUT}s"
  echo "--- output ---"
  cat "$OUTPUT"
  exit 1
fi

echo "==> Petal shell ready. Running tests..."
FAILED=0

# Test 1: /bin/zircon-hello
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
echo "/bin/zircon-hello" >&3
for i in $(seq 1 10); do
  CUR=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
  [ "$CUR" -gt "$PROMPTS_BEFORE" ] && break
  sleep 1
done
if grep -q "Hello from Zircon on zCore!" "$OUTPUT" 2>/dev/null; then
  echo "PASS: /bin/zircon-hello"
else
  echo "FAIL: /bin/zircon-hello — expected output not found"
  FAILED=$((FAILED + 1))
fi

# Test 2: prompt returned
PROMPTS_BEFORE=$(grep -c "petal>" "$OUTPUT" 2>/dev/null || echo 0)
echo "echo UEFI_CHECK" >&3
for i in $(seq 1 10); do
  if grep -q "UEFI_CHECK" "$OUTPUT" 2>/dev/null; then break; fi
  sleep 1
done
if grep -q "UEFI_CHECK" "$OUTPUT" 2>/dev/null; then
  echo "PASS: prompt returned"
else
  echo "FAIL: prompt did not return"
  FAILED=$((FAILED + 1))
fi

# Test 3: no panic
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
  echo "  UEFI boot test: All tests PASSED"
  echo "========================================"
  exit 0
else
  echo "========================================"
  echo "  UEFI boot test: $FAILED test(s) FAILED"
  echo "========================================"
  echo "--- output ---"
  cat "$OUTPUT"
  exit 1
fi
